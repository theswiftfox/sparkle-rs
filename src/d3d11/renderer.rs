use std::*;
use super::super::{window};
use super::{backend};

pub struct D3D11Renderer {
    backend : backend::D3D11Backend,
    window : window::Window
}

impl D3D11Renderer {
    pub fn create(width: i32, height: i32, title: &str) -> Result<D3D11Renderer, &'static str> {
        let window = window::Window::create_window(width, height, "main", title)?;
        let backend = backend::D3D11Backend::init(&window)?;
        let renderer = D3D11Renderer {
            backend: backend,
            window: window
        };
        
        Ok(renderer)
    }

    pub fn cleanup(&mut self) {
        self.backend.cleanup();
    }

    pub fn update(&mut self) -> Result<bool, &'static str> {
        let should_close = self.window.update();
        self.backend.present()?;
        
        Ok(should_close)
    }
}