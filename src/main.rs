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

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};

use crate::{
    engine::backend::GpuBackend as _,
    input::{
        first_person::FPSController,
        input_handler::{
            Action, ApplicationRequest, Button, InputHandler as _, ScrollAxis, translate_key,
        },
    },
};
fn pause() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    write!(stdout, "Press enter to continue...").unwrap();
    stdout.flush().unwrap();

    let _ = stdin.read(&mut [0u8]).unwrap();
}

fn run_vulkan() -> Result<(), Box<dyn std::error::Error>> {
    let settings = engine::settings::Settings::load();
    let (width, height) = settings.resolution;

    println!(
        "sparkle-rs: creating {}x{} window with Vulkan backend.",
        width, height
    );

    let (window, event_loop) = window::Window::new(width, height, "Sparkle-rs")?;
    let w = window.winit_window_arc();

    let vulkan_backend = engine::vulkan_backend::initialize(w, &settings)?;

    window.run(event_loop, move |window, events| {
        // handle_events(events, &mut editor, &mut last_cursor_pos, &mut fps, window);
        if let Err(e) = vulkan_backend.draw() {
            println!("Draw failed: {e:?}");
        }
    })?;

    Ok(())
}

fn run_wgpu() -> Result<(), Box<dyn std::error::Error>> {
    let settings = engine::settings::Settings::load();
    let (width, height) = settings.resolution;
    println!(
        "sparkle-rs: creating {}x{} window with wgpu backend.",
        width, height
    );

    let (window, event_loop) = window::Window::new(width, height, "Sparkle-rs")?;
    let w = window.winit_window_arc();

    let aspect = width as f32 / height as f32;
    let fov = settings.camera_fov;
    let view_distance = settings.view_distance;

    let backend = engine::wgpu_backend::WgpuBackend::init(w.clone())?;
    let mut renderer = engine::renderer::Renderer::create(backend, settings)?;

    let mut fps = FPSController::create(aspect, fov, 0.1, view_distance);

    match renderer.init_draw_programs() {
        Ok(()) => {}
        Err(e) => {
            eprintln!(
                "Warning: draw program init failed: {} (falling back to clear-only)",
                e
            );
        }
    }

    let mut editor = editor::Editor::new(&w, renderer.backend(), aspect, fov, 0.1, view_distance);
    renderer.set_camera_projection(editor.orbit_camera());

    let scene_path = "assets/glTF/Sponza.gltf";
    match renderer.load_scene(scene_path) {
        Ok(()) => println!("Scene loaded: {}", scene_path),
        Err(e) => {
            eprintln!("Warning: scene loading failed: {}", e);
        }
    }

    let mut last_mode: Option<editor::EditorMode> = None;
    let mut last_cursor_pos: Option<(f64, f64)> = None;

    window.run(event_loop, move |window, events| {
        handle_events(events, &mut editor, &mut last_cursor_pos, &mut fps, window);

        let (ww, wh) = window.get_resolution();
        let (bw, bh) = renderer.backend().resolution();
        if ww != bw || wh != bh {
            renderer.resize(ww, wh);
        }

        let current_mode = editor.mode();
        if last_mode != Some(current_mode) {
            match current_mode {
                editor::EditorMode::Editor => {
                    renderer.set_camera_projection(editor.orbit_camera());
                    println!("Switched to Editor mode (orbit camera)");
                }
                editor::EditorMode::Play => {
                    renderer.set_camera_projection(&fps);
                    println!("Switched to Play mode (FPS camera)");
                }
            }
            last_mode = Some(current_mode);
        }

        editor.begin_frame(window.winit_window(), &mut renderer);

        if editor.pending_quit {
            window.request_quit();
            return;
        }

        if let Some(ref _path) = editor.pending_scene_load.take() {}

        match current_mode {
            editor::EditorMode::Editor => {
                renderer.update_state(0.016, editor.orbit_camera());

                if let Err(e) = renderer.render_scene(editor.orbit_camera()) {
                    eprintln!("Render error: {}", e);
                    return;
                }
            }
            editor::EditorMode::Play => {
                fps.update(0.016, &mut renderer.settings_mut());
                renderer.update_state(0.016, &mut fps);

                if let Err(e) = renderer.render_scene(&fps) {
                    eprintln!("Render error: {}", e);
                    return;
                }
            }
        }

        editor.render_overlay(&mut renderer);

        if let Err(e) = renderer.finish_frame() {
            eprintln!("Frame finish error: {}", e);
        }
    })?;

    Ok(())
}

fn handle_events(
    events: &[WindowEvent],
    editor: &mut editor::Editor,
    last_cursor_pos: &mut Option<(f64, f64)>,
    fps: &mut FPSController,
    window: &mut window::Window,
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
            *last_cursor_pos = Some((position.x, position.y));
        }

        let consumed = editor.handle_window_event(window.winit_window(), event);

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
                    ApplicationRequest::Quit => window.request_quit(),
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
                        window.winit_window().set_cursor_visible(false);
                        let size = window.winit_window().inner_size();
                        let cx = size.width as f64 / 2.0;
                        let cy = size.height as f64 / 2.0;
                        let _ = window
                            .winit_window()
                            .set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                        *last_cursor_pos = Some((cx, cy));
                    }
                    ApplicationRequest::UnsnapMouse => {
                        window.winit_window().set_cursor_visible(true);
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
