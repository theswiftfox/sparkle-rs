//! Editor module for sparkle-rs.
//!
//! Provides an integrated editor mode with egui-based UI for scene inspection.
//! Camera controllers live on the main thread; this module only receives a
//! read-only CameraSnapshot for gizmo/picking operations.
//!
//! The editor owns the egui context and drives the UI each frame on the MAIN THREAD.
//! GPU rendering of the egui overlay is handled by EditorRenderer on the render thread.
//!
//! Architecture:
//!   - Editor (main thread): runs UI, produces FullOutput + EditCommands
//!   - EditorRenderer (render thread): tessellates FullOutput, renders overlay

pub mod edit_commands;
pub mod gizmo;
pub mod picking;
pub mod transform;
pub mod ui;
pub mod undo;

pub use edit_commands::{EditCommand, EditCommands};

use crate::app_handler::CameraCommand;
use crate::engine::geometry::Light;
use crate::engine::scene_info::NodeInfo;
use crate::input::CameraSnapshot;

use std::time::Instant;

/// Whether the application is in editor or play mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    /// Orbit camera, egui UI visible, scene editing.
    Editor,
    /// FPS camera, no UI overlay, game-like controls.
    Play,
}

/// Scene snapshot data passed to the editor each frame.
/// This is a lightweight read-only view of the scene state.
#[derive(Debug, Clone)]
pub struct SceneSnapshot {
    /// Scenegraph tree info
    pub tree: Option<NodeInfo>,
    /// Current lights
    pub lights: Vec<Light>,
}

impl SceneSnapshot {
    pub fn empty() -> Self {
        Self {
            tree: None,
            lights: Vec::new(),
        }
    }
}

/// The main editor state - lives on MAIN THREAD.
///
/// Owns the egui context and UI state. Runs UI each frame and produces
/// FullOutput + EditCommands that are sent to the render thread.
pub struct Editor {
    mode: EditorMode,
    egui_ctx: egui::Context,
    frame_start: Instant,
    frame_count: u32,
    fps: f32,
    fps_update_timer: f32,
    frame_time_ms: f32,

    /// Set when the user requests a scene load via the menu.
    pub pending_scene_load: Option<String>,
    /// Set when the user requests quit via the menu.
    pub pending_quit: bool,
    /// Set when user toggles mode via menu.
    pub pending_mode_toggle: bool,
    /// Camera commands produced by UI (orientation snap, etc.)
    pub pending_camera_commands: Vec<CameraCommand>,

    /// Currently selected node name (None = nothing selected).
    selected_node: Option<String>,

    /// State for the transform gizmo (active axis, drag start transform, etc.).
    gizmo_state: gizmo::GizmoState,
    /// Undo stack for scene edits, supporting merging multiple edits into one undo
    undo_stack: undo::UndoStack,
    /// Whether gizmo was dragging last frame (for undo recording on drag end).
    gizmo_was_dragging: bool,
    /// The node's local transform mat4 when gizmo drag started (for undo).
    gizmo_drag_old_transform: Option<glm::Mat4>,

    /// Panel visibility flags (toggled via View menu or keyboard shortcuts).
    show_hierarchy: bool,
    show_inspector: bool,
    show_lights: bool,

    /// Pending edits collected during UI frame
    pending_edits: EditCommands,
    /// Pending save/load requests
    pending_save: bool,
    pending_load: bool,
}

