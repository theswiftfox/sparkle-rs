use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};
use winit::{
    dpi::PhysicalSize,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
};

use crate::util::mtx_lock;

#[derive(Clone)]
pub struct App {
    width: u32,
    height: u32,
    title: String,
    initialized: Arc<AtomicBool>,
    quit_requested: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<WindowEvent>>>,
    winit_notify: mpsc::Sender<Arc<Window>>,
}

impl App {
    /// Creates the winit window and event loop immediately.
    /// Returns the window handle and event loop separately so you can
    /// initialize a render backend before starting the event loop.
    pub fn new(
        width: u32,
        height: u32,
        title: &str,
        winit_notify: mpsc::Sender<Arc<Window>>,
    ) -> Result<(Self, EventLoop<()>), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        Ok((
            Self {
                width,
                height,
                title: title.to_owned(),
                initialized: Arc::new(AtomicBool::new(false)),
                quit_requested: Arc::new(AtomicBool::new(false)),
                events: Arc::new(Mutex::new(Vec::new())),
                winit_notify,
            },
            event_loop,
        ))
    }

    // #[must_use]
    // pub fn winit_window(&self) -> Option<Arc<winit::window::Window>> {
    //     let _l = mtx_lock(&self.lock);
    //     self.winit_window.clone()
    // }

    // #[must_use]
    // pub fn is_initialized(&self) -> bool {
    //     let _l = mtx_lock(&self.lock);
    //     self.winit_window.is_some()
    // }

    pub fn wants_quit(&self) -> bool {
        self.quit_requested.load(Ordering::SeqCst)
    }

    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::SeqCst);
    }

    pub fn poll_events(&self) -> Vec<WindowEvent> {
        std::mem::take(&mut mtx_lock(&self.events))
    }
}

pub struct Window {
    window_ref: Arc<winit::window::Window>,
    h_wnd: RawWindowHandle,
    h_dsp: RawDisplayHandle,
}

impl Window {
    #[must_use]
    pub fn h_wnd(&self) -> RawWindowHandle {
        self.h_wnd
    }
    #[must_use]
    pub fn h_dsp(&self) -> RawDisplayHandle {
        self.h_dsp
    }
    #[must_use]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.window_ref.inner_size()
    }

    #[must_use]
    pub fn winit_window(&self) -> &winit::window::Window {
        &self.window_ref
    }

    #[must_use]
    pub fn winit_window_arc(&self) -> Arc<winit::window::Window> {
        Arc::clone(&self.window_ref)
    }
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.initialized.load(Ordering::SeqCst) {
            match event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_title(&self.title)
                        .with_inner_size(LogicalSize::new(self.width, self.height)), // .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None))),
                )
                .map_err(|e| format!("{e}"))
                .and_then(|w| {
                    let hwnd = w
                        .window_handle()
                        .map(|rwh| rwh.as_raw())
                        .map_err(|e| format!("{e}"))?;
                    let hdsp = w
                        .display_handle()
                        .map(|rdh| rdh.as_raw())
                        .map_err(|e| format!("{e}"))?;
                    Ok((w, hwnd, hdsp))
                }) {
                Ok((w, hwnd, hdsp)) => {
                    let window = Arc::new(Window {
                        window_ref: Arc::new(w),
                        h_wnd: hwnd,
                        h_dsp: hdsp,
                    });
                    if let Err(e) = self.winit_notify.send(window) {
                        eprintln!("Failed to notify about window creation: {e}");
                    }
                    self.initialized.store(true, Ordering::SeqCst)
                }
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
            // WindowEvent::Resized(size) => {
            //     self.width = size.width;
            //     self.height = size.height;
            // }
            WindowEvent::RedrawRequested => {
                // skip those
                return;
            }
            _ => {}
        }
        mtx_lock(&self.events).push(event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.wants_quit() {
            event_loop.exit();
            return;
        }
    }
}
