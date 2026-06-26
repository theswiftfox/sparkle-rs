//! Editor UI panels drawn with egui.
//!
//! Viewport-dominant design: the 3D viewport fills the entire window.
//! UI elements are minimal overlays:
//! - Top-left: hamburger menu button (File / Edit / View)
//! - Bottom-left: FPS counter + frame time
//! - Floating windows: Hierarchy, Inspector, Lights (togglable via View menu
//!   or keyboard shortcuts H / I / L)

use super::transform::DecomposedTransform;
use crate::engine::geometry::{Light, LightType};
use crate::engine::scene_info::NodeInfo;

/// Draw a compact hamburger menu button in the top-left corner.
///
/// Clicking the button opens a popup containing File, Edit, and View
/// sections.  Panel visibility is controlled via the View section.
pub fn draw_hamburger_menu(
    ctx: &egui::Context,
    pending_scene_load: &mut Option<String>,
    pending_quit: &mut bool,
    toggle_mode: &mut bool,
    pending_save: &mut bool,
    pending_load: &mut bool,
    pending_undo: &mut bool,
    pending_redo: &mut bool,
    can_undo: bool,
    can_redo: bool,
    undo_desc: &Option<String>,
    redo_desc: &Option<String>,
    show_hierarchy: &mut bool,
    show_inspector: &mut bool,
    show_lights: &mut bool,
    rt_supported: bool,
    use_ray_tracing: &mut bool,
) {
    egui::Area::new(egui::Id::new("hamburger_area"))
        .fixed_pos(egui::pos2(8.0, 8.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(180))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::same(2))
                .show(ui, |ui| {
                    ui.menu_button(
                        egui::RichText::new("\u{2261}")
                            .size(20.0)
                            .color(egui::Color32::WHITE),
                        |ui| {
                            // File
                            ui.label(egui::RichText::new("File").strong());
                            if ui.button("  Open glTF Scene...").clicked() {
                                *pending_scene_load = Some("assets/glTF/Sponza.gltf".to_string());
                                ui.close_kind(egui::UiKind::Menu);
                            }
                            if ui.button("  Save Scene  (Ctrl+S)").clicked() {
                                *pending_save = true;
                                ui.close_kind(egui::UiKind::Menu);
                            }
                            if ui.button("  Load Scene  (Ctrl+L)").clicked() {
                                *pending_load = true;
                                ui.close_kind(egui::UiKind::Menu);
                            }
                            ui.separator();

                            //  Edit
                            ui.label(egui::RichText::new("Edit").strong());
                            let undo_label = match undo_desc {
                                Some(d) => format!("  Undo: {}  (Ctrl+Z)", d),
                                None => "  Undo  (Ctrl+Z)".to_string(),
                            };
                            if ui
                                .add_enabled(can_undo, egui::Button::new(undo_label))
                                .clicked()
                            {
                                *pending_undo = true;
                                ui.close_kind(egui::UiKind::Menu);
                            }
                            let redo_label = match redo_desc {
                                Some(d) => format!("  Redo: {}  (Ctrl+Shift+Z)", d),
                                None => "  Redo  (Ctrl+Shift+Z)".to_string(),
                            };
                            if ui
                                .add_enabled(can_redo, egui::Button::new(redo_label))
                                .clicked()
                            {
                                *pending_redo = true;
                                ui.close_kind(egui::UiKind::Menu);
                            }
                            ui.separator();

                            //  View
                            ui.label(egui::RichText::new("View").strong());
                            ui.checkbox(show_hierarchy, "  Hierarchy  (H)");
                            ui.checkbox(show_inspector, "  Inspector  (I)");
                            ui.checkbox(show_lights, "  Lights  (J)");
                            ui.separator();

                            //  Render
                            ui.label(egui::RichText::new("Render").strong());
                            ui.add_enabled(
                                rt_supported,
                                egui::Checkbox::new(use_ray_tracing, "  Ray Tracing"),
                            );
                            ui.separator();

                            if ui.button("  Toggle Play Mode  (F1)").clicked() {
                                *toggle_mode = true;
                                ui.close_kind(egui::UiKind::Menu);
                            }
                            if ui
                                .button(
                                    egui::RichText::new("  Quit").color(egui::Color32::LIGHT_RED),
                                )
                                .clicked()
                            {
                                *pending_quit = true;
                                ui.close_kind(egui::UiKind::Menu);
                            }
                            ui.separator();

                            //  Shortcuts
                            ui.label(egui::RichText::new("Shortcuts").strong());
                            let grey = egui::Color32::from_white_alpha(140);
                            for line in [
                                "T / R / G      Translate / Rotate / Scale",
                                "H / I / J       Hierarchy / Inspector / Lights",
                                "F1                Toggle Play Mode",
                                "RMB drag    Orbit camera",
                                "MMB drag    Pan camera",
                                "Scroll            Zoom",
                            ] {
                                ui.label(egui::RichText::new(line).size(11.0).color(grey));
                            }
                        },
                    );
                });
        });
}

