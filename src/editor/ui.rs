//! Editor UI panels drawn with egui.
//!
//! This module contains the actual panel/menu layout code, kept separate
//! from the Editor struct's lifecycle management in `mod.rs`.

use super::EditorMode;

/// Draw the top menu bar (File, View).
///
/// Outputs are communicated via the mutable references:
/// - `pending_scene_load`: set to Some(path) if the user clicks "Open Scene"
/// - `pending_quit`: set to true if the user clicks Quit
/// - `toggle_mode`: set to true if the user clicks the mode toggle
pub fn draw_menu_bar(
    ctx: &egui::Context,
    pending_scene_load: &mut Option<String>,
    pending_quit: &mut bool,
    toggle_mode: &mut bool,
) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open glTF Scene...").clicked() {
                    // For now, hardcoded to the Sponza scene.
                    // Phase 3 will add a proper file dialog.
                    *pending_scene_load = Some("assets/glTF/Sponza.gltf".to_string());
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    *pending_quit = true;
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
///
/// This is a small transparent overlay in the top-right corner that is
/// always visible regardless of mode.
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
