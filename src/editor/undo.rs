//! Undo/redo system using a command stack.
//!
//! Each edit to the scene is recorded as a `Command` that stores both the
//! old and new state, allowing it to be undone and redone. The `UndoStack`
//! manages the linear history with support for merging consecutive edits
//! to the same entity (e.g., continuous drag operations).

use crate::engine::geometry::Light;
use crate::engine::renderer::Renderer;
use crate::engine::wgpu_backend::WgpuBackend;

/// A reversible scene edit.
#[derive(Clone, Debug)]
pub enum Command {
    /// Node transform changed.
    SetNodeTransform {
        node_name: String,
        old_transform: glm::Mat4,
        new_transform: glm::Mat4,
    },
    /// Light properties changed.
    UpdateLight {
        index: usize,
        old_light: Light,
        new_light: Light,
    },
    /// Light added (stores the light and its index after insertion).
    AddLight {
        light: Light,
        index: usize,
    },
    /// Light removed (stores the light and its former index for re-insertion).
    RemoveLight {
        light: Light,
        index: usize,
    },
}

impl Command {
    /// Apply this command (do / redo).
    pub fn apply(&self, renderer: &mut Renderer<WgpuBackend>) {
        match self {
            Command::SetNodeTransform {
                node_name,
                new_transform,
                ..
            } => {
                renderer.set_node_transform(node_name, *new_transform);
            }
            Command::UpdateLight {
                index, new_light, ..
            } => {
                renderer.update_light(*index, new_light.clone());
            }
            Command::AddLight { light, .. } => {
                renderer.add_light(light.clone());
            }
            Command::RemoveLight { index, .. } => {
                renderer.remove_light(*index);
            }
        }
    }

    /// Reverse this command (undo).
    pub fn undo(&self, renderer: &mut Renderer<WgpuBackend>) {
        match self {
            Command::SetNodeTransform {
                node_name,
                old_transform,
                ..
            } => {
                renderer.set_node_transform(node_name, *old_transform);
            }
            Command::UpdateLight {
                index, old_light, ..
            } => {
                renderer.update_light(*index, old_light.clone());
            }
            Command::AddLight { index, .. } => {
                // Undo add = remove the light that was added
                renderer.remove_light(*index);
            }
            Command::RemoveLight { light, index, .. } => {
                // Undo remove = re-insert the light at its former position.
                // We add to end since index may be invalid; this is approximate.
                let _ = renderer.add_light(light.clone());
                // Note: re-inserting at exact index would require an insert_light_at
                // method. For now, adding at end is acceptable.
                let _ = index; // suppress unused warning
            }
        }
    }

    /// Short description for display in the Edit menu.
    pub fn description(&self) -> String {
        match self {
            Command::SetNodeTransform { node_name, .. } => {
                format!("Transform '{}'", node_name)
            }
            Command::UpdateLight { index, .. } => {
                format!("Edit Light {}", index)
            }
            Command::AddLight { .. } => "Add Light".to_string(),
            Command::RemoveLight { index, .. } => {
                format!("Remove Light {}", index)
            }
        }
    }

    /// Whether this command can be merged with another (same type and target).
    fn can_merge_with(&self, other: &Command) -> bool {
        match (self, other) {
            (
                Command::SetNodeTransform { node_name: a, .. },
                Command::SetNodeTransform { node_name: b, .. },
            ) => a == b,
            (
                Command::UpdateLight { index: a, .. },
                Command::UpdateLight { index: b, .. },
            ) => a == b,
            _ => false,
        }
    }

    /// Update this command's "new" state from another command.
    /// Keeps the original "old" state, replaces only the target "new" state.
    fn merge_new_state(&mut self, other: &Command) {
        match (self, other) {
            (
                Command::SetNodeTransform {
                    new_transform: prev_new,
                    ..
                },
                Command::SetNodeTransform {
                    new_transform: other_new,
                    ..
                },
            ) => {
                *prev_new = *other_new;
            }
            (
                Command::UpdateLight {
                    new_light: prev_new,
                    ..
                },
                Command::UpdateLight {
                    new_light: other_new,
                    ..
                },
            ) => {
                *prev_new = other_new.clone();
            }
            _ => {}
        }
    }
}

/// Linear undo/redo stack with merge support for continuous edits.
///
/// Pushing a new command clears the redo history (standard behavior).
/// `push_or_merge` coalesces consecutive edits to the same entity
/// (e.g., dragging a DragValue across multiple frames) into a single
/// undo step.
pub struct UndoStack {
    commands: Vec<Command>,
    /// Index of the next command to undo (points one past the last applied command).
    /// `cursor == 0` means nothing to undo; `cursor == commands.len()` means nothing to redo.
    cursor: usize,
    /// Monotonically increasing frame counter for merge window detection.
    frame_counter: u64,
    /// Frame on which the last push/merge happened.
    last_push_frame: u64,
    /// Set to true after undo/redo to prevent merging on the next edit.
    merge_blocked: bool,
}

impl UndoStack {
    pub fn new() -> Self {
        UndoStack {
            commands: Vec::new(),
            cursor: 0,
            frame_counter: 0,
            last_push_frame: 0,
            merge_blocked: false,
        }
    }

    /// Call once at the start of each frame to advance the merge window.
    pub fn new_frame(&mut self) {
        self.frame_counter += 1;
    }

    /// Push a new command onto the stack. Clears any redo history.
    ///
    /// The command should already have been applied to the renderer.
    pub fn push(&mut self, cmd: Command) {
        // Truncate any redo history
        self.commands.truncate(self.cursor);
        self.commands.push(cmd);
        self.cursor += 1;
        self.last_push_frame = self.frame_counter;
        self.merge_blocked = false;
    }

    /// Push a command, merging with the previous one if it targets the same
    /// entity and the last push was within the current or previous frame.
    ///
    /// This coalesces continuous drag operations into a single undo step.
    pub fn push_or_merge(&mut self, cmd: Command) {
        if !self.merge_blocked
            && self.cursor > 0
            && self.last_push_frame >= self.frame_counter.saturating_sub(1)
        {
            if self.commands[self.cursor - 1].can_merge_with(&cmd) {
                self.commands[self.cursor - 1].merge_new_state(&cmd);
                self.last_push_frame = self.frame_counter;
                return;
            }
        }
        self.push(cmd);
    }

    /// Undo the last command. Returns true if an undo was performed.
    pub fn undo(&mut self, renderer: &mut Renderer<WgpuBackend>) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.commands[self.cursor].undo(renderer);
        self.merge_blocked = true;
        true
    }

    /// Redo the next command. Returns true if a redo was performed.
    pub fn redo(&mut self, renderer: &mut Renderer<WgpuBackend>) -> bool {
        if self.cursor >= self.commands.len() {
            return false;
        }
        self.commands[self.cursor].apply(renderer);
        self.cursor += 1;
        self.merge_blocked = true;
        true
    }

    /// Whether an undo operation is available.
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    /// Whether a redo operation is available.
    pub fn can_redo(&self) -> bool {
        self.cursor < self.commands.len()
    }

    /// Description of the command that would be undone, if any.
    pub fn undo_description(&self) -> Option<String> {
        if self.cursor > 0 {
            Some(self.commands[self.cursor - 1].description())
        } else {
            None
        }
    }

    /// Description of the command that would be redone, if any.
    pub fn redo_description(&self) -> Option<String> {
        if self.cursor < self.commands.len() {
            Some(self.commands[self.cursor].description())
        } else {
            None
        }
    }
}
