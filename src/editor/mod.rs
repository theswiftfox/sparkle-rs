//! Editor module for sparkle-rs.
//!
//! Provides an integrated editor mode with egui-based UI, an orbit camera for
//! scene inspection, and toggling between Editor and Play modes.
//!
//! The editor owns all egui state (context, winit integration, wgpu renderer)
//! and drives the orbit camera directly from raw winit events.

pub mod ui;

use crate::engine::backend::GpuBackend;
use crate::engine::renderer::Renderer;
use crate::engine::wgpu_backend::WgpuBackend;
use crate::input::orbit::OrbitCamera;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

/// Whether the application is in editor or play mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    /// Orbit camera, egui UI visible, scene editing.
    Editor,
    /// FPS camera, no UI overlay, game-like controls.
    Play,
}

/// Tracks which mouse buttons are currently held for orbit camera input.
struct MouseState {
    right_down: bool,
    middle_down: bool,
}

/// The main editor state.
///
/// Owns the egui context, winit/wgpu integration layers, orbit camera, and
/// mode toggle. Created once during initialization and passed into the
/// frame loop.
pub struct Editor {
    mode: EditorMode,
    egui_ctx: egui::Context,
    egui_winit: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    orbit_camera: Rc<RefCell<OrbitCamera>>,
    mouse_state: MouseState,
    frame_start: Instant,
    frame_count: u32,
    fps: f32,
    fps_update_timer: f32,
    /// Set to true when the user requests a scene load via the menu.
    pub pending_scene_load: Option<String>,
    /// Set to true when the user requests quit via the menu.
    pub pending_quit: bool,
    /// Whether egui wants pointer input this frame (cursor is over a panel).
    egui_wants_pointer: bool,
    /// Whether egui wants keyboard input this frame (text field focused, etc.).
    egui_wants_keyboard: bool,
    /// Egui output from the last `begin_frame()` call, consumed by `render_overlay()`.
    pending_egui_output: Option<egui::FullOutput>,
}

impl Editor {
    /// Create a new editor.
    ///
    /// Must be called after the wgpu backend is initialized.
    pub fn new(
        window: &winit::window::Window,
        backend: &WgpuBackend,
        aspect: f32,
        fov: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let egui_ctx = egui::Context::default();

        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None, // max texture size (None = use device default)
        );

        let egui_renderer = egui_wgpu::Renderer::new(
            backend.device(),
            backend.surface_format(),
            None, // depth format (egui renders as overlay, no depth)
            1,    // sample count
            false,
        );

        let orbit_camera = OrbitCamera::new_ptr(aspect, fov, near, far);

