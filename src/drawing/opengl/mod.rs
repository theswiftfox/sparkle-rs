use crate::drawing;
use crate::drawing::Renderer;
use crate::window::Window;
use glium::Surface;

pub struct OpenGLRenderer<W : Window> {
    window : W,
    target: Option<glium::Frame>,
}

impl<W> Renderer for OpenGLRenderer<W> where W: Window {
    fn create(width: i32, height: i32, title: &str) -> OpenGLRenderer<W> {
        OpenGLRenderer {
            window: W::create_window(width, height, "main", title),
            target: None,
        }
    }
    fn cleanup(&mut self) {

    }
    fn update(&mut self) -> Result<bool, &'static str> {
        let should_close = self.window.update();
        if should_close {
            return Ok(false);
        }
        self.target = Some(self.window.get_handle().draw());
        self.clear();
        
        match self.target.take().unwrap().finish() {
            Ok(_) => Ok(true),
            Err(_) => Err("SwapBuffer error occured")
        }
    }
}

impl<W> OpenGLRenderer<W> where W: Window {
    fn clear(&mut self) {
        let target = self.target.as_mut().unwrap();
        let clear_color = drawing::colors_linear::BACKGROUND;
        target.clear_color(clear_color.x, clear_color.y, clear_color.z, clear_color.w);
    }
}