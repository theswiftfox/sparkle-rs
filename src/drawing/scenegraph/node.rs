use cgmath::Matrix4;
use cgmath::Quaternion;
use cgmath::Vector3;
use std::cell::RefCell;
use std::rc::Rc as shared_ptr;

use super::drawable::Drawable;

#[derive(Clone)]
struct Node {
    model: Matrix4<f32>,
    children: Vec<shared_ptr<RefCell<Node>>>,

    drawable: Option<shared_ptr<RefCell<dyn Drawable>>>,
}

impl Node {
    fn apply_pre_transform(&self, model: Matrix4<f32>) -> shared_ptr<RefCell<Node>> {
        let node = shared_ptr::from(RefCell::from(self.clone()));
        node.borrow_mut().model = model * self.model;
        return node;
    }
    fn traverse(&self, model: Matrix4<f32>) -> Vec<shared_ptr<RefCell<Node>>> {
        let mut nodes: Vec<shared_ptr<RefCell<Node>>> = Vec::new();
        let me = self.apply_pre_transform(model);
        for c in self.children.iter() {
            let mut others = c.borrow().traverse(me.borrow().model);
            nodes.append(&mut others);
        }
        nodes.push(me);
        return nodes;
    }
    fn add_child(&mut self, node: shared_ptr<RefCell<Node>>) {
        self.children.push(node);
    }
    fn make_drawable(&mut self, drawable: shared_ptr<RefCell<dyn Drawable>>) {
        self.drawable = Some(drawable)
    }
    fn translate(&mut self, t: Vector3<f32>) {
        let t_mat = Matrix4::from_translation(t);
        self.model = self.model * t_mat;
    }
    fn rotate(&mut self, r: Quaternion<f32>) {}
    fn scale(&mut self, s: f32, axis: Vector3<f32>) {}
    fn get_bounding_volume(&self) {}
    fn draw(&self, model: Matrix4<f32>) {
        let me = self.apply_pre_transform(model);
        let me_ref = me.borrow();
        if me_ref.drawable.is_some() {
            let drawable = me_ref.drawable.as_ref().unwrap().borrow();
            drawable.draw(me_ref.model);
        }
    }
}