impl Editor {
    /// Create a new editor with the given egui context.
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            mode: EditorMode::Editor,
            egui_ctx: ctx,
            frame_start: Instant::now(),
            frame_count: 0,
            fps: 0.0,
            fps_update_timer: 0.0,
            frame_time_ms: 0.0,
            pending_scene_load: None,
            pending_quit: false,
            pending_mode_toggle: false,
            pending_camera_commands: Vec::new(),
            selected_node: None,
            gizmo_state: gizmo::GizmoState::new(),
            undo_stack: undo::UndoStack::new(),
            gizmo_was_dragging: false,
            gizmo_drag_old_transform: None,
            show_hierarchy: false,
            show_inspector: false,
            show_lights: false,
            pending_edits: Vec::new(),
            pending_save: false,
            pending_load: false,
        }
    }

    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: EditorMode) {
        self.mode = mode;
    }

    /// Get the selected node name
    pub fn selected_node(&self) -> Option<&String> {
        self.selected_node.as_ref()
    }

    /// Set the selected node
    pub fn set_selected_node(&mut self, name: Option<String>) {
        self.selected_node = name;
    }

    /// Run the UI for one frame.
    ///
    /// This is called on the MAIN THREAD. It processes egui input,
    /// runs all UI panels, handles interactions, and produces:
    /// - FullOutput: egui output to be tessellated and rendered on render thread
    /// - EditCommands: scene modifications to apply on render thread
    ///
    /// # Arguments
    /// * `raw_input` - egui input from egui_winit
    /// * `camera` - Current camera snapshot (for gizmos/picking)
    /// * `mode` - Current editor mode
    /// * `window_size` - Current window dimensions (width, height)
    /// * `delta_t` - Time since last frame in seconds
    /// * `render_frame_time_ms` - Actual render frame time from render thread (for FPS display)
    ///
    /// # Returns
    /// (FullOutput, EditCommands) - to be sent to render thread
    pub fn run_ui(
        &mut self,
        raw_input: egui::RawInput,
        camera: &CameraSnapshot,
        mode: EditorMode,
        _window_size: (u32, u32),
        _delta_t: f32,
        render_frame_time_ms: f32,
    ) -> (egui::FullOutput, EditCommands) {
        self.mode = mode;
        self.pending_edits.clear();
        self.pending_save = false;
        self.pending_load = false;

        // Update FPS counter using render thread timing (not local timing)
        // Use exponential moving average (EMA) with alpha=0.1 for smooth display
        const EMA_ALPHA: f32 = 0.1;
        self.frame_time_ms = if self.frame_time_ms == 0.0 {
            // First frame - use raw value
            render_frame_time_ms
        } else {
            // EMA smoothing: new_value = alpha * current + (1-alpha) * previous
            EMA_ALPHA * render_frame_time_ms + (1.0 - EMA_ALPHA) * self.frame_time_ms
        };

        // Calculate FPS from smoothed frame time
        if self.frame_time_ms > 0.0 {
            self.fps = 1000.0 / self.frame_time_ms;
        }

        // Advance undo merge window
        self.undo_stack.new_frame();

        // Collect state needed inside the egui closure
        let fps = self.fps;
        let frame_time_ms = self.frame_time_ms;
        let mut pending_scene_load = None;
        let mut pending_quit = false;
        let mut toggle_mode = false;
        let mut pending_save = false;
        let mut pending_load = false;
        let mut pending_undo = false;
        let mut pending_redo = false;
        let mut selected_node = self.selected_node.clone();

        // Panel visibility flags
        let mut show_hierarchy = self.show_hierarchy;
        let mut show_inspector = self.show_inspector;
        let mut show_lights = self.show_lights;

        // Undo state for the Edit menu
        let can_undo = self.undo_stack.can_undo();
        let can_redo = self.undo_stack.can_redo();
        let undo_desc = self.undo_stack.undo_description();
        let redo_desc = self.undo_stack.redo_description();

        // Collect edits produced by the UI
        let mut transform_edits: Vec<(String, glm::Mat4)> = Vec::new();
        let mut light_edits: Vec<(usize, Light)> = Vec::new();
        let mut light_adds: Vec<Light> = Vec::new();
        let mut light_removes: Vec<usize> = Vec::new();

        // Extract gizmo state to avoid borrow conflict
        let mut gizmo_state = std::mem::replace(&mut self.gizmo_state, gizmo::GizmoState::new());

        // Camera matrices for gizmos/picking
        let cam_view = camera.view_matrix;
        let cam_proj = camera.projection_matrix;
        let egui_wants_keyboard = self.egui_ctx.egui_wants_keyboard_input();
        let egui_wants_pointer = self.egui_ctx.egui_wants_pointer_input();

        // Gizmo + picking results
        let mut gizmo_consumed = false;
        let mut gizmo_transform_edit: Option<(String, glm::Mat4)> = None;
        let mut orientation_snap: Option<(f32, f32)> = None;

        // We need a placeholder scene snapshot for now
        // In the full implementation, this comes from the render thread
        let scene_snapshot: Option<NodeInfo> = None; // TODO: receive from render thread
        let scene_lights: Vec<Light> = Vec::new(); // TODO: receive from render thread

        let full_output = self.egui_ctx.run_ui(raw_input, |ctx| {
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
                // Hamburger menu (top-left)
                ui::draw_hamburger_menu(
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
                    &mut show_hierarchy,
                    &mut show_inspector,
                    &mut show_lights,
                );

                // Floating panels (only when visible)
                ui::draw_hierarchy_window(
                    ctx,
                    &mut show_hierarchy,
                    &scene_snapshot,
                    &mut selected_node,
                );
                ui::draw_inspector_window(
                    ctx,
                    &mut show_inspector,
                    &scene_snapshot,
                    &selected_node,
                    &mut transform_edits,
                );
                ui::draw_light_window(
                    ctx,
                    &mut show_lights,
                    &scene_lights,
                    &mut light_edits,
                    &mut light_adds,
                    &mut light_removes,
                );

                // Camera orientation gizmo (top-right)
                let orient_result = gizmo::draw_orientation_gizmo(ctx, &cam_view);
                if orient_result.snap_to.is_some() {
                    orientation_snap = orient_result.snap_to;
                }
            }

            // FPS + frame time overlay (bottom-left, always visible)
            ui::draw_viewport_overlay(ctx, fps, frame_time_ms);

            // Gizmo mode keys (T/R/S)
            if mode == EditorMode::Editor && !egui_wants_keyboard {
                let (key_t, key_r, key_g) = ctx.input(|i| {
                    (
                        i.key_pressed(egui::Key::T),
                        i.key_pressed(egui::Key::R),
                        i.key_pressed(egui::Key::G),
                    )
                });
                if key_t {
                    gizmo_state.mode = gizmo::GizmoMode::Translate;
                }
                if key_r {
                    gizmo_state.mode = gizmo::GizmoMode::Rotate;
                }
                if key_g {
                    gizmo_state.mode = gizmo::GizmoMode::Scale;
                }

                // Panel toggle keys (H / I / J)
                let (key_h, key_i, key_j) = ctx.input(|i| {
                    (
                        i.key_pressed(egui::Key::H),
                        i.key_pressed(egui::Key::I),
                        i.key_pressed(egui::Key::J),
                    )
                });
                if key_h {
                    show_hierarchy = !show_hierarchy;
                }
                if key_i {
                    show_inspector = !show_inspector;
                }
                if key_j {
                    show_lights = !show_lights;
                }
            }

            // Gizmo interaction + rendering
            if mode == EditorMode::Editor {
                if let Some(ref sel_name) = selected_node.clone() {
                    if let Some(ref snapshot) = scene_snapshot {
                        if let Some(node) = ui::find_node_pub(snapshot, sel_name) {
                            let screen_rect = ctx.content_rect();
                            let gizmo_result = gizmo::draw_and_interact(
                                ctx,
                                &mut gizmo_state,
                                node,
                                &cam_view,
                                &cam_proj,
                                screen_rect.width(),
                                screen_rect.height(),
                            );

                            gizmo_consumed = gizmo_result.consumed_pointer;
                            if let Some(new_mat) = gizmo_result.transform_edit {
                                gizmo_transform_edit = Some((sel_name.clone(), new_mat));
                            }
                        }
                    }
                }
            }

            // Viewport picking
            if mode == EditorMode::Editor && !gizmo_consumed {
                let clicked_primary =
                    ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
                if clicked_primary && !egui_wants_pointer {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        let screen_rect = ctx.content_rect();
                        let ray = picking::screen_to_ray(
                            pos.x,
                            pos.y,
                            screen_rect.width(),
                            screen_rect.height(),
                            &cam_view,
                            &cam_proj,
                        );

                        if let Some(snapshot) = &scene_snapshot {
                            if let Some(hit) = picking::pick_node(&ray, snapshot) {
                                selected_node = Some(hit.node_name);
                            } else {
                                // Clicked empty space — deselect
                                selected_node = None;
                            }
                        }
                    }
                }
            }
        });

        // Update persistent state
        self.pending_scene_load = pending_scene_load;
        self.pending_quit = pending_quit;
        self.pending_mode_toggle = toggle_mode;
        self.selected_node = selected_node;
        self.show_hierarchy = show_hierarchy;
        self.show_inspector = show_inspector;
        self.show_lights = show_lights;
        self.pending_save = pending_save;
        self.pending_load = pending_load;

        // Send orientation snap as camera command
        if let Some((az, el)) = orientation_snap {
            self.pending_camera_commands
                .push(CameraCommand::OrientationSnap(az, el));
        }

        // Restore gizmo state
        self.gizmo_state = gizmo_state;

        // Track gizmo drag start/end for undo recording
        let gizmo_is_dragging = self.gizmo_state.active_axis.is_some();
        if gizmo_is_dragging && !self.gizmo_was_dragging {
            if let Some(ref st) = self.gizmo_state.start_transform {
                self.gizmo_drag_old_transform = Some(st.to_mat4());
            }
        }
        if !gizmo_is_dragging && self.gizmo_was_dragging {
            if let Some(_old_mat) = self.gizmo_drag_old_transform.take() {
                if let Some(ref _sel_name) = self.selected_node {
                    // TODO: Get current transform from scene snapshot
                    // For now, we'll track this differently
                }
            }
        }
        self.gizmo_was_dragging = gizmo_is_dragging;

        // Build EditCommands from collected edits
        // Transform edits
        for (name, new_mat) in transform_edits {
            self.pending_edits.push(EditCommand::SetNodeTransform {
                node_name: name,
                new_transform: new_mat,
            });
        }

        // Gizmo transform edit (single)
        if let Some((name, new_mat)) = gizmo_transform_edit {
            self.pending_edits.push(EditCommand::SetNodeTransform {
                node_name: name,
                new_transform: new_mat,
            });
        }

        // Light edits
        for (idx, new_light) in light_edits {
            self.pending_edits.push(EditCommand::UpdateLight {
                index: idx,
                new_light,
            });
        }

        // Light removes (in reverse order)
        light_removes.sort_unstable();
        for idx in light_removes.into_iter().rev() {
            self.pending_edits
                .push(EditCommand::RemoveLight { index: idx });
        }

        // Light adds
        for light in light_adds {
            self.pending_edits.push(EditCommand::AddLight { light });
        }

        // Undo/redo
        if pending_undo {
            self.pending_edits.push(EditCommand::Undo);
        }
        if pending_redo {
            self.pending_edits.push(EditCommand::Redo);
        }

        // Extract the edit commands to return
        let edit_commands = std::mem::take(&mut self.pending_edits);

        (full_output, edit_commands)
    }
}

