pub mod node;

use node::Node;

use crate::engine::backend::RenderItem;

use super::backend::{GpuBackend, ObjType};
use super::geometry::Light;

pub struct Scenegraph<B: GpuBackend> {
    transform: glm::Mat4,
    lights: Vec<Light>,
    root: Option<Node<B>>,
}

impl<B: GpuBackend> Scenegraph<B> {
    pub fn empty() -> Scenegraph<B> {
        Scenegraph {
            transform: glm::identity(),
            root: None,
            lights: Vec::<Light>::new(),
        }
    }

    pub fn set_root(&mut self, node: Node<B>) {
        self.root = Some(node)
    }

    pub fn add_light(&mut self, light: Light) {
        self.lights.push(light)
    }

    pub fn update_light(&mut self, light: Light, index: usize) -> Result<(), SceneGraphError> {
        if index >= self.lights.len() {
            return Err(SceneGraphError::new(
                "Light index out of bounds",
                &ErrorCause::NotFound,
            ));
        }
        self.lights[index] = light;
        Ok(())
    }

    pub fn get_lights(&self) -> &Vec<Light> {
        &self.lights
    }

    pub fn clear_lights(&mut self) {
        self.lights.clear();
    }

    pub fn remove_light(&mut self, index: usize) -> Result<(), SceneGraphError> {
        if index >= self.lights.len() {
            return Err(SceneGraphError::new(
                "Light index out of bounds",
                &ErrorCause::NotFound,
            ));
        }
        self.lights.remove(index);
        Ok(())
    }

    pub fn root(&self) -> Option<&Node<B>> {
        self.root.as_ref()
    }

    pub fn build_matrices(&mut self, backend: &B) {
        if let Some(root) = &mut self.root {
            root.build_model(backend, &self.transform);
        }
    }

    pub fn draw(&self, backend: &mut B, object_type: ObjType) {
        if let Ok(drawables) = self.traverse() {
            for i in 0..drawables.len() {
                let drawable = &drawables[i];
                if drawable.object_type() != object_type {
                    continue;
                }
                let mat = drawable.material();
                let mut rebind_material = false;
                if i == 0 || !drawables[i - 1].material().eq(mat) {
                    rebind_material = true;
                }
                drawable.draw(backend, rebind_material);
            }
        }
    }

    pub fn get_node_named(&self, name: &str) -> Result<&Node<B>, SceneGraphError> {
        let Some(root) = &self.root else {
            return Err(SceneGraphError::err_empty("Root node is empty"));
        };

        if root.name.as_ref().map(|n| n == name) == Some(true) {
            return Ok(self.root.as_ref().unwrap());
        }

        root.get_named(name)
    }
    pub fn get_node_named_mut(&mut self, name: &str) -> Result<&mut Node<B>, SceneGraphError> {
        let Some(root) = &mut self.root else {
            return Err(SceneGraphError::err_empty("Root node is empty"));
        };

        if root.name.as_ref().map(|n| n == name) == Some(true) {
            return Ok(root);
        }

        todo!()
    }

    pub fn traverse(&self) -> Result<Vec<RenderItem<'_, B>>, SceneGraphError> {
        let Some(root) = &self.root else {
            return Err(SceneGraphError::new("", &ErrorCause::Empty));
        };
        let mut drawables = root.get_drawables();
        for node in &root.traverse() {
            drawables.append(&mut node.get_drawables());
        }
        if drawables.is_empty() {
            return Err(SceneGraphError::err_empty("Scene has no drawables"));
        }
        drawables.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Ok(drawables)
    }

    pub fn get_drawables_named(&self, name: &str) -> Option<Vec<RenderItem<'_, B>>> {
        match self.get_node_named(name) {
            Ok(n) => Some(n.get_drawables()),
            Err(_) => None,
        }
    }

    pub fn remove_node_named(&mut self, name: &str) -> Result<(), SceneGraphError> {
        let Some(root) = &mut self.root else {
            return Err(SceneGraphError::err_empty("No root"));
        };
        match root.remove_node_named(name) {
            true => Ok(()),
            false => Err(SceneGraphError::new(name, &ErrorCause::NotFound)),
        }
    }

    pub fn clear(&mut self) -> Result<(), SceneGraphError> {
        let Some(root) = &mut self.root else {
            return Ok(());
        };
        root.destroy();
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
            cause: *cause,
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
