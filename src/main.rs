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
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};

use crate::{
    app_handler::{App, Window},
    editor::{Editor, EditorMode},
    engine::{
        backend::GpuBackend, renderer::Renderer, settings::Settings, vulkan_backend::VulkanBackend,
        wgpu_backend::WgpuBackend,
    },
    input::{
        first_person::FPSController,
        input_handler::{
            Action, ApplicationRequest, Button, InputHandler as _, ScrollAxis, translate_key,
        },
    },
};
fn pause() {
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    write!(stdout, "Press enter to continue...").unwrap();
    stdout.flush().unwrap();

    let _ = stdin.read(&mut [0u8]).unwrap();
}

enum Backend {
    Vulkan,
    Wgpu,
}

fn vk_render_loop(
    app: App,
    settings: Settings,
    window_rcv: mpsc::Receiver<Arc<Window>>,
    should_quit: Arc<AtomicBool>,
) {
    let mut last_mode: Option<editor::EditorMode> = None;
    let mut last_cursor_pos: Option<(f64, f64)> = None;

    let mut renderer = None;
    let mut editor = None;
    let mut window = None;

    let (width, height) = settings.resolution;
    let aspect = width as f32 / height as f32;
    let fov = settings.camera_fov;
    let view_distance = settings.view_distance;
    let mut fps = FPSController::create(aspect, fov, 0.1, view_distance);

    let mut start = Instant::now();
    loop {
        if window.is_none() {
            println!("Waiting for Window creation..");
            let w = match window_rcv.recv_timeout(Duration::from_secs(30)) {
                Ok(w) => w,
                Err(RecvTimeoutError::Timeout) => {
                    // window not ready, wait again
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    eprintln!("Window initialization channel closed. Cannot proceed.");
                    break;
                }
            };
            window = Some(w)
        }

        if renderer.is_none() && window.is_some() {
            let w = window.as_ref().expect("has to be some here!");
            println!("Got a Window, initializing VK");
            match create_vk_renderer(&w, &settings) {
                Ok((r, e)) => {
                    renderer = Some(r);
                    editor = Some(e);
                }
                Err(e) => {
                    eprintln!("Failed to create VK renderer: {}", e);
                    app.request_quit();
                    return;
                }
            }
        }

        if app.wants_quit() || should_quit.load(Ordering::SeqCst) {
            if let Some(renderer) = &mut renderer {
                if let Err(e) = renderer.backend_mut().wait_idle() {
                    eprintln!("Wait Idle error on shutddown: {e}")
                }
            }
            break;
        }

        let events = app.poll_events();

        let elapsed = start.elapsed().as_secs_f32();
        start = Instant::now();

        let Some(window) = &window else {
            // no window, nothing to do.
            continue;
        };
        run_event_loop::<VulkanBackend>(
            &app,
            window.winit_window(),
            &events,
            renderer.as_mut(),
            editor.as_mut(),
            &mut last_cursor_pos,
            &mut last_mode,
            &mut fps,
            elapsed,
        );
    }
}

fn run_vulkan() -> Result<(), Box<dyn std::error::Error>> {
    let settings = engine::settings::Settings::load();
    let (width, height) = settings.resolution;

    println!(
        "sparkle-rs: creating {}x{} window with Vulkan backend.",
        width, height
    );

    let (window_snd, window_rcv) = mpsc::channel();

    let (mut app, event_loop) = app_handler::App::new(width, height, "Sparkle-rs", window_snd)?;

    let should_quit = Arc::new(AtomicBool::new(false));

    let render_thread = {
        let should_quit = Arc::clone(&should_quit);

        let app = app.clone();
        std::thread::spawn(move || {
            vk_render_loop(app, settings, window_rcv, should_quit);
        })
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Window Event Loop failure: {e}");
    }

    should_quit.store(true, Ordering::SeqCst);

    if let Err(e) = render_thread.join() {
        eprintln!("Failed to exit render loop cleanly: {e:?}");
    }

    Ok(())
}

// fn run_wgpu() -> Result<(), Box<dyn std::error::Error>> {
//     let settings = engine::settings::Settings::load();
//     let (width, height) = settings.resolution;
//     println!(
//         "sparkle-rs: creating {}x{} window with wgpu backend.",
//         width, height
//     );

//     let (window_snd, window_rcv) = mpsc::channel();
//     let (window, event_loop) = app_handler::App::new(width, height, "Sparkle-rs", window_snd)?;

//     let aspect = width as f32 / height as f32;
//     let fov = settings.camera_fov;
//     let view_distance = settings.view_distance;

//     let mut fps = FPSController::create(aspect, fov, 0.1, view_distance);

//     let mut last_mode: Option<editor::EditorMode> = None;
//     let mut last_cursor_pos: Option<(f64, f64)> = None;

