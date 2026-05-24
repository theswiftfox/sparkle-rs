use std::{
    ffi::{CStr, c_char},
    ops::Deref,
    sync::Arc,
};

use crate::engine::{
    backend::{GpuError, GpuErrorKind},
    settings::Settings,
};

mod util;

const VALIDATION_LAYER: &CStr =
    unsafe { CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0") };

struct LogicalDevice {
    device: ash::Device,
    graphics_queue_index: u32,
}

impl LogicalDevice {
    fn get_graphics_queue(&self) -> ash::vk::Queue {
        unsafe { self.device.get_device_queue(self.graphics_queue_index, 0) }
    }
}

impl Deref for LogicalDevice {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

pub fn initialize(window: Arc<winit::window::Window>, settings: &Settings) -> Result<(), GpuError> {
    let enable_validation = settings.gpu_validation;
    let sync_mode = settings.sync_mode;

    let context = unsafe { ash::Entry::load() }
        .map_err(|_| GpuError::new("Failed to load Vulkan entry", GpuErrorKind::Other))?;

    let instance = create_instance(&context, &window, enable_validation)?;

    let debug_messenger = if enable_validation {
        Some(setup_debug_messenger(&context, &instance)?)
    } else {
        None
    };

    let physical_device = get_physical_device(&instance)?;

    let surface = util::create_surface(&context, &instance, &window)?;

    let logical_device = create_logical_device(&context, &instance, physical_device, surface)?;

    let queue = logical_device.get_graphics_queue();

    todo!()
}

fn create_swapchain(
    context: &ash::Entry,
    instance: &ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
    surface: ash::vk::SurfaceKHR,
    logical_device: &LogicalDevice,
) -> Result<(), GpuError> {
    let surface_khr = ash::khr::surface::Instance::new(context, instance);

    let surface_capababilities = unsafe {
        surface_khr
            .get_physical_device_surface_capabilities(physical_device, surface)
            .map_err(|e| {
                GpuError::new(
                    format!("Failed to get surface capabilities: {:?}", e),
                    GpuErrorKind::Other,
                )
            })?
    };
    let surface_formats = unsafe {
        surface_khr
            .get_physical_device_surface_formats(physical_device, surface)
            .map_err(|e| {
                GpuError::new(
                    format!("Failed to get surface formats: {:?}", e),
                    GpuErrorKind::Other,
                )
            })?
    };
    let present_modes = unsafe {
        surface_khr
            .get_physical_device_surface_present_modes(physical_device, surface)
            .map_err(|e| {
                GpuError::new(
                    format!("Failed to get present modes: {:?}", e),
                    GpuErrorKind::Other,
                )
            })?
    };

    todo!()
}

fn create_logical_device(
    context: &ash::Entry,
    instance: &ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
    surface: ash::vk::SurfaceKHR,
) -> Result<LogicalDevice, GpuError> {
    let queue_fam_props =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

    let khr_instance = ash::khr::surface::Instance::new(context, instance);

    let Some((idx, _)) = queue_fam_props.iter().enumerate().find(|(idx, q)| {
        let idx = *idx as u32;
        let surface_support = match unsafe {
            khr_instance.get_physical_device_surface_support(physical_device, idx, surface)
        } {
            Ok(support) => support,
            Err(e) => {
                println!(
                    "Failed to query surface support for queue family {}: {:?}, skipping",
                    idx, e
                );
                return false;
            }
        };
        q.queue_flags.contains(ash::vk::QueueFlags::GRAPHICS) && surface_support
    }) else {
        return Err(GpuError::new(
            "Selected physical device does not have graphics queue family with present support",
            GpuErrorKind::Other,
        ));
    };

    let prio = 0.5f32;
    let queue_info = ash::vk::DeviceQueueCreateInfo {
        queue_family_index: idx as u32,
        p_queue_priorities: &prio,
        ..Default::default()
    };

    let mut ext_state_struct = ash::vk::PhysicalDeviceExtendedDynamicStateFeaturesEXT {
        extended_dynamic_state: ash::vk::TRUE,
        ..Default::default()
    };
    let mut vk_13_feats = ash::vk::PhysicalDeviceVulkan13Features {
        dynamic_rendering: ash::vk::TRUE,
        p_next: &mut ext_state_struct as *mut _ as *mut std::ffi::c_void,
        ..Default::default()
    };
    let mut base_struct = ash::vk::PhysicalDeviceFeatures2 {
        p_next: &mut vk_13_feats as *mut _ as *mut std::ffi::c_void,
        ..Default::default()
    };
    let device_exts = [ash::vk::KHR_SWAPCHAIN_NAME];
    let device_exts_ptr = device_exts
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();

    let create_info = ash::vk::DeviceCreateInfo {
        p_next: &mut base_struct as *mut _ as *mut std::ffi::c_void,
        queue_create_info_count: 1,
        p_queue_create_infos: &queue_info,
        enabled_extension_count: device_exts.len() as u32,
        pp_enabled_extension_names: device_exts_ptr.as_ptr(),
        ..Default::default()
    };

    let device =
        unsafe { instance.create_device(physical_device, &create_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create logical device: {:?}", e),
                GpuErrorKind::DeviceCreation,
            )
        })?;
    Ok(LogicalDevice {
        device,
        graphics_queue_index: idx as u32,
    })
}

fn get_physical_device(instance: &ash::Instance) -> Result<ash::vk::PhysicalDevice, GpuError> {
    let devices = unsafe { instance.enumerate_physical_devices() }.map_err(|e| {
        GpuError::new(
            format!("Failed to enumerate physical devices: {:?}", e),
            GpuErrorKind::Other,
        )
    })?;
    if devices.is_empty() {
        return Err(GpuError::new(
            "No Vulkan-compatible physical devices found",
            GpuErrorKind::Other,
        ));
    }

    // pick device. prefer discrete GPU. remove devices that don't support required features
    let evaluated_devices = devices
        .iter()
        .filter_map(|device| {
            let features = unsafe { instance.get_physical_device_features(*device) };
            let has_required_features = features.geometry_shader == ash::vk::TRUE;

            if !has_required_features {
                println!(
                    "Physical device {:?} does not support required features, skipping",
                    device
                );
                return None;
            }

            let properties = unsafe { instance.get_physical_device_properties(*device) };

            // require vulkan 1.3
            if properties.api_version < ash::vk::API_VERSION_1_3 {
                println!(
                    "Physical device {:?} does not support Vulkan 1.3), skipping",
                    device,
                );
                return None;
            }
            // we need graphics queue family
            let queue_families =
                unsafe { instance.get_physical_device_queue_family_properties(*device) };
            if !queue_families
                .iter()
                .any(|q| q.queue_flags.contains(ash::vk::QueueFlags::GRAPHICS))
            {
                println!(
                    "Physical device {:?} does not have graphics queue family, skipping",
                    device,
                );
                return None;
            }

            // check the device supports all extensions we want
            let device_exts = unsafe {
                instance
                    .enumerate_device_extension_properties(*device)
                    .unwrap_or_default()
            };
            let required_exts = [ash::vk::KHR_SWAPCHAIN_NAME];
            if let Some(ext) = required_exts.iter().find(|ext| {
                !device_exts.iter().any(|prop| {
                    let extension_name = unsafe { CStr::from_ptr(prop.extension_name.as_ptr()) };
                    extension_name == **ext
                })
            }) {
                println!(
                    "Physical device {:?} does not support required extension {}, skipping",
                    device,
                    ext.to_string_lossy()
                );
                return None;
            }

            // check additional required features
            // check for extended dynamic state
            let mut features_ext_state: ash::vk::PhysicalDeviceExtendedDynamicStateFeaturesEXT =
                Default::default();
            // dynamic rendering feature
            let mut features3 = ash::vk::PhysicalDeviceVulkan13Features {
                p_next: &mut features_ext_state as *mut _ as *mut std::ffi::c_void,
                ..Default::default()
            };
            let mut features2 = ash::vk::PhysicalDeviceFeatures2 {
                p_next: &mut features3 as *mut _ as *mut std::ffi::c_void,
                ..Default::default()
            };
            unsafe { instance.get_physical_device_features2(*device, &mut features2) };
            if !features3.dynamic_rendering == ash::vk::TRUE
                && features_ext_state.extended_dynamic_state == ash::vk::TRUE
            {
                println!(
                    "Physical device {:?} does not support dynamic rendering, skipping",
                    device,
                );
                return None;
            }

            let discrete = properties.device_type == ash::vk::PhysicalDeviceType::DISCRETE_GPU;
            let has_preferred_features = features.tessellation_shader == ash::vk::TRUE
                && features.sampler_anisotropy == ash::vk::TRUE;

            Some((*device, discrete, has_preferred_features))
        })
        .collect::<Vec<_>>();

    if evaluated_devices.is_empty() {
        return Err(GpuError::new(
            "No suitable physical devices found (missing required features)",
            GpuErrorKind::Other,
        ));
    }

    // rank devices by discrete > integrated, then by having preferred features
    let best_device = evaluated_devices
        .into_iter()
        .max_by_key(|(_, discrete, has_features)| (*discrete as u8) << 1 | (*has_features as u8))
        .unwrap()
        .0;

    let properties = unsafe { instance.get_physical_device_properties(best_device) };
    let mem_properties = unsafe { instance.get_physical_device_memory_properties(best_device) };
    let device_name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }.to_string_lossy();
    let device_type_str = match properties.device_type {
        ash::vk::PhysicalDeviceType::DISCRETE_GPU => "Discrete GPU",
        ash::vk::PhysicalDeviceType::INTEGRATED_GPU => "Integrated GPU",
        ash::vk::PhysicalDeviceType::VIRTUAL_GPU => "Virtual GPU",
        ash::vk::PhysicalDeviceType::CPU => "CPU",
        _ => "Other",
    };
    let vram_bytes = mem_properties.memory_heaps[..mem_properties.memory_heap_count as usize]
        .iter()
        .find(|heap| heap.flags.contains(ash::vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|h| h.size)
        .unwrap_or(0);
    let vram_gib = vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let major = ash::vk::api_version_major(properties.api_version);
    let minor = ash::vk::api_version_minor(properties.api_version);
    println!("Selected GPU: {device_name} ({device_type_str})");
    println!("  Vulkan: {major}.{minor}");
    println!("  VRAM: {vram_gib:.1} GiB");

    Ok(best_device)
}

fn create_instance(
    context: &ash::Entry,
    window: &winit::window::Window,
    enable_validation: bool,
) -> Result<ash::Instance, GpuError> {
    let app_name = "Sparkle VK";
    let app_name_cstr = std::ffi::CString::new(app_name)
        .map_err(|_| GpuError::new("Failed to create CStr", GpuErrorKind::Other))?;
    let application_version = ash::vk::make_api_version(0, 1, 0, 0);
    let engine_name = "Sparkle Engine";
    let engine_name_cstr = std::ffi::CString::new(engine_name)
        .map_err(|_| GpuError::new("Failed to create CStr", GpuErrorKind::Other))?;
    let engine_version = ash::vk::make_api_version(0, 1, 0, 0);

    let app_info = ash::vk::ApplicationInfo {
        p_application_name: app_name_cstr.as_ptr(),
        application_version,
        p_engine_name: engine_name_cstr.as_ptr(),
        engine_version,
        api_version: ash::vk::API_VERSION_1_3,
        ..Default::default()
    };

    let instance_exts = util::get_instance_extensions(&window)?;
    let instance_exts_ptr = instance_exts
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();

    let mut extension_properties = unsafe { context.enumerate_instance_extension_properties(None) }
        .map_err(|_| {
            GpuError::new(
                "Failed to enumerate instance extensions",
                GpuErrorKind::Other,
            )
        })?;

    if let Some(ext) = instance_exts.iter().find(|ext| {
        !extension_properties.iter().any(|prop| {
            let extension_name = unsafe { CStr::from_ptr(prop.extension_name.as_ptr()) };
            extension_name == **ext
        })
    }) {
        return Err(GpuError::new(
            format!(
                "Required instance extension {} not supported",
                ext.to_string_lossy()
            ),
            GpuErrorKind::Other,
        ));
    }

    let validation_layers = if enable_validation {
        setup_validation_layers(&context, &mut extension_properties)?
    } else {
        Vec::new()
    };
    let validation_layers_ptr = validation_layers
        .iter()
        .map(|layer| layer.as_ptr())
        .collect::<Vec<_>>();

    let instance_info = ash::vk::InstanceCreateInfo {
        p_application_info: &app_info,
        enabled_extension_count: instance_exts.len() as u32,
        pp_enabled_extension_names: instance_exts_ptr.as_ptr(),
        enabled_layer_count: validation_layers.len() as u32,
        pp_enabled_layer_names: validation_layers_ptr.as_ptr(),
        ..Default::default()
    };

    unsafe { context.create_instance(&instance_info, None) }
        .map_err(|_| GpuError::new("Failed to create Vulkan instance", GpuErrorKind::Other))
}

fn setup_validation_layers(
    context: &ash::Entry,
    extension_properties: &mut Vec<ash::vk::ExtensionProperties>,
) -> Result<Vec<&'static CStr>, GpuError> {
    let layer_properties = unsafe { context.enumerate_instance_layer_properties() }
        .map_err(|_| GpuError::new("Failed to enumerate instance layers", GpuErrorKind::Other))?;
    if !layer_properties.iter().any(|prop| {
        let layer_name = unsafe { CStr::from_ptr(prop.layer_name.as_ptr()) };
        layer_name == VALIDATION_LAYER
    }) {
        println!(
            "Warning: Validation layer {} not found, skipping validation",
            VALIDATION_LAYER.to_string_lossy()
        );
        Ok(Vec::new())
    } else {
        let mut debug_ext_name: [c_char; 256] = [0; 256];
        debug_ext_name.copy_from_slice(
            ash::vk::EXT_DEBUG_UTILS_NAME
                .to_bytes_with_nul()
                .iter()
                .map(|&b| b as c_char)
                .collect::<Vec<_>>()
                .as_slice(),
        );
        extension_properties.push(ash::vk::ExtensionProperties {
            extension_name: debug_ext_name,
            spec_version: ash::vk::EXT_DEBUG_UTILS_SPEC_VERSION,
        });
        Ok(vec![VALIDATION_LAYER])
    }
}

fn setup_debug_messenger(
    context: &ash::Entry,
    instance: &ash::Instance,
) -> Result<ash::vk::DebugUtilsMessengerEXT, GpuError> {
    let severity_flags = ash::vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
        | ash::vk::DebugUtilsMessageSeverityFlagsEXT::ERROR;
    let type_flags = ash::vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
        | ash::vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION;

    let create_info = ash::vk::DebugUtilsMessengerCreateInfoEXT {
        message_severity: severity_flags,
        message_type: type_flags,
        pfn_user_callback: Some(util::debug_callback),
        ..Default::default()
    };

    let debug_utils_loader = ash::ext::debug_utils::Instance::new(context, instance);

    unsafe {
        debug_utils_loader
            .create_debug_utils_messenger(&create_info, None)
            .map_err(|_| GpuError::new("Failed to create debug messenger", GpuErrorKind::Other))
    }
}
