
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

pub trait Window {
    fn create_window(width: i32, height: i32, name: &str, title: &str) -> Self;
    fn update(&self) -> bool;

    fn get_width(&self) -> u32;
    fn get_height(&self) -> u32;
    
    #[cfg(target_os = "windows")]
    fn get_handle(&self) -> winapi::shared::windef::HWND;
}