        Editor {
            mode: EditorMode::Editor,
            egui_ctx,
            egui_winit,
            egui_renderer,
            orbit_camera,
            mouse_state: MouseState {
                right_down: false,
                middle_down: false,
            },
            frame_start: Instant::now(),
            frame_count: 0,
            fps: 0.0,
            fps_update_timer: 0.0,
            pending_scene_load: None,
            pending_quit: false,
            egui_wants_pointer: false,
            egui_wants_keyboard: false,
            pending_egui_output: None,
        }
    }

    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    pub fn orbit_camera(&self) -> Rc<RefCell<OrbitCamera>> {
        self.orbit_camera.clone()
    }

    /// Returns true if egui consumed pointer input this frame
    /// (cursor is over a panel or widget).
    pub fn wants_pointer_input(&self) -> bool {
        self.egui_wants_pointer
    }

    /// Returns true if egui consumed keyboard input this frame.
    pub fn wants_keyboard_input(&self) -> bool {
        self.egui_wants_keyboard
    }

    /// Toggle between Editor and Play modes. Returns the new mode.
    pub fn toggle_mode(&mut self) -> EditorMode {
        self.mode = match self.mode {
            EditorMode::Editor => EditorMode::Play,
            EditorMode::Play => EditorMode::Editor,
        };
        self.mode
    }

    /// Forward a winit window event to egui. Returns true if egui consumed it.
    ///
    /// Call this BEFORE forwarding the event to the game input handler.
    /// If this returns true, the game input handler should skip the event.
    pub fn handle_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        let response = self.egui_winit.on_window_event(window, event);

        // Update cached "wants input" state
        self.egui_wants_pointer = self.egui_ctx.wants_pointer_input();
        self.egui_wants_keyboard = self.egui_ctx.wants_keyboard_input();

        // In editor mode, also handle orbit camera input for events
        // that egui didn't consume.
        if self.mode == EditorMode::Editor && !response.consumed {
            self.handle_orbit_input(event);
        }

        // In play mode, don't let egui consume events (UI is hidden)
        if self.mode == EditorMode::Play {
            return false;
        }

        // In editor mode, consume ALL mouse/keyboard/scroll events so they
        // never reach the game input handler (e.g., FPS controller). The orbit
        // camera handles relevant viewport input via handle_orbit_input above.
        // Structural events like Resized are handled before this method is
        // called and are not affected.
        true
    }

    /// Process mouse/scroll events for the orbit camera.
    fn handle_orbit_input(&mut self, event: &winit::event::WindowEvent) {
        use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};

        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = *state == ElementState::Pressed;
                match button {
                    MouseButton::Right => self.mouse_state.right_down = pressed,
                    MouseButton::Middle => self.mouse_state.middle_down = pressed,
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { .. } => {
                // Cursor movement is handled via handle_mouse_delta() called
                // from the window module's delta tracking.
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 24.0,
                };
                if scroll.abs() > 0.0 {
                    self.orbit_camera.borrow_mut().zoom(scroll);
                }
            }
            _ => {}
        }
    }

    /// Feed mouse movement delta to the orbit camera.
    /// Called from the window module's cursor tracking.
    pub fn handle_mouse_delta(&mut self, dx: f32, dy: f32) {
        if self.mode != EditorMode::Editor {
            return;
        }
        if self.egui_wants_pointer {
            return;
        }
        if self.mouse_state.right_down {
            self.orbit_camera.borrow_mut().orbit(dx, dy);
        } else if self.mouse_state.middle_down {
            self.orbit_camera.borrow_mut().pan(dx, dy);
        }
    }

    /// Begin an egui frame: gather input, run the UI, produce draw commands.
    ///
    /// Call once per frame, before `render_overlay()`. The actual egui panels
    /// and menus are drawn inside this call.
    pub fn begin_frame(&mut self, window: &winit::window::Window) {
        let raw_input = self.egui_winit.take_egui_input(window);

        // Update FPS counter
        self.frame_count += 1;
        let elapsed = self.frame_start.elapsed().as_secs_f32();
        self.fps_update_timer += elapsed;
        if self.fps_update_timer >= 0.5 {
            self.fps = self.frame_count as f32 / self.fps_update_timer;
            self.frame_count = 0;
            self.fps_update_timer = 0.0;
        }
        self.frame_start = Instant::now();

        // Run egui frame — all UI drawing happens in the closure
        let mode = self.mode;
        let fps = self.fps;
        let mut pending_scene_load = None;
        let mut pending_quit = false;
        let mut toggle_mode = false;

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            if mode == EditorMode::Editor {
                ui::draw_menu_bar(
                    ctx,
                    &mut pending_scene_load,
                    &mut pending_quit,
                    &mut toggle_mode,
                );
            }
            ui::draw_viewport_overlay(ctx, fps, mode);
        });

        self.pending_egui_output = Some(full_output);
        self.pending_scene_load = pending_scene_load;
        self.pending_quit = pending_quit;
        if toggle_mode {
            self.toggle_mode();
        }
    }

    /// Render the egui overlay onto the backbuffer.
    ///
    /// Call after the scene has been rendered (renderer.render_scene())
    /// but before finish_frame(). This produces a wgpu command buffer that
    /// draws egui's triangles on top of the scene.
    pub fn render_overlay(&mut self, renderer: &mut Renderer<WgpuBackend>) {
        let full_output = match self.pending_egui_output.take() {
            Some(output) => output,
            None => return, // begin_frame() was not called
        };
        let backend = renderer.backend_mut();

        let clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        // Upload textures
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(backend.device(), backend.queue(), *id, image_delta);
        }

        let (res_w, res_h) = backend.resolution();
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [res_w, res_h],
            pixels_per_point: full_output.pixels_per_point,
        };

        // Render egui via a helper to avoid self-referential borrow issues
        // between `self.egui_renderer` and the wgpu encoder/render pass.
        render_egui(
            &mut self.egui_renderer,
            backend.device(),
            backend.queue(),
            backend.backbuffer_view(),
            &clipped_primitives,
            &screen_descriptor,
        );

        // Free textures that egui no longer needs
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }
}

/// Helper function to render egui primitives.
///
/// Separated from `Editor::render_overlay` to avoid self-referential borrow
/// issues: the `egui_renderer` borrow and the wgpu encoder borrow would
/// conflict if done through `&mut self` in the same scope.
fn render_egui(
    egui_renderer: &mut egui_wgpu::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backbuffer_view: &wgpu::TextureView,
    clipped_primitives: &[egui::ClippedPrimitive],
    screen_descriptor: &egui_wgpu::ScreenDescriptor,
) {
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui_encoder"),
        });

    egui_renderer.update_buffers(
        device,
        queue,
        &mut encoder,
        clipped_primitives,
        screen_descriptor,
    );

    {
        let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: backbuffer_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // forget_lifetime() erases the compile-time borrow tie between the
        // render pass and the encoder, replacing it with a runtime check.
        // This is the standard pattern for egui-wgpu 0.31+ which requires
        // RenderPass<'static>.
        egui_renderer.render(
            &mut render_pass.forget_lifetime(),
            clipped_primitives,
            screen_descriptor,
        );
    }

    queue.submit(std::iter::once(encoder.finish()));
}
