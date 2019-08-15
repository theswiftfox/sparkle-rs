use crate::window::Window;

pub struct WindowNix {

}

impl Window for WindowNix {
    fn create_window(width: i32, height: i32, name: &str, title: &str) -> WindowNix {
        WindowNix { }
    }
    fn update(&self) -> bool {
        true
    }
}