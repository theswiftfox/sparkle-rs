use crate::controller::input_handler::{Action, Button, Direction, InputHandler, Key};
use crate::utils;
use crate::window::Window;
use std::cell::RefCell;
use std::rc::Rc as shared_ptr;
use std::*;
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::HWND;
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
        }));
        wnd.borrow_mut().create(width, height, name, title);
        return wnd;
    }
    fn update(&self) -> bool {
        unsafe {
            let mut msg: MSG = mem::uninitialized();
            if GetMessageW(&mut msg, self.handle, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);

                return true;
            }
        }
        false
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
    fn window_proc(&self, hwnd: HWND, msg: UINT, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if self.input_handler.is_none() {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        } else {
            let handler = self.input_handler.as_ref().unwrap().borrow();
            match msg {
                WM_DESTROY => {
                    unsafe { PostQuitMessage(0) };
                    return 0;
                }
                WM_SIZE => {
                    return 0;
                }
                WM_KEYDOWN => {
                    handler.handle_key(WindowWin::get_sparkle_key(wparam), Action::Down);
                    return 0;
                }
                WM_KEYUP => {
                    handler.handle_key(WindowWin::get_sparkle_key(wparam), Action::Up);
                    return 0;
                }
                WM_LBUTTONDOWN => {
                    return 0;
                }
                WM_LBUTTONUP => {
                    return 0;
                }
                WM_MBUTTONDOWN => {
                    return 0;
                }
                WM_MBUTTONUP => {
                    return 0;
                }
                WM_RBUTTONDOWN => {
                    return 0;
                }
                WM_RBUTTONUP => {
                    return 0;
                }
                WM_MOUSEWHEEL => {
                    return 0;
                }
                WM_MOUSEMOVE => {
                    return 0;
                }
                _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            }
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
