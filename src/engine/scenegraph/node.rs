use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::engine::backend::{Drawable, GpuBackend, ObjType};
use crate::engine::geometry::AABB;
use super::{ErrorCause, SceneGraphError};

pub struct Node<B: GpuBackend> {
    uuid: u64,
    pub name: Option<String>,
    model: glm::Mat4,
    model_orig: glm::Mat4,
    children: HashMap<String, Rc<RefCell<Node<B>>>>,

    drawables: Vec<Rc<RefCell<Drawable<B>>>>,
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
        drawable: Option<Vec<Rc<RefCell<Drawable<B>>>>>,
    ) -> Rc<RefCell<Node<B>>> {
        let n = Rc::new(RefCell::new(Node {
            uuid: 0, // TODO
            name: match name {
                Some(n) => Some(n.to_string()),
                None => None,
            },
            model,
            model_orig: model,
            drawables: Vec::new(),
            children: HashMap::new(),
        }));
        if let Some(d) = drawable {
            n.borrow_mut().drawables = d;
        }
        n
    }

    pub fn destroy(&mut self) {
        self.drawables.clear();
        for (_, c) in &self.children {
            c.borrow_mut().destroy();
        }
        self.children.clear();
    }

    pub fn get_named(&self, name: &str) -> Result<Rc<RefCell<Node<B>>>, SceneGraphError> {
        if self.children.is_empty() {
            return Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        }
        if let Some(c) = self.children.get(name) {
            return Ok(c.clone());
        }
        for (_, node) in &self.children {
            match node.borrow().get_named(name) {
                Ok(c) => return Ok(c),
                Err(e) => return Err(e),
            }
        }
        Err(SceneGraphError::new(name, &ErrorCause::NotFound))
    }

    pub fn get_drawables(&self) -> Vec<Rc<RefCell<Drawable<B>>>> {
        self.drawables.clone()
    }

    pub fn traverse(&self) -> Vec<Rc<RefCell<Node<B>>>> {
        let mut nodes: Vec<Rc<RefCell<Node<B>>>> = Vec::new();
        for (_, c) in &self.children {
            nodes.push(c.clone());
            let mut others = c.borrow().traverse();
            nodes.append(&mut others);
        }
        nodes
    }

    pub fn add_child(&mut self, node: Rc<RefCell<Node<B>>>) -> Result<(), SceneGraphError> {
        let n = node.borrow();
        let key = match &n.name {
            Some(n) => n.clone(),
            None => n.uuid.to_string(),
        };
        if self.children.contains_key(&key) {
            return Err(SceneGraphError::new(
                "Duplicate Name",
                &ErrorCause::InvalidState,
            ));
        }
        drop(n); // unborrow node so we can move it into children
        self.children.insert(key, node);
        Ok(())
    }

    pub fn remove_node_named(&mut self, name: &str) -> bool {
        if self.children.is_empty() {
            return false;
        }
        if let Some(v) = self.children.remove(name) {
            v.borrow_mut().destroy();
            return true;
        }
        for (_, c) in &self.children {
            if c.borrow_mut().remove_node_named(name) {
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
            if c.borrow().uuid == uuid {
                key = k.clone();
                break;
            }
        }
        if !key.is_empty() {
            let n = self.children.remove(&key).unwrap();
            n.borrow_mut().destroy();
            return true;
        }
        for (_, c) in &self.children {
            if c.borrow_mut().remove_node_uuid(uuid) {
                return true;
            }
        }
        false
    }

    pub fn add_drawable(&mut self, drawable: Rc<RefCell<Drawable<B>>>) {
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
    pub fn children_list(&self) -> Vec<Rc<RefCell<Node<B>>>> {
        self.children.values().cloned().collect()
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
            aabb.merge(drawable.borrow().aabb());
        }
        aabb
    }

    /// Compute the world-space AABB by transforming the local AABB.
    pub fn world_aabb(&self) -> AABB {
        self.local_aabb().transformed(&self.model)
    }

    pub fn build_model(&mut self, backend: &B, model: &glm::Mat4) {
        self.model = model * self.model_orig;
        for drawable in &self.drawables {
            drawable.borrow_mut().update_model(backend, &self.model);
        }
        for (_, c) in &self.children {
            c.borrow_mut().build_model(backend, &self.model);
        }
    }

    pub fn draw(&self, backend: &mut B, object_type: ObjType) {
        for drawable in &self.drawables {
            if drawable.borrow().object_type() != object_type {
                continue;
            }
            drawable.borrow().draw(backend, true);
        }
        for (_, c) in &self.children {
            c.borrow().draw(backend, object_type);
        }
    }
}
