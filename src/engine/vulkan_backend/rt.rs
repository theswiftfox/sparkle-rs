use crate::engine::{backend::AccelerationStructureType, vulkan_backend::buffer::VulkanBuffer};

pub struct RtDeviceProperties {
    pub min_scratch_offset_alignment: u32,
    pub shader_group_handle_size: u32,
    pub shader_group_base_alignment: u32,
    pub shader_group_handle_alignment: u32,
    pub max_ray_recursion_depth: u32,
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
