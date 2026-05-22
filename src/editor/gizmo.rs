//! Transform gizmos rendered via egui's paint API.
//!
//! Projects 3D gizmo axes onto the screen and draws colored lines/arrows.
//! Supports click+drag interaction for axis-constrained translation,
//! rotation (Euler angle adjustment), and uniform scaling.

use crate::engine::scene_info::NodeInfo;
use super::transform::DecomposedTransform;

/// Which transform operation the gizmo performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

/// Which axis is being manipulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// Persistent gizmo state across frames.
pub struct GizmoState {
    pub mode: GizmoMode,
    /// Which axis is currently being dragged (None = not dragging).
    pub active_axis: Option<Axis>,
    /// Screen position where the drag started.
    pub drag_start: egui::Pos2,
    /// The node's decomposed transform at drag start (for relative edits).
    pub start_transform: Option<DecomposedTransform>,
}

impl GizmoState {
    pub fn new() -> Self {
        GizmoState {
            mode: GizmoMode::Translate,
            active_axis: None,
            drag_start: egui::Pos2::ZERO,
            start_transform: None,
        }
    }
}

/// Axis colors.
const COLOR_X: egui::Color32 = egui::Color32::from_rgb(220, 50, 50);
const COLOR_Y: egui::Color32 = egui::Color32::from_rgb(50, 200, 50);
const COLOR_Z: egui::Color32 = egui::Color32::from_rgb(50, 100, 220);
const COLOR_HOVER: egui::Color32 = egui::Color32::YELLOW;
const GIZMO_LENGTH: f32 = 80.0; // pixels on screen
const HIT_DISTANCE: f32 = 10.0; // pixels proximity for axis hover

