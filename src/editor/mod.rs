//! Editor module for sparkle-rs.
//!
//! Provides an integrated editor mode with egui-based UI, an orbit camera for
//! scene inspection, and toggling between Editor and Play modes.
//!
//! The editor owns all egui state (context, winit integration, wgpu renderer)
//! and drives the orbit camera directly from raw winit events.

pub mod gizmo;
pub mod picking;
pub mod transform;
pub mod ui;
pub mod undo;

use crate::engine::backend::GpuBackend;
use crate::engine::geometry::Light;
use crate::engine::renderer::Renderer;
use crate::engine::scene_info::NodeInfo;
use crate::engine::wgpu_backend::WgpuBackend;
use crate::input::Camera;
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
    /// Currently selected node name (None = nothing selected).
    pub selected_node: Option<String>,
    /// Snapshot of the scenegraph tree, refreshed each frame in begin_frame.
    pub scene_snapshot: Option<NodeInfo>,
    /// Snapshot of lights, refreshed each frame in begin_frame.
    pub scene_lights: Vec<Light>,

    /// State for the transform gizmo (active axis, drag start transform, etc.).
    gizmo_state: gizmo::GizmoState,
    /// Undo stack for scene edits, supporting merging multiple edits into one undo
    undo_stack: undo::UndoStack,
    /// Whether gizmo was dragging last frame (for undo recording on drag end).
    gizmo_was_dragging: bool,
    /// The node's local transform mat4 when gizmo drag started (for undo).
    gizmo_drag_old_transform: Option<glm::Mat4>,
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
            selected_node: None,
            scene_snapshot: None,
            scene_lights: Vec::new(),
            gizmo_state: gizmo::GizmoState::new(),
            undo_stack: undo::UndoStack::new(),
            gizmo_was_dragging: false,
            gizmo_drag_old_transform: None,
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

    /// Begin an egui frame: extract scene data, gather input, run the UI,
    /// produce draw commands.
    ///
    /// Takes `&mut Renderer` to extract scene snapshots and apply edits.
    /// Call once per frame, before `render_overlay()`.
    pub fn begin_frame(
        &mut self,
        window: &winit::window::Window,
        renderer: &mut Renderer<WgpuBackend>,
    ) {
        // Advance undo merge window
        self.undo_stack.new_frame();

        // Extract scene data snapshots for the UI to read
        self.scene_snapshot = renderer.scene_tree();
        self.scene_lights = renderer.lights().clone();

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

        // Collect state needed inside the egui closure (avoid borrowing self)
        let mode = self.mode;
        let fps = self.fps;
        let mut pending_scene_load = None;
        let mut pending_quit = false;
        let mut toggle_mode = false;
        let mut pending_save = false;
        let mut pending_load = false;
        let mut pending_undo = false;
        let mut pending_redo = false;
        let mut selected_node = self.selected_node.clone();
        // Use references instead of deep-cloning the scene tree every frame.
        // The closure captures these refs by copy (references are Copy).
        let scene_snapshot = &self.scene_snapshot;
        let scene_lights = &self.scene_lights;

        // Undo state for the Edit menu (captured before the frame)
        let can_undo = self.undo_stack.can_undo();
        let can_redo = self.undo_stack.can_redo();
        let undo_desc = self.undo_stack.undo_description();
        let redo_desc = self.undo_stack.redo_description();

        // Collect edits produced by the UI
        let mut transform_edits: Vec<(String, glm::Mat4)> = Vec::new();
        let mut light_edits: Vec<(usize, Light)> = Vec::new();
        let mut light_adds: Vec<Light> = Vec::new();
        let mut light_removes: Vec<usize> = Vec::new();

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            // Check for keyboard shortcuts
            ctx.input(|i| {
                if i.modifiers.command && i.key_pressed(egui::Key::S) {
                    pending_save = true;
                }
                if i.modifiers.command && i.key_pressed(egui::Key::L) {
                    pending_load = true;
                }
                if i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z) {
                    pending_undo = true;
                }
                if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z) {
                    pending_redo = true;
                }
            });

            if mode == EditorMode::Editor {
                ui::draw_menu_bar(
                    ctx,
                    &mut pending_scene_load,
                    &mut pending_quit,
                    &mut toggle_mode,
                    &mut pending_save,
                    &mut pending_load,
                    &mut pending_undo,
                    &mut pending_redo,
                    can_undo,
                    can_redo,
                    &undo_desc,
                    &redo_desc,
                );

                // Left panel: scene hierarchy
                ui::draw_hierarchy_panel(ctx, scene_snapshot, &mut selected_node);

                // Right panel: inspector + light editor
                ui::draw_inspector_panel(ctx, scene_snapshot, &selected_node, &mut transform_edits);
                ui::draw_light_editor(
                    ctx,
                    scene_lights,
                    &mut light_edits,
                    &mut light_adds,
                    &mut light_removes,
                );
            }
            ui::draw_viewport_overlay(ctx, fps, mode);
        });

        self.pending_egui_output = Some(full_output);
        self.pending_scene_load = pending_scene_load;
        self.pending_quit = pending_quit;
        self.selected_node = selected_node;
        if toggle_mode {
            self.toggle_mode();
        }

        // Handle gizmo mode keys (T/R/S) — only when egui doesn't want keyboard
        if self.mode == EditorMode::Editor && !self.egui_wants_keyboard {
            let (key_t, key_r, key_g) = self.egui_ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::T),
                    i.key_pressed(egui::Key::R),
                    i.key_pressed(egui::Key::G),
                )
            });
            if key_t {
                self.gizmo_state.mode = gizmo::GizmoMode::Translate;
            }
            if key_r {
                self.gizmo_state.mode = gizmo::GizmoMode::Rotate;
            }
            if key_g {
                self.gizmo_state.mode = gizmo::GizmoMode::Scale;
            }
        }

        // Gizmo interaction + rendering (if a node is selected)
        let mut gizmo_consumed = false;
        let mut gizmo_transform_edit: Option<(String, glm::Mat4)> = None;
        if self.mode == EditorMode::Editor {
            if let Some(ref sel_name) = self.selected_node.clone() {
                if let Some(ref snapshot) = self.scene_snapshot {
                    if let Some(node) = ui::find_node_pub(snapshot, sel_name) {
                        let (vw, vh) = renderer.backend().resolution();
                        let cam = self.orbit_camera.borrow();
                        let view = cam.view_mat();
                        let proj = cam.projection_mat();
                        drop(cam);

                        let gizmo_result = gizmo::draw_and_interact(
                            &self.egui_ctx,
                            &mut self.gizmo_state,
                            node,
                            &view,
                            &proj,
                            vw as f32,
                            vh as f32,
                        );

                        gizmo_consumed = gizmo_result.consumed_pointer;
                        if let Some(new_mat) = gizmo_result.transform_edit {
                            gizmo_transform_edit = Some((sel_name.clone(), new_mat));
                        }
                    }
                }
            }
        }

        // Track gizmo drag start/end for undo recording.
        // Gizmo edits are applied every frame during drag, but recorded as a
        // single undo command spanning drag-start to drag-end.
        let gizmo_is_dragging = self.gizmo_state.active_axis.is_some();
        if gizmo_is_dragging && !self.gizmo_was_dragging {
            // Drag started: save the original transform from gizmo's start_transform
            if let Some(ref st) = self.gizmo_state.start_transform {
                self.gizmo_drag_old_transform = Some(st.to_mat4());
            }
        }
        if !gizmo_is_dragging && self.gizmo_was_dragging {
            // Drag ended: push a single undo command (old -> final)
            if let Some(old_mat) = self.gizmo_drag_old_transform.take() {
                if let Some(ref sel_name) = self.selected_node {
                    if let Some(ref snapshot) = self.scene_snapshot {
                        if let Some(node) = ui::find_node_pub(snapshot, sel_name) {
                            // The snapshot was captured at the start of this frame,
                            // reflecting the last gizmo edit from the previous frame
                            // (the final dragged position).
                            self.undo_stack.push(undo::Command::SetNodeTransform {
                                node_name: sel_name.clone(),
                                old_transform: old_mat,
                                new_transform: node.local_transform,
                            });
                        }
                    }
                }
            }
        }
        self.gizmo_was_dragging = gizmo_is_dragging;

        // Viewport picking: left-click in the 3D viewport selects a node
        // (only if gizmo didn't consume the click)
        if self.mode == EditorMode::Editor && !gizmo_consumed {
            let clicked_primary = self
                .egui_ctx
                .input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
            if clicked_primary && !self.egui_wants_pointer {
                if let Some(pos) = self.egui_ctx.input(|i| i.pointer.interact_pos()) {
                    let (vw, vh) = renderer.backend().resolution();
                    let cam = self.orbit_camera.borrow();
                    let view = cam.view_mat();
                    let proj = cam.projection_mat();
                    let ray =
                        picking::screen_to_ray(pos.x, pos.y, vw as f32, vh as f32, &view, &proj);
                    drop(cam);

                    if let Some(ref snapshot) = self.scene_snapshot {
                        if let Some(hit) = picking::pick_node(&ray, snapshot) {
                            self.selected_node = Some(hit.node_name);
                        } else {
                            // Clicked empty space — deselect
                            self.selected_node = None;
                        }
                    }
                }
            }
        }

        // Apply inspector transform edits (with undo recording via merge)
        for (name, new_mat) in &transform_edits {
            if let Some(ref snapshot) = self.scene_snapshot {
                if let Some(node) = ui::find_node_pub(snapshot, name) {
                    self.undo_stack
                        .push_or_merge(undo::Command::SetNodeTransform {
                            node_name: name.clone(),
                            old_transform: node.local_transform,
                            new_transform: *new_mat,
                        });
                }
            }
            renderer.set_node_transform(name, *new_mat);
        }

        // Apply gizmo transform edit (undo recorded separately on drag end)
        if let Some((name, new_mat)) = gizmo_transform_edit {
            renderer.set_node_transform(&name, new_mat);
        }

        // Apply light edits (with undo recording via merge)
        for (idx, new_light) in &light_edits {
            if *idx < self.scene_lights.len() {
                self.undo_stack.push_or_merge(undo::Command::UpdateLight {
                    index: *idx,
                    old_light: self.scene_lights[*idx].clone(),
                    new_light: new_light.clone(),
                });
            }
            renderer.update_light(*idx, new_light.clone());
        }

        // Remove lights in reverse order so indices stay valid
        light_removes.sort_unstable();
        for idx in light_removes.into_iter().rev() {
            if idx < self.scene_lights.len() {
                self.undo_stack.push(undo::Command::RemoveLight {
                    light: self.scene_lights[idx].clone(),
                    index: idx,
                });
            }
            renderer.remove_light(idx);
        }

        // Add lights
        for light in &light_adds {
            let index = renderer.lights().len();
            renderer.add_light(light.clone());
            self.undo_stack.push(undo::Command::AddLight {
                light: light.clone(),
                index,
            });
        }

        // Handle undo/redo
        if pending_undo {
            self.undo_stack.undo(renderer);
        }
        if pending_redo {
            self.undo_stack.redo(renderer);
        }

        // Handle save/load
        if pending_save {
            self.save_scene(renderer);
        }
        if pending_load {
            self.load_scene_data(renderer);
        }
    }

    /// Save the current scene state to a .ron file next to the glTF file.
    fn save_scene(&self, renderer: &Renderer<WgpuBackend>) {
        if let Some(data) = renderer.extract_scene_data() {
            let save_path = scene_save_path(renderer.scene_file());
            match data.save_to_file(&save_path) {
                Ok(()) => println!("Scene saved to: {}", save_path),
                Err(e) => eprintln!("Failed to save scene: {}", e),
            }
        } else {
            eprintln!("No scene loaded — nothing to save.");
        }
    }

    /// Load a scene overlay from a .ron file and apply it.
    fn load_scene_data(&mut self, renderer: &mut Renderer<WgpuBackend>) {
        use crate::engine::scene_data::SceneData;

        let load_path = scene_save_path(renderer.scene_file());
        match SceneData::load_from_file(&load_path) {
            Ok(data) => {
                renderer.apply_scene_data(&data);
                println!("Scene loaded from: {}", load_path);
            }
            Err(e) => {
                eprintln!("Failed to load scene: {}", e);
            }
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
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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

/// Derive the .ron save path from the glTF scene file path.
///
/// Example: "assets/glTF/Sponza.gltf" -> "assets/glTF/Sponza.scene.ron"
fn scene_save_path(scene_file: Option<&str>) -> String {
    match scene_file {
        Some(path) => {
            let base = path
                .strip_suffix(".gltf")
                .or_else(|| path.strip_suffix(".glb"))
                .unwrap_or(path);
            format!("{}.scene.ron", base)
        }
        None => "scene.ron".to_string(),
    }
}
