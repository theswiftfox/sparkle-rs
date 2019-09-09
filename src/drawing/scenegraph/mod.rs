pub mod drawable;
pub mod node;

use node::Node;
use std::cell::RefCell;
use std::rc::Rc as shared_ptr;
use cgmath::num_traits::One;

pub struct Scenegraph {
    transform: cgmath::Matrix4<f32>,
    root: Option<shared_ptr<RefCell<Node>>>,
}

impl Scenegraph {
    pub fn empty() -> Scenegraph {
        Scenegraph {
            transform: cgmath::Matrix4::one(),
            root: None,
        }
    }
    pub fn set_root(&mut self, node: shared_ptr<RefCell<Node>>) {
        self.root = Some(node)
    }

    pub fn draw(&self) {
        if self.root.is_some() {
            self.root.as_ref().unwrap().borrow().draw(self.transform);
        }
    }
    pub fn get_node(&self, name: &str) -> Result<shared_ptr<RefCell<Node>>, SceneGraphError> {
        if self.root.is_none() {
            Err(SceneGraphError::err_empty("Root node is empty"))
        } else {
            match self.root.as_ref().unwrap().borrow().name.as_str() {
                n if (n == name) => Ok(self.root.as_ref().unwrap().clone()),
                _ => self.root.as_ref().unwrap().borrow().get_named(name),
            }
        }
    }

    pub fn traverse(&self) -> Result<Vec<shared_ptr<RefCell<Node>>>, SceneGraphError>  {
        if self.root.is_none() {
            Err(SceneGraphError::new("", &ErrorCause::Empty))
        } else {
            let nodes = self.root.as_ref().unwrap().borrow().traverse(self.transform);
            if nodes.is_empty() {
                Err(SceneGraphError::err_empty("Root has no children"))
            } else {
                Ok(nodes)
            }
        }
    }

    pub fn get_drawable(&self, name: &str) -> Option<shared_ptr<RefCell<dyn drawable::Drawable>>> {
        let node = match self.get_node(name) {
            Ok(n) => Some(n),
            Err(_) => None
        };
        match node {
            Some(n) => n.borrow().get_drawable(),
            None => None,
        }
    }

    pub fn remove_node(&mut self, name: &str) -> Result<(), SceneGraphError> {
        if self.root.is_none() {
            return Err(SceneGraphError::err_empty("No root"))
        } else {
            self.root.as_ref().unwrap().borrow_mut().remove_node(name)
        }
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