/// Draw the viewport overlay: FPS counter and frame time at the bottom-left.
pub fn draw_viewport_overlay(ctx: &egui::Context, fps: f32, frame_time_ms: f32) {
    let screen = ctx.content_rect();

    egui::Area::new(egui::Id::new("viewport_overlay"))
        .fixed_pos(egui::pos2(10.0, screen.max.y - 32.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(160))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:.0} FPS  {:.2} ms", fps, frame_time_ms))
                                .color(egui::Color32::WHITE)
                                .size(13.0),
                        );
                    });
                });
        });
}

/// Scene hierarchy as a floating window.
///
/// The `open` flag is toggled by the X button on the window title bar
/// and by keyboard shortcut / View menu.
pub fn draw_hierarchy_window(
    ctx: &egui::Context,
    open: &mut bool,
    scene_snapshot: &Option<NodeInfo>,
    selected_node: &mut Option<String>,
) {
    egui::Window::new("Hierarchy")
        .open(open)
        .default_pos(egui::pos2(10.0, 50.0))
        .default_width(220.0)
        .default_height(400.0)
        .resizable(true)
        .show(ctx, |ui| {
            if let Some(root) = scene_snapshot {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    draw_node_tree(ui, root, selected_node, 0);
                });
            } else {
                ui.label("No scene loaded.");
            }
        });
}

/// Recursively draw a node and its children as a collapsible tree.
fn draw_node_tree(
    ui: &mut egui::Ui,
    node: &NodeInfo,
    selected_node: &mut Option<String>,
    depth: usize,
) {
    let is_selected = selected_node.as_deref() == Some(&node.name);
    let has_children = !node.children.is_empty();

    let label_text = if node.num_drawables > 0 {
        format!("{} [{}]", node.name, node.num_drawables)
    } else {
        node.name.clone()
    };

    if has_children {
        let id = egui::Id::new(format!("node_{}_{}", node.name, depth));
        let header =
            egui::CollapsingHeader::new(egui::RichText::new(&label_text).color(if is_selected {
                egui::Color32::LIGHT_BLUE
            } else {
                egui::Color32::WHITE
            }))
            .id_salt(id)
            .default_open(depth < 1);

        let response = header.show(ui, |ui| {
            for child in &node.children {
                draw_node_tree(ui, child, selected_node, depth + 1);
            }
        });

        if response.header_response.clicked() {
            *selected_node = Some(node.name.clone());
        }
    } else {
        let response = ui.selectable_label(is_selected, &label_text);
        if response.clicked() {
            *selected_node = Some(node.name.clone());
        }
    }
}

