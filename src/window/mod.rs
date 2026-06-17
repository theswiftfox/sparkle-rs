use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};

pub struct Window {
    winit_window: Option<Arc<winit::window::Window>>,
    width: u32,
    height: u32,
    title: String,
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
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        Ok((
            Self {
                winit_window: None,
                width,
                height,
                title: title.to_owned(),
                quit_requested: false,
            },
            event_loop,
        ))
    }

    #[must_use]
    pub fn winit_window(&self) -> Option<Arc<winit::window::Window>> {
        self.winit_window.clone()
    }

    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.winit_window.is_some()
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
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.winit_window.is_none() {
            match event_loop.create_window(
                winit::window::Window::default_attributes()
                    .with_title(&self.window.title)
                    .with_inner_size(LogicalSize::new(self.window.width, self.window.height)), // .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None))),
            ) {
                Ok(w) => self.window.winit_window = Some(Arc::new(w)),
                Err(e) => eprintln!("Failed to create window: {e}"),
            }
        }
    }

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
            WindowEvent::RedrawRequested => {
                let events = std::mem::take(&mut self.events);
                (self.frame_fn)(self.window, &events);
                return;
            }
            _ => {}
        }
        self.events.push(event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.quit_requested {
            event_loop.exit();
            return;
        }
        if let Some(w) = &self.window.winit_window {
            w.request_redraw();
        }
    }
}
