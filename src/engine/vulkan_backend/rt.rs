use crate::engine::{
    backend::{AccelerationStructureType, GpuError, GpuErrorKind},
    vulkan_backend::{VulkanBackend, buffer::VulkanBuffer},
};

pub const IDX_RAYGEN: u32 = 0;
pub const IDX_MISS: u32 = 1;
pub const IDX_MISS_SHADOW: u32 = 2;
pub const IDX_CHIT: u32 = 3;

pub struct RtFeature {
    pub pipeline_loader: ash::khr::ray_tracing_pipeline::Device,
    pub properties: RtDeviceProperties,
    pub pipeline_layout: ash::vk::PipelineLayout,
    pub descriptor_layout: ash::vk::DescriptorSetLayout,
}

pub struct RtDeviceProperties {
    pub min_scratch_offset_alignment: u32,
    pub shader_group_handle_size: u32,
    pub shader_group_base_alignment: u32,
    pub shader_group_handle_alignment: u32,
    pub max_ray_recursion_depth: u32,
}

pub struct SbtRegions {
    pub raygen: ash::vk::StridedDeviceAddressRegionKHR,
    pub miss: ash::vk::StridedDeviceAddressRegionKHR,
    pub hit: ash::vk::StridedDeviceAddressRegionKHR,
    pub callable: ash::vk::StridedDeviceAddressRegionKHR,
}

pub struct RtSbt {
    pub buffer: VulkanBuffer,
    pub regions: SbtRegions,
}

pub struct AccelerationStructure {
    pub(super) handle: ash::vk::AccelerationStructureKHR,
    pub(super) buffer: VulkanBuffer,
}

impl Into<ash::vk::AccelerationStructureTypeKHR> for AccelerationStructureType {
    fn into(self) -> ash::vk::AccelerationStructureTypeKHR {
        match self {
            AccelerationStructureType::Blas => ash::vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            AccelerationStructureType::Tlas => ash::vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        }
    }
}

