use crate::input::input_handler::InputHandler;
use crate::input::Camera;

pub mod generate;
pub mod geometry;
pub mod scenegraph;

#[cfg(target_os = "windows")]
crate mod d3d11;
#[cfg(target_os = "linux")]
crate mod opengl;

pub trait Renderer {
    fn create(width: i32, height: i32, title: &str) -> Self;
    fn cleanup(&mut self);
    fn update(&mut self) -> Result<bool, Box<dyn std::error::Error>>;
    fn change_camera(&mut self, cam: std::rc::Rc<std::cell::RefCell<dyn Camera>>);
    fn change_input_handler(&mut self, handler: std::rc::Rc<std::cell::RefCell<dyn InputHandler>>);
}

#[allow(dead_code)] // we don't want warnings if some color is not used..
pub mod colors_linear {
    pub fn background() -> glm::Vec4 {
        glm::vec4(0.052860655f32, 0.052860655f32, 0.052860655f32, 1.0f32)
    }
    pub fn green() -> glm::Vec4 {
        glm::vec4(0.005181516f32, 0.201556236f32, 0.005181516f32, 1.0f32)
    }
    pub fn blue() -> glm::Vec4 {
        glm::vec4(0.001517635f32, 0.114435382f32, 0.610495627f32, 1.0f32)
    }
    pub fn red() -> glm::Vec4 {
        glm::vec4(0.545724571f32, 0.026241219f32, 0.001517635f32, 1.0f32)
    }
    pub fn white() -> glm::Vec4 {
        glm::vec4(0.052860655f32, 0.052860655f32, 0.052860655f32, 1.0f32)
    }
}

use crate::window::*;
#[cfg(target_os = "linux")]
pub fn create_backend(
    width: i32,
    height: i32,
    title: &str,
) -> opengl::OpenGLRenderer<linux::WindowNix> {
    opengl::OpenGLRenderer::create(width, height, title)
}

#[cfg(target_os = "windows")]
pub fn create_backend(
    width: i32,
    height: i32,
    title: &str,
) -> d3d11::D3D11Renderer<windows::WindowWin> {
    d3d11::D3D11Renderer::create(width, height, title)
}
