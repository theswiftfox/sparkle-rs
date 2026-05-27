use std::{
    cell::Cell,
    ffi::{CStr, c_void},
    ops::Deref,
    rc::Rc,
    sync::Arc,
};

use crate::engine::{
    backend::{GpuError, GpuErrorKind, RenderTargetDesc, SamplerDesc, TextureFormat},
    settings::{Settings, SyncMode},
    vulkan_backend::texture::VulkanTexture,
};

mod buffer;
mod gpu_backend_impl;
mod renderpass;
mod texture;
mod util;

const VK_API_VERSION: u32 = ash::vk::API_VERSION_1_3;

const VALIDATION_LAYER: &CStr =
    unsafe { CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0") };
const SHADER_ENTRY_POINT: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") };

const REQUIRED_EXTS: [&CStr; 3] = [
    ash::vk::KHR_SWAPCHAIN_NAME,
    ash::vk::KHR_SHADER_DRAW_PARAMETERS_NAME,
    ash::vk::KHR_SYNCHRONIZATION2_NAME,
];

const FRAMES_IN_FLIGHT: u32 = 2u32;

struct Instance {
    instance: ash::Instance,
    debug_messenger: Option<ash::vk::DebugUtilsMessengerEXT>,
    validation_enabled: bool,
}

impl Deref for Instance {
    type Target = ash::Instance;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

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

struct Swapchain {
    fn_ptr: ash::khr::swapchain::Device,
    swapchain: ash::vk::SwapchainKHR,
    swapchain_images: Vec<VulkanTexture>,
    swapchain_extent: ash::vk::Extent2D,
    surface_format: ash::vk::SurfaceFormatKHR,
    surface: ash::vk::SurfaceKHR,
    sync_mode: SyncMode,
}

struct SyncObjects {
    present_completed_sems: Vec<ash::vk::Semaphore>,
    render_completed_sems: Vec<ash::vk::Semaphore>,
    draw_fences: Vec<ash::vk::Fence>,
}

struct Descriptors {
    pool: ash::vk::DescriptorPool,
    layout: ash::vk::DescriptorSetLayout,
    sets: [ash::vk::DescriptorSet; FRAMES_IN_FLIGHT as usize],
}

struct CurrentFrame {
    idx: usize,
    render_idx: u32,
    command_buffer: ash::vk::CommandBuffer,
    fence: ash::vk::Fence,
    present_sem: ash::vk::Semaphore,
    render_sem: ash::vk::Semaphore,
}

struct CommandPool {
    render_pool: ash::vk::CommandPool,
    short_lived: ash::vk::CommandPool,
}

pub struct VulkanBackend {
    window: Arc<winit::window::Window>,
    context: ash::Entry,
    instance: Instance,
    phys_device: ash::vk::PhysicalDevice,
    device: LogicalDevice,
    swapchain: Swapchain,
    depth_targets: [VulkanTexture; FRAMES_IN_FLIGHT as usize],
    queue: ash::vk::Queue,
    graphics_pipeline: ash::vk::Pipeline,
    pipeline_layout: ash::vk::PipelineLayout,
    command_pool: CommandPool,
    command_buffers: [ash::vk::CommandBuffer; FRAMES_IN_FLIGHT as usize],
    descriptors: Descriptors,
    sync_objects: SyncObjects,
    khr_sync: ash::khr::synchronization2::Device,
    frame_idx: usize,
    current_frame: Option<CurrentFrame>,
    current_pass_targets: Vec<(
        ash::vk::Image,
        ash::vk::ImageAspectFlags,
        Rc<Cell<ash::vk::ImageLayout>>,
    )>,
}

pub fn initialize(
    window: Arc<winit::window::Window>,
    settings: &Settings,
) -> Result<VulkanBackend, GpuError> {
    let enable_validation = settings.gpu_validation;
    let sync_mode = settings.sync_mode;

    let context = unsafe { ash::Entry::load() }
        .map_err(|_| GpuError::new("Failed to load Vulkan entry", GpuErrorKind::Other))?;

    let mut instance = create_instance(&context, &window, enable_validation)?;

    if instance.validation_enabled {
        instance.debug_messenger = Some(setup_debug_messenger(&context, &instance)?);
    }

    let physical_device = get_physical_device(&instance)?;

    let surface = util::create_surface(&context, &instance, &window)?;

    let logical_device = create_logical_device(&context, &instance, physical_device, surface)?;

    let queue = logical_device.get_graphics_queue();

    let (swapchain, depth_targets) = create_swapchain_and_depth_buffer(
        &context,
        &instance,
        &window,
        physical_device,
        &logical_device,
        surface,
        sync_mode,
    )?;

    let pipeline = create_graphics_pipeline(&logical_device, &swapchain)?;

    let command_pool = create_command_pool(&logical_device)?;

    let command_buffers = create_command_buffers(&logical_device, command_pool.render_pool)?
        .try_into()
        .map_err(|_| {
            GpuError::new(
                "Command Buffers do not match expected FRAMES_IN_FLIGHT",
                GpuErrorKind::Other,
            )
        })?;

    let sync_objects = create_sync_objs(&logical_device, &swapchain)?;

    let khr_sync = ash::khr::synchronization2::Device::new(&instance, &logical_device);

    let desc_pool = create_descriptor_pool(&logical_device)?;

    let desc_set_layout = create_descriptor_set_layout(&logical_device)?;

    let desc_sets = create_descriptor_sets(&logical_device, desc_pool, desc_set_layout)?
        .try_into()
        .map_err(|_| {
            GpuError::new(
                "Descriptor Sets do not match expected FRAMES_IN_FLIGHT",
                GpuErrorKind::Other,
            )
        })?;

    let pipeline_layout = create_pipeline_layout(&logical_device, desc_set_layout)?;

    Ok(VulkanBackend {
        window,
        context,
        instance,
        phys_device: physical_device,
        device: logical_device,
        swapchain,
        depth_targets,
        queue,
        graphics_pipeline: pipeline,
        pipeline_layout,
        command_pool,
        command_buffers,
        descriptors: Descriptors {
            pool: desc_pool,
            layout: desc_set_layout,
            sets: desc_sets,
        },
        sync_objects,
        khr_sync,
        frame_idx: 0,
        current_frame: None,
        current_pass_targets: Vec::new(),
    })
}

fn create_pipeline_layout(
    device: &LogicalDevice,
    descriptor_layout: ash::vk::DescriptorSetLayout,
) -> Result<ash::vk::PipelineLayout, GpuError> {
    let create_info = ash::vk::PipelineLayoutCreateInfo {
        set_layout_count: 1,
        p_set_layouts: &descriptor_layout,
        ..Default::default()
    };

    unsafe { device.create_pipeline_layout(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create pipeline layout: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })
}

fn create_descriptor_sets(
    device: &LogicalDevice,
    pool: ash::vk::DescriptorPool,
    layout: ash::vk::DescriptorSetLayout,
) -> Result<Vec<ash::vk::DescriptorSet>, GpuError> {
    let layouts = (0..FRAMES_IN_FLIGHT).map(|_| layout).collect::<Vec<_>>();
    let alloc_info = ash::vk::DescriptorSetAllocateInfo {
        descriptor_pool: pool,
        descriptor_set_count: layouts.len() as u32,
        p_set_layouts: layouts.as_ptr(),
        ..Default::default()
    };

    let sets = unsafe { device.allocate_descriptor_sets(&alloc_info) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create bindless descriptor sets: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })?;

    if sets.len() != FRAMES_IN_FLIGHT as usize {
        Err(GpuError::new(
            "Allocate Descriptor Sets returned less Sets than expected",
            GpuErrorKind::ResourceCreation,
        ))
    } else {
        Ok(sets)
    }
}

fn create_descriptor_set_layout(
    device: &LogicalDevice,
) -> Result<ash::vk::DescriptorSetLayout, GpuError> {
    let frame_consts_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 0,
        descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: 1,
        stage_flags: ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let per_instance_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 1,
        descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: 1,
        stage_flags: ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let pixel_data_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 2,
        descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: 1,
        stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let light_data_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 3,
        descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: 1,
        stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let shadow_map_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 4,
        descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: 1,
        stage_flags: ash::vk::ShaderStageFlags::VERTEX,
        ..Default::default()
    };
    let texture_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 5,
        descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: 16,
        stage_flags: ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let cubemap_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 6,
        descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: 4,
        stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let comparison_img_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 7,
        descriptor_type: ash::vk::DescriptorType::SAMPLED_IMAGE,
        descriptor_count: 4,
        stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let comparison_sampler_binding = ash::vk::DescriptorSetLayoutBinding {
        binding: 8,
        descriptor_type: ash::vk::DescriptorType::SAMPLER,
        descriptor_count: 4,
        stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
        ..Default::default()
    };
    let bindings = [
        frame_consts_binding,
        per_instance_binding,
        pixel_data_binding,
        light_data_binding,
        shadow_map_binding,
        texture_binding,
        cubemap_binding,
        comparison_img_binding,
        comparison_sampler_binding,
    ];

    let binding_flags = ash::vk::DescriptorBindingFlags::PARTIALLY_BOUND
        | ash::vk::DescriptorBindingFlags::UPDATE_AFTER_BIND;

    let flags = (0..bindings.len())
        .map(|_| binding_flags)
        .collect::<Vec<_>>();
    let binding_flags_info = ash::vk::DescriptorSetLayoutBindingFlagsCreateInfo {
        binding_count: flags.len() as u32,
        p_binding_flags: flags.as_ptr(),
        ..Default::default()
    };

    let create_info = ash::vk::DescriptorSetLayoutCreateInfo {
        flags: ash::vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL,
        p_next: &binding_flags_info as *const _ as *const _,
        binding_count: bindings.len() as u32,
        p_bindings: bindings.as_ptr(),
        ..Default::default()
    };

    unsafe { device.create_descriptor_set_layout(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create bindless descriptor set layout: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })
}

