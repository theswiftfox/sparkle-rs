//! Editor UI panels drawn with egui.
//!
//! This module contains the actual panel/menu layout code, kept separate
//! from the Editor struct's lifecycle management in `mod.rs`.
//!
//! Layout:
//! - Top: menu bar (File, View)
//! - Left panel: scene hierarchy tree
//! - Right panel: node inspector (P/R/S) + light editor (below)
//! - Center: 3D viewport (passthrough, no egui content)
//! - Overlay: FPS counter + mode indicator

use super::EditorMode;
use super::transform::DecomposedTransform;
use crate::engine::geometry::{Light, LightType};
use crate::engine::scene_info::NodeInfo;

/// Draw the top menu bar (File, Edit, View).
///
/// Outputs are communicated via the mutable references:
/// - `pending_scene_load`: set to Some(path) if the user clicks "Open Scene"
/// - `pending_quit`: set to true if the user clicks Quit
/// - `toggle_mode`: set to true if the user clicks the mode toggle
/// - `pending_undo`/`pending_redo`: set to true if the user clicks Undo/Redo
pub fn draw_menu_bar(
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
) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open glTF Scene...").clicked() {
                    *pending_scene_load = Some("assets/glTF/Sponza.gltf".to_string());
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Save Scene (Ctrl+S)").clicked() {
                    *pending_save = true;
                    ui.close_menu();
                }
                if ui.button("Load Scene (Ctrl+L)").clicked() {
                    *pending_load = true;
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    *pending_quit = true;
                    ui.close_menu();
                }
            });

            ui.menu_button("Edit", |ui| {
                let undo_label = match undo_desc {
                    Some(d) => format!("Undo: {} (Ctrl+Z)", d),
                    None => "Undo (Ctrl+Z)".to_string(),
                };
                if ui
                    .add_enabled(can_undo, egui::Button::new(undo_label))
                    .clicked()
                {
                    *pending_undo = true;
                    ui.close_menu();
                }

                let redo_label = match redo_desc {
                    Some(d) => format!("Redo: {} (Ctrl+Shift+Z)", d),
                    None => "Redo (Ctrl+Shift+Z)".to_string(),
                };
                if ui
                    .add_enabled(can_redo, egui::Button::new(redo_label))
                    .clicked()
                {
                    *pending_redo = true;
                    ui.close_menu();
                }
            });

            ui.menu_button("View", |ui| {
                if ui.button("Toggle Play Mode (F1)").clicked() {
                    *toggle_mode = true;
                    ui.close_menu();
                }
            });
        });
    });
}

/// Draw the viewport overlay (FPS counter, mode indicator).
pub fn draw_viewport_overlay(ctx: &egui::Context, fps: f32, mode: EditorMode) {
    let mode_label = match mode {
        EditorMode::Editor => "EDITOR",
        EditorMode::Play => "PLAY",
    };

    egui::Area::new(egui::Id::new("viewport_overlay"))
        .fixed_pos(egui::pos2(10.0, 40.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(160))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("{:.0} FPS | {} (F1)", fps, mode_label))
                            .color(egui::Color32::WHITE)
                            .size(13.0),
                    );
                });
        });
}

/// Left panel: Scene hierarchy
///
/// Shows a recursive tree of all nodes. Clicking a node selects it.
pub fn draw_hierarchy_panel(
    ctx: &egui::Context,
    scene_snapshot: &Option<NodeInfo>,
    selected_node: &mut Option<String>,
) {
    egui::SidePanel::left("hierarchy_panel")
        .default_width(220.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Scene Hierarchy");
            ui.separator();

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
        // Use CollapsingHeader for nodes with children
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

        // Click on header to select
        if response.header_response.clicked() {
            *selected_node = Some(node.name.clone());
        }
    } else {
        // Leaf node — simple selectable label
        let response = ui.selectable_label(is_selected, &label_text);
        if response.clicked() {
            *selected_node = Some(node.name.clone());
        }
    }
}

/// Right panel: Inspector (top) + Light Editor (bottom)
///
/// Shows the selected node's transform decomposed into Position, Rotation
/// (Euler degrees), and Scale, with editable DragValue fields.
pub fn draw_inspector_panel(
    ctx: &egui::Context,
    scene_snapshot: &Option<NodeInfo>,
    selected_node: &Option<String>,
    transform_edits: &mut Vec<(String, glm::Mat4)>,
) {
    egui::SidePanel::right("inspector_panel")
        .default_width(280.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Inspector");
            ui.separator();

            if let Some(sel_name) = selected_node {
                if let Some(root) = scene_snapshot {
                    if let Some(node) = find_node(root, sel_name) {
                        ui.label(egui::RichText::new(&node.name).strong().size(14.0));
                        ui.label(format!(
                            "Drawables: {} | Children: {}",
                            node.num_drawables, node.num_children
                        ));
                        ui.separator();

                        // Decompose local transform for editing
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
                ui.label("Click a node in the hierarchy.");
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

/// Light Editor (bottom of right panel — drawn as a separate bottom panel)
///
/// Lists all lights with editable type, color, direction/position, and radius.
/// Supports adding and removing lights.
pub fn draw_light_editor(
    ctx: &egui::Context,
    lights: &[Light],
    light_edits: &mut Vec<(usize, Light)>,
    light_adds: &mut Vec<Light>,
    light_removes: &mut Vec<usize>,
) {
    egui::TopBottomPanel::bottom("light_editor")
        .default_height(180.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Lights");
                if ui.button("+ Add Light").clicked() {
                    light_adds.push(Light {
                        position: glm::vec3(0.0, -1.0, 0.0),
                        t: LightType::Directional,
                        color: glm::vec3(1.0, 1.0, 1.0),
                        radius: 1.0,
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

                    egui::CollapsingHeader::new(format!("Light {} ({})", idx, type_label))
                        .id_salt(format!("light_{}", idx))
                        .default_open(false)
                        .show(ui, |ui| {
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
                                // Use DragValues for HDR color (values can exceed 1.0)
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
