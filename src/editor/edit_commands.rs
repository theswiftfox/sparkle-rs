//! Edit commands produced by the UI on the main thread.
//!
//! These commands are sent to the render thread and applied to the scene there.
//! This decouples UI interaction from scene mutation while keeping the scene
//! on the render thread (avoiding complex cross-thread sharing of Rc/RefCell).

use crate::engine::geometry::Light;

/// Commands that modify the scene, produced by Editor UI on main thread.
#[derive(Debug, Clone)]
pub enum EditCommand {
    /// Set a node's local transform
    SetNodeTransform {
        node_name: String,
        new_transform: glm::Mat4,
    },
    /// Update a light at specific index
    UpdateLight { index: usize, new_light: Light },
    /// Add a new light
    AddLight { light: Light },
    /// Remove light at index
    RemoveLight { index: usize },
    /// Undo last operation
    Undo,
    /// Redo last undone operation
    Redo,
}

/// Batch of edit commands produced by one UI frame.
pub type EditCommands = Vec<EditCommand>;
