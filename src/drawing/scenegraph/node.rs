use cgmath::Matrix4;
use cgmath::Quaternion;
use cgmath::Vector3;
use std::cell::RefCell;
use std::rc::Rc as shared_ptr;
use std::collections::HashMap;

use super::drawable::Drawable;
use super::{SceneGraphError, ErrorCause};

#[derive(Clone)]
pub struct Node {
    uuid: u64,
    name: String,
    model: Matrix4<f32>,
    children: HashMap<String, shared_ptr<RefCell<Node>>>,

    drawable: Option<shared_ptr<RefCell<dyn Drawable>>>,
}

impl Node {
    pub fn create(name: &str, model: Matrix4<f32>, drawable: Option<shared_ptr<RefCell<dyn Drawable>>>) -> shared_ptr<RefCell<Node>> {
        shared_ptr::new(RefCell::new(Node {
            uuid: 0, // TODO
            name: name.to_string(),
            model: model,
            drawable: drawable,
            children: HashMap::new(),
        }))
    }
    pub fn get_named(&self, name: &str) -> Result<shared_ptr<RefCell<Node>>, SceneGraphError> {
        if self.children.is_empty() {
            return Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        }
        match self.children.get(name) {
            Some(c) => return Ok(c.clone()),
            _ => {},
        };
        for (_, node) in &self.children {
            match node.borrow().get_named(name) {
                Ok(c) => return Ok(c),
                Err(e) => return Err(e),
            }
        }
        Err(SceneGraphError::new(name, &ErrorCause::NotFound))
    }
    pub fn get_drawable(&self) -> Option<shared_ptr<RefCell<dyn Drawable>>> {
        match &self.drawable {
            Some(d) => Some(d.clone()),
            None => None
        }
    }
    pub fn apply_pre_transform(&self, model: Matrix4<f32>) -> shared_ptr<RefCell<Node>> {
        let node = shared_ptr::from(RefCell::from(self.clone()));
        node.borrow_mut().model = model * self.model;
        return node;
    }
    pub fn traverse(&self, model: Matrix4<f32>) -> Vec<shared_ptr<RefCell<Node>>> {
        let mut nodes: Vec<shared_ptr<RefCell<Node>>> = Vec::new();
        let me = self.apply_pre_transform(model);
        for (_, c) in &self.children {
            let mut others = c.borrow().traverse(me.borrow().model);
            nodes.append(&mut others);
        }
        nodes.push(me);
        return nodes;
    }
    pub fn add_child(&mut self, node: shared_ptr<RefCell<Node>>) -> Result<(), SceneGraphError> {
        let key = &node.borrow().name.clone();
        if self.children.contains_key(key) {
            return Err(SceneGraphError::new("Duplicate Name", &ErrorCause::InvalidState));
        }
        self.children.insert(key.to_string(), node);
        Ok(())
    }
    pub fn remove_node(&mut self, name: &str) -> Result<(), SceneGraphError> {
        if self.children.is_empty() {
            return Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        } else {
            match self.children.remove(name) {
                Some(_) => return Ok(()),
                _ => {},
            }
            for (_, c) in &self.children {
                return c.borrow_mut().remove_node(name);
            }
        }
        Ok(())
    }
    pub fn make_drawable(&mut self, drawable: shared_ptr<RefCell<dyn Drawable>>) {
        self.drawable = Some(drawable)
    }
    pub fn translate(&mut self, t: Vector3<f32>) {
        let t_mat = Matrix4::from_translation(t);
        self.model = self.model * t_mat;
    }
    pub fn rotate(&mut self, r: Quaternion<f32>) {
        let r_mat = Matrix4::from(r);
        self.model = self.model * r_mat;
    }
    pub fn scale(&mut self, s: f32) {
        let s_mat = Matrix4::from_scale(s);
        self.model = self.model * s_mat;
    }
    pub fn get_bounding_volume(&self) {}
    pub fn draw(&self, model: Matrix4<f32>) {
        let me = self.apply_pre_transform(model);
        let me_ref = me.borrow();
        if me_ref.drawable.is_some() {
            let drawable = me_ref.drawable.as_ref().unwrap().borrow();
            drawable.draw(me_ref.model);
        }
    }
}
