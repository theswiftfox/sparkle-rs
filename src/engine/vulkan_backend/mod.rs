use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    ffi::CStr,
    ops::Deref,
    rc::Rc,
    sync::{Arc, Mutex},
};

use crate::{
    app_handler::Window,
    engine::{
        backend::{
            GpuError, GpuErrorKind, RenderTargetDesc, RenderTargetUsage, SamplerDesc, TextureFormat,
        },
        compute_push::ComputePushConstants,
        settings::{Settings, SyncMode},
        vulkan_backend::texture::VulkanTexture,
    },
    util as crate_utils,
};

use winit::raw_window_handle::RawWindowHandle;

mod buffer;
mod egui;
mod gpu_backend_impl;
mod image_layout_transition;
mod rt;
mod texture;
mod util;

const ENABLE_MARKER: bool = false;

const VK_API_VERSION: u32 = ash::vk::API_VERSION_1_3;

const VALIDATION_LAYER: &CStr =
    unsafe { CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0") };
const SHADER_ENTRY_POINT: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") };

const REQUIRED_EXTS: [&CStr; 3] = [
    ash::vk::KHR_SWAPCHAIN_NAME,
    ash::vk::KHR_SHADER_DRAW_PARAMETERS_NAME,
    ash::vk::KHR_SYNCHRONIZATION2_NAME,
    // ash::vk::EXT_SWAPCHAIN_COLORSPACE_NAME,
    // ash::vk::EXT_HDR_METADATA_NAME,
];

const RT_EXTS: [&CStr; 4] = [
    ash::vk::KHR_ACCELERATION_STRUCTURE_NAME,
    ash::vk::KHR_RAY_TRACING_PIPELINE_NAME,
    ash::vk::KHR_DEFERRED_HOST_OPERATIONS_NAME,
    ash::vk::KHR_PUSH_DESCRIPTOR_NAME,
];

const FRAMES_IN_FLIGHT: u32 = 2u32;

const GAMMA_DEFAULT: f32 = 2.4f32;

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
    main_queue_index: u32,
    debug_utils_ext: Option<ash::ext::debug_utils::Device>,
    rt_supported: bool,
}

impl LogicalDevice {
    fn get_main_queue(&self) -> ash::vk::Queue {
        unsafe { self.device.get_device_queue(self.main_queue_index, 0) }
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
    surface_format: SurfaceFormat,
    surface: ash::vk::SurfaceKHR,
    sync_mode: SyncMode,
}

struct SurfaceFormat {
    format: ash::vk::SurfaceFormatKHR,
    is_hdr: bool,
}
impl Deref for SurfaceFormat {
    type Target = ash::vk::SurfaceFormatKHR;

    fn deref(&self) -> &Self::Target {
        &self.format
    }
}
impl Default for SurfaceFormat {
    fn default() -> Self {
        Self {
            format: Default::default(),
            is_hdr: Default::default(),
        }
    }
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
    pass_targets: Vec<(
        ash::vk::Image,
        ash::vk::ImageAspectFlags,
        Rc<Cell<ash::vk::ImageLayout>>,
    )>,
    pending_push: PushConstants,
}

struct CommandPool {
    render_pool: ash::vk::CommandPool,
    short_lived: ash::vk::CommandPool,
}

const MAX_BINDLESS_TEXTURES: u32 = 1024;
const MAX_BINDLESS_CUBEMAPS: u32 = 4;
const MAX_BINDLESS_SHADOW_IMAGES: u32 = 4;
const MAX_BINDLESS_COMPARISON_SAMPLERS: u32 = 4;

struct TextureRegistry {
    next_2d: u32,
    free_2d: Vec<u32>,
    next_cube: u32,
    free_cube: Vec<u32>,
    next_shadow: u32,
    free_shadow: Vec<u32>,
}

impl TextureRegistry {
    fn new() -> Self {
        TextureRegistry {
            next_2d: 0,
            free_2d: Vec::new(),
            next_cube: 0,
            free_cube: Vec::new(),
            next_shadow: 0,
            free_shadow: Vec::new(),
        }
    }

    fn allocate_2d(&mut self) -> u32 {
        if let Some(slot) = self.free_2d.pop() {
            slot
        } else {
            let slot = self.next_2d;
            assert!(
                slot < MAX_BINDLESS_TEXTURES,
                "Exceeded max bindless 2D texture slots"
            );
            self.next_2d += 1;
            slot
        }
    }

    fn allocate_cube(&mut self) -> u32 {
        if let Some(slot) = self.free_cube.pop() {
            slot
        } else {
            let slot = self.next_cube;
            assert!(
                slot < MAX_BINDLESS_CUBEMAPS,
                "Exceeded max bindless cubemap slots"
            );
            self.next_cube += 1;
            slot
        }
    }

    fn allocate_shadow(&mut self) -> u32 {
        if let Some(slot) = self.free_shadow.pop() {
            slot
        } else {
            let slot = self.next_shadow;
            assert!(
                slot < MAX_BINDLESS_SHADOW_IMAGES,
                "Exceeded max bindless shadow image slots"
            );
            self.next_shadow += 1;
            slot
        }
    }

    #[allow(dead_code)]
    fn release_2d(&mut self, slot: u32) {
        self.free_2d.push(slot);
    }

    #[allow(dead_code)]
    fn release_cube(&mut self, slot: u32) {
        self.free_cube.push(slot);
    }

