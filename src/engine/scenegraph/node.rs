use std::collections::HashMap;

use super::{ErrorCause, SceneGraphError};
use crate::engine::backend::{Drawable, GpuBackend, ObjType};
use crate::engine::geometry::AABB;

pub struct Node<B: GpuBackend> {
    uuid: u64,
    pub name: Option<String>,
    model: glm::Mat4,
    model_orig: glm::Mat4,
    children: HashMap<String, Node<B>>,

    drawables: Vec<Drawable<B>>,
}

// Manual Clone: derive would add unnecessary B: Clone bound.
// All fields are cloneable without B being Clone (Rc handles it).
impl<B: GpuBackend> Clone for Node<B> {
    fn clone(&self) -> Self {
        Node {
            uuid: self.uuid,
            name: self.name.clone(),
            model: self.model,
            model_orig: self.model_orig,
            children: self.children.clone(),
            drawables: self.drawables.clone(),
        }
    }
}

impl<B: GpuBackend> Node<B> {
    pub fn create(
        name: Option<&str>,
        model: glm::Mat4,
        drawable: Option<Vec<Drawable<B>>>,
    ) -> Node<B> {
        let mut n = Node {
            uuid: 0, // TODO
            name: match name {
                Some(n) => Some(n.to_string()),
                None => None,
            },
            model,
            model_orig: model,
            drawables: Vec::new(),
            children: HashMap::new(),
        };
        if let Some(d) = drawable {
            n.drawables = d;
        }
        n
    }

    pub fn destroy(&mut self) {
        self.drawables.clear();
        for (_, mut c) in self.children.drain() {
            c.destroy();
        }
        self.children.clear();
    }

    pub fn get_named(&self, name: &str) -> Result<&Node<B>, SceneGraphError> {
        if self.children.is_empty() {
            return Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        }
        if let Some(c) = self.children.get(name) {
            return Ok(c);
        }
        for (_, node) in &self.children {
            match node.get_named(name) {
                Ok(c) => return Ok(c),
                Err(e) => return Err(e),
            }
        }
        Err(SceneGraphError::new(name, &ErrorCause::NotFound))
    }

    pub fn get_named_mut(&mut self, name: &str) -> Result<&mut Node<B>, SceneGraphError> {
        if self.children.is_empty() {
            return Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        }
        if self.children.contains_key(name) {
            return Ok(self.children.get_mut(name).unwrap());
        }
        for (_, node) in &mut self.children {
            match node.get_named_mut(name) {
                Ok(c) => return Ok(c),
                Err(e) => return Err(e),
            }
        }
        Err(SceneGraphError::new(name, &ErrorCause::NotFound))
    }

    pub fn get_drawables(&self) -> Vec<&Drawable<B>> {
        self.drawables.iter().collect()
    }

    pub fn traverse(&self) -> Vec<&Node<B>> {
        let mut nodes: Vec<&Node<B>> = Vec::new();
        for (_, c) in &self.children {
            nodes.push(c);
            let mut others = c.traverse();
            nodes.append(&mut others);
        }
        nodes
    }

    pub fn add_child(&mut self, node: Node<B>) -> Result<(), SceneGraphError> {
        let key = match &node.name {
            Some(n) => n.clone(),
            None => node.uuid.to_string(),
        };
        if self.children.contains_key(&key) {
            return Err(SceneGraphError::new(
                "Duplicate Name",
                &ErrorCause::InvalidState,
            ));
        }
        self.children.insert(key, node);
        Ok(())
    }

    pub fn remove_node_named(&mut self, name: &str) -> bool {
        if self.children.is_empty() {
            return false;
        }
        if let Some(mut v) = self.children.remove(name) {
            v.destroy();
            return true;
        }
        for (_, c) in &mut self.children {
            if c.remove_node_named(name) {
                return true;
            }
        }
        false
    }

    pub fn remove_node_uuid(&mut self, uuid: u64) -> bool {
        if self.children.is_empty() {
            return false;
        }
        let mut key = String::default();
        for (k, c) in &self.children {
            if c.uuid == uuid {
                key = k.clone();
                break;
            }
        }
        if !key.is_empty() {
            let mut n = self.children.remove(&key).unwrap();
            n.destroy();
            return true;
        }
        for (_, c) in &mut self.children {
            if c.remove_node_uuid(uuid) {
                return true;
            }
        }
        false
    }

    pub fn add_drawable(&mut self, drawable: Drawable<B>) {
        self.drawables.push(drawable)
    }

    pub fn translate(&mut self, t: glm::Vec3) {
        self.model = glm::translate(&self.model, &t);
    }

    pub fn rotate(&mut self, r: glm::Quat) {
        let rot = glm::quat_to_mat4(&r);
        self.model = rot * self.model;
    }

    pub fn scale(&mut self, s: f32) {
        self.model = glm::scale(&self.model, &glm::vec3(s, s, s));
    }

    /// Returns the local transform matrix (model_orig).
    pub fn local_transform(&self) -> glm::Mat4 {
        self.model_orig
    }

    /// Returns the computed world-space transform matrix.
    pub fn world_transform(&self) -> glm::Mat4 {
        self.model
    }

    /// Set the local transform matrix (model_orig).
    ///
    /// Call `build_model()` afterwards to propagate to children.
    pub fn set_local_transform(&mut self, mat: glm::Mat4) {
        self.model_orig = mat;
    }

    /// Returns a list of direct children as `Rc<RefCell<Node<B>>>`.
    ///
    /// Order is not guaranteed (HashMap iteration order).
    pub fn children_list(&self) -> Vec<&Node<B>> {
        self.children.values().collect()
    }

    /// Returns the number of drawables on this node.
    pub fn num_drawables(&self) -> usize {
        self.drawables.len()
    }

    pub fn get_bounding_volume(&self) {}

    /// Compute the local-space AABB for this node by merging all drawable AABBs.
    ///
    /// Returns `AABB::empty()` if the node has no drawables.
    pub fn local_aabb(&self) -> AABB {
        let mut aabb = AABB::empty();
        for drawable in &self.drawables {
            aabb.merge(drawable.aabb());
        }
        aabb
    }

    /// Compute the world-space AABB by transforming the local AABB.
    pub fn world_aabb(&self) -> AABB {
        self.local_aabb().transformed(&self.model)
    }

    pub fn build_model(&mut self, backend: &B, model: &glm::Mat4) {
        self.model = model * self.model_orig;
        for drawable in &mut self.drawables {
            drawable.update_model(backend, &self.model);
        }
        for (_, c) in &mut self.children {
            c.build_model(backend, &self.model);
        }
    }

    pub fn draw(&self, backend: &mut B, object_type: ObjType) {
        for drawable in &self.drawables {
            if drawable.object_type() != object_type {
                continue;
            }
            drawable.draw(backend, true);
        }
        for (_, c) in &self.children {
            c.draw(backend, object_type);
        }
    }
}