//     let mut renderer = None;
//     let mut editor = None;

//     window.run(event_loop, move |window, events| {
//         if renderer.is_none() && window.is_initialized() {
//             match create_wgpu_renderer(window, &settings) {
//                 Ok((r, e)) => {
//                     renderer = Some(r);
//                     editor = Some(e);
//                 }
//                 Err(e) => {
//                     eprintln!("Failed to create WGPU renderer: {}", e);
//                     window.request_quit();
//                     return;
//                 }
//             }
//         }
//         run_event_loop(
//             window,
//             events,
//             renderer.as_mut(),
//             editor.as_mut(),
//             &mut last_cursor_pos,
//             &mut last_mode,
//             &mut fps,
//         );
//     })?;

//     Ok(())
// }

fn run_event_loop<B: GpuBackend>(
    app: &App,
    window: &winit::window::Window,
    events: &[WindowEvent],
    renderer: Option<&mut Renderer<B>>,
    editor: Option<&mut Editor<B>>,
    last_cursor_pos: &mut Option<(f64, f64)>,
    last_mode: &mut Option<EditorMode>,
    fps: &mut FPSController,
    detla_t: f32,
) {
    if events.contains(&WindowEvent::CloseRequested) {
        if let Some(renderer) = &renderer
            && let Err(e) = renderer.backend().wait_idle()
        {
            println!("Error while waiting for GPU to idle: {e:?}");
        }
    }
    let Some(editor) = editor else {
        return;
    };
    handle_events(events, editor, last_cursor_pos, fps, app, window);

    let Some(renderer) = renderer else {
        return;
    };

    let winit::dpi::PhysicalSize {
        width: ww,
        height: wh,
    } = window.inner_size();
    let (bw, bh) = renderer.backend().resolution();
    if ww != bw || wh != bh {
        renderer.resize(ww, wh);
    }

    let current_mode = editor.mode();
    if *last_mode != Some(current_mode) {
        match current_mode {
            editor::EditorMode::Editor => {
                renderer.set_camera_projection(editor.orbit_camera());
                println!("Switched to Editor mode (orbit camera)");
            }
            editor::EditorMode::Play => {
                renderer.set_camera_projection(fps);
                println!("Switched to Play mode (FPS camera)");
            }
        }
        *last_mode = Some(current_mode);
    }

    if let Err(e) = renderer.backend_mut().begin_frame() {
        eprintln!("begin_frame error: {}", e);
        app.request_quit();
        return;
    }

    editor.begin_frame(window, renderer);

    if editor.pending_quit {
        app.request_quit();
        return;
    }

    if current_mode == editor::EditorMode::Play {
        fps.update(detla_t, renderer.settings_mut());
        renderer.update_state(detla_t, fps);
        if let Err(e) = renderer.render_scene(fps) {
            eprintln!("Render error: {}", e);
            app.request_quit();
            return;
        }
    } else {
        renderer.update_state(detla_t, editor.orbit_camera());
        if let Err(e) = renderer.render_scene(editor.orbit_camera()) {
            eprintln!("Render error: {}", e);
            app.request_quit();
            return;
        }
    }
    editor.render_overlay(renderer);
    if let Err(e) = renderer.finish_frame() {
        eprintln!("Frame finish error: {}", e);
    }

    window.pre_present_notify();
    if let Err(e) = renderer.present() {
        eprintln!("Frame present error: {e}")
    }
}

fn create_vk_renderer(
    window: &Arc<Window>,
    settings: &Settings,
) -> Result<(Renderer<VulkanBackend>, Editor<VulkanBackend>), Box<dyn std::error::Error>> {
    let (width, height) = settings.resolution;

    let aspect = width as f32 / height as f32;
    let fov = settings.camera_fov;
    let view_distance = settings.view_distance;

    let vk_backend = engine::vulkan_backend::initialize(Arc::clone(window), &settings)?;
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

    let mut editor = editor::Editor::<engine::vulkan_backend::VulkanBackend>::new(
        window.winit_window(),
        aspect,
        fov,
        0.1,
        view_distance,
    );
    renderer.set_camera_projection(editor.orbit_camera());

    let scene_path = "assets/glTF/Sponza.gltf";
    match renderer.load_scene(scene_path) {
        Ok(()) => println!("Scene loaded: {}", scene_path),
        Err(e) => {
            eprintln!("Warning: scene loading failed: {}", e);
        }
    }

    Ok((renderer, editor))
}