/// Node inspector as a floating window.
///
/// Shows the selected node's transform decomposed into Position, Rotation
/// (Euler degrees), and Scale with editable drag-value fields.
pub fn draw_inspector_window(
    ctx: &egui::Context,
    open: &mut bool,
    scene_snapshot: &Option<NodeInfo>,
    selected_node: &Option<String>,
    transform_edits: &mut Vec<(String, glm::Mat4)>,
) {
    egui::Window::new("Inspector")
        .open(open)
        .default_pos(egui::pos2(240.0, 50.0))
        .default_width(280.0)
        .resizable(true)
        .show(ctx, |ui| {
            if let Some(sel_name) = selected_node {
                if let Some(root) = scene_snapshot {
                    if let Some(node) = find_node(root, sel_name) {
                        ui.label(egui::RichText::new(&node.name).strong().size(14.0));
                        ui.label(format!(
                            "Drawables: {} | Children: {}",
                            node.num_drawables, node.num_children
                        ));
                        ui.separator();

                        let mut decomposed = DecomposedTransform::from_mat4(&node.local_transform);

                        let mut changed = false;
                        changed |= draw_vec3_editor(ui, "Position", &mut decomposed.position, 0.01);
                        changed |= draw_vec3_editor(ui, "Rotation", &mut decomposed.rotation, 0.5);
                        changed |= draw_vec3_editor(ui, "Scale", &mut decomposed.scale, 0.01);

                        if changed {
                            let new_mat = decomposed.to_mat4();
                            transform_edits.push((node.name.clone(), new_mat));
                        }
                    } else {
                        ui.label("Selected node not found in scene.");
                    }
                } else {
                    ui.label("No scene loaded.");
                }
            } else {
                ui.label("No node selected.");
                ui.label("Click a node in the viewport or hierarchy.");
            }
        });
}

/// Draw a labeled 3-component editor (X/Y/Z) with drag values.
/// Returns true if any value changed.
fn draw_vec3_editor(ui: &mut egui::Ui, label: &str, values: &mut [f32; 3], speed: f32) -> bool {
    let mut changed = false;

    ui.label(egui::RichText::new(label).strong());
    ui.horizontal(|ui| {
        ui.label("X");
        if ui
            .add(
                egui::DragValue::new(&mut values[0])
                    .speed(speed)
                    .max_decimals(3),
            )
            .changed()
        {
            changed = true;
        }
        ui.label("Y");
        if ui
            .add(
                egui::DragValue::new(&mut values[1])
                    .speed(speed)
                    .max_decimals(3),
            )
            .changed()
        {
            changed = true;
        }
        ui.label("Z");
        if ui
            .add(
                egui::DragValue::new(&mut values[2])
                    .speed(speed)
                    .max_decimals(3),
            )
            .changed()
        {
            changed = true;
        }
    });
    ui.add_space(4.0);

    changed
}

