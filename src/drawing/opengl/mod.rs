use crate::drawing::Renderer;
use crate::window::Window;

pub struct OpenGLRenderer<W : Window> {
    window : W,
}

impl<W> Renderer for OpenGLRenderer<W> where W: Window {
    fn create(width: i32, height: i32, title: &str) -> OpenGLRenderer<W> {
        OpenGLRenderer {
            window: W::create_window(width, height, "main", title),
        }
    }
    fn cleanup(&mut self) {

    }
    fn update(&mut self) -> Result<bool, &'static str> {


        Ok(true)
    }
}