pub fn create_rt_descriptor_layout(
    device: &ash::Device,
) -> Result<ash::vk::DescriptorSetLayout, GpuError> {
    let bindings = [
        ash::vk::DescriptorSetLayoutBinding {
            binding: 0,
            descriptor_type: ash::vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::ALL,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 1,
            descriptor_type: ash::vk::DescriptorType::STORAGE_IMAGE,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::ALL,
            ..Default::default()
        },
        ash::vk::DescriptorSetLayoutBinding {
            binding: 2,
            descriptor_type: ash::vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::ALL,
            ..Default::default()
        },
    ];

    let create_info = ash::vk::DescriptorSetLayoutCreateInfo {
        flags: ash::vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR,
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

pub fn create_pipeline_layout(
    device: &ash::Device,
    set0: ash::vk::DescriptorSetLayout,
    rt_set: ash::vk::DescriptorSetLayout,
) -> Result<ash::vk::PipelineLayout, GpuError> {
    let set_layouts = [set0, rt_set];

    let push_range = ash::vk::PushConstantRange {
        stage_flags: ash::vk::ShaderStageFlags::RAYGEN_KHR
            | ash::vk::ShaderStageFlags::MISS_KHR
            | ash::vk::ShaderStageFlags::CLOSEST_HIT_KHR,
        offset: 0,
        size: 16, // frame_index: u32, width: u32, height: u32, number_of_lights: u32
    };

    let create_info = ash::vk::PipelineLayoutCreateInfo {
        set_layout_count: set_layouts.len() as u32,
        p_set_layouts: set_layouts.as_ptr(),
        push_constant_range_count: 1,
        p_push_constant_ranges: &push_range,
        ..Default::default()
    };

    unsafe { device.create_pipeline_layout(&create_info, None) }.map_err(|e| {
        GpuError::new(
            format!("Failed to create RT pipeline layout: {e:?}"),
            GpuErrorKind::ResourceCreation,
        )
    })
}

impl VulkanBackend {
    pub fn create_sbt(
        &self,
        pipeline_loader: &ash::khr::ray_tracing_pipeline::Device,
        pipeline: ash::vk::Pipeline,
        props: &RtDeviceProperties,
    ) -> Result<RtSbt, GpuError> {
        let handle_size = props.shader_group_handle_size as u64;
        let handle_alignment = props.shader_group_handle_alignment as u64;
        let base_alignment = props.shader_group_base_alignment as u64;

        // aligned size of a single handle within a region
        let handle_stride = align_up(handle_size, handle_alignment);
        // each region is aligned to base_alignment; raygen stride must equal its region size
        let region_size = align_up(handle_stride, base_alignment);

        let total_size = region_size * 4; // raygen + primary miss + shadow miss + hit

        // query raw handles from the driver
        let handles_raw = unsafe {
            pipeline_loader.get_ray_tracing_shader_group_handles(
                pipeline,
                0,
                4,
                (handle_size * 4) as usize,
            )
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to get shader group handles: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        let (sbt_buf, sbt_mem) = VulkanBackend::create_buffer(
            &self.instance,
            &self.device,
            self.phys_device,
            total_size,
            ash::vk::BufferUsageFlags::SHADER_BINDING_TABLE_KHR
                | ash::vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | ash::vk::BufferUsageFlags::TRANSFER_DST,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        self.vulkan_handle_tracker.register_buffer(sbt_buf);
        self.vulkan_handle_tracker.register_device_memory(sbt_mem);

        // map and write each handle at its region offset
        let mapped = unsafe {
            self.device
                .map_memory(sbt_mem, 0, total_size, ash::vk::MemoryMapFlags::empty())
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to map SBT memory: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })? as *mut u8;

        unsafe {
            let hs = handle_size as usize;
            let rs = region_size as usize;
            // raygen at offset 0
            std::ptr::copy_nonoverlapping(handles_raw.as_ptr(), mapped, hs);
            // primary miss at offset region_size
            std::ptr::copy_nonoverlapping(handles_raw.as_ptr().add(hs), mapped.add(rs), hs);
            // shadow miss at offset 2 * region_size
            std::ptr::copy_nonoverlapping(handles_raw.as_ptr().add(hs * 2), mapped.add(rs * 2), hs);
            // hit at offset 3 * region_size
            std::ptr::copy_nonoverlapping(handles_raw.as_ptr().add(hs * 3), mapped.add(rs * 3), hs);
        }

        let base_addr = unsafe {
            self.device
                .get_buffer_device_address(&ash::vk::BufferDeviceAddressInfo {
                    buffer: sbt_buf,
                    ..Default::default()
                })
        };

        let regions = SbtRegions {
            // raygen: stride must equal size per Vulkan spec
            raygen: ash::vk::StridedDeviceAddressRegionKHR {
                device_address: base_addr,
                stride: region_size,
                size: region_size,
            },
            // miss region covers both primary miss (index 0) and shadow miss (index 1)
            miss: ash::vk::StridedDeviceAddressRegionKHR {
                device_address: base_addr + region_size,
                stride: region_size,
                size: region_size * 2,
            },
            hit: ash::vk::StridedDeviceAddressRegionKHR {
                device_address: base_addr + region_size * 3,
                stride: region_size,
                size: region_size,
            },
            callable: ash::vk::StridedDeviceAddressRegionKHR::default(),
        };

        let buffer = VulkanBuffer {
            buffer: sbt_buf,
            memory: sbt_mem,
            mapped: mapped as *mut std::ffi::c_void,
            flags: ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
            size: total_size,
            device_handle: self.device.device.clone(),
            per_frame_copies: None,
            vulkan_handle_tracker: self.vulkan_handle_tracker.clone(),
            is_storage_buffer: false,
        };

        Ok(RtSbt { buffer, regions })
    }
}

#[inline]
fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}
