//! Lightweight, GPU-type-free snapshot of the scenegraph for editor UI.
//!
//! `NodeInfo` mirrors the scenegraph tree structure but contains only the
//! data the editor needs to display (name, transforms, child count, etc.).
//! This avoids threading `Rc<RefCell<Node<B>>>` through egui closures.

use crate::engine::backend::GpuBackend;
use crate::engine::geometry::AABB;
use crate::engine::scenegraph::node::Node;
use std::cell::RefCell;
use std::rc::Rc;

/// Lightweight snapshot of a single scenegraph node.
#[derive(Clone, Debug)]
pub struct NodeInfo {
    /// Display name (falls back to "<unnamed>" if None).
    pub name: String,
    /// The node's local transform (model_orig).
    pub local_transform: glm::Mat4,
    /// The node's computed world-space transform (model).
    pub world_transform: glm::Mat4,
    /// World-space axis-aligned bounding box (union of all drawable AABBs).
    pub world_aabb: AABB,
    /// Number of drawables attached to this node.
    pub num_drawables: usize,
    /// Number of direct children.
    pub num_children: usize,
    /// Recursive child snapshots.
    pub children: Vec<NodeInfo>,
}

impl NodeInfo {
    /// Recursively extract a `NodeInfo` tree from a scenegraph `Node`.
    pub fn from_node<B: GpuBackend>(node: &Rc<RefCell<Node<B>>>) -> NodeInfo {
        let n = node.borrow();
        let name = n.name.clone().unwrap_or_else(|| "<unnamed>".to_string());

        let children_nodes = n.children_list();
        let children: Vec<NodeInfo> = children_nodes
            .iter()
            .map(|child| NodeInfo::from_node(child))
            .collect();

        NodeInfo {
            name,
            local_transform: n.local_transform(),
            world_transform: n.world_transform(),
            world_aabb: n.world_aabb(),
            num_drawables: n.num_drawables(),
            num_children: children.len(),
            children,
        }
    }
}
