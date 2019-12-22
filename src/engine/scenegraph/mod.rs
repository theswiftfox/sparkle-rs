pub mod node;

use node::Node;
use std::cell::RefCell;
use std::rc::Rc;

use super::d3d11::drawable::{DxDrawable, ObjType};
use super::geometry::Light;

pub struct Scenegraph {
    transform: glm::Mat4,
    directional_light: Light,
    light_proj: glm::Mat4,
    root: Option<Rc<RefCell<Node>>>,
}

impl Scenegraph {
    pub fn empty() -> Scenegraph {
        Scenegraph {
            transform: glm::identity(),
            root: None,
            directional_light: Light {
                direction: glm::zero(),
                color: glm::zero(),
            },
            light_proj: glm::ortho_zo(-25.0, 25.0, -25.0, 25.0, 1.0, 70.0),
        }
    }
    pub fn set_root(&mut self, node: Rc<RefCell<Node>>) {
        self.root = Some(node)
    }

    pub fn set_directional_light(&mut self, light: Light) {
        self.directional_light = light
    }

    pub fn set_light_direction(&mut self, dir: glm::Vec3) {
        self.directional_light.direction = glm::vec3_to_vec4(&dir)
    }
    pub fn set_light_color(&mut self, color: glm::Vec3) {
        self.directional_light.color = glm::vec3_to_vec4(&color)
    }

    pub fn get_directional_light(&self) -> &Light {
        return &self.directional_light;
    }
    pub fn get_light_proj(&self) -> &glm::Mat4 {
        return &self.light_proj;
    }

    pub fn build_matrices(&mut self) {
        if let Some(root) = &mut self.root {
            root.borrow_mut().build_model(&self.transform);
        }
    }

    pub fn draw(&self, object_type: ObjType) {
        // if self.root.is_some() {
        //     self.root.as_ref().unwrap().borrow().draw(object_type);
        // }
        if let Ok(drawables) = self.traverse() {
            for i in 0..drawables.len() {
                let drawable = &drawables[i];
                if drawable.borrow().object_type() != object_type {
                    continue;
                }
                let drawable = drawable.borrow();
                let mat = drawable.material();
                let mut rebind_material = false;
                if i == 0 || !&drawables[i - 1].borrow().material().eq(mat) {
                    rebind_material = true;
                }
                drawable.draw(rebind_material);
            }
        }
    }
    pub fn get_node_named(&self, name: &str) -> Result<Rc<RefCell<Node>>, SceneGraphError> {
        if self.root.is_none() {
            Err(SceneGraphError::err_empty("Root node is empty"))
        } else {
            match &self.root.as_ref().unwrap().borrow().name {
                Some(n) => {
                    if n.as_str() == name {
                        return Ok(self.root.as_ref().unwrap().clone());
                    }
                }
                None => (),
            };
            self.root.as_ref().unwrap().borrow().get_named(name)
        }
    }

    pub fn traverse(&self) -> Result<Vec<Rc<RefCell<DxDrawable>>>, SceneGraphError> {
        if self.root.is_none() {
            Err(SceneGraphError::new("", &ErrorCause::Empty))
        } else {
            let nodes = self.root.as_ref().unwrap().borrow().traverse();
            if nodes.is_empty() {
                Err(SceneGraphError::err_empty("Root has no children"))
            } else {
                let mut drawables: Vec<Rc<RefCell<DxDrawable>>> = Vec::new();
                for node in &nodes {
                    drawables.append(&mut node.borrow().get_drawables())
                }
                drawables.sort_by(|a, b| a.partial_cmp(b).unwrap());
                Ok(drawables)
            }
        }
    }

    pub fn get_drawables_named(&self, name: &str) -> Option<Vec<Rc<RefCell<DxDrawable>>>> {
        let node = match self.get_node_named(name) {
            Ok(n) => Some(n),
            Err(_) => None,
        };
        match node {
            Some(n) => Some(n.borrow().get_drawables()),
            None => None,
        }
    }

    pub fn remove_node_named(&mut self, name: &str) -> Result<(), SceneGraphError> {
        if self.root.is_none() {
            return Err(SceneGraphError::err_empty("No root"));
        } else {
            match self
                .root
                .as_ref()
                .unwrap()
                .borrow_mut()
                .remove_node_named(name)
            {
                true => Ok(()),
                false => Err(SceneGraphError::new(name, &ErrorCause::NotFound)),
            }
        }
    }

    pub fn clear(&mut self) -> Result<(), SceneGraphError> {
        if self.root.is_none() {
            return Ok(());
        }
        self.root.as_ref().unwrap().borrow_mut().destroy();
        self.root = None;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorCause {
    NotFound,
    InvalidState,
    Empty,
}

impl std::fmt::Display for ErrorCause {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let msg = match *self {
            ErrorCause::NotFound => "NotFound",
            ErrorCause::InvalidState => "Invalid SG state",
            ErrorCause::Empty => "Empty SG",
        };
        write!(f, "{}", msg)
    }
}

#[derive(Debug, Clone)]
pub struct SceneGraphError {
    message: String,
    cause: ErrorCause,
}

impl SceneGraphError {
    pub fn new(msg: &str, cause: &ErrorCause) -> SceneGraphError {
        SceneGraphError {
            message: msg.to_string(),
            cause: cause.clone(),
        }
    }
    pub fn err_empty(msg: &str) -> SceneGraphError {
        SceneGraphError {
            message: msg.to_string(),
            cause: ErrorCause::Empty,
        }
    }
}

impl std::fmt::Display for SceneGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}: {}", self.cause, self.message)
    }
}
impl std::error::Error for SceneGraphError {
    fn description(&self) -> &str {
        &self.message
    }
}
