#![allow(unused_assignments)]
#![allow(dead_code)]
extern crate nalgebra_glm as glm;

mod app_handler;
mod editor;
mod engine;
mod import;
mod input;
mod util;

use std::{
    io::{Read as _, Write as _},
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{Receiver, RecvTimeoutError, TryRecvError},
    },
    time::{Duration, Instant},
};

use crate::{
    app_handler::{FrameData, RenderChannels, RenderFrameInfo, Window},
    editor::{EditCommand, EditCommands, EditorMode, EditorRenderer},
    engine::{
        backend::GpuBackend, renderer::Renderer, settings::Settings, vulkan_backend::VulkanBackend,
    },
};

fn pause() {
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    write!(stdout, "Press enter to continue...").unwrap();
    stdout.flush().unwrap();

    let _ = stdin.read(&mut [0u8]).unwrap();
}

/// Drain channel to get the latest FrameData, discarding older ones.
/// Returns None if channel is disconnected (quit signal).
fn drain_latest(receiver: &Receiver<FrameData>) -> Result<FrameData, ()> {
    // First, block until we get at least one frame
    let mut latest = match receiver.recv() {
        Ok(f) => f,
        Err(_) => return Err(()), // channel closed
    };

    // Then drain any additional queued frames (keep latest state)
    loop {
        match receiver.try_recv() {
            Ok(mut newer) => {
                // Merge edit commands from older frame into newer
                newer.edit_commands.extend(latest.edit_commands);

                // Merge texture deltas carefully:
                // If newer has a full update (pos=None) for a texture, it replaces older updates
                // If newer has partial updates, we need older updates too (for the base texture)
                let mut textures_to_skip = std::collections::HashSet::new();
                for (id, delta) in &newer.full_output.textures_delta.set {
                    if delta.pos.is_none() {
                        // Full update - older updates for this texture are obsolete
                        textures_to_skip.insert(*id);
                    }
                }

                // Add older textures that aren't being fully replaced
                for (id, delta) in latest.full_output.textures_delta.set {
                    if !textures_to_skip.contains(&id) {
                        newer.full_output.textures_delta.set.push((id, delta));
                    }
                }

                // Merge free list (textures to delete)
                newer
                    .full_output
                    .textures_delta
                    .free
                    .extend(latest.full_output.textures_delta.free);

                latest = newer;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
    Ok(latest)
}

/// Apply edit commands to the scene through the renderer.
fn apply_edit_commands<B: GpuBackend>(commands: EditCommands, renderer: &mut Renderer<B>) {
    for cmd in commands {
        match cmd {
            EditCommand::SetNodeTransform {
                node_name,
                new_transform,
            } => {
                renderer.set_node_transform(&node_name, new_transform);
            }
            EditCommand::UpdateLight { index, new_light } => {
                renderer.update_light(index, new_light);
            }
            EditCommand::AddLight { light } => {
                renderer.add_light(light);
            }
            EditCommand::RemoveLight { index } => {
                renderer.remove_light(index);
            }
            EditCommand::Undo => {
                // TODO: Implement undo on render thread
                // This requires moving the undo stack to render thread
                // or sending undo data back and forth
            }
            EditCommand::Redo => {
                // TODO: Implement redo on render thread
            }
        }
    }
}

fn vk_render_loop(channels: RenderChannels, settings: Settings) {
    let RenderChannels {
        frame_receiver,
        cmd_sender: _,
        window_receiver,
        render_info_sender,
        quit_flag,
        ..
    } = channels;

    // Wait for window from main thread
    println!("Waiting for Window creation..");
    let (window, egui_ctx) = match window_receiver.recv_timeout(Duration::from_secs(30)) {
        Ok((w, ctx)) => (w, ctx),
        Err(RecvTimeoutError::Timeout) => {
            eprintln!("Timed out waiting for window.");
            return;
        }
        Err(RecvTimeoutError::Disconnected) => {
            eprintln!("Window initialization channel closed. Cannot proceed.");
            return;
        }
    };

    println!("Got a Window, initializing VK");

    // Create renderer + editor renderer
    // Note: egui::Context is cheap to clone (it's Arc-based)
    let (mut renderer, editor_renderer) =
        match create_vk_renderer(&window, egui_ctx.clone(), &settings) {
            Ok((r, er)) => (r, er),
            Err(e) => {
                eprintln!("Failed to create VK renderer: {}", e);
                quit_flag.store(true, Ordering::SeqCst);
                return;
            }
        };

    let mut last_mode = EditorMode::Editor;
    let mut first_frame = true;

    // Main render loop — consumes FrameData from main thread
    loop {
        if quit_flag.load(Ordering::SeqCst) {
            break;
        }

        let frame = match drain_latest(&frame_receiver) {
            Ok(f) => f,
            Err(()) => break, // channel closed = quit
        };

        // Check for quit signal from main thread
        if frame.pending_quit {
            break;
        }

        // Handle scene load request
        if let Some(scene_path) = frame.pending_scene_load {
            match renderer.load_scene(&scene_path) {
                Ok(()) => println!("Scene loaded: {}", scene_path),
                Err(e) => eprintln!("Failed to load scene: {}", e),
            }
        }

        // Detect mode change → update camera projection
        if first_frame || frame.mode != last_mode {
            renderer.set_camera_projection(&frame.camera);
            last_mode = frame.mode;
            first_frame = false;
        }

        // Resize if needed
        let (bw, bh) = renderer.backend().resolution();
        if frame.window_size != (bw, bh) && frame.window_size.0 > 0 && frame.window_size.1 > 0 {
            renderer.resize(frame.window_size.0, frame.window_size.1);
        }

        // Begin frame timing measurement (wall-clock)
        // TODO: Enrich with GPU timestamps via Vulkan timestamp queries for more precise
        // GPU-side timing breakdown (vertex shader, fragment shader, present wait, etc.)
        let frame_start = Instant::now();

        // Begin GPU frame
        if let Err(e) = renderer.backend_mut().begin_frame() {
            eprintln!("begin_frame error: {}", e);
            quit_flag.store(true, Ordering::SeqCst);
            break;
        }

        // Apply edit commands from main thread UI
        apply_edit_commands(frame.edit_commands, &mut renderer);

        // Check quit from editor (if any quit command was sent)
        // Note: quit is now primarily handled on main thread via pending_quit

        // Update scene + render
        let mut cam = frame.camera.clone();
        renderer.update_state(frame.delta_t, &mut cam);
        if let Err(e) = renderer.render_scene(&cam) {
            eprintln!("Render error: {}", e);
            quit_flag.store(true, Ordering::SeqCst);
            break;
        }

        // Render egui overlay using FullOutput from main thread
        editor_renderer.render_overlay(&frame.full_output, &mut renderer);

        // Finish + present
        if let Err(e) = renderer.finish_frame() {
            eprintln!("Frame finish error: {}", e);
        }
        if let Err(e) = renderer.present() {
            eprintln!("Frame present error: {e}");
        }

        // Calculate frame time and send to main thread
        let frame_time_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        let render_info = RenderFrameInfo {
            frame_time_ms,
            gpu_time_ms: None, // TODO: Add GPU timestamp queries
        };
        // Use try_send - if channel is full (main thread hasn't consumed), it will overwrite
        // This matches our "latest only" semantics
        let _ = render_info_sender.try_send(render_info);
    }

    // Clean shutdown
    if let Err(e) = renderer.backend_mut().wait_idle() {
        eprintln!("Wait idle error on shutdown: {e}");
    }
}

fn create_vk_renderer(
    window: &Arc<Window>,
    egui_ctx: egui::Context,
    settings: &Settings,
) -> Result<(Renderer<VulkanBackend>, EditorRenderer), Box<dyn std::error::Error>> {
    let vk_backend = engine::vulkan_backend::initialize(Arc::clone(window), settings)?;
    let mut renderer = engine::renderer::Renderer::create(vk_backend, settings.clone())?;

    match renderer.init_draw_programs() {
        Ok(()) => {}
        Err(e) => {
            eprintln!(
                "Warning: draw program init failed: {} (falling back to clear-only)",
                e
            );
        }
    }

    // Create editor renderer with shared egui context
    // This ensures texture IDs are valid between Editor (main thread) and renderer
    let editor_renderer = EditorRenderer::new(egui_ctx);

    let scene_path = "assets/glTF/Sponza.gltf";
    match renderer.load_scene(scene_path) {
        Ok(()) => println!("Scene loaded: {}", scene_path),
        Err(e) => {
            eprintln!("Warning: scene loading failed: {}", e);
        }
    }

    Ok((renderer, editor_renderer))
}

fn run_vulkan() -> Result<(), Box<dyn std::error::Error>> {
    let settings = engine::settings::Settings::load();
    let (width, height) = settings.resolution;

    println!(
        "sparkle-rs: creating {}x{} window with Vulkan backend.",
        width, height
    );

    let (mut app, event_loop, channels) = app_handler::App::new(
        width,
        height,
        "Sparkle-rs",
        settings.camera_fov,
        0.1,
        settings.view_distance,
        settings.clone(),
    )?;

    let quit_flag = Arc::clone(&channels.quit_flag);

    let render_thread = {
        std::thread::spawn(move || {
            vk_render_loop(channels, settings);
        })
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Window Event Loop failure: {e}");
    }

    quit_flag.store(true, Ordering::SeqCst);

    if let Err(e) = render_thread.join() {
        eprintln!("Failed to exit render loop cleanly: {e:?}");
    }

    Ok(())
}

fn main() {
    match run_vulkan() {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e);
            pause();
        }
    }
}