fn create_descriptor_pool(device: &LogicalDevice) -> Result<ash::vk::DescriptorPool, GpuError> {
    let uniform_pool_info = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: 10,
    };
    let cis_pool_info = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: 40,
    };
    let sampled_image_pool_info = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::SAMPLED_IMAGE,
        descriptor_count: 8,
    };
    let sampler_pool_info = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::SAMPLER,
        descriptor_count: 8,
    };
    let pool_sizes = [
        uniform_pool_info,
        cis_pool_info,
        sampled_image_pool_info,
        sampler_pool_info,
    ];
    let create_info = ash::vk::DescriptorPoolCreateInfo {
        flags: ash::vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND,
        max_sets: FRAMES_IN_FLIGHT,
        pool_size_count: 4,
        p_pool_sizes: pool_sizes.as_ptr(),
        ..Default::default()
    };

    unsafe { device.create_descriptor_pool(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create bindless descriptor pool: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })
}

fn create_command_buffers(
    device: &LogicalDevice,
    command_pool: ash::vk::CommandPool,
) -> Result<Vec<ash::vk::CommandBuffer>, GpuError> {
    let alloc_info = ash::vk::CommandBufferAllocateInfo {
        command_pool,
        level: ash::vk::CommandBufferLevel::PRIMARY,
        command_buffer_count: FRAMES_IN_FLIGHT,
        ..Default::default()
    };

    let command_buffers = unsafe { device.allocate_command_buffers(&alloc_info) }.map_err(|e| {
        GpuError::new(
            format!("Failed to allocate command buffer: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })?;

    Ok(command_buffers)
}

fn create_command_pool(device: &LogicalDevice) -> Result<CommandPool, GpuError> {
    let pool_create_info = ash::vk::CommandPoolCreateInfo {
        flags: ash::vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
        queue_family_index: device.graphics_queue_index,
        ..Default::default()
    };

    let graphics = unsafe { device.create_command_pool(&pool_create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create command pool: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })?;

    let pool_create_info = ash::vk::CommandPoolCreateInfo {
        flags: ash::vk::CommandPoolCreateFlags::TRANSIENT,
        queue_family_index: device.graphics_queue_index,
        ..Default::default()
    };
    let short_lived =
        unsafe { device.create_command_pool(&pool_create_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create command pool: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

    Ok(CommandPool {
        render_pool: graphics,
        short_lived,
    })
}

fn create_sync_objs(
    device: &LogicalDevice,
    swapchain: &Swapchain,
) -> Result<SyncObjects, GpuError> {
    fn create_default_sem(device: &LogicalDevice) -> Result<ash::vk::Semaphore, GpuError> {
        unsafe {
            device.create_semaphore(
                &ash::vk::SemaphoreCreateInfo {
                    ..Default::default()
                },
                None,
            )
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to create semaphore: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })
    }
    let render_completed_sems = (0..swapchain.swapchain_images.len())
        .map(|_| create_default_sem(device))
        .collect::<Result<Vec<_>, GpuError>>()?;

    let (present_completed_sems, draw_fences) = (0..FRAMES_IN_FLIGHT)
        .map(|_| {
            let present_completed_sems = create_default_sem(device)?;
            let draw_fence = unsafe {
                device.create_fence(
                    &ash::vk::FenceCreateInfo {
                        flags: ash::vk::FenceCreateFlags::SIGNALED,
                        ..Default::default()
                    },
                    None,
                )
            }
            .map_err(|e| {
                GpuError::new(
                    format!("Fence create failed: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;
            Ok((present_completed_sems, draw_fence))
        })
        .collect::<Result<Vec<_>, GpuError>>()?
        .into_iter()
        .unzip();

    Ok(SyncObjects {
        present_completed_sems,
        render_completed_sems,
        draw_fences,
    })
}

fn create_graphics_pipeline(
    device: &ash::Device,
    swapchain: &Swapchain,
) -> Result<ash::vk::Pipeline, GpuError> {
    let shader_vert = util::load_shader_blob("src/shaders/spv/example.vert.spv")?;
    let shader_pxl = util::load_shader_blob("src/shaders/spv/example.pxl.spv")?;

    let shader_module_vert = create_shader_module(&shader_vert, device, "Vertex Shader")?;
    let shader_module_pxl = create_shader_module(&shader_pxl, device, "Pixel Shader")?;

    let vtx_shader_stage_create = ash::vk::PipelineShaderStageCreateInfo {
        stage: ash::vk::ShaderStageFlags::VERTEX,
        module: shader_module_vert,
        p_name: SHADER_ENTRY_POINT.as_ptr(),
        ..Default::default()
    };
    let pxl_shader_stage_create = ash::vk::PipelineShaderStageCreateInfo {
        stage: ash::vk::ShaderStageFlags::FRAGMENT,
        module: shader_module_pxl,
        p_name: SHADER_ENTRY_POINT.as_ptr(),
        ..Default::default()
    };
    let shader_stages = [vtx_shader_stage_create, pxl_shader_stage_create];

    let dynamic_states = [
        ash::vk::DynamicState::VIEWPORT,
        ash::vk::DynamicState::SCISSOR,
    ];

    let dynamic_state_create_info = ash::vk::PipelineDynamicStateCreateInfo {
        dynamic_state_count: dynamic_states.len() as u32,
        p_dynamic_states: dynamic_states.as_ptr(),
        ..Default::default()
    };

    let vtx_input_state_info = ash::vk::PipelineVertexInputStateCreateInfo {
        ..Default::default()
    };
    let input_assembly_info = ash::vk::PipelineInputAssemblyStateCreateInfo {
        topology: ash::vk::PrimitiveTopology::TRIANGLE_LIST,
        ..Default::default()
    };

    let viewport_create_info = ash::vk::PipelineViewportStateCreateInfo {
        viewport_count: 1,
        scissor_count: 1,
        ..Default::default()
    };

    let rasterization_state_create_info = ash::vk::PipelineRasterizationStateCreateInfo {
        depth_clamp_enable: ash::vk::FALSE,
        rasterizer_discard_enable: ash::vk::FALSE,
        polygon_mode: ash::vk::PolygonMode::FILL,
        cull_mode: ash::vk::CullModeFlags::BACK,
        front_face: ash::vk::FrontFace::CLOCKWISE,
        depth_bias_enable: ash::vk::FALSE,
        line_width: 1.0f32,
        ..Default::default()
    };

    let multisample_state_create_info = ash::vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: ash::vk::SampleCountFlags::TYPE_1,
        sample_shading_enable: ash::vk::FALSE,
        ..Default::default()
    };

    let blend_attachment_state = ash::vk::PipelineColorBlendAttachmentState {
        blend_enable: ash::vk::FALSE,
        color_write_mask: ash::vk::ColorComponentFlags::RGBA,
        ..Default::default()
    };
    let blend_state_create_info = ash::vk::PipelineColorBlendStateCreateInfo {
        logic_op_enable: ash::vk::FALSE,
        logic_op: ash::vk::LogicOp::COPY,
        attachment_count: 1,
        p_attachments: &blend_attachment_state as *const _,
        ..Default::default()
    };

    let pipeline_layout_create_info = ash::vk::PipelineLayoutCreateInfo {
        set_layout_count: 0,
        push_constant_range_count: 0,
        ..Default::default()
    };
    let pipeline_layout =
        unsafe { device.create_pipeline_layout(&pipeline_layout_create_info, None) }.map_err(
            |e| {
                GpuError::new(
                    format!("Pipeline Layout creation failed: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            },
        )?;
    let rendering_create_info = ash::vk::PipelineRenderingCreateInfo {
        color_attachment_count: 1,
        p_color_attachment_formats: &swapchain.surface_format.format as *const _,
        ..Default::default()
    };

    let pipeline_create_info = ash::vk::GraphicsPipelineCreateInfo {
        stage_count: 2,
        p_stages: shader_stages.as_ptr(),
        p_vertex_input_state: &vtx_input_state_info as *const _,
        p_input_assembly_state: &input_assembly_info as *const _,
        p_viewport_state: &viewport_create_info as *const _,
        p_rasterization_state: &rasterization_state_create_info as *const _,
        p_multisample_state: &multisample_state_create_info as *const _,
        p_color_blend_state: &blend_state_create_info as *const _,
        p_dynamic_state: &dynamic_state_create_info as *const _,
        layout: pipeline_layout,
        render_pass: ash::vk::RenderPass::null(),
        p_next: &rendering_create_info as *const _ as *const c_void,
        ..Default::default()
    };

    let pipeline = unsafe {
        device.create_graphics_pipelines(
            ash::vk::PipelineCache::null(),
            &[pipeline_create_info],
            None,
        )
    }
    .map_err(|e| {
        GpuError::new(
            format!("Failed to create graphics pipeline: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })?;

    pipeline.into_iter().next().ok_or_else(|| {
        GpuError::new(
            "Create pipeline returned empty response",
            GpuErrorKind::ResourceCreation,
        )
    })
}

fn create_shader_module(
    code: &[u8],
    device: &ash::Device,
    module_desc: &str,
) -> Result<ash::vk::ShaderModule, GpuError> {
    let create_info = ash::vk::ShaderModuleCreateInfo {
        code_size: code.len(),
        p_code: code.as_ptr() as *const u32,
        ..Default::default()
    };

    unsafe { device.create_shader_module(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create shader module {module_desc}: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })
}

fn create_image_views(
    device: &LogicalDevice,
    images: &[ash::vk::Image],
    format: ash::vk::Format,
) -> Result<Vec<ash::vk::ImageView>, GpuError> {
    let create_info = ash::vk::ImageViewCreateInfo {
        view_type: ash::vk::ImageViewType::TYPE_2D,
        format,
        subresource_range: ash::vk::ImageSubresourceRange {
            aspect_mask: ash::vk::ImageAspectFlags::COLOR,
            layer_count: 1,
            level_count: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut image_views = Vec::new();
    for image in images {
        let create_info = create_info.image(*image);
        let img_view = unsafe { device.create_image_view(&create_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create ImageView: {:?}", e),
                GpuErrorKind::ResourceCreation,
            )
        })?;
        image_views.push(img_view)
    }

    Ok(image_views)
}

fn create_swapchain_and_depth_buffer(
    context: &ash::Entry,
    instance: &ash::Instance,
    window: &winit::window::Window,
    physical_device: ash::vk::PhysicalDevice,
    logical_device: &LogicalDevice,
    surface: ash::vk::SurfaceKHR,
    sync_mode: SyncMode,
) -> Result<(Swapchain, [VulkanTexture; FRAMES_IN_FLIGHT as usize]), GpuError> {
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
    let swap_extent = util::choose_swap_extent(&surface_capababilities, &window);
    let swap_image_count = util::choose_swap_min_image_count(&surface_capababilities);
    let swap_format = util::choose_swapchain_format(&surface_formats)?;
    let present_mode = util::choose_present_mode(&present_modes, sync_mode)?;

    let engine_fmt: TextureFormat = swap_format.format.try_into()?;

    let create_info = ash::vk::SwapchainCreateInfoKHR {
        surface,
        min_image_count: swap_image_count,
        image_format: swap_format.format,
        image_color_space: swap_format.color_space,
        image_extent: swap_extent,
        image_array_layers: 1,
        image_usage: ash::vk::ImageUsageFlags::COLOR_ATTACHMENT,
        image_sharing_mode: ash::vk::SharingMode::EXCLUSIVE,
        pre_transform: surface_capababilities.current_transform,
        composite_alpha: ash::vk::CompositeAlphaFlagsKHR::OPAQUE,
        present_mode,
        clipped: ash::vk::TRUE,
        ..Default::default()
    };

    let swapchain_khr = ash::khr::swapchain::Device::new(instance, logical_device);
    let swapchain = unsafe { swapchain_khr.create_swapchain(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create swapchain: {:?}", e),
            GpuErrorKind::ResourceCreation,
        )
    })?;
    let swapchain_images =
        unsafe { swapchain_khr.get_swapchain_images(swapchain) }.map_err(|e| {
            GpuError::new(
                format!("Failed to retrieve swapchain images: {:?}", e),
                GpuErrorKind::ResourceCreation,
            )
        })?;

    let swapchain_image_views =
        create_image_views(logical_device, &swapchain_images, swap_format.format)?;

    let sampler_info = ash::vk::SamplerCreateInfo {
        mag_filter: ash::vk::Filter::LINEAR,
        min_filter: ash::vk::Filter::LINEAR,
        anisotropy_enable: ash::vk::FALSE,
        mipmap_mode: ash::vk::SamplerMipmapMode::LINEAR,
        address_mode_u: ash::vk::SamplerAddressMode::CLAMP_TO_EDGE,
        address_mode_v: ash::vk::SamplerAddressMode::CLAMP_TO_EDGE,
        address_mode_w: ash::vk::SamplerAddressMode::CLAMP_TO_EDGE,
        compare_enable: ash::vk::FALSE,
        compare_op: ash::vk::CompareOp::ALWAYS,
        ..Default::default()
    };
    let sampler = unsafe { logical_device.create_sampler(&sampler_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create swapchain sampler: {:?}", e),
            GpuErrorKind::ResourceCreation,
        )
    })?;

    let swapchain_images = swapchain_images
        .into_iter()
        .zip(swapchain_image_views.into_iter())
        .map(|(img, view)| VulkanTexture {
            image: img,
            image_view: view,
            mem: ash::vk::DeviceMemory::null(),
            sampler,
            width: swap_extent.width,
            height: swap_extent.height,
            format: engine_fmt,
            view_type: ash::vk::ImageViewType::TYPE_2D,
            compare_enabled: false,
            id: 0,
            device_handle: logical_device.device.clone(),
            current_layout: Rc::new(Cell::new(ash::vk::ImageLayout::UNDEFINED)),
        })
        .collect::<Vec<_>>();

    // depth targets
    let depth_target_desc = RenderTargetDesc {
        width: swap_extent.width,
        height: swap_extent.height,
        format: TextureFormat::Depth32Float,
        sampler: SamplerDesc::default(),
    };
    let depth_targets = (0..FRAMES_IN_FLIGHT)
        .map(|_| {
            VulkanBackend::create_vk_render_target(
                instance,
                logical_device,
                physical_device,
                &depth_target_desc,
            )
        })
        .collect::<Result<Vec<_>, GpuError>>()?;
    let depth_targets: [VulkanTexture; FRAMES_IN_FLIGHT as usize] =
        depth_targets.try_into().map_err(|_| {
            GpuError::new(
                "Unable to create expected amount of depth targets",
                GpuErrorKind::ResourceCreation,
            )
        })?;

    Ok((
        Swapchain {
            fn_ptr: swapchain_khr,
            swapchain,
            swapchain_images,
            swapchain_extent: swap_extent,
            surface_format: swap_format,
            surface,
            sync_mode,
        },
        depth_targets,
    ))
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
        queue_count: 1,
        ..Default::default()
    };

    let mut ext_state_struct = ash::vk::PhysicalDeviceExtendedDynamicStateFeaturesEXT {
        extended_dynamic_state: ash::vk::TRUE,
        ..Default::default()
    };
    let mut vk_13_feats = ash::vk::PhysicalDeviceVulkan13Features {
        dynamic_rendering: ash::vk::TRUE,
        synchronization2: ash::vk::TRUE,
        p_next: &mut ext_state_struct as *mut _ as *mut std::ffi::c_void,
        ..Default::default()
    };
    let mut indexing_feats = ash::vk::PhysicalDeviceDescriptorIndexingFeatures {
        descriptor_binding_uniform_buffer_update_after_bind: ash::vk::TRUE,
        descriptor_binding_partially_bound: ash::vk::TRUE,
        descriptor_binding_sampled_image_update_after_bind: ash::vk::TRUE,
        runtime_descriptor_array: ash::vk::TRUE,
        p_next: &mut vk_13_feats as *mut _ as *mut std::ffi::c_void,
        ..Default::default()
    };
    let mut shader_float16_feats = ash::vk::PhysicalDeviceShaderFloat16Int8Features {
        shader_float16: ash::vk::TRUE,
        p_next: &mut indexing_feats as *mut _ as *mut std::ffi::c_void,
        ..Default::default()
    };
    let mut base_struct = ash::vk::PhysicalDeviceFeatures2 {
        features: ash::vk::PhysicalDeviceFeatures {
            sampler_anisotropy: ash::vk::TRUE,
            shader_int16: ash::vk::TRUE,
            ..Default::default()
        },
        p_next: &mut shader_float16_feats as *mut _ as *mut std::ffi::c_void,
        ..Default::default()
    };
    let device_exts = REQUIRED_EXTS;
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
            let has_required_features = features.geometry_shader == ash::vk::TRUE
                && features.sampler_anisotropy == ash::vk::TRUE;

            if !has_required_features {
                println!(
                    "Physical device {:?} does not support required features, skipping",
                    device
                );
                return None;
            }

            let properties = unsafe { instance.get_physical_device_properties(*device) };

            // require vulkan 1.3
            if properties.api_version < VK_API_VERSION {
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

            if let Some(ext) = REQUIRED_EXTS.iter().find(|ext| {
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
            let is_gpu = properties.device_type == ash::vk::PhysicalDeviceType::DISCRETE_GPU
                || properties.device_type == ash::vk::PhysicalDeviceType::INTEGRATED_GPU;
            let has_preferred_features = features.tessellation_shader == ash::vk::TRUE
                && features.sampler_anisotropy == ash::vk::TRUE;

            println!(
                "Evaluated physical device {:?}: type={:?}, discrete={}, preferred_features={}",
                device, properties.device_type, discrete, has_preferred_features
            );
            Some((*device, discrete, is_gpu, has_preferred_features))
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
        .max_by_key(|(_, discrete, is_gpu, has_features)| {
            (*discrete as u8) << 3 | (*is_gpu as u8) << 2 | (*has_features as u8)
        })
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
) -> Result<Instance, GpuError> {
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
        api_version: VK_API_VERSION,
        ..Default::default()
    };

    let mut instance_exts = util::get_instance_extensions(&window)?;

    let extension_properties = unsafe { context.enumerate_instance_extension_properties(None) }
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

    let (validation_layers, layers_enabled) = if enable_validation {
        setup_validation_layers(&context, &mut instance_exts)?
    } else {
        (Vec::new(), false)
    };
    let validation_layers_ptr = validation_layers
        .iter()
        .map(|layer| layer.as_ptr())
        .collect::<Vec<_>>();

    let instance_exts_ptr = instance_exts
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();

    let validation_features = ash::vk::ValidationFeaturesEXT {
        p_enabled_validation_features: &ash::vk::ValidationFeatureEnableEXT::GPU_ASSISTED,
        enabled_validation_feature_count: 1,
        ..Default::default()
    };

    let instance_info = ash::vk::InstanceCreateInfo {
        p_application_info: &app_info,
        enabled_extension_count: instance_exts.len() as u32,
        pp_enabled_extension_names: instance_exts_ptr.as_ptr(),
        enabled_layer_count: validation_layers.len() as u32,
        pp_enabled_layer_names: validation_layers_ptr.as_ptr(),
        p_next: &validation_features as *const _ as *const _,
        ..Default::default()
    };

    let instance = unsafe { context.create_instance(&instance_info, None) }
        .map_err(|_| GpuError::new("Failed to create Vulkan instance", GpuErrorKind::Other))?;

    Ok(Instance {
        instance,
        debug_messenger: None,
        validation_enabled: layers_enabled,
    })
}

fn setup_validation_layers(
    context: &ash::Entry,
    instance_exts: &mut Vec<&'static CStr>,
) -> Result<(Vec<&'static CStr>, bool), GpuError> {
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
        Ok((Vec::new(), false))
    } else {
        instance_exts.push(ash::vk::EXT_DEBUG_UTILS_NAME);
        Ok((vec![VALIDATION_LAYER], true))
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