fn create_wgpu_renderer(
    window: &Arc<Window>,
    settings: &Settings,
) -> Result<(Renderer<WgpuBackend>, Editor<WgpuBackend>), Box<dyn std::error::Error>> {
    let (width, height) = settings.resolution;

    let aspect = width as f32 / height as f32;
    let fov = settings.camera_fov;
    let view_distance = settings.view_distance;

    let backend = engine::wgpu_backend::WgpuBackend::init(window.winit_window_arc())?;
    let mut renderer = engine::renderer::Renderer::create(backend, settings.clone())?;

    match renderer.init_draw_programs() {
        Ok(()) => {}
        Err(e) => {
            eprintln!(
                "Warning: draw program init failed: {} (falling back to clear-only)",
                e
            );
        }
    }

    let mut editor = editor::Editor::<engine::wgpu_backend::WgpuBackend>::new(
        window.winit_window(),
        aspect,
        fov,
        0.1,
        view_distance,
    );
    renderer.set_camera_projection(editor.orbit_camera());

    let scene_path = "assets/glTF/Sponza.gltf";
    match renderer.load_scene(scene_path) {
        Ok(()) => println!("Scene loaded: {}", scene_path),
        Err(e) => {
            eprintln!("Warning: scene loading failed: {}", e);
        }
    }

    Ok((renderer, editor))
}

fn handle_events<B: GpuBackend>(
    events: &[WindowEvent],
    editor: &mut editor::Editor<B>,
    last_cursor_pos: &mut Option<(f64, f64)>,
    fps: &mut FPSController,
    app: &App,
    window: &winit::window::Window,
) {
    for event in events {
        if let WindowEvent::KeyboardInput {
            event: key_event, ..
        } = event
        {
            if key_event.state == ElementState::Pressed && !key_event.repeat {
                if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F1) =
                    key_event.physical_key
                {
                    editor.toggle_mode();
                }
            }
        }

        if let WindowEvent::CursorMoved { position, .. } = event {
            if let Some((lx, ly)) = last_cursor_pos {
                let dx = position.x - *lx;
                let dy = position.y - *ly;

                editor.handle_mouse_delta(dx as f32, dy as f32);

                if editor.mode() == editor::EditorMode::Play {
                    fps.handle_mouse_move(dx as i32, dy as i32);
                }
            }

            let centre_on_move = editor.mode() == editor::EditorMode::Play && fps.is_aiming();
            if centre_on_move {
                let size = window.inner_size();
                let cx = size.width as f64 / 2.0;
                let cy = size.height as f64 / 2.0;
                let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                *last_cursor_pos = Some((cx, cy));
            } else {
                *last_cursor_pos = Some((position.x, position.y));
            }
        }

        let consumed = editor.handle_window_event(window, event);

        if consumed {
            continue;
        }

        match event {
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                let action = match key_event.state {
                    ElementState::Pressed => Action::Down,
                    ElementState::Released => Action::Up,
                };
                let key = match key_event.physical_key {
                    winit::keyboard::PhysicalKey::Code(code) => translate_key(code),
                    _ => input::input_handler::Key::None,
                };
                match fps.handle_key(key, action) {
                    ApplicationRequest::Quit => app.request_quit(),
                    _ => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let action = match state {
                    ElementState::Pressed => Action::Down,
                    ElementState::Released => Action::Up,
                };
                let btn = match button {
                    MouseButton::Left => Button::Left,
                    MouseButton::Right => Button::Right,
                    MouseButton::Middle => Button::Middle,
                    _ => continue,
                };
                match fps.handle_mouse(btn, action) {
                    ApplicationRequest::SnapMouse => {
                        window.set_cursor_visible(false);
                        let size = window.inner_size();
                        let cx = size.width as f64 / 2.0;
                        let cy = size.height as f64 / 2.0;
                        let _ =
                            window.set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                        *last_cursor_pos = Some((cx, cy));
                    }
                    ApplicationRequest::UnsnapMouse => {
                        window.set_cursor_visible(true);
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    if y.abs() > 0.0 {
                        fps.handle_wheel(ScrollAxis::Vertical, y * 24.0);
                    }
                    if x.abs() > 0.0 {
                        fps.handle_wheel(ScrollAxis::Horizontal, x * 24.0);
                    }
                }
                MouseScrollDelta::PixelDelta(pos) => {
                    if pos.y.abs() > 0.0 {
                        fps.handle_wheel(ScrollAxis::Vertical, pos.y as f32);
                    }
                    if pos.x.abs() > 0.0 {
                        fps.handle_wheel(ScrollAxis::Horizontal, pos.x as f32);
                    }
                }
            },
            _ => {}
        }
    }
}

fn main() {
    // match run_wgpu() {
    //     Ok(_) => (),
    //     Err(e) => {
    //         println!("{}", e);
    //         pause();
    //     }
    // }
    match run_vulkan() {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e);
            pause();
        }
    }
}