/// Light editor as a floating window.
///
/// Lists all lights with editable type, color, direction/position, and radius.
/// Supports adding and removing lights.
pub fn draw_light_window(
    ctx: &egui::Context,
    open: &mut bool,
    lights: &[Light],
    selected_light: &mut Option<usize>,
    light_edits: &mut Vec<(usize, Light)>,
    light_adds: &mut Vec<Light>,
    light_removes: &mut Vec<usize>,
) {
    egui::Window::new("Lights")
        .open(open)
        .default_pos(egui::pos2(10.0, 460.0))
        .default_width(320.0)
        .default_height(180.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("+ Add Light").clicked() {
                    light_adds.push(Light {
                        position: glm::vec3(0.0, -1.0, 0.0),
                        t: LightType::Directional,
                        color: glm::vec3(1.0, 1.0, 1.0),
                        radius: 1.0,
                        penumbra_radius: 0.0,
                        light_proj: glm::identity(),
                    });
                }
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, light) in lights.iter().enumerate() {
                    let mut edited_light = light.clone();
                    let mut changed = false;
                    let mut remove = false;

                    let type_label = match light.t {
                        LightType::Ambient => "Ambient",
                        LightType::Directional => "Directional",
                        LightType::Area => "Area",
                    };

                    let is_selected = *selected_light == Some(idx);

                    egui::CollapsingHeader::new(format!("Light {} ({})", idx, type_label))
                        .id_salt(format!("light_{}", idx))
                        .default_open(is_selected)
                        .show(ui, |ui| {
                            // Selection UI at the top
                            ui.horizontal(|ui| {
                                if is_selected {
                                    ui.label(egui::RichText::new("● Selected").color(egui::Color32::YELLOW));
                                } else if ui.button("Select").clicked() {
                                    *selected_light = Some(idx);
                                }
                            });
                            ui.separator();

                            // Light type selector
                            ui.horizontal(|ui| {
                                ui.label("Type:");
                                let mut type_idx = match edited_light.t {
                                    LightType::Ambient => 0,
                                    LightType::Directional => 1,
                                    LightType::Area => 2,
                                };
                                if egui::ComboBox::from_id_salt(format!("light_type_{}", idx))
                                    .width(100.0)
                                    .show_index(ui, &mut type_idx, 3, |i| {
                                        ["Ambient", "Directional", "Area"][i].to_string()
                                    })
                                    .changed()
                                {
                                    edited_light.t = match type_idx {
                                        0 => LightType::Ambient,
                                        1 => LightType::Directional,
                                        _ => LightType::Area,
                                    };
                                    changed = true;
                                }
                            });

                            // Color
                            ui.horizontal(|ui| {
                                ui.label("Color:");
                                let mut color = [
                                    edited_light.color.x,
                                    edited_light.color.y,
                                    edited_light.color.z,
                                ];
                                for (i, label) in ["R", "G", "B"].iter().enumerate() {
                                    ui.label(*label);
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut color[i])
                                                .speed(0.1)
                                                .max_decimals(2)
                                                .range(0.0..=100.0),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                                edited_light.color = glm::vec3(color[0], color[1], color[2]);
                            });

                            // Position / Direction
                            if edited_light.t != LightType::Ambient {
                                let pos_label = if edited_light.t == LightType::Directional {
                                    "Direction:"
                                } else {
                                    "Position:"
                                };
                                ui.horizontal(|ui| {
                                    ui.label(pos_label);
                                    let mut pos = [
                                        edited_light.position.x,
                                        edited_light.position.y,
                                        edited_light.position.z,
                                    ];
                                    for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
                                        ui.label(*label);
                                        if ui
                                            .add(
                                                egui::DragValue::new(&mut pos[i])
                                                    .speed(0.01)
                                                    .max_decimals(3),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                    edited_light.position = glm::vec3(pos[0], pos[1], pos[2]);
                                });
                            }

                            // Radius
                            if edited_light.t == LightType::Area {
                                ui.horizontal(|ui| {
                                    ui.label("Radius:");
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut edited_light.radius)
                                                .speed(0.1)
                                                .max_decimals(2)
                                                .range(0.0..=1000.0),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });
                            }

                            ui.horizontal(|ui| {
                                if ui
                                    .button(
                                        egui::RichText::new("Remove")
                                            .color(egui::Color32::LIGHT_RED),
                                    )
                                    .clicked()
                                {
                                    remove = true;
                                }
                            });
                        });

                    if remove {
                        light_removes.push(idx);
                    } else if changed {
                        light_edits.push((idx, edited_light));
                    }
                }
            });
        });
}

/// Recursively find a node by name in the snapshot tree (public version).
pub fn find_node_pub<'a>(node: &'a NodeInfo, name: &str) -> Option<&'a NodeInfo> {
    find_node(node, name)
}

/// Recursively find a node by name in the snapshot tree.
fn find_node<'a>(node: &'a NodeInfo, name: &str) -> Option<&'a NodeInfo> {
    if node.name == name {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_node(child, name) {
            return Some(found);
        }
    }
    None
}
