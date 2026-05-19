#![allow(unused_assignments)]
#![allow(dead_code)]
extern crate nalgebra_glm as glm;

mod editor;
mod engine;
mod import;
mod input;
mod window;

use std::io;
use std::io::prelude::*;
fn pause() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    write!(stdout, "Press enter to continue...").unwrap();
    stdout.flush().unwrap();

    let _ = stdin.read(&mut [0u8]).unwrap();
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    use engine::backend::GpuBackend;
    use input::first_person::FPSController;
    use std::cell::RefCell;
    use std::rc::Rc;

    let settings = engine::settings::Settings::load();
    let (width, height) = settings.resolution;
    println!(
        "sparkle-rs: creating {}x{} window with wgpu backend.",
        width, height
    );

    let window = window::Window::new(width, height, "Sparkle-rs");

    let mut renderer: Option<engine::renderer::Renderer<engine::wgpu_backend::WgpuBackend>> = None;
    let mut settings_opt = Some(settings);

    // Shared editor state for event callbacks.
    // The Rc<RefCell<Option<...>>> pattern lets us set the editor in the frame
    // callback while having the event callbacks (set earlier) reference it.
    let editor_rc: Rc<RefCell<Option<editor::Editor>>> = Rc::new(RefCell::new(None));

    // FPS controller — stored in a shared Rc so the event filter
    // callback (which captures this) can switch cameras on mode change.
    let fps_for_f1: Rc<RefCell<Option<Rc<RefCell<FPSController>>>>> =
        Rc::new(RefCell::new(None));

    // Track the last editor mode to detect mode changes and switch cameras.
    let mut last_mode: Option<editor::EditorMode> = None;

    // Set up the event filter closure that forwards events to the editor.
    let editor_for_events = editor_rc.clone();

    window.run(move |window| {
        // ---- One-time initialization (first frame) ----
        if renderer.is_none() {
            if let Some(w) = window.winit_window_arc() {
                match engine::wgpu_backend::WgpuBackend::init(w.clone()) {
                    Ok(backend) => {
                        let s = settings_opt.take().unwrap();
                        let aspect = width as f32 / height as f32;
                        let fov = s.camera_fov;
                        let view_distance = s.view_distance;

                        match engine::renderer::Renderer::create(backend, s) {
                            Ok(mut r) => {
                                // Create FPS camera controller
                                let fps = FPSController::create_ptr(
                                    aspect,
                                    fov,
                                    0.1,
                                    view_distance,
                                );
                                *fps_for_f1.borrow_mut() = Some(fps.clone());

                                // Wire input handler to window + renderer
                                window.set_input_handler(fps.clone());
                                r.set_input_handler(fps.clone());

                                // Initialize all draw programs (shaders + pipelines)
                                match r.init_draw_programs() {
                                    Ok(()) => {}
                                    Err(e) => {
                                        eprintln!(
                                            "Warning: draw program init failed: {} \
                                             (falling back to clear-only)",
                                            e
                                        );
                                    }
                                }

                                // Create the editor
                                let ed = editor::Editor::new(
                                    &w,
                                    r.backend(),
                                    aspect,
                                    fov,
                                    0.1,
                                    view_distance,
                                );

                                // Editor starts in Editor mode — use orbit camera
                                let orbit = ed.orbit_camera();
                                r.set_camera(orbit);

                                // Store the editor in the shared Rc so event
                                // callbacks can access it.
                                *editor_for_events.borrow_mut() = Some(ed);

                                // Set up event filter: forward events to editor/egui
                                let editor_for_filter = editor_for_events.clone();
                                window.set_event_filter(move |winit_win, event| {
                                    let mut editor_cell = editor_for_filter.borrow_mut();
                                    if let Some(ref mut ed) = *editor_cell {
                                        // Handle F1 mode toggle before egui processes
                                        // the event. This is never consumed by egui.
                                        if let winit::event::WindowEvent::KeyboardInput {
                                            event: key_event,
                                            ..
                                        } = event
                                        {
                                            if key_event.state
                                                == winit::event::ElementState::Pressed
                                                && !key_event.repeat
                                            {
                                                if let winit::keyboard::PhysicalKey::Code(
                                                    winit::keyboard::KeyCode::F1,
                                                ) = key_event.physical_key
                                                {
                                                    ed.toggle_mode();
                                                    // Don't consume — let it fall through
                                                    // so the frame callback sees the new mode.
                                                }
                                            }
                                        }

                                        return ed.handle_window_event(winit_win, event);
                                    }
                                    false
                                });

                                // Set up mouse delta callback for orbit camera
                                let editor_for_delta = editor_for_events.clone();
                                window.set_mouse_delta_callback(move |dx, dy| {
                                    let mut editor_cell = editor_for_delta.borrow_mut();
                                    if let Some(ref mut ed) = *editor_cell {
                                        ed.handle_mouse_delta(dx, dy);
                                    }
                                });

                                // Load glTF scene
                                let scene_path = "assets/glTF/Sponza.gltf";
                                match r.load_scene(scene_path) {
                                    Ok(()) => println!("Scene loaded: {}", scene_path),
                                    Err(e) => {
                                        eprintln!(
                                            "Warning: scene loading failed: {}",
                                            e
                                        );
                                    }
                                }
                                renderer = Some(r);
                            }
                            Err(e) => {
                                eprintln!("Failed to create renderer: {}", e);
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to initialize wgpu backend: {}", e);
                        return;
                    }
                }
            }
        }

        // ---- Per-frame update ----
        if let Some(ref mut r) = renderer {
            // Handle resize
            let (ww, wh) = window.get_resolution();
            let (bw, bh) = r.backend().resolution();
            if ww != bw || wh != bh {
                r.resize(ww, wh);
            }

            // Borrow the editor for this frame
            let mut editor_cell = editor_for_events.borrow_mut();
            if let Some(ref mut ed) = *editor_cell {
                // Detect mode changes and switch cameras accordingly
                let current_mode = ed.mode();
                if last_mode != Some(current_mode) {
                    match current_mode {
                        editor::EditorMode::Editor => {
                            let orbit = ed.orbit_camera();
                            r.set_camera(orbit);
                            println!("Switched to Editor mode (orbit camera)");
                        }
                        editor::EditorMode::Play => {
                            if let Some(ref fps) = *fps_for_f1.borrow() {
                                r.set_camera(fps.clone());
                                println!("Switched to Play mode (FPS camera)");
                            }
                        }
                    }
                    last_mode = Some(current_mode);
                }

                // Begin egui frame (draws UI, extracts scene data, applies edits)
                if let Some(w) = window.winit_window() {
                    ed.begin_frame(w, r);
                }

                // Handle pending actions from editor UI
                if ed.pending_quit {
                    window.request_quit();
                    return;
                }

                // Handle scene load requests
                if let Some(ref _path) = ed.pending_scene_load.take() {
                    // Scene loading will be improved in Phase 3 with a file dialog.
                    // For now the menu just reloads the default scene.
                }

                // Step 1: Update input and camera state
                r.update_state(0.016);

                // Step 2: Render the scene (all passes, no present)
                if let Err(e) = r.render_scene() {
                    eprintln!("Render error: {}", e);
                    return;
                }

                // Step 3: Render egui overlay on top of the scene
                ed.render_overlay(r);

                // Step 4: Submit and present
                if let Err(e) = r.finish_frame() {
                    eprintln!("Frame finish error: {}", e);
                }
            } else {
                // No editor — use the legacy single-call path
                if let Err(e) = r.update(0.016) {
                    eprintln!("Render error: {}", e);
                }
            }
        }
    });

    Ok(())
}

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e);
            pause();
        }
    }
}
