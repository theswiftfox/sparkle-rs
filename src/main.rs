#![allow(unused_assignments)]
#![allow(dead_code)]
extern crate nalgebra_glm as glm;

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

    let settings = engine::settings::Settings::load();
    let (width, height) = settings.resolution;
    println!(
        "sparkle-rs: creating {}x{} window with wgpu backend.",
        width, height
    );

    let window = window::Window::new(width, height, "Sparkle-rs");
    let mut renderer: Option<engine::renderer::Renderer<engine::wgpu_backend::WgpuBackend>> = None;
    let mut settings_opt = Some(settings);

    window.run(move |window| {
        // Initialize renderer on first frame (winit window must be created first)
        if renderer.is_none() {
            if let Some(w) = window.winit_window_arc() {
                match engine::wgpu_backend::WgpuBackend::init(w) {
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

                                // Set camera AFTER draw programs exist so projection
                                // matrix is propagated to all passes.
                                r.set_camera(fps.clone());

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

        if let Some(ref mut r) = renderer {
            // Handle resize
            let (ww, wh) = window.get_resolution();
            let (bw, bh) = r.backend().resolution();
            if ww != bw || wh != bh {
                r.resize(ww, wh);
            }

            // Update & render
            if let Err(e) = r.update(0.016) {
                eprintln!("Render error: {}", e);
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