/// Project a 3D world-space point to 2D screen coordinates.
///
/// Returns `None` if the point is behind the camera.
fn project_to_screen(
    point: &glm::Vec3,
    view: &glm::Mat4,
    proj: &glm::Mat4,
    screen_w: f32,
    screen_h: f32,
) -> Option<egui::Pos2> {
    let clip = proj * view * glm::vec4(point.x, point.y, point.z, 1.0);
    if clip.w <= 0.0 {
        return None; // behind camera
    }
    let ndc = glm::vec3(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
    let screen_x = (ndc.x * 0.5 + 0.5) * screen_w;
    let screen_y = (1.0 - (ndc.y * 0.5 + 0.5)) * screen_h;
    Some(egui::Pos2::new(screen_x, screen_y))
}

/// Compute the screen-space endpoint of a gizmo axis.
///
/// Tries to place the axis tip `GIZMO_LENGTH` pixels from the origin,
/// regardless of depth. Falls back if projection fails.
fn axis_screen_endpoint(
    origin_world: &glm::Vec3,
    axis_dir: &glm::Vec3,
    origin_screen: &egui::Pos2,
    view: &glm::Mat4,
    proj: &glm::Mat4,
    screen_w: f32,
    screen_h: f32,
) -> egui::Pos2 {
    // Project a point 1 unit along the axis
    let world_tip = origin_world + axis_dir;
    if let Some(tip_screen) = project_to_screen(&world_tip, view, proj, screen_w, screen_h) {
        let delta = tip_screen - *origin_screen;
        let len = delta.length();
        if len > 1.0 {
            // Normalize to GIZMO_LENGTH pixels
            *origin_screen + delta * (GIZMO_LENGTH / len)
        } else {
            *origin_screen + egui::Vec2::new(GIZMO_LENGTH, 0.0)
        }
    } else {
        // Fallback
        *origin_screen + egui::Vec2::new(GIZMO_LENGTH, 0.0)
    }
}

/// Distance from a point to a line segment (in 2D screen space).
fn point_to_segment_dist(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let len_sq = ab.length_sq();
    if len_sq < 1e-6 {
        return ap.length();
    }
    let t = (ap.x * ab.x + ap.y * ab.y) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length()
}

/// Result of gizmo interaction for a single frame.
pub struct GizmoResult {
    /// Whether the gizmo consumed the pointer this frame (prevents picking).
    pub consumed_pointer: bool,
    /// If a transform edit was produced, the new local Mat4.
    pub transform_edit: Option<glm::Mat4>,
}

/// Draw the gizmo and handle interaction.
///
/// Returns a `GizmoResult` indicating whether the gizmo consumed input
/// and whether a transform edit was produced.
pub fn draw_and_interact(
    ctx: &egui::Context,
    state: &mut GizmoState,
    node: &NodeInfo,
    view: &glm::Mat4,
    proj: &glm::Mat4,
    screen_w: f32,
    screen_h: f32,
) -> GizmoResult {
    let mut result = GizmoResult {
        consumed_pointer: false,
        transform_edit: None,
    };

    // Get the node's world-space position (translation of world transform)
    let world_pos = glm::vec3(
        node.world_transform[(0, 3)],
        node.world_transform[(1, 3)],
        node.world_transform[(2, 3)],
    );

    // Project the origin to screen
    let origin_screen = match project_to_screen(&world_pos, view, proj, screen_w, screen_h) {
        Some(p) => p,
        None => return result, // origin behind camera
    };

    // Compute axis endpoints
    let axes = [
        (Axis::X, glm::vec3(1.0, 0.0, 0.0), COLOR_X),
        (Axis::Y, glm::vec3(0.0, 1.0, 0.0), COLOR_Y),
        (Axis::Z, glm::vec3(0.0, 0.0, 1.0), COLOR_Z),
    ];

    let mut axis_endpoints: Vec<(Axis, egui::Pos2, egui::Color32)> = Vec::new();
    for (axis, dir, color) in &axes {
        let tip = axis_screen_endpoint(
            &world_pos, dir, &origin_screen, view, proj, screen_w, screen_h,
        );
        axis_endpoints.push((*axis, tip, *color));
    }

    // Determine which axis the mouse is hovering over
    let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
    let mut hovered_axis: Option<Axis> = None;
    if let Some(pos) = pointer_pos {
        let mut best_dist = HIT_DISTANCE;
        for (axis, tip, _) in &axis_endpoints {
            let d = point_to_segment_dist(pos, origin_screen, *tip);
            if d < best_dist {
                best_dist = d;
                hovered_axis = Some(*axis);
            }
        }
    }

    // Handle drag interaction
    let primary_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
    let primary_pressed = ctx.input(|i| {
        i.pointer.button_pressed(egui::PointerButton::Primary)
    });
    let primary_released = ctx.input(|i| {
        i.pointer.button_released(egui::PointerButton::Primary)
    });

    if primary_pressed && hovered_axis.is_some() && state.active_axis.is_none() {
        // Start drag
        state.active_axis = hovered_axis;
        state.drag_start = pointer_pos.unwrap_or(egui::Pos2::ZERO);
        state.start_transform = Some(DecomposedTransform::from_mat4(&node.local_transform));
        result.consumed_pointer = true;
    }

    if let Some(active) = state.active_axis {
        result.consumed_pointer = true;

        if primary_down {
            if let (Some(pos), Some(start_t)) = (pointer_pos, &state.start_transform) {
                let drag_delta = pos - state.drag_start;

                // Find the screen-space direction of the active axis
                let axis_idx = match active {
                    Axis::X => 0,
                    Axis::Y => 1,
                    Axis::Z => 2,
                };
                let (_, axis_tip, _) = &axis_endpoints[axis_idx];
                let axis_screen_dir = *axis_tip - origin_screen;
                let axis_screen_len = axis_screen_dir.length();

                if axis_screen_len > 1.0 {
                    let axis_norm = axis_screen_dir / axis_screen_len;
                    // Project drag delta onto axis direction
                    let projected = drag_delta.x * axis_norm.x + drag_delta.y * axis_norm.y;

                    let mut new_t = start_t.clone();
                    match state.mode {
                        GizmoMode::Translate => {
                            // Scale: 1 pixel per 0.01 world units (adjusted by gizmo size)
                            let scale_factor = projected / GIZMO_LENGTH;
                            new_t.position[axis_idx] =
                                start_t.position[axis_idx] + scale_factor * 2.0;
                        }
                        GizmoMode::Rotate => {
                            // 1 pixel = 0.5 degrees
                            new_t.rotation[axis_idx] =
                                start_t.rotation[axis_idx] + projected * 0.5;
                        }
                        GizmoMode::Scale => {
                            // Multiplicative: drag right = scale up
                            let factor = 1.0 + projected / GIZMO_LENGTH;
                            let factor = factor.max(0.01);
                            new_t.scale[axis_idx] = start_t.scale[axis_idx] * factor;
                        }
                    }
                    result.transform_edit = Some(new_t.to_mat4());
                }
            }
        }

        if primary_released {
            state.active_axis = None;
            state.start_transform = None;
        }
    }

    // Draw the gizmo lines using egui's painter
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("gizmo_overlay"),
    ));

    let mode_label = match state.mode {
        GizmoMode::Translate => "T",
        GizmoMode::Rotate => "R",
        GizmoMode::Scale => "S",
    };

    for (axis, tip, base_color) in &axis_endpoints {
        let is_active = state.active_axis == Some(*axis);
        let is_hovered = hovered_axis == Some(*axis) && state.active_axis.is_none();
        let color = if is_active || is_hovered {
            COLOR_HOVER
        } else {
            *base_color
        };
        let width = if is_active { 3.0 } else { 2.0 };

        painter.line_segment([origin_screen, *tip], egui::Stroke::new(width, color));

        // Draw arrowhead / mode indicator at the tip
        let dir = (*tip - origin_screen).normalized();
        match state.mode {
            GizmoMode::Translate => {
                // Arrow head
                let perp = egui::Vec2::new(-dir.y, dir.x) * 4.0;
                let back = *tip - dir * 8.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![*tip, back + perp, back - perp],
                    color,
                    egui::Stroke::NONE,
                ));
            }
            GizmoMode::Rotate => {
                // Small circle at tip
                painter.circle_filled(*tip, 4.0, color);
            }
            GizmoMode::Scale => {
                // Small square at tip
                let s = 4.0;
                painter.rect_filled(
                    egui::Rect::from_center_size(*tip, egui::Vec2::splat(s * 2.0)),
                    0.0,
                    color,
                );
            }
        }

        // Axis label
        let label = match axis {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        };
        painter.text(
            *tip + dir * 12.0,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(11.0),
            color,
        );
    }

    // Mode label near origin
    painter.text(
        origin_screen + egui::Vec2::new(0.0, 14.0),
        egui::Align2::CENTER_TOP,
        mode_label,
        egui::FontId::proportional(10.0),
        egui::Color32::WHITE,
    );

    result
}

