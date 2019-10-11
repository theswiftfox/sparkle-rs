use crate::input::input_handler::{
    Action, ApplicationRequest, Button, InputHandler, Key, ScrollAxis,
};
use crate::utils;
use crate::window::Window;
use std::cell::RefCell;
use std::rc::Rc as shared_ptr;
use std::*;
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HWND, POINT, RECT};
use winapi::shared::windowsx::{GET_X_LPARAM, GET_Y_LPARAM};
use winapi::um::libloaderapi::*;
use winapi::um::winuser::*;

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let lpcs = lparam as LPCREATESTRUCTW;
        let ptr = (*lpcs).lpCreateParams as *mut WindowWin;
        if !ptr.is_null() {
            // (*ptr).handle = hwnd;
            let res = DefWindowProcW(hwnd, msg, wparam, lparam);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as LPARAM);
            return res;
        }
        return 0;
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowWin;
    if !ptr.is_null() {
        return (*ptr).window_proc(hwnd, msg, wparam, lparam);
    } else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
}

pub struct WindowWin {
    handle: HWND,
    width: u32,
    height: u32,
    input_handler: Option<std::rc::Rc<std::cell::RefCell<dyn InputHandler>>>,
    request_quit: bool,
    snap_mouse: bool,

    last_x: i32,
    last_y: i32,
}

impl Window for WindowWin {
    fn get_width(&self) -> u32 {
        self.width
    }
    fn get_height(&self) -> u32 {
        self.height
    }
    fn get_handle(&self) -> winapi::shared::windef::HWND {
        self.handle
    }

    fn create_window(
        width: i32,
        height: i32,
        name: &str,
        title: &str,
    ) -> shared_ptr<RefCell<WindowWin>> {
        let wnd = shared_ptr::new(RefCell::new(WindowWin {
            handle: ptr::null_mut(),
            width: width as u32,
            height: height as u32,
            input_handler: None,
            request_quit: false,
            snap_mouse: false,
            last_x: std::i32::MIN,
            last_y: std::i32::MIN,
        }));
        wnd.borrow_mut().create(width, height, name, title);
        return wnd;
    }
    fn update(&mut self) -> bool {
        if self.request_quit {
            return false;
        }
        unsafe {
            let mut msg: MSG = mem::MaybeUninit::<MSG>::uninit().assume_init();
            while PeekMessageW(&mut msg, self.handle, 0, 0, PM_REMOVE) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if self.snap_mouse && self.input_handler.is_some() {
                let mut rect: RECT = Default::default();
                if GetWindowRect(self.handle, &mut rect as *mut _) != 1 {
                    println!("Error getting Window Rectangle");
                    return false; // todo: maybe log instead or smth?
                }
                let cx = (rect.left + rect.right) / 2;
                let cy = (rect.top + rect.bottom) / 2;

                let mut pos: POINT = Default::default();
                if GetCursorPos(&mut pos as *mut _) == 1 {
                    let x = pos.x - cx;
                    let y = pos.y - cy;

                    self.input_handler
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .handle_mouse_move(x, y);

                    SetCursorPos(cx, cy);
                }
            }
        }
        true
    }
    fn set_input_handler(&mut self, handler: std::rc::Rc<std::cell::RefCell<dyn InputHandler>>) {
        self.input_handler = Some(handler.clone())
    }
}
impl WindowWin {
    fn create(&mut self, width: i32, height: i32, name: &str, title: &str) {
        let name = utils::to_wide_str(name);
        let title = utils::to_wide_str(title);

        unsafe {
            let instance = GetModuleHandleW(ptr::null_mut());
            let window_class = WNDCLASSW {
                style: CS_OWNDC | CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: name.as_ptr(),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hIcon: ptr::null_mut(),
                hCursor: ptr::null_mut(),
                hbrBackground: ptr::null_mut(),
                lpszMenuName: ptr::null_mut(),
            };

            RegisterClassW(&window_class);
            let handle = CreateWindowExW(
                0,
                name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width,
                height,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                (self as *mut WindowWin) as *mut _,
            );

            if handle.is_null() {
                panic!("Unable to obtain window handle!")
            }
            self.handle = handle;
        }
    }

