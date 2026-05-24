use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};

pub struct Window {
    winit_window: Arc<winit::window::Window>,
    width: u32,
    height: u32,
    quit_requested: bool,
}

impl Window {
    /// Creates the winit window and event loop immediately.
    /// Returns the window handle and event loop separately so you can
    /// initialize a render backend before starting the event loop.
    pub fn new(
        width: u32,
        height: u32,
        title: &str,
    ) -> Result<(Self, EventLoop<()>), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        #[allow(deprecated)]
        let winit_window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_title(title)
                        .with_inner_size(PhysicalSize::new(width, height)),
                )?,
        );
        Ok((
            Window {
                winit_window,
                width,
                height,
                quit_requested: false,
            },
            event_loop,
        ))
    }

    pub fn winit_window(&self) -> &winit::window::Window {
        &self.winit_window
    }

    pub fn winit_window_arc(&self) -> Arc<winit::window::Window> {
        self.winit_window.clone()
    }

    pub fn get_resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn request_quit(&mut self) {
        self.quit_requested = true;
    }

    /// Run the event loop, calling `frame_fn` each frame with the window
    /// and any pending window events (e.g. keyboard, mouse, resize).
    pub fn run<F>(
        mut self,
        event_loop: EventLoop<()>,
        mut frame_fn: F,
    ) -> Result<(), winit::error::EventLoopError>
    where
        F: FnMut(&mut Window, &[WindowEvent]) + 'static,
    {
        let mut app = WindowApp {
            window: &mut self,
            frame_fn: &mut frame_fn,
            events: Vec::new(),
        };
        event_loop.run_app(&mut app)
    }
}

struct WindowApp<'a, F: FnMut(&mut Window, &[WindowEvent]) + 'a> {
    window: &'a mut Window,
    frame_fn: &'a mut F,
    events: Vec<WindowEvent>,
}

impl<'a, F: FnMut(&mut Window, &[WindowEvent]) + 'a> ApplicationHandler for WindowApp<'a, F> {
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => {
                self.window.width = size.width;
                self.window.height = size.height;
            }
            WindowEvent::RedrawRequested => return,
            _ => {}
        }
        self.events.push(event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.quit_requested {
            event_loop.exit();
            return;
        }
        let events = std::mem::take(&mut self.events);
        (self.frame_fn)(self.window, &events);
        self.window.winit_window.request_redraw();
    }
}
