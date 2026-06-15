use std::ffi::{CStr, c_void};

use winit::raw_window_handle::{
    HasDisplayHandle as _, HasWindowHandle as _, RawDisplayHandle, RawWindowHandle,
};

use crate::engine::{
    backend::{CompareFunc, CullMode, GpuError, GpuErrorKind, LoadOp, VertexFormat, ViewportDesc},
    settings::SyncMode,
    vulkan_backend::{SurfaceFormat, Swapchain, VulkanBackend, create_swapchain_and_depth_buffer},
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
    hdr_preferred: bool,
) -> Result<SurfaceFormat, GpuError> {
    const HDR_COLOR_SPACES: [ash::vk::ColorSpaceKHR; 2] = [
        ash::vk::ColorSpaceKHR::HDR10_HLG_EXT,
        ash::vk::ColorSpaceKHR::HDR10_ST2084_EXT,
    ];
    const HDR_COLOR_FORMAT: ash::vk::Format = ash::vk::Format::A2B10G10R10_UNORM_PACK32;
    const DEFAULT_COLOR_SPACE: ash::vk::ColorSpaceKHR = ash::vk::ColorSpaceKHR::SRGB_NONLINEAR;
    const DEFAULT_COLOR_FORMAT: ash::vk::Format = ash::vk::Format::B8G8R8A8_SRGB;

    if available_formats.is_empty() {
        return Err(GpuError::new(
            "No available surface formats",
            GpuErrorKind::Other,
        ));
    }
    // as first fallback take BGRA8 with SRGB nonlinear color space, as it's widely supported and has good color accuracy
    // if none match take the first available format

    let (hdr_formats, sdr_formats): (Vec<_>, Vec<_>) = available_formats
        .iter()
        .filter_map(|f| {
            if f.format != DEFAULT_COLOR_FORMAT && f.format != HDR_COLOR_FORMAT {
                return None;
            }
            println!("checking format: {f:?}");
            if hdr_preferred
                && f.format == HDR_COLOR_FORMAT
                && HDR_COLOR_SPACES.contains(&f.color_space)
            {
                return Some((*f, true));
            } else if f.format == DEFAULT_COLOR_FORMAT
                && f.color_space == ash::vk::ColorSpaceKHR::SRGB_NONLINEAR
            {
                return Some((*f, false));
            }
            return None;
        })
        .partition(|(_, is_hdr)| *is_hdr);
    println!(
        "Filtered formats:\n hdr_preferred: {hdr_preferred}\n HDR: {:?}\n SDR: {:?}",
        hdr_formats, sdr_formats
    );

    if hdr_preferred && let Some((format, is_hdr)) = hdr_formats.first().cloned() {
        Ok(SurfaceFormat { format, is_hdr })
    } else if let Some((format, is_hdr)) = sdr_formats.first().cloned() {
        Ok(SurfaceFormat { format, is_hdr })
    } else {
        let format = available_formats[0];
        eprintln!("No preferred surface found, falling back to {:?}", format);
        Ok(SurfaceFormat {
            format,
            is_hdr: false,
        })
    }
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

pub fn choose_swap_extent(
    capabilities: &ash::vk::SurfaceCapabilitiesKHR,
    window: &winit::window::Window,
) -> ash::vk::Extent2D {
    if capabilities.current_extent.width != u32::MAX {
        // The surface has specified a fixed size, so we must use it
        capabilities.current_extent
    } else {
        // The surface allows us to choose the extent, so we use the window size clamped to the allowed range
        // let winit::dpi::LogicalSize::<u32> { width, height } =
        //     window.inner_size().to_logical(window.scale_factor());
        let winit::dpi::PhysicalSize::<u32> { width, height } = window.inner_size();
        ash::vk::Extent2D {
            width: width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    }
}

pub fn choose_swap_min_image_count(capabilities: &ash::vk::SurfaceCapabilitiesKHR) -> u32 {
    let min_image_count = capabilities.min_image_count.max(3); // Prefer triple buffering if supported
    if capabilities.max_image_count > 0 && min_image_count > capabilities.max_image_count {
        println!(
            "Requested swapchain image count {} exceeds the maximum supported {}, falling back to max",
            min_image_count, capabilities.max_image_count
        );
        capabilities.max_image_count
    } else {
        min_image_count
    }
}

pub fn load_shader_blob(path: impl AsRef<std::path::Path>) -> Result<Vec<u8>, GpuError> {
    std::fs::read(path).map_err(|e| {
        GpuError::new(
            format!("Failed to read shader SPIR-V blob: {e}"),
            GpuErrorKind::Other,
        )
    })
}

impl VulkanBackend {
    pub fn create_image(
        instance: &ash::Instance,
        device: &ash::Device,
        phys_device: ash::vk::PhysicalDevice,
        width: u32,
        height: u32,
        format: ash::vk::Format,
        mip_levels: u32,
        tiling: ash::vk::ImageTiling,
        usage: ash::vk::ImageUsageFlags,
        properties: ash::vk::MemoryPropertyFlags,
    ) -> Result<(ash::vk::Image, ash::vk::DeviceMemory), GpuError> {
        let create_info = ash::vk::ImageCreateInfo {
            image_type: ash::vk::ImageType::TYPE_2D,
            format,
            extent: ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            mip_levels,
            array_layers: 1,
            samples: ash::vk::SampleCountFlags::TYPE_1,
            tiling,
            usage,
            sharing_mode: ash::vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };

        let image = unsafe { device.create_image(&create_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create image resource: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        let mem_reqs = unsafe { device.get_image_memory_requirements(image) };
        let alloc_info = ash::vk::MemoryAllocateInfo {
            allocation_size: mem_reqs.size,
            memory_type_index: Self::find_memory_type(
                instance,
                phys_device,
                mem_reqs.memory_type_bits,
                properties,
            )?,
            ..Default::default()
        };
        let device_mem = unsafe { device.allocate_memory(&alloc_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to allocate Image Memory: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;
        unsafe { device.bind_image_memory(image, device_mem, 0) }.map_err(|e| {
            GpuError::new(
                format!("Failed to bind memory to image: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        Ok((image, device_mem))
    }
    pub fn create_buffer(
        instance: &ash::Instance,
        device: &ash::Device,
        phys_device: ash::vk::PhysicalDevice,
        device_size: ash::vk::DeviceSize,
        usage: ash::vk::BufferUsageFlags,
        properties: ash::vk::MemoryPropertyFlags,
    ) -> Result<(ash::vk::Buffer, ash::vk::DeviceMemory), GpuError> {
        let create_info = ash::vk::BufferCreateInfo {
            size: device_size,
            usage,
            sharing_mode: ash::vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let buffer = unsafe { device.create_buffer(&create_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create buffer with size {device_size}, usage {usage:?}: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        let mem_reqs = unsafe { device.get_buffer_memory_requirements(buffer) };
        let alloc_info = ash::vk::MemoryAllocateInfo {
            allocation_size: mem_reqs.size,
            memory_type_index: Self::find_memory_type(
                instance,
                phys_device,
                mem_reqs.memory_type_bits,
                properties,
            )?,
            ..Default::default()
        };
        let device_mem = unsafe { device.allocate_memory(&alloc_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to allocate buffer: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        unsafe { device.bind_buffer_memory(buffer, device_mem, 0) }.map_err(|e| {
            GpuError::new(
                format!("Failed to bind buffer to memory: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        Ok((buffer, device_mem))
    }

    pub fn find_memory_type(
        instance: &ash::Instance,
        phys_device: ash::vk::PhysicalDevice,
        type_filter: u32,
        properties: ash::vk::MemoryPropertyFlags,
    ) -> Result<u32, GpuError> {
        let mut mem_props = ash::vk::PhysicalDeviceMemoryProperties2 {
            ..Default::default()
        };
        unsafe {
            instance.get_physical_device_memory_properties2(phys_device, &mut mem_props);
        }
        // find memory type where the type_filter bit is set to 1
        for i in 0..mem_props.memory_properties.memory_type_count {
            let props = mem_props.memory_properties.memory_types[i as usize];
            let flags_match = type_filter & (1 << i) != 0;
            let props_contained = props.property_flags & properties == properties;

            // println!(
            //     "Checking memory at [{i}]:
            //         flags: {:?} [{:?}]
            //         filter_flags_check: {}",
            //     props.property_flags, props_contained, flags_match,
            // );
            if flags_match && props_contained {
                return Ok(i);
            }
        }

        Err(GpuError {
            message: format!(
                "No suitable memory type found for filter {type_filter:#034b} and flags {properties:?}"
            ),
            kind: GpuErrorKind::ResourceCreation,
        })
    }

    pub fn begin_single_time_commands(&self) -> Result<ash::vk::CommandBuffer, GpuError> {
        let alloc_info = ash::vk::CommandBufferAllocateInfo {
            command_pool: self.command_pool.short_lived,
            command_buffer_count: 1,
            level: ash::vk::CommandBufferLevel::PRIMARY,
            ..Default::default()
        };
        let command_buffer = unsafe { self.device.allocate_command_buffers(&alloc_info) }
            .map_err(|e| {
                GpuError::new(
                    format!("Failed to allocate single use command buffer: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })
            .and_then(|buffs| {
                let buff = buffs.first().ok_or_else(|| {
                    GpuError::new(
                        "Allocate command buffers returned empty result",
                        GpuErrorKind::ResourceCreation,
                    )
                })?;
                Ok(*buff)
            })?;

        let begin_info = ash::vk::CommandBufferBeginInfo {
            flags: ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
            ..Default::default()
        };

        unsafe {
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to begin one time command buffer: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        Ok(command_buffer)
    }

    pub fn end_single_time_commands(
        &self,
        command_buffer: ash::vk::CommandBuffer,
    ) -> Result<(), GpuError> {
        unsafe { self.device.end_command_buffer(command_buffer) }.map_err(|e| {
            GpuError::new(
                format!("Failed to end one time command buffer: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        let submit_info = ash::vk::SubmitInfo {
            command_buffer_count: 1,
            p_command_buffers: &command_buffer,
            ..Default::default()
        };

        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], ash::vk::Fence::null())
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to submit one time command buffer: {e:?}"),
                GpuErrorKind::Other,
            )
        })?;

        unsafe { self.device.queue_wait_idle(self.queue) }.map_err(|e| {
            GpuError::new(
                format!("Failed to wait for completeion of one time command buffer: {e:?}"),
                GpuErrorKind::Other,
            )
        })?;

        unsafe {
            self.device
                .free_command_buffers(self.command_pool.short_lived, &[command_buffer]);
        }
        Ok(())
    }

    pub fn copy_buffer_cmd(
        &self,
        src: ash::vk::Buffer,
        src_offset: ash::vk::DeviceSize,
        dst: ash::vk::Buffer,
        dst_offset: ash::vk::DeviceSize,
        size: ash::vk::DeviceSize,
    ) -> Result<(), GpuError> {
        let cmd_buff = self.begin_single_time_commands()?;

        self.copy_buffer(cmd_buff, src, src_offset, dst, dst_offset, size);

        self.end_single_time_commands(cmd_buff)
    }

    pub fn copy_buffer(
        &self,
        command_buffer: ash::vk::CommandBuffer,
        src: ash::vk::Buffer,
        src_offset: ash::vk::DeviceSize,
        dst: ash::vk::Buffer,
        dst_offset: ash::vk::DeviceSize,
        size: ash::vk::DeviceSize,
    ) {
        unsafe {
            self.device.cmd_copy_buffer(
                command_buffer,
                src,
                dst,
                &[ash::vk::BufferCopy {
                    size,
                    src_offset,
                    dst_offset,
                }],
            );
        }
    }

    pub fn copy_buffer_to_image(
        &self,
        command_buffer: ash::vk::CommandBuffer,
        src: ash::vk::Buffer,
        src_offset: ash::vk::DeviceSize,
        dst: ash::vk::Image,
        width: u32,
        height: u32,
        base_array_layer: u32,
        layer_count: u32,
    ) {
        let copy_region = ash::vk::BufferImageCopy {
            buffer_offset: src_offset,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: ash::vk::ImageSubresourceLayers {
                aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer,
                layer_count,
            },
            image_offset: ash::vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            },
        };

        unsafe {
            self.device.cmd_copy_buffer_to_image(
                command_buffer,
                src,
                dst,
                ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy_region],
            );
        }
    }

    pub fn copy_to_buffer(
        &self,
        staging_mem: ash::vk::DeviceMemory,
        data: *const c_void,
        size: ash::vk::DeviceSize,
    ) -> Result<(), GpuError> {
        let data_ptr = unsafe {
            self.device
                .map_memory(staging_mem, 0, size, ash::vk::MemoryMapFlags::empty())
        }
        .map_err(|e| {
            GpuError::new(
                format!("Map device memory failed: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;
        unsafe {
            data_ptr.copy_from(data, size as usize);
        }
        unsafe {
            self.device.unmap_memory(staging_mem);
        }

        Ok(())
    }

    pub fn recreate_swapchain(&mut self) -> Result<(), GpuError> {
        unsafe { self.device.device_wait_idle() }.map_err(|e| {
            GpuError::new(
                format!("Failed to wait for device idle during swapchain recreation: {e:?}"),
                GpuErrorKind::Other,
            )
        })?;

        let mut old = Swapchain {
            swapchain: ash::vk::SwapchainKHR::null(),
            swapchain_images: Vec::new(),
            swapchain_extent: ash::vk::Extent2D::default(),
            fn_ptr: self.swapchain.fn_ptr.clone(),
            surface_format: SurfaceFormat::default(),
            surface: ash::vk::SurfaceKHR::null(),
            sync_mode: self.swapchain.sync_mode,
        };
        std::mem::swap(&mut old, &mut self.swapchain);

        let (mut new_swapchain, mut new_depth) = create_swapchain_and_depth_buffer(
            &self.context,
            &self.instance,
            &self.window,
            self.phys_device,
            &self.device,
            old.surface,
            old.sync_mode,
            old.surface_format.is_hdr,
        )?;

        std::mem::swap(&mut self.swapchain, &mut new_swapchain);
        std::mem::swap(&mut self.depth_targets, &mut new_depth);

        let Swapchain {
            fn_ptr,
            swapchain,
            swapchain_images,
            ..
        } = new_swapchain;

        // cleanup the old data
        unsafe {
            fn_ptr.destroy_swapchain(swapchain, None);
        }
        for image in swapchain_images {
            image.destroy();
        }
        for depth in new_depth {
            depth.destroy();
        }

        Ok(())
    }
}

pub fn gpu_error_out_of_range(resource_name: &str, idx: usize, len: usize) -> GpuError {
    GpuError::new(
        format!("Index {idx} outside of range for {resource_name} with length {len}"),
        GpuErrorKind::ResourceUpdate,
    )
}

impl Into<ash::vk::AttachmentLoadOp> for LoadOp {
    fn into(self) -> ash::vk::AttachmentLoadOp {
        match self {
            LoadOp::Clear => ash::vk::AttachmentLoadOp::CLEAR,
            LoadOp::Load => ash::vk::AttachmentLoadOp::LOAD,
        }
    }
}

impl Into<ash::vk::Viewport> for ViewportDesc {
    fn into(self) -> ash::vk::Viewport {
        ash::vk::Viewport {
            x: self.x,
            y: self.y + self.height,
            width: self.width,
            height: -self.height,
            min_depth: self.min_depth,
            max_depth: self.max_depth,
        }
    }
}
impl Into<ash::vk::Viewport> for &ViewportDesc {
    fn into(self) -> ash::vk::Viewport {
        (*self).into()
    }
}

impl Into<ash::vk::Format> for VertexFormat {
    fn into(self) -> ash::vk::Format {
        match self {
            VertexFormat::Float32x2 => ash::vk::Format::R32G32_SFLOAT,
            VertexFormat::Float32x3 => ash::vk::Format::R32G32B32_SFLOAT,
            VertexFormat::Float32x4 => ash::vk::Format::R32G32B32A32_SFLOAT,
        }
    }
}
impl Into<ash::vk::CompareOp> for CompareFunc {
    fn into(self) -> ash::vk::CompareOp {
        match self {
            CompareFunc::Never => ash::vk::CompareOp::NEVER,
            CompareFunc::Less => ash::vk::CompareOp::LESS,
            CompareFunc::LessEqual => ash::vk::CompareOp::LESS_OR_EQUAL,
            CompareFunc::Equal => ash::vk::CompareOp::EQUAL,
            CompareFunc::GreaterEqual => ash::vk::CompareOp::GREATER_OR_EQUAL,
            CompareFunc::Greater => ash::vk::CompareOp::GREATER,
            CompareFunc::Always => ash::vk::CompareOp::ALWAYS,
        }
    }
}

impl Into<ash::vk::CullModeFlags> for CullMode {
    fn into(self) -> ash::vk::CullModeFlags {
        match self {
            CullMode::None => ash::vk::CullModeFlags::NONE,
            CullMode::Front => ash::vk::CullModeFlags::FRONT,
            CullMode::Back => ash::vk::CullModeFlags::BACK,
        }
    }
}