    #[allow(dead_code)]
    fn release_shadow(&mut self, slot: u32) {
        self.free_shadow.push(slot);
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct PushConstants {
    model: [f32; 16],
    tex0: u32,
    tex1: u32,
    tex2: u32,
    tex3: u32,
    tex4: u32,
    has_parallax: u32,
    is_instanced: u32,
}

impl Default for PushConstants {
    fn default() -> Self {
        PushConstants {
            model: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            tex0: 0,
            tex1: 0,
            tex2: 0,
            tex3: 0,
            tex4: u32::MAX,
            has_parallax: 0,
            is_instanced: 0,
        }
    }
}

#[repr(C, align(4))]
#[derive(Clone, Copy)]
pub(crate) struct SpecializationConstants {
    hdr_enabled: u32,
    gamma: f32,
}

impl Default for SpecializationConstants {
    fn default() -> Self {
        Self {
            hdr_enabled: 0,
            gamma: 2.2f32,
        }
    }
}

#[derive(Clone)]
pub struct VulkanHandleTracker {
    device: Arc<Mutex<ash::Device>>,
    ac_device: Arc<Mutex<Option<ash::khr::acceleration_structure::Device>>>,
    active_samplers: Arc<Mutex<HashSet<ash::vk::Sampler>>>,
    active_image_views: Arc<Mutex<HashSet<ash::vk::ImageView>>>,
    active_images: Arc<Mutex<HashSet<ash::vk::Image>>>,
    active_device_memory: Arc<Mutex<HashSet<ash::vk::DeviceMemory>>>,
    active_buffers: Arc<Mutex<HashSet<ash::vk::Buffer>>>,
    active_pipelines: Arc<Mutex<HashSet<ash::vk::Pipeline>>>,
    acceleration_structures: Arc<Mutex<HashSet<ash::vk::AccelerationStructureKHR>>>,
}

impl VulkanHandleTracker {
    pub fn new(
        device: ash::Device,
        ac_device: Option<ash::khr::acceleration_structure::Device>,
    ) -> Self {
        VulkanHandleTracker {
            device: Arc::new(Mutex::new(device)),
            ac_device: Arc::new(Mutex::new(ac_device)),
            active_samplers: Arc::new(Mutex::new(HashSet::new())),
            active_image_views: Arc::new(Mutex::new(HashSet::new())),
            active_images: Arc::new(Mutex::new(HashSet::new())),
            active_device_memory: Arc::new(Mutex::new(HashSet::new())),
            active_buffers: Arc::new(Mutex::new(HashSet::new())),
            active_pipelines: Arc::new(Mutex::new(HashSet::new())),
            acceleration_structures: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn register_sampler(&self, sampler: ash::vk::Sampler) {
        crate_utils::mtx_lock(&self.active_samplers).insert(sampler);
    }

    pub fn unregister_sampler(&self, sampler: ash::vk::Sampler) {
        crate_utils::mtx_lock(&self.active_samplers).remove(&sampler);
    }

    pub fn register_image_view(&self, view: ash::vk::ImageView) {
        crate_utils::mtx_lock(&self.active_image_views).insert(view);
    }

    pub fn unregister_image_view(&self, view: ash::vk::ImageView) {
        crate_utils::mtx_lock(&self.active_image_views).remove(&view);
    }

    pub fn register_image(&self, image: ash::vk::Image) {
        crate_utils::mtx_lock(&self.active_images).insert(image);
    }

    pub fn unregister_image(&self, image: ash::vk::Image) {
        crate_utils::mtx_lock(&self.active_images).remove(&image);
    }

    pub fn register_device_memory(&self, mem: ash::vk::DeviceMemory) {
        crate_utils::mtx_lock(&self.active_device_memory).insert(mem);
    }

    pub fn unregister_device_memory(&self, mem: ash::vk::DeviceMemory) {
        crate_utils::mtx_lock(&self.active_device_memory).remove(&mem);
    }

    pub fn register_buffer(&self, buffer: ash::vk::Buffer) {
        crate_utils::mtx_lock(&self.active_buffers).insert(buffer);
    }

    pub fn unregister_buffer(&self, buffer: ash::vk::Buffer) {
        crate_utils::mtx_lock(&self.active_buffers).remove(&buffer);
    }

    pub fn register_pipeline(&self, pipeline: ash::vk::Pipeline) {
        crate_utils::mtx_lock(&self.active_pipelines).insert(pipeline);
    }

    pub fn unregister_pipeline(&self, pipeline: ash::vk::Pipeline) {
        crate_utils::mtx_lock(&self.active_pipelines).remove(&pipeline);
    }

    pub fn register_acceleration_structure(&self, structure: ash::vk::AccelerationStructureKHR) {
        crate_utils::mtx_lock(&self.acceleration_structures).insert(structure);
    }

    pub fn unregister_acceleration_structure(&self, structure: ash::vk::AccelerationStructureKHR) {
        crate_utils::mtx_lock(&self.acceleration_structures).remove(&structure);
    }

    pub fn cleanup_leftover(&self) {
        for pipeline in crate_utils::mtx_lock(&self.active_pipelines).drain() {
            unsafe { crate_utils::mtx_lock(&self.device).destroy_pipeline(pipeline, None) };
        }
        for sampler in crate_utils::mtx_lock(&self.active_samplers).drain() {
            // SAFETY: Caller must ensure no concurrent use of Vulkan device while this is called
            unsafe { crate_utils::mtx_lock(&self.device).destroy_sampler(sampler, None) };
        }
        for view in crate_utils::mtx_lock(&self.active_image_views).drain() {
            unsafe { crate_utils::mtx_lock(&self.device).destroy_image_view(view, None) };
        }
        for image in crate_utils::mtx_lock(&self.active_images).drain() {
            unsafe { crate_utils::mtx_lock(&self.device).destroy_image(image, None) };
        }
        for buffer in crate_utils::mtx_lock(&self.active_buffers).drain() {
            unsafe { crate_utils::mtx_lock(&self.device).destroy_buffer(buffer, None) };
        }
        for mem in crate_utils::mtx_lock(&self.active_device_memory).drain() {
            unsafe { crate_utils::mtx_lock(&self.device).free_memory(mem, None) };
        }
        if let Some(ac_device) = crate_utils::mtx_lock(&self.ac_device).as_ref() {
            for accel_struct in crate_utils::mtx_lock(&self.acceleration_structures).drain() {
                unsafe {
                    ac_device.destroy_acceleration_structure(accel_struct, None);
                }
            }
        }
    }
}

pub struct VulkanBackend {
    window: Arc<Window>,
    context: ash::Entry,
    instance: Instance,
    phys_device: ash::vk::PhysicalDevice,
    device: LogicalDevice,
    swapchain: Swapchain,
    depth_targets: [VulkanTexture; FRAMES_IN_FLIGHT as usize],
    queue: ash::vk::Queue,
    // graphics_pipeline: ash::vk::Pipeline,
    pipeline_layout: ash::vk::PipelineLayout,
    compute_pipeline_layout: ash::vk::PipelineLayout,
    command_pool: CommandPool,
    command_buffers: [ash::vk::CommandBuffer; FRAMES_IN_FLIGHT as usize],
    descriptors: Descriptors,
    sync_objects: SyncObjects,
    khr_sync: ash::khr::synchronization2::Device,
    push_descriptor: ash::khr::push_descriptor::Device,
    frame_idx: usize,
    current_frame: Option<CurrentFrame>,
    texture_registry: RefCell<TextureRegistry>,
    egui_renderer: Option<egui::EguiRenderer>,
    vulkan_handle_tracker: VulkanHandleTracker,
    rt_feature: Option<rt::RtFeature>,
}

impl Drop for VulkanBackend {
    fn drop(&mut self) {
        unsafe {
            // Wait for GPU idle
            let _ = self.device.device_wait_idle();

            // Destroy egui_renderer first (Option)
            if let Some(egui) = self.egui_renderer.take() {
                egui.destroy();
            }

            // Clear texture registry
            self.texture_registry.borrow_mut().free_2d.clear();
            self.texture_registry.borrow_mut().free_cube.clear();
            self.texture_registry.borrow_mut().free_shadow.clear();

            // Take current_frame
            self.current_frame = None;

            // Destroy sync objects
            for fence in &self.sync_objects.draw_fences {
                self.device.destroy_fence(*fence, None);
            }
            for sem in &self.sync_objects.present_completed_sems {
                self.device.destroy_semaphore(*sem, None);
            }
            for sem in &self.sync_objects.render_completed_sems {
                self.device.destroy_semaphore(*sem, None);
            }

            // Destroy descriptors
            self.device
                .destroy_descriptor_pool(self.descriptors.pool, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptors.layout, None);

            // Destroy command pools
            self.device
                .destroy_command_pool(self.command_pool.render_pool, None);
            self.device
                .destroy_command_pool(self.command_pool.short_lived, None);

            // Destroy pipeline
            // self.device.destroy_pipeline(self.graphics_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_pipeline_layout(self.compute_pipeline_layout, None);

            // Destroy RT feature layouts
            if let Some(rt) = &self.rt_feature {
                self.device
                    .destroy_pipeline_layout(rt.pipeline_layout, None);
                self.device
                    .destroy_descriptor_set_layout(rt.descriptor_layout, None);
            }

            let emtpy_target =
                VulkanTexture::null(self.device.clone(), self.vulkan_handle_tracker.clone());
            // Destroy depth targets (take ownership to call destroy)
            for depth_target in std::mem::replace(
                &mut self.depth_targets,
                [emtpy_target.clone(), emtpy_target],
            ) {
                depth_target.destroy();
            }

            // Destroy swapchain image views and shared sampler
            // (swapchain images themselves are owned by the swapchain, don't destroy them)
            let mut sampler_destroyed = false;
            for tex in &self.swapchain.swapchain_images {
                if tex.image_view != ash::vk::ImageView::null() {
                    self.vulkan_handle_tracker
                        .unregister_image_view(tex.image_view);
                    self.device.destroy_image_view(tex.image_view, None);
                }
                if !sampler_destroyed && tex.sampler != ash::vk::Sampler::null() {
                    self.vulkan_handle_tracker.unregister_sampler(tex.sampler);
                    self.device.destroy_sampler(tex.sampler, None);
                    sampler_destroyed = true;
                }
            }

            self.swapchain
                .fn_ptr
                .destroy_swapchain(self.swapchain.swapchain, None);
            self.swapchain.swapchain = ash::vk::SwapchainKHR::null();

            // Save surface handle for later destruction (after device)
            let surface_handle = self.swapchain.surface;
            self.swapchain.surface = ash::vk::SurfaceKHR::null();

            // Destroy debug messenger before instance (using original entry)
            if let Some(messenger) = self.instance.debug_messenger {
                let debug_utils =
                    ash::ext::debug_utils::Instance::new(&self.context, &self.instance);
                debug_utils.destroy_debug_utils_messenger(messenger, None);
                self.instance.debug_messenger = None;
            }

            // Destroy remaining tracked resources
            self.vulkan_handle_tracker.cleanup_leftover();

            // Destroy device before surface (Vulkan spec: all device resources must be freed first)
            self.device.destroy_device(None);

            // Destroy surface after device, before instance
            let surface_ext = ash::khr::surface::Instance::new(&self.context, &self.instance);
            surface_ext.destroy_surface(surface_handle, None);

            // Instance dropped by its own Drop impl
            self.instance.destroy_instance(None);
        }
    }
}

pub fn initialize(window: Arc<Window>, settings: &Settings) -> Result<VulkanBackend, GpuError> {
    let enable_validation = settings.gpu_validation;
    let sync_mode = settings.sync_mode;

    let context = unsafe { ash::Entry::load() }
        .map_err(|_| GpuError::new("Failed to load Vulkan entry", GpuErrorKind::Other))?;

    let mut instance = create_instance(&context, window.h_wnd(), enable_validation)?;
    println!("Vulkan instance created successfully");

    if instance.validation_enabled {
        instance.debug_messenger = Some(setup_debug_messenger(&context, &instance)?);
    }

    let (physical_device, rt_props) = get_physical_device(&instance)?;

    let rt_supported = rt_props.is_some();
    println!("Physical device selected successfully");

    let surface = util::create_surface(&context, &instance, &window)?;
    println!("Surface created successfully");

    let logical_device =
        create_logical_device(&context, &instance, physical_device, surface, rt_supported)?;
    println!("Logical device created successfully");

    let queue = logical_device.get_main_queue();
    println!("Graphics queue retrieved successfully");

    let ac_device = if logical_device.rt_supported {
        Some(ash::khr::acceleration_structure::Device::new(
            &instance,
            &logical_device.device,
        ))
    } else {
        None
    };
    let vk_handle_tracker = VulkanHandleTracker::new(logical_device.device.clone(), ac_device);

    let (swapchain, depth_targets) = create_swapchain_and_depth_buffer(
        &context,
        &instance,
        {
            let size = window.inner_size();
            (size.width, size.height)
        },
        physical_device,
        &logical_device,
        surface,
        sync_mode,
        settings.hdr_preferred,
        vk_handle_tracker.clone(),
        ash::vk::SwapchainKHR::null(),
    )?;
    println!("Swapchain and depth buffer created successfully");

    // let pipeline = create_graphics_pipeline(&logical_device, &swapchain)?;
    // println!("Graphics pipeline created successfully");

    let command_pool = create_command_pool(&logical_device)?;
    println!("Command pools created successfully");

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
    let push_descriptor = ash::khr::push_descriptor::Device::new(&instance, &logical_device);

    let desc_pool = create_descriptor_pool(&logical_device)?;

    let desc_set_layout = create_descriptor_set_layout(&logical_device)?;

    let rt_feature = if let Some(props) = rt_props {
        let loader = ash::khr::ray_tracing_pipeline::Device::new(&instance, &logical_device);
        let desc_layout = rt::create_rt_descriptor_layout(&logical_device)?;
        let layout = rt::create_pipeline_layout(&logical_device, desc_set_layout, desc_layout)?;

        Some(rt::RtFeature {
            pipeline_loader: loader,
            properties: props,
            pipeline_layout: layout,
            descriptor_layout: desc_layout,
        })
    } else {
        None
    };

    let desc_sets = create_descriptor_sets(&logical_device, desc_pool, desc_set_layout)?
        .try_into()
        .map_err(|_| {
            GpuError::new(
                "Descriptor Sets do not match expected FRAMES_IN_FLIGHT",
                GpuErrorKind::Other,
            )
        })?;

    let pipeline_layout = create_pipeline_layout(&logical_device, desc_set_layout)?;
    let compute_pipeline_layout = create_compute_pipeline_layout(&logical_device, desc_set_layout)?;

    Ok(VulkanBackend {
        window,
        context,
        instance,
        phys_device: physical_device,
        device: logical_device,
        swapchain,
        depth_targets,
        queue,
        // graphics_pipeline: pipeline,
        pipeline_layout,
        compute_pipeline_layout,
        command_pool,
        command_buffers,
        descriptors: Descriptors {
            pool: desc_pool,
            layout: desc_set_layout,
            sets: desc_sets,
        },
        sync_objects,
        khr_sync,
        push_descriptor,
        frame_idx: 0,
        current_frame: None,
        texture_registry: RefCell::new(TextureRegistry::new()),
        egui_renderer: None,
        vulkan_handle_tracker: vk_handle_tracker,
        rt_feature,
    })
}

fn create_pipeline_layout(
    device: &LogicalDevice,
    descriptor_layout: ash::vk::DescriptorSetLayout,
) -> Result<ash::vk::PipelineLayout, GpuError> {
    let push_range = ash::vk::PushConstantRange {
        stage_flags: ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::FRAGMENT,
        offset: 0,
        size: std::mem::size_of::<PushConstants>() as u32,
    };
    let create_info = ash::vk::PipelineLayoutCreateInfo {
        set_layout_count: 1,
        p_set_layouts: &descriptor_layout,
        push_constant_range_count: 1,
        p_push_constant_ranges: &push_range,
        ..Default::default()
    };

    unsafe { device.create_pipeline_layout(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create pipeline layout: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })
}

fn create_compute_pipeline_layout(
    device: &LogicalDevice,
    descriptor_layout: ash::vk::DescriptorSetLayout,
) -> Result<ash::vk::PipelineLayout, GpuError> {
    let push_range = ash::vk::PushConstantRange {
        stage_flags: ash::vk::ShaderStageFlags::COMPUTE,
        offset: 0,
        size: std::mem::size_of::<ComputePushConstants>() as u32, // maxInstances + assetOffset + maxHeight + spawn params + scale + tilt + terrainSegmentsF
    };
    let create_info = ash::vk::PipelineLayoutCreateInfo {
        set_layout_count: 1,
        p_set_layouts: &descriptor_layout,
        push_constant_range_count: 1,
        p_push_constant_ranges: &push_range,
        ..Default::default()
    };
    unsafe { device.create_pipeline_layout(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create compute pipeline layout: {e:?}"),
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
    // Binding 0: Main ViewProj UBO (view+proj, 128B) — deferred_pre vtx, forward vtx
    // Binding 1: Camera pixel UBO (cameraPos+ssao, 16B) — deferred_light pxl, forward pxl
    // Binding 2: Light data UBO (Light struct, 96B) — deferred_light pxl, forward pxl
    // Binding 3: Shadow LightSpace UBO (lightSpaceMatrix, 64B) — shadow vtx
    // Binding 4: Skybox ViewProj UBO (view+proj, 128B) — skybox vtx
    // Binding 5: DeferredPre NearFar UBO (near/far, 16B) — deferred_pre pxl
    // Binding 6: Global 2D texture array (CIS[1024]) — all pixel shaders
    // Binding 7: Cubemap array (CIS[4]) — skybox pxl
    // Binding 8: Shadow depth images (SAMPLED_IMAGE[4]) — shadow module
    // Binding 9: Comparison samplers (SAMPLER[4]) — shadow module
    // Binding 10: instance transforms (procedural gen)
    // Binding 11: structured buffers (procedural gen) - draw commands
    // Binding 12: texture binding for compute
    let bindings = [
        ash::vk::DescriptorSetLayoutBinding {
            binding: 0,
            descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::VERTEX
                | ash::vk::ShaderStageFlags::FRAGMENT
                | ash::vk::ShaderStageFlags::RAYGEN_KHR,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 1,
            descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 2,
            descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 3,
            descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::VERTEX,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 4,
            descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 5,
            descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 6,
            descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: MAX_BINDLESS_TEXTURES,
            stage_flags: ash::vk::ShaderStageFlags::VERTEX
                | ash::vk::ShaderStageFlags::FRAGMENT
                | ash::vk::ShaderStageFlags::ANY_HIT_KHR,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 7,
            descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: MAX_BINDLESS_CUBEMAPS,
            stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 8,
            descriptor_type: ash::vk::DescriptorType::SAMPLED_IMAGE,
            descriptor_count: MAX_BINDLESS_SHADOW_IMAGES,
            stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 9,
            descriptor_type: ash::vk::DescriptorType::SAMPLER,
            descriptor_count: MAX_BINDLESS_COMPARISON_SAMPLERS,
            stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 10,
            descriptor_type: ash::vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::COMPUTE,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 11,
            descriptor_type: ash::vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::COMPUTE,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 12,
            descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::COMPUTE,
            ..Default::default()
        },
        // Binding 13: RT lights array (STORAGE_BUFFER) — closest-hit shader
        ash::vk::DescriptorSetLayoutBinding {
            binding: 13,
            descriptor_type: ash::vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::RAYGEN_KHR
                | ash::vk::ShaderStageFlags::CLOSEST_HIT_KHR,
            ..Default::default()
        },
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
    // 6 UBOs per set * FRAMES_IN_FLIGHT
    let uniform_pool_info = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::UNIFORM_BUFFER,
        descriptor_count: 6 * FRAMES_IN_FLIGHT,
    };
    // 1024 + 4 CIS per set * FRAMES_IN_FLIGHT
    let cis_pool_info = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: (MAX_BINDLESS_TEXTURES + MAX_BINDLESS_CUBEMAPS + 1) * FRAMES_IN_FLIGHT,
    };
    let sampled_image_pool_info = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::SAMPLED_IMAGE,
        descriptor_count: MAX_BINDLESS_SHADOW_IMAGES * FRAMES_IN_FLIGHT,
    };
    let sampler_pool_info = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::SAMPLER,
        descriptor_count: MAX_BINDLESS_COMPARISON_SAMPLERS * FRAMES_IN_FLIGHT,
    };
    let storage_pool_infos = ash::vk::DescriptorPoolSize {
        ty: ash::vk::DescriptorType::STORAGE_BUFFER,
        descriptor_count: 3 * FRAMES_IN_FLIGHT, // bindings 10, 11, 13
    };
    let pool_sizes = [
        uniform_pool_info,
        cis_pool_info,
        sampled_image_pool_info,
        sampler_pool_info,
        storage_pool_infos,
    ];
    let create_info = ash::vk::DescriptorPoolCreateInfo {
        flags: ash::vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND,
        max_sets: FRAMES_IN_FLIGHT,
        pool_size_count: pool_sizes.len() as u32,
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
        queue_family_index: device.main_queue_index,
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
        queue_family_index: device.main_queue_index,
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

// fn create_graphics_pipeline(
//     device: &ash::Device,
//     swapchain: &Swapchain,
// ) -> Result<ash::vk::Pipeline, GpuError> {
//     let shader_vert = util::load_shader_blob("src/shaders/spv/example.vert.spv")?;
//     let shader_pxl = util::load_shader_blob("src/shaders/spv/example.pxl.spv")?;

//     let shader_module_vert = create_shader_module(&shader_vert, device, "Vertex Shader")?;
//     let shader_module_pxl = create_shader_module(&shader_pxl, device, "Pixel Shader")?;

//     let vtx_shader_stage_create = ash::vk::PipelineShaderStageCreateInfo {
//         stage: ash::vk::ShaderStageFlags::VERTEX,
//         module: shader_module_vert,
//         p_name: SHADER_ENTRY_POINT.as_ptr(),
//         ..Default::default()
//     };
//     let pxl_shader_stage_create = ash::vk::PipelineShaderStageCreateInfo {
//         stage: ash::vk::ShaderStageFlags::FRAGMENT,
//         module: shader_module_pxl,
//         p_name: SHADER_ENTRY_POINT.as_ptr(),
//         ..Default::default()
//     };
//     let shader_stages = [vtx_shader_stage_create, pxl_shader_stage_create];

//     let dynamic_states = [
//         ash::vk::DynamicState::VIEWPORT,
//         ash::vk::DynamicState::SCISSOR,
//     ];

//     let dynamic_state_create_info = ash::vk::PipelineDynamicStateCreateInfo {
//         dynamic_state_count: dynamic_states.len() as u32,
//         p_dynamic_states: dynamic_states.as_ptr(),
//         ..Default::default()
//     };

//     let vtx_input_state_info = ash::vk::PipelineVertexInputStateCreateInfo {
//         ..Default::default()
//     };
//     let input_assembly_info = ash::vk::PipelineInputAssemblyStateCreateInfo {
//         topology: ash::vk::PrimitiveTopology::TRIANGLE_LIST,
//         ..Default::default()
//     };

//     let viewport_create_info = ash::vk::PipelineViewportStateCreateInfo {
//         viewport_count: 1,
//         scissor_count: 1,
//         ..Default::default()
//     };

//     let rasterization_state_create_info = ash::vk::PipelineRasterizationStateCreateInfo {
//         depth_clamp_enable: ash::vk::FALSE,
//         rasterizer_discard_enable: ash::vk::FALSE,
//         polygon_mode: ash::vk::PolygonMode::FILL,
//         cull_mode: ash::vk::CullModeFlags::BACK,
//         front_face: ash::vk::FrontFace::CLOCKWISE,
//         depth_bias_enable: ash::vk::FALSE,
//         line_width: 1.0f32,
//         ..Default::default()
//     };

//     let multisample_state_create_info = ash::vk::PipelineMultisampleStateCreateInfo {
//         rasterization_samples: ash::vk::SampleCountFlags::TYPE_1,
//         sample_shading_enable: ash::vk::FALSE,
//         ..Default::default()
//     };

//     let blend_attachment_state = ash::vk::PipelineColorBlendAttachmentState {
//         blend_enable: ash::vk::FALSE,
//         color_write_mask: ash::vk::ColorComponentFlags::RGBA,
//         ..Default::default()
//     };
//     let blend_state_create_info = ash::vk::PipelineColorBlendStateCreateInfo {
//         logic_op_enable: ash::vk::FALSE,
//         logic_op: ash::vk::LogicOp::COPY,
//         attachment_count: 1,
//         p_attachments: &blend_attachment_state as *const _,
//         ..Default::default()
//     };

//     let pipeline_layout_create_info = ash::vk::PipelineLayoutCreateInfo {
//         set_layout_count: 0,
//         push_constant_range_count: 0,
//         ..Default::default()
//     };
//     let pipeline_layout =
//         unsafe { device.create_pipeline_layout(&pipeline_layout_create_info, None) }.map_err(
//             |e| {
//                 GpuError::new(
//                     format!("Pipeline Layout creation failed: {e:?}"),
//                     GpuErrorKind::ResourceCreation,
//                 )
//             },
//         )?;
//     let rendering_create_info = ash::vk::PipelineRenderingCreateInfo {
//         color_attachment_count: 1,
//         p_color_attachment_formats: &swapchain.surface_format.format.format as *const _,
//         ..Default::default()
//     };

//     let pipeline_create_info = ash::vk::GraphicsPipelineCreateInfo {
//         stage_count: 2,
//         p_stages: shader_stages.as_ptr(),
//         p_vertex_input_state: &vtx_input_state_info as *const _,
//         p_input_assembly_state: &input_assembly_info as *const _,
//         p_viewport_state: &viewport_create_info as *const _,
//         p_rasterization_state: &rasterization_state_create_info as *const _,
//         p_multisample_state: &multisample_state_create_info as *const _,
//         p_color_blend_state: &blend_state_create_info as *const _,
//         p_dynamic_state: &dynamic_state_create_info as *const _,
//         layout: pipeline_layout,
//         render_pass: ash::vk::RenderPass::null(),
//         p_next: &rendering_create_info as *const _ as *const c_void,
//         ..Default::default()
//     };

//     let pipeline = unsafe {
//         device.create_graphics_pipelines(
//             ash::vk::PipelineCache::null(),
//             &[pipeline_create_info],
//             None,
//         )
//     }
//     .map_err(|e| {
//         GpuError::new(
//             format!("Failed to create graphics pipeline: {e:?}"),
//             GpuErrorKind::ResourceCreation,
//         )
//     })?;

//     // Shader modules can be destroyed after pipeline creation
//     unsafe {
//         device.destroy_shader_module(shader_module_vert, None);
//         device.destroy_shader_module(shader_module_pxl, None);
//         device.destroy_pipeline_layout(pipeline_layout, None);
//     }

//     pipeline.into_iter().next().ok_or_else(|| {
//         GpuError::new(
//             "Create pipeline returned empty response",
//             GpuErrorKind::ResourceCreation,
//         )
//     })
// }

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
    window_size: (u32, u32),
    physical_device: ash::vk::PhysicalDevice,
    logical_device: &LogicalDevice,
    surface: ash::vk::SurfaceKHR,
    sync_mode: SyncMode,
    hdr_preferred: bool,
    vk_handle_tracker: VulkanHandleTracker,
    old_swapchain: ash::vk::SwapchainKHR,
) -> Result<(Swapchain, [VulkanTexture; FRAMES_IN_FLIGHT as usize]), GpuError> {
    println!("Querying surface capabilities and formats...");
    let surface_khr = ash::khr::surface::Instance::new(context, instance);

    println!("Retrieving surface capabilities...");
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
    println!(
        "Surface capabilities retrieved successfully: {:?}",
        surface_capababilities
    );
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
    println!(
        "Surface formats retrieved successfully, count: {}",
        surface_formats.len()
    );
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
    println!(
        "Present modes retrieved successfully, count: {}",
        present_modes.len()
    );
    let swap_extent = util::choose_swap_extent(&surface_capababilities, window_size);
    println!("Chosen swap extent: {:?}", swap_extent);
    let swap_image_count = util::choose_swap_min_image_count(&surface_capababilities);
    println!("Chosen swap image count: {}", swap_image_count);
    let swap_format = util::choose_swapchain_format(&surface_formats, hdr_preferred)?;
    println!(
        "Chosen swap format: {:?} colorspace: {:?}",
        swap_format.format.format, swap_format.color_space
    );
    let present_mode = util::choose_present_mode(&present_modes, sync_mode)?;
    println!("Chosen present mode: {:?}", present_mode);

    let engine_fmt: TextureFormat = swap_format.format.format.try_into()?;

    let create_info = ash::vk::SwapchainCreateInfoKHR {
        surface,
        min_image_count: swap_image_count,
        image_format: swap_format.format.format,
        image_color_space: swap_format.color_space,
        image_extent: swap_extent,
        image_array_layers: 1,
        image_usage: ash::vk::ImageUsageFlags::COLOR_ATTACHMENT,
        image_sharing_mode: ash::vk::SharingMode::EXCLUSIVE,
        pre_transform: surface_capababilities.current_transform,
        composite_alpha: ash::vk::CompositeAlphaFlagsKHR::OPAQUE,
        present_mode,
        clipped: ash::vk::TRUE,
        old_swapchain,
        ..Default::default()
    };

    let swapchain_khr = ash::khr::swapchain::Device::new(instance, logical_device);
    println!("Creating swapchain...");
    let swapchain = unsafe { swapchain_khr.create_swapchain(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create swapchain: {:?}", e),
            GpuErrorKind::ResourceCreation,
        )
    })?;
    println!("Swapchain created successfully");
    let swapchain_images =
        unsafe { swapchain_khr.get_swapchain_images(swapchain) }.map_err(|e| {
            GpuError::new(
                format!("Failed to retrieve swapchain images: {:?}", e),
                GpuErrorKind::ResourceCreation,
            )
        })?;
    println!(
        "Swapchain images retrieved successfully, count: {}",
        swapchain_images.len()
    );

    let swapchain_image_views =
        create_image_views(logical_device, &swapchain_images, swap_format.format.format)?;
    println!("Swapchain image views created successfully");

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
    println!("Swapchain sampler created successfully");
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
            aspect: ash::vk::ImageAspectFlags::COLOR,
            mip_levels: 1,
            view_type: ash::vk::ImageViewType::TYPE_2D,
            compare_enabled: false,
            id: 0,
            descriptor_index: u32::MAX,
            device_handle: logical_device.device.clone(),
            current_layout: Rc::new(Cell::new(ash::vk::ImageLayout::UNDEFINED)),
            vullkan_handle_tracker: vk_handle_tracker.clone(),
        })
        .collect::<Vec<_>>();
    println!("Swapchain textures created successfully");

    // depth targets
    let depth_target_desc = RenderTargetDesc {
        width: swap_extent.width,
        height: swap_extent.height,
        format: TextureFormat::Depth32Float,
        sampler: SamplerDesc::default(),
        usage: RenderTargetUsage::Depth,
    };
    let depth_targets = (0..FRAMES_IN_FLIGHT)
        .map(|_| {
            VulkanBackend::create_vk_render_target(
                instance,
                logical_device,
                physical_device,
                &depth_target_desc,
                vk_handle_tracker.clone(),
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
    println!("Depth targets created successfully");

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
    instance: &Instance,
    physical_device: ash::vk::PhysicalDevice,
    surface: ash::vk::SurfaceKHR,
    with_rt: bool,
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
        q.queue_flags.contains(ash::vk::QueueFlags::GRAPHICS)
            && surface_support
            && q.queue_flags.contains(ash::vk::QueueFlags::COMPUTE)
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

    let mut accel_features = ash::vk::PhysicalDeviceAccelerationStructureFeaturesKHR {
        acceleration_structure: ash::vk::TRUE,
        ..Default::default()
    };
    let mut rt_features = ash::vk::PhysicalDeviceRayTracingPipelineFeaturesKHR {
        ray_tracing_pipeline: ash::vk::TRUE,
        p_next: &mut accel_features as *mut _ as *mut _,
        ..Default::default()
    };

    let mut vk_12_feats = ash::vk::PhysicalDeviceVulkan12Features {
        buffer_device_address: ash::vk::TRUE,
        ..Default::default()
    };

    if with_rt {
        vk_12_feats.p_next = &mut rt_features as *mut _ as *mut _;
    }

    let mut ext_state_struct = ash::vk::PhysicalDeviceExtendedDynamicStateFeaturesEXT {
        extended_dynamic_state: ash::vk::TRUE,
        p_next: &mut vk_12_feats as *mut _ as *mut std::ffi::c_void,
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
        descriptor_binding_update_unused_while_pending: ash::vk::TRUE,
        descriptor_binding_storage_buffer_update_after_bind: ash::vk::TRUE,
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
            shader_sampled_image_array_dynamic_indexing: ash::vk::TRUE,
            shader_uniform_buffer_array_dynamic_indexing: ash::vk::TRUE,
            ..Default::default()
        },
        p_next: &mut shader_float16_feats as *mut _ as *mut std::ffi::c_void,
        ..Default::default()
    };
    let required_exts = if with_rt {
        REQUIRED_EXTS.iter().chain(RT_EXTS.iter())
    } else {
        REQUIRED_EXTS.iter().chain([].iter())
    };
    let device_exts_ptr = required_exts.map(|ext| ext.as_ptr()).collect::<Vec<_>>();

    let create_info = ash::vk::DeviceCreateInfo {
        p_next: &mut base_struct as *mut _ as *mut std::ffi::c_void,
        queue_create_info_count: 1,
        p_queue_create_infos: &queue_info,
        enabled_extension_count: device_exts_ptr.len() as u32,
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

    let debug_utils_ext = if instance.validation_enabled {
        Some(ash::ext::debug_utils::Device::new(&instance, &device))
    } else {
        None
    };

    Ok(LogicalDevice {
        device,
        main_queue_index: idx as u32,
        debug_utils_ext,
        rt_supported: with_rt,
    })
}

fn get_physical_device(
    instance: &ash::Instance,
) -> Result<(ash::vk::PhysicalDevice, Option<rt::RtDeviceProperties>), GpuError> {
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
            let has_required_features = /*features.geometry_shader == ash::vk::TRUE
                &&*/ features.sampler_anisotropy == ash::vk::TRUE;

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
            let mut with_rt = true;
            if let Some(ext) = RT_EXTS.iter().find(|ext| {
                !device_exts.iter().any(|prop| {
                    let extension_name = unsafe { CStr::from_ptr(prop.extension_name.as_ptr()) };
                    extension_name == **ext
                })
            }) {
                println!(
                    "Physical device {:?} does not support raytracing extension {}, skipping",
                    device,
                    ext.to_string_lossy()
                );
                with_rt = false;
            }

            // check additional required features
            // check for update after bind on storage buffers
            let mut descriptor_indexing_features: ash::vk::PhysicalDeviceDescriptorIndexingFeatures = Default::default();
            // check for extended dynamic state
            let mut features_ext_state = ash::vk::PhysicalDeviceExtendedDynamicStateFeaturesEXT {
                p_next: &mut descriptor_indexing_features as *mut _ as *mut _,
                ..Default::default()
            };
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
            if descriptor_indexing_features.descriptor_binding_storage_buffer_update_after_bind != ash::vk::TRUE {
                println!(
                    "Physical device {:?} does not support updating storage buffers after bind. skipping",
                    device
                );
                return None;
            }

            let discrete = properties.device_type == ash::vk::PhysicalDeviceType::DISCRETE_GPU;
            let is_gpu = properties.device_type == ash::vk::PhysicalDeviceType::DISCRETE_GPU
                || properties.device_type == ash::vk::PhysicalDeviceType::INTEGRATED_GPU;
            let has_preferred_features = features.tessellation_shader == ash::vk::TRUE
                && features.sampler_anisotropy == ash::vk::TRUE;

            println!(
                "Evaluated physical device {:?}: type={:?}, discrete={}, preferred_features={}, raytracing={}",
                device, properties.device_type, discrete, has_preferred_features, with_rt
            );
            Some((*device, discrete, is_gpu, has_preferred_features, with_rt))
        })
        .collect::<Vec<_>>();

    if evaluated_devices.is_empty() {
        return Err(GpuError::new(
            "No suitable physical devices found (missing required features)",
            GpuErrorKind::Other,
        ));
    }

    // rank devices by discrete > integrated, then by having preferred features
    let (best_device, _, _, _, with_rt) = evaluated_devices
        .into_iter()
        .max_by_key(|(_, discrete, is_gpu, has_features, with_rt)| {
            (*discrete as u8) << 4
                | (*with_rt as u8) << 3
                | (*is_gpu as u8) << 2
                | (*has_features as u8)
        })
        .unwrap();

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

    let rt_props = if with_rt {
        let mut rt_pipeline_props =
            ash::vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
        let mut accel_props = ash::vk::PhysicalDeviceAccelerationStructurePropertiesKHR {
            p_next: &mut rt_pipeline_props as *mut _ as *mut _,
            ..Default::default()
        };
        let mut props2 = ash::vk::PhysicalDeviceProperties2 {
            p_next: &mut accel_props as *mut _ as *mut _,
            ..Default::default()
        };
        unsafe { instance.get_physical_device_properties2(best_device, &mut props2) };
        Some(rt::RtDeviceProperties {
            min_scratch_offset_alignment: accel_props
                .min_acceleration_structure_scratch_offset_alignment,
            shader_group_handle_size: rt_pipeline_props.shader_group_handle_size,
            shader_group_base_alignment: rt_pipeline_props.shader_group_base_alignment,
            shader_group_handle_alignment: rt_pipeline_props.shader_group_handle_alignment,
            max_ray_recursion_depth: rt_pipeline_props.max_ray_recursion_depth,
        })
    } else {
        None
    };

    println!("Selected GPU: {device_name} ({device_type_str})");
    println!("  Vulkan: {major}.{minor}");
    println!("  VRAM: {vram_gib:.1} GiB");

    Ok((best_device, rt_props))
}

fn create_instance(
    context: &ash::Entry,
    hwnd: RawWindowHandle,
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

    let mut instance_exts = util::get_instance_extensions(hwnd)?;

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
