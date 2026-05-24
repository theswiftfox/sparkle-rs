use std::ffi::{CStr, c_void};

use wgpu::rwh::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

use crate::engine::{
    backend::{GpuError, GpuErrorKind},
    settings::SyncMode,
};

pub fn get_instance_extensions(
    window: &winit::window::Window,
) -> Result<Vec<&'static CStr>, GpuError> {
    let raw_handle: RawWindowHandle = window
        .window_handle()
        .map(|rwh| rwh.as_raw())
        .map_err(|_| GpuError::new("Failed to create RawWindowHandle", GpuErrorKind::Other))?;

    match raw_handle {
        RawWindowHandle::AppKit(_) => Ok(vec![
            ash::vk::KHR_SURFACE_NAME,
            ash::vk::EXT_METAL_SURFACE_NAME,
        ]),
        RawWindowHandle::Xlib(_) => Ok(vec![
            ash::vk::KHR_SURFACE_NAME,
            ash::vk::KHR_XLIB_SURFACE_NAME,
        ]),
        RawWindowHandle::Xcb(_) => Ok(vec![
            ash::vk::KHR_SURFACE_NAME,
            ash::vk::KHR_XCB_SURFACE_NAME,
        ]),
        RawWindowHandle::Wayland(_) => Ok(vec![
            ash::vk::KHR_SURFACE_NAME,
            ash::vk::KHR_WAYLAND_SURFACE_NAME,
        ]),
        RawWindowHandle::Win32(_) => Ok(vec![
            ash::vk::KHR_SURFACE_NAME,
            ash::vk::KHR_WIN32_SURFACE_NAME,
        ]),
        RawWindowHandle::AndroidNdk(_) => Ok(vec![
            ash::vk::KHR_SURFACE_NAME,
            ash::vk::KHR_ANDROID_SURFACE_NAME,
        ]),
        _ => Err(GpuError::new(
            "Unsupported window handle type",
            GpuErrorKind::Other,
        )),
    }
}

pub unsafe extern "system" fn debug_callback(
    flags: ash::vk::DebugUtilsMessageSeverityFlagsEXT,
    type_: ash::vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const ash::vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::ffi::c_void,
) -> ash::vk::Bool32 {
    let severity = if flags.contains(ash::vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE) {
        "VERBOSE"
    } else if flags.contains(ash::vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        "INFO"
    } else if flags.contains(ash::vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        "WARNING"
    } else if flags.contains(ash::vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        "ERROR"
    } else {
        "UNKNOWN"
    };
    let msg_type = if type_.contains(ash::vk::DebugUtilsMessageTypeFlagsEXT::GENERAL) {
        "GENERAL"
    } else if type_.contains(ash::vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION) {
        "VALIDATION"
    } else if type_.contains(ash::vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE) {
        "PERFORMANCE"
    } else {
        "UNKNOWN"
    };
    let message = unsafe { CStr::from_ptr((*data).p_message) }.to_string_lossy();
    eprintln!("Vulkan Debug: [{severity}|{msg_type}] {message}");
    ash::vk::FALSE
}

