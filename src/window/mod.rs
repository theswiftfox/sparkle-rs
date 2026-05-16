use crate::input::input_handler::{
    Action, ApplicationRequest, Button, InputHandler, Key, ScrollAxis,
};

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowId,
};

/// Cross-platform window backed by winit.
pub struct Window {
    winit_window: Option<Arc<winit::window::Window>>,
    width: u32,
    height: u32,
    input_handler: Option<Rc<RefCell<dyn InputHandler>>>,
    snap_mouse: bool,
    last_pos: Option<(f64, f64)>,
    title: String,
}

impl Window {
    pub fn new(width: u32, height: u32, title: &str) -> Self {
        Window {
            winit_window: None,
            width,
            height,
            input_handler: None,
            snap_mouse: false,
            last_pos: None,
            title: title.to_string(),
        }
    }

    pub fn get_resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }

    /// Returns a reference to the underlying winit window, if created.
    /// The window is created lazily when the event loop starts.
    pub fn winit_window(&self) -> Option<&winit::window::Window> {
        self.winit_window.as_deref()
    }

    /// Returns a cloneable Arc handle to the winit window.
    /// Needed for wgpu surface creation (requires `Surface<'static>`).
    pub fn winit_window_arc(&self) -> Option<Arc<winit::window::Window>> {
        self.winit_window.clone()
    }

    pub fn set_title(&mut self, subtitle: &str) {
        let combined = format!("{} {}", self.title, subtitle);
        if let Some(w) = &self.winit_window {
            w.set_title(&combined);
        }
    }

    pub fn set_input_handler(
        &mut self,
        handler: Rc<RefCell<dyn InputHandler>>,
    ) {
        self.input_handler = Some(handler);
    }

    /// Runs the event loop, calling `frame_fn` once per frame.
    /// This consumes the Window and blocks until the application exits.
    pub fn run(self, frame_fn: impl FnMut(&mut Window) + 'static) {
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        let mut app = WindowApp {
            window: self,
            frame_fn: Box::new(frame_fn),
        };
        event_loop.run_app(&mut app).expect("Event loop error");
    }

    fn handle_window_event(&mut self, event: WindowEvent, event_loop: &ActiveEventLoop) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.width = size.width;
                self.height = size.height;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(ref handler) = self.input_handler {
                    let action = match event.state {
                        ElementState::Pressed => Action::Down,
                        ElementState::Released => Action::Up,
                    };
                    let key = if let PhysicalKey::Code(code) = event.physical_key {
                        translate_key(code)
                    } else {
                        Key::None
                    };
                    match handler.borrow_mut().handle_key(key, action) {
                        ApplicationRequest::Quit => event_loop.exit(),
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(ref handler) = self.input_handler {
                    let action = match state {
                        ElementState::Pressed => Action::Down,
                        ElementState::Released => Action::Up,
                    };
                    let btn = match button {
                        MouseButton::Left => Button::Left,
                        MouseButton::Right => Button::Right,
                        MouseButton::Middle => Button::Middle,
                        _ => return,
                    };
                    match handler.borrow_mut().handle_mouse(btn, action) {
                        ApplicationRequest::SnapMouse => {
                            self.snap_mouse = true;
                            if let Some(w) = &self.winit_window {
                                w.set_cursor_visible(false);
                            }
                        }
                        ApplicationRequest::UnsnapMouse => {
                            self.snap_mouse = false;
                            if let Some(w) = &self.winit_window {
                                w.set_cursor_visible(true);
                            }
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(ref handler) = self.input_handler {
                    match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            if y.abs() > 0.0 {
                                handler
                                    .borrow_mut()
                                    .handle_wheel(ScrollAxis::Vertical, y * 24.0);
                            }
                            if x.abs() > 0.0 {
                                handler
                                    .borrow_mut()
                                    .handle_wheel(ScrollAxis::Horizontal, x * 24.0);
                            }
                        }
                        MouseScrollDelta::PixelDelta(pos) => {
                            if pos.y.abs() > 0.0 {
                                handler
                                    .borrow_mut()
                                    .handle_wheel(ScrollAxis::Vertical, pos.y as f32);
                            }
                            if pos.x.abs() > 0.0 {
                                handler
                                    .borrow_mut()
                                    .handle_wheel(ScrollAxis::Horizontal, pos.x as f32);
                            }
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ref handler) = self.input_handler {
                    if let Some((lx, ly)) = self.last_pos {
                        let dx = position.x - lx;
                        let dy = position.y - ly;
                        handler
                            .borrow_mut()
                            .handle_mouse_move(dx as i32, dy as i32);
                    }
                    if self.snap_mouse {
                        if let Some(w) = &self.winit_window {
                            let size = w.inner_size();
                            let cx = size.width as f64 / 2.0;
                            let cy = size.height as f64 / 2.0;
                            let _ = w.set_cursor_position(
                                winit::dpi::PhysicalPosition::new(cx, cy),
                            );
                            self.last_pos = Some((cx, cy));
                        }
                    } else {
                        self.last_pos = Some((position.x, position.y));
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Rendering is driven by about_to_wait / frame_fn
            }
            _ => {}
        }
    }
}

// -- Event loop integration via ApplicationHandler (winit 0.30) --

struct WindowApp {
    window: Window,
    frame_fn: Box<dyn FnMut(&mut Window)>,
}

impl ApplicationHandler for WindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        if self.window.winit_window.is_none() {
            let attrs = winit::window::Window::default_attributes()
                .with_title(&self.window.title)
                .with_inner_size(PhysicalSize::new(self.window.width, self.window.height));
            self.window.winit_window = Some(Arc::new(
                event_loop
                    .create_window(attrs)
                    .expect("Failed to create window"),
            ));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        self.window.handle_window_event(event, event_loop);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        (self.frame_fn)(&mut self.window);
        if let Some(w) = &self.window.winit_window {
            w.request_redraw();
        }
    }
}

// -- Key translation from winit KeyCode to sparkle Key --

fn translate_key(code: KeyCode) -> Key {
    match code {
        KeyCode::ArrowLeft => Key::KeyLeft,
        KeyCode::ArrowRight => Key::KeyRight,
        KeyCode::ArrowUp => Key::KeyUp,
        KeyCode::ArrowDown => Key::KeyDown,
        KeyCode::Space => Key::Space,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Return,
        KeyCode::CapsLock => Key::Caps,
        KeyCode::ShiftLeft => Key::Shift,
        KeyCode::ShiftRight => Key::ShiftR,
        KeyCode::ControlLeft => Key::CtrlL,
        KeyCode::ControlRight => Key::CtrlR,
        KeyCode::Escape => Key::Esc,
        KeyCode::Quote => Key::Apostrophe,
        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::Digit0 => Key::Zero,
        KeyCode::Digit1 => Key::One,
        KeyCode::Digit2 => Key::Two,
        KeyCode::Digit3 => Key::Three,
        KeyCode::Digit4 => Key::Four,
        KeyCode::Digit5 => Key::Five,
        KeyCode::Digit6 => Key::Six,
        KeyCode::Digit7 => Key::Seven,
        KeyCode::Digit8 => Key::Eight,
        KeyCode::Digit9 => Key::Nine,
        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,
        KeyCode::Minus => Key::Minus,
        KeyCode::Equal => Key::Equals,
        KeyCode::BracketLeft => Key::BracketL,
        KeyCode::BracketRight => Key::BracketR,
        KeyCode::Semicolon => Key::Semicolon,
        KeyCode::Slash => Key::Slash,
        KeyCode::Backslash => Key::Backslash,
        KeyCode::Period => Key::Point,
        KeyCode::Insert => Key::Ins,
        KeyCode::Delete => Key::Del,
        KeyCode::PrintScreen => Key::PrntScr,
        _ => Key::None,
    }
}