    fn window_proc(&mut self, hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if self.input_handler.is_none() {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        } else {
            let handler = self.input_handler.as_ref().unwrap().clone();
            let was_snapped = self.snap_mouse;
            match msg {
                WM_DESTROY => {
                    unsafe { PostQuitMessage(0) };
                    self.request_quit = true;
                    return 0;
                }
                WM_SIZE => {
                    return 0;
                }
                WM_KEYDOWN => {
                    match handler
                        .borrow_mut()
                        .handle_key(WindowWin::get_sparkle_key(wparam), Action::Down)
                    {
                        ApplicationRequest::Quit => {
                            unsafe { PostQuitMessage(0) };
                            self.request_quit = true;
                        }
                        _ => {}
                    };
                }
                WM_KEYUP => {
                    match handler
                        .borrow_mut()
                        .handle_key(WindowWin::get_sparkle_key(wparam), Action::Up)
                    {
                        ApplicationRequest::Quit => {
                            unsafe { PostQuitMessage(0) };
                            self.request_quit = true;
                        }
                        _ => {}
                    };
                }
                WM_LBUTTONDOWN => {
                    match handler
                        .borrow_mut()
                        .handle_mouse(Button::Left, Action::Down)
                    {
                        ApplicationRequest::SnapMouse => self.snap_mouse = true,
                        ApplicationRequest::UnsnapMouse => self.snap_mouse = false,
                        _ => {}
                    };
                }
                WM_LBUTTONUP => {
                    match handler.borrow_mut().handle_mouse(Button::Left, Action::Up) {
                        ApplicationRequest::SnapMouse => self.snap_mouse = true,
                        ApplicationRequest::UnsnapMouse => self.snap_mouse = false,
                        _ => {}
                    };
                }
                WM_MBUTTONDOWN => {
                    match handler
                        .borrow_mut()
                        .handle_mouse(Button::Middle, Action::Down)
                    {
                        ApplicationRequest::SnapMouse => self.snap_mouse = true,
                        ApplicationRequest::UnsnapMouse => self.snap_mouse = false,
                        _ => {}
                    };
                }
                WM_MBUTTONUP => {
                    match handler
                        .borrow_mut()
                        .handle_mouse(Button::Middle, Action::Up)
                    {
                        ApplicationRequest::SnapMouse => self.snap_mouse = true,
                        ApplicationRequest::UnsnapMouse => self.snap_mouse = false,
                        _ => {}
                    };
                }
                WM_RBUTTONDOWN => {
                    match handler
                        .borrow_mut()
                        .handle_mouse(Button::Right, Action::Down)
                    {
                        ApplicationRequest::SnapMouse => self.snap_mouse = true,
                        ApplicationRequest::UnsnapMouse => self.snap_mouse = false,
                        _ => {}
                    };
                }
                WM_RBUTTONUP => {
                    match handler.borrow_mut().handle_mouse(Button::Right, Action::Up) {
                        ApplicationRequest::SnapMouse => self.snap_mouse = true,
                        ApplicationRequest::UnsnapMouse => self.snap_mouse = false,
                        _ => {}
                    };
                }
                WM_MOUSEWHEEL => {
                    handler.borrow_mut().handle_wheel(
                        ScrollAxis::Vertical,
                        f32::from(GET_WHEEL_DELTA_WPARAM(wparam)) / 5.0f32,
                    );
                }
                WM_MOUSEHWHEEL => {
                    handler.borrow_mut().handle_wheel(
                        ScrollAxis::Horizontal,
                        f32::from(GET_WHEEL_DELTA_WPARAM(wparam)) / 5.0f32,
                    );
                }
                WM_MOUSEMOVE => {
                    if self.snap_mouse {
                        return 0;
                    }

                    let wx = GET_X_LPARAM(lparam);
                    let wy = GET_Y_LPARAM(lparam);

                    if self.last_x != std::i32::MIN && self.last_y != std::i32::MIN {
                        let x = wx - self.last_x;
                        let y = wy - self.last_y;
                        //println!("x({}) y({}), px({}) py({}) Dx({}) Dy({})", wx, wy, self.last_x, self.last_y, x, y);
                        handler.borrow_mut().handle_mouse_move(x, y);
                    }
                    self.last_x = wx;
                    self.last_y = wy;
                }
                _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            }
            if self.snap_mouse && !was_snapped {
                unsafe {
                    ShowCursor(0);
                    // ClipCursor(&rect as *const _);
                    //SetCapture(self.handle);
                };
            } else if !self.snap_mouse && was_snapped {
                unsafe {
                    ShowCursor(1);
                    // if ReleaseCapture() != 1 {
                    //     return -1;
                    // }
                    // ClipCursor(ptr::null());
                };
            }
            return 0;
        }
    }

    fn get_sparkle_key(wparam: WPARAM) -> Key {
        match wparam as i32 {
            VK_LEFT => Key::KeyLeft,
            VK_RIGHT => Key::KeyRight,
            VK_UP => Key::KeyUp,
            VK_DOWN => Key::KeyDown,
            VK_SPACE => Key::Space,
            VK_BACK => Key::Backspace,
            VK_SHIFT => Key::Shift,
            VK_CONTROL => Key::CtrlL,
            VK_ESCAPE => Key::Esc,
            VK_F1 => Key::F1,
            VK_F2 => Key::F2,
            VK_F3 => Key::F3,
            VK_F4 => Key::F4,
            VK_F5 => Key::F5,
            VK_F6 => Key::F6,
            VK_F7 => Key::F7,
            VK_F8 => Key::F8,
            VK_F9 => Key::F9,
            VK_F10 => Key::F10,
            VK_F11 => Key::F11,
            VK_F12 => Key::F12,
            0x30 => Key::Zero,
            0x31 => Key::One,
            0x32 => Key::Two,
            0x33 => Key::Three,
            0x34 => Key::Four,
            0x35 => Key::Five,
            0x36 => Key::Six,
            0x37 => Key::Seven,
            0x38 => Key::Eight,
            0x39 => Key::Nine,
            0x41 => Key::A,
            0x42 => Key::B,
            0x43 => Key::C,
            0x44 => Key::D,
            0x45 => Key::E,
            0x46 => Key::F,
            0x47 => Key::G,
            0x48 => Key::H,
            0x49 => Key::I,
            0x4A => Key::J,
            0x4B => Key::K,
            0x4C => Key::L,
            0x4D => Key::M,
            0x4E => Key::N,
            0x4F => Key::O,
            0x50 => Key::P,
            0x51 => Key::Q,
            0x52 => Key::R,
            0x53 => Key::S,
            0x54 => Key::T,
            0x55 => Key::U,
            0x56 => Key::V,
            0x57 => Key::W,
            0x58 => Key::X,
            0x59 => Key::Y,
            0x5A => Key::Z,
            _ => Key::None,
        }
    }
}
