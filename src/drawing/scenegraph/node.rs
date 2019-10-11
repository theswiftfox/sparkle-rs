use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc as shared_ptr;

use super::drawable::Drawable;
use super::{ErrorCause, SceneGraphError};

#[derive(Clone)]
pub struct Node {
    uuid: u64,
    pub name: Option<String>,
    model: glm::Mat4,
    children: HashMap<String, shared_ptr<RefCell<Node>>>,

    drawable: Option<shared_ptr<RefCell<dyn Drawable>>>,
}

impl Node {
    pub fn create(
        name: Option<&str>,
        model: glm::Mat4,
        drawable: Option<shared_ptr<RefCell<dyn Drawable>>>,
    ) -> shared_ptr<RefCell<Node>> {
        shared_ptr::new(RefCell::new(Node {
            uuid: 0, // TODO
            name: match name {
                Some(n) => Some(n.to_string()),
                None => None
            },
            model: model,
            drawable: drawable,
            children: HashMap::new(),
        }))
    }
    pub fn destroy(&mut self) {
        // if self.drawable.is_some() {
        //     let d = self.drawable.unwrap();
        //     self.drawable = None;
        //     drop(d);
        // }
        self.drawable = None; // this should reduce the ref count if we had a drawable and rc should auto delete if ref count == 0?
        for (_, c) in &self.children {
            c.borrow_mut().destroy();
        }
        self.children.clear();
    }
    pub fn get_named(&self, name: &str) -> Result<shared_ptr<RefCell<Node>>, SceneGraphError> {
        if self.children.is_empty() {
            return Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        }
        match self.children.get(name) {
            Some(c) => return Ok(c.clone()),
            _ => {}
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
            None => None,
        }
    }
    pub fn apply_pre_transform(&self, model: glm::Mat4) -> shared_ptr<RefCell<Node>> {
        let node = shared_ptr::from(RefCell::from(self.clone()));
        node.borrow_mut().model = model * self.model;
        return node;
    }
    pub fn traverse(&self, model: glm::Mat4) -> Vec<shared_ptr<RefCell<Node>>> {
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
        self.children.insert(key.to_string(), node);
        Ok(())
    }
    pub fn remove_node_named(&mut self, name: &str) -> bool {
        if self.children.is_empty() {
            return false; //Err(SceneGraphError::new(name, &ErrorCause::NotFound));
        } else {
            match self.children.remove(name) {
                Some(v) => {
                    v.borrow_mut().destroy();
                    return true
                },
                _ => {}
            }
            for (_, c) in &self.children {
                match c.borrow_mut().remove_node_named(name) {
                    true => return true,
                    false => (),
                }
            }
        }
        false
    }
    pub fn remove_node_uuid(&mut self, uuid: u64) -> bool {
        if self.children.is_empty() {
            return false; //Err(SceneGraphError::new(&format!("UUID: {} not found", uuid), &ErrorCause::NotFound))
        }
        let mut key = String::default();
        for (k, c) in &self.children {
            if c.borrow().uuid == uuid {
                key = k.clone();
                break;
            }
        }
        if key.len() > 0 {
            let n = self.children.remove(&key).unwrap();
            n.borrow_mut().destroy();
            return true;
        } else {
            for (_, c) in &self.children {
                match c.borrow_mut().remove_node_uuid(uuid) {
                    true => return true,
                    false => (),
                }
            }
        }
        false
    }
    pub fn make_drawable(&mut self, drawable: shared_ptr<RefCell<dyn Drawable>>) {
        self.drawable = Some(drawable)
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
    pub fn get_bounding_volume(&self) {}
    pub fn draw(&self, model: glm::Mat4) {
        let me = self.apply_pre_transform(model);
        let me_ref = me.borrow();
        if me_ref.drawable.is_some() {
            let mut drawable = me_ref.drawable.as_ref().unwrap().borrow_mut();
            drawable.draw(me_ref.model);
        }
    }
}