pub fn create_surface(
    context: &ash::Entry,
    instance: &ash::Instance,
    window: &winit::window::Window,
) -> Result<ash::vk::SurfaceKHR, GpuError> {
    let raw_handle: RawWindowHandle = window
        .window_handle()
        .map(|rwh| rwh.as_raw())
        .map_err(|_| GpuError::new("Failed to create RawWindowHandle", GpuErrorKind::Other))?;

    match raw_handle {
        RawWindowHandle::Win32(handle) => {
            let hinstance = handle
                .hinstance
                .ok_or_else(|| {
                    GpuError::new("No valid hInstance set on the window", GpuErrorKind::Other)
                })?
                .get();
            let hwnd = handle.hwnd.get();
            let create_info = ash::vk::Win32SurfaceCreateInfoKHR {
                hinstance,
                hwnd,
                ..Default::default()
            };

            let win32_instance = ash::khr::win32_surface::Instance::new(context, instance);
            unsafe { win32_instance.create_win32_surface(&create_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to create Win32 surface: {e}"),
                    GpuErrorKind::Other,
                )
            })
        }
        RawWindowHandle::Xlib(handle) => {
            let RawDisplayHandle::Xlib(display_handle) =
                window.display_handle().map(|dh| dh.as_raw()).map_err(|_| {
                    GpuError::new("Failed to create RawDisplayHandle", GpuErrorKind::Other)
                })?
            else {
                return Err(GpuError::new(
                    "Got non xlib display handle for Xlib window",
                    GpuErrorKind::Other,
                ));
            };

            let dpy = display_handle
                .display
                .ok_or_else(|| {
                    GpuError::new("No valid display set on the window", GpuErrorKind::Other)
                })?
                .as_ptr();
            let window = handle.window;
            let create_info = ash::vk::XlibSurfaceCreateInfoKHR {
                dpy,
                window,
                ..Default::default()
            };
            let xlib_instance = ash::khr::xlib_surface::Instance::new(context, instance);
            unsafe { xlib_instance.create_xlib_surface(&create_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to create Xlib surface: {e}"),
                    GpuErrorKind::Other,
                )
            })
        }
        RawWindowHandle::Xcb(handle) => {
            let RawDisplayHandle::Xcb(xcb_display_handle) =
                window.display_handle().map(|dh| dh.as_raw()).map_err(|_| {
                    GpuError::new("Failed to create RawDisplayHandle", GpuErrorKind::Other)
                })?
            else {
                return Err(GpuError::new(
                    "Got non xcb display handle for Xcb window",
                    GpuErrorKind::Other,
                ));
            };
            let connection = xcb_display_handle
                .connection
                .ok_or_else(|| {
                    GpuError::new("No valid connection set on the window", GpuErrorKind::Other)
                })?
                .as_ptr() as *mut c_void;
            let window = handle.window.get();
            let create_info = ash::vk::XcbSurfaceCreateInfoKHR {
                connection,
                window,
                ..Default::default()
            };

            let xcb_instance = ash::khr::xcb_surface::Instance::new(context, instance);
            unsafe { xcb_instance.create_xcb_surface(&create_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to create Xcb surface: {e}"),
                    GpuErrorKind::Other,
                )
            })
        }
        RawWindowHandle::Wayland(handle) => {
            let RawDisplayHandle::Wayland(display_handle) =
                window.display_handle().map(|dh| dh.as_raw()).map_err(|_| {
                    GpuError::new("Failed to create RawDisplayHandle", GpuErrorKind::Other)
                })?
            else {
                return Err(GpuError::new(
                    "Got non Wayland display handle for Wayland window",
                    GpuErrorKind::Other,
                ));
            };
            let display = display_handle.display.as_ptr();
            let surface = handle.surface.as_ptr();

            let create_info = ash::vk::WaylandSurfaceCreateInfoKHR {
                display,
                surface,
                ..Default::default()
            };

            let wayland_instance = ash::khr::wayland_surface::Instance::new(context, instance);
            unsafe { wayland_instance.create_wayland_surface(&create_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to create Wayland surface: {e}"),
                    GpuErrorKind::Other,
                )
            })
        }
        RawWindowHandle::AndroidNdk(_) => {
            todo!()
        }
        RawWindowHandle::AppKit(_) => {
            todo!()
        }
        _ => Err(GpuError::new(
            "Unsupported window handle type",
            GpuErrorKind::Other,
        )),
    }
}

pub fn choose_swapchain_format(
    available_formats: &[ash::vk::SurfaceFormatKHR],
) -> Result<ash::vk::SurfaceFormatKHR, GpuError> {
    if available_formats.is_empty() {
        return Err(GpuError::new(
            "No available surface formats",
            GpuErrorKind::Other,
        ));
    }
    // Prefer BGRA8 with SRGB nonlinear color space, as it's widely supported and has good color accuracy
    Ok(available_formats
        .iter()
        .find(|f| {
            f.format == ash::vk::Format::B8G8R8A8_SRGB
                && f.color_space == ash::vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .cloned()
        .unwrap_or_else(|| {
            // Fallback to the first available format if the preferred one isn't found
            available_formats[0]
        }))
}

pub fn choose_present_mode(
    available_modes: &[ash::vk::PresentModeKHR],
    sync_mode: SyncMode,
) -> Result<ash::vk::PresentModeKHR, GpuError> {
    if available_modes.is_empty() {
        return Err(GpuError::new(
            "No available present modes",
            GpuErrorKind::Other,
        ));
    }

    let preferred_mode = match sync_mode {
        SyncMode::VSync => ash::vk::PresentModeKHR::FIFO, // Always supported, closest to vsync
        SyncMode::AdaptiveVSync => ash::vk::PresentModeKHR::MAILBOX, // closest adaptive VSync
        SyncMode::Immediate => ash::vk::PresentModeKHR::IMMEDIATE,
        SyncMode::Mailbox => ash::vk::PresentModeKHR::MAILBOX,
    };

    Ok(available_modes
        .iter()
        .find(|&&mode| mode == preferred_mode)
        .cloned()
        .unwrap_or_else(|| {
            println!(
                "Preferred present mode {:?} not available, falling back to FIFO",
                preferred_mode
            );
            // Fallback to FIFO if the preferred mode isn't found, as it's guaranteed to be supported
            ash::vk::PresentModeKHR::FIFO
        }))
}
