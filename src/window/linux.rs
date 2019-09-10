use crate::window::Window;
use glium::glutin;

pub struct WindowNix {
    width: u32,
    height: u32,
    event_loop: glutin::EventsLoop,
    display: glium::Display,
}

impl Window for WindowNix {
    fn create_window(
        width: i32,
        height: i32,
        _name: &str,
        title: &str,
    ) -> std::rc::Rc<std::cell::RefCell<WindowNix>> {
        let event_loop = glutin::EventsLoop::new();
        let wb = glutin::WindowBuilder::new()
            .with_dimensions(glium::glutin::dpi::LogicalSize {
                width: width.into(),
                height: height.into(),
            })
            .with_title(title);
        let cb = glutin::ContextBuilder::new();
        let display = glium::Display::new(wb, cb, &event_loop).unwrap();
        std::rc::Rc::new(std::cell::RefCell::new(WindowNix {
            width: width as u32,
            height: height as u32,
            event_loop: event_loop,
            display: display,
        }))
    }
    fn update(&mut self) -> bool {
        let mut should_close = false;
        self.event_loop.poll_events(|event| match event {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::CloseRequested => should_close = true,
                _ => (),
            },
            _ => (),
        });
        return should_close;
    }

    fn get_width(&self) -> u32 {
        self.width
    }
    fn get_height(&self) -> u32 {
        self.height
    }

    fn get_handle(&self) -> &glium::Display {
        &self.display
    }
}