// Camera orientation gizmo (top-right corner)

/// Result of orientation gizmo interaction.
pub struct OrientationGizmoResult {
    /// If the user clicked an axis, the target (azimuth, elevation) in degrees.
    pub snap_to: Option<(f32, f32)>,
}

/// Draw an interactive camera orientation gizmo in the top-right corner.
///
/// Shows the three world axes (X/Y/Z) projected through the current camera
/// rotation, with colored lines and labeled endpoints.  Clicking an axis
/// endpoint snaps the orbit camera to face along that axis.
pub fn draw_orientation_gizmo(
    ctx: &egui::Context,
    view: &glm::Mat4,
) -> OrientationGizmoResult {
    let mut result = OrientationGizmoResult { snap_to: None };

    let screen_rect = ctx.screen_rect();
    let gizmo_radius = 50.0;
    let axis_len = 40.0;
    let margin = 24.0;
    let center = egui::Pos2::new(
        screen_rect.max.x - gizmo_radius - margin,
        gizmo_radius + margin + 8.0,
    );

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("orientation_gizmo"),
    ));

    // Semi-transparent background circle
    painter.circle_filled(
        center,
        gizmo_radius,
        egui::Color32::from_black_alpha(80),
    );
    painter.circle_stroke(
        center,
        gizmo_radius,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(40)),
    );

    // Axis definitions:  label, world direction, color, snap (azimuth, elevation)
    //
    // OrbitCamera places the eye at:
    //   focus + (dist*cos(el)*cos(az), dist*sin(el), dist*cos(el)*sin(az))
    // and looks toward focus.
    //
    // Snap angles position the camera so it looks along the negative of the
    // corresponding world axis (e.g., clicking +X places the camera on the
    // +X side, looking toward -X).
    let axes_pos: [(&str, [f32; 3], egui::Color32, (f32, f32)); 3] = [
        ("X", [1.0, 0.0, 0.0], COLOR_X, (0.0, 0.0)),
        ("Y", [0.0, 1.0, 0.0], COLOR_Y, (-90.0, 89.9)),
        ("Z", [0.0, 0.0, 1.0], COLOR_Z, (90.0, 0.0)),
    ];
    let axes_neg: [(&str, [f32; 3], egui::Color32, (f32, f32)); 3] = [
        ("", [-1.0, 0.0, 0.0], COLOR_X, (180.0, 0.0)),
        ("", [0.0, -1.0, 0.0], COLOR_Y, (-90.0, -89.9)),
        ("", [0.0, 0.0, -1.0], COLOR_Z, (-90.0, 0.0)),
    ];

    // Project each axis direction through the view rotation (upper-left 3x3).
    // view_dir.x  -> screen right
    // view_dir.y  -> screen up  (negated for screen coords)
    // view_dir.z  -> depth into screen (used for draw ordering)
    struct Endpoint {
        screen: egui::Pos2,
        depth: f32,
        color: egui::Color32,
        label: &'static str,
        snap: (f32, f32),
        is_positive: bool,
    }

    let project = |dir: [f32; 3]| -> (egui::Pos2, f32) {
        let vx = view[(0, 0)] * dir[0] + view[(0, 1)] * dir[1] + view[(0, 2)] * dir[2];
        let vy = view[(1, 0)] * dir[0] + view[(1, 1)] * dir[1] + view[(1, 2)] * dir[2];
        let vz = view[(2, 0)] * dir[0] + view[(2, 1)] * dir[1] + view[(2, 2)] * dir[2];
        let pos = egui::Pos2::new(center.x + vx * axis_len, center.y - vy * axis_len);
        (pos, vz)
    };

    let mut endpoints: Vec<Endpoint> = Vec::with_capacity(6);
    for &(label, dir, color, snap) in &axes_pos {
        let (screen, depth) = project(dir);
        endpoints.push(Endpoint {
            screen,
            depth,
            color,
            label,
            snap,
            is_positive: true,
        });
    }
    for &(label, dir, color, snap) in &axes_neg {
        let (screen, depth) = project(dir);
        endpoints.push(Endpoint {
            screen,
            depth,
            color,
            label,
            snap,
            is_positive: false,
        });
    }

    // Sort back-to-front (most negative depth drawn first).
    endpoints.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap_or(std::cmp::Ordering::Equal));

    // Input state for click detection.
    let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
    let clicked = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));

    for ep in &endpoints {
        // Draw line from center to endpoint
        let alpha = ((ep.depth * 0.5 + 0.5).clamp(0.2, 1.0) * 255.0) as u8;
        let line_color = egui::Color32::from_rgba_unmultiplied(
            ep.color.r(),
            ep.color.g(),
            ep.color.b(),
            alpha,
        );
        painter.line_segment([center, ep.screen], egui::Stroke::new(2.0, line_color));

        if ep.is_positive {
            // Positive endpoint: colored filled circle + label
            let tip_color = egui::Color32::from_rgba_unmultiplied(
                ep.color.r(),
                ep.color.g(),
                ep.color.b(),
                alpha.max(180),
            );
            painter.circle_filled(ep.screen, 11.0, tip_color);
            painter.text(
                ep.screen,
                egui::Align2::CENTER_CENTER,
                ep.label,
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
            );
        } else {
            // Negative endpoint: small dark dot
            painter.circle_filled(
                ep.screen,
                4.0,
                egui::Color32::from_rgba_unmultiplied(100, 100, 100, alpha),
            );
        }

        // Click detection
        let hit_radius = if ep.is_positive { 14.0 } else { 8.0 };
        if let Some(pos) = pointer_pos {
            if clicked && (pos - ep.screen).length() < hit_radius {
                result.snap_to = Some(ep.snap);
            }
        }
    }

    // "<Persp>" label below the gizmo
    painter.text(
        egui::Pos2::new(center.x, center.y + gizmo_radius + 6.0),
        egui::Align2::CENTER_TOP,
        "<Persp>",
        egui::FontId::proportional(10.0),
        egui::Color32::from_white_alpha(160),
    );

    result
}