/// Editor renderer - lives on RENDER THREAD.
///
/// Receives FullOutput from the Editor, tessellates it, and renders the overlay.
pub struct EditorRenderer {
    /// Shared egui context (same instance as Editor on main thread)
    egui_ctx: egui::Context,
}

impl EditorRenderer {
    /// Create a new editor renderer with shared egui context.
    ///
    /// The context must be the same instance (or a clone) of the Editor's context
    /// so that texture IDs are valid.
    pub fn new(egui_ctx: egui::Context) -> Self {
        Self { egui_ctx }
    }

    /// Render the egui overlay from FullOutput.
    ///
    /// This is called on the RENDER THREAD after scene rendering.
    /// It tessellates the FullOutput and submits it to the GPU backend.
    ///
    /// # Arguments
    /// * `full_output` - The egui output produced by Editor::run_ui()
    /// * `renderer` - The renderer to submit GPU commands to
    pub fn render_overlay<B: crate::engine::backend::GpuBackend>(
        &self,
        full_output: &egui::FullOutput,
        renderer: &mut crate::engine::renderer::Renderer<B>,
    ) {
        let clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes.clone(), full_output.pixels_per_point);

        renderer.backend_mut().render_egui(
            &full_output.textures_delta,
            &clipped_primitives,
            full_output.pixels_per_point,
        );
    }
}
