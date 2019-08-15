pub mod geometry;

#[cfg(target_os = "windows")]
mod d3d11;
#[cfg(target_os = "linux")]
mod opengl;

pub trait Renderer {
    fn create(width: i32, height: i32, title: &str) -> Self;
    fn cleanup(&mut self);
    fn update(&mut self) -> Result<bool, &'static str>;
}

#[allow(dead_code)] // we don't want warnings if some color is not used..
mod colors_linear {
    pub const BACKGROUND: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.052860655f32,
        y: 0.052860655f32,
        z: 0.052860655f32,
        w: 1.0f32,
    };
    pub const GREEN: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.005181516f32,
        y: 0.201556236f32,
        z: 0.005181516f32,
        w: 1.0f32,
    };
    pub const BLUE: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.001517635f32,
        y: 0.114435382f32,
        z: 0.610495627f32,
        w: 1.0f32,
    };
    pub const RED: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.545724571f32,
        y: 0.026241219f32,
        z: 0.001517635f32,
        w: 1.0f32,
    };
    pub const WHITE: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.052860655f32,
        y: 0.052860655f32,
        z: 0.052860655f32,
        w: 1.0f32,
    };
}

use crate::window::*;
#[cfg(target_os = "linux")]
pub fn create_backend(width: i32, height: i32, title: &str) -> opengl::OpenGLRenderer<linux::WindowNix> {
    opengl::OpenGLRenderer::create(width, height, title)
}

#[cfg(target_os = "windows")]
pub fn create_backend(width: i32, height: i32, title: &str) -> d3d11::D3D11Renderer<windows::WindowWin> {
    d3d11::D3D11Renderer::create(width, height, title)
}