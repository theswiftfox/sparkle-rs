use std::*;
use winapi::shared::windef::{HWND};
use winapi::um::libloaderapi::*;
use winapi::um::winuser::*;
use super::utils as utils;

pub struct Window {
    pub handle: HWND,
    pub width: u32,
    pub height: u32
}

impl Window {
    pub fn create_window(width: i32, height: i32, name: &str, title: &str) -> Result<Window, &'static str> {
        let name = utils::to_wide_str(name);
        let title = utils::to_wide_str(title);

        unsafe {
            let instance = GetModuleHandleW(ptr::null_mut());
            let window_class = WNDCLASSW {
                style: CS_OWNDC | CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some( DefWindowProcW ),
                hInstance: instance,
                lpszClassName: name.as_ptr(),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hIcon: ptr::null_mut(),
                hCursor: ptr::null_mut(),
                hbrBackground: ptr::null_mut(),
                lpszMenuName: ptr::null_mut()
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
                ptr::null_mut());

            if handle.is_null() {
                Err( "Unable to obtain window handle!" )
            } else {
                Ok( Window { handle: handle, width: width as u32, height: height as u32 } )
            }
        }
    }

    pub fn update(&self) -> bool {
        unsafe {
            let mut msg : MSG =  mem::uninitialized(); 
            if GetMessageW(&mut msg, self.handle, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);

                return true
            }
        }
        false
    }
}
