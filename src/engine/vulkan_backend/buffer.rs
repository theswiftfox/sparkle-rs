use std::os::raw::c_void;

use crate::engine::{
    backend::{BufferUsage, GpuBuffer, GpuError, GpuErrorKind},
    vulkan_backend::VulkanBackend,
};

pub struct VulkanBuffer {
    pub buffer: ash::vk::Buffer,
    pub memory: ash::vk::DeviceMemory,
    pub mapped: *mut c_void,
    pub flags: ash::vk::MemoryPropertyFlags,
    pub size: ash::vk::DeviceSize,
}

impl VulkanBuffer {
    pub fn destroy(device: &ash::Device, buffer: VulkanBuffer) {
        let VulkanBuffer { buffer, memory, .. } = buffer;

        unsafe {
            if buffer != ash::vk::Buffer::null() {
                device.destroy_buffer(buffer, None);
            }
            if memory != ash::vk::DeviceMemory::null() {
                device.free_memory(memory, None);
            }
        }
    }
}

impl VulkanBackend {
    pub fn create_vulkan_buffer(
        &self,
        size: ash::vk::DeviceSize,
        usage: ash::vk::BufferUsageFlags,
        properties: ash::vk::MemoryPropertyFlags,
    ) -> Result<VulkanBuffer, GpuError> {
        let (buffer, memory) = Self::create_buffer(
            &self.instance,
            &self.device,
            self.phys_device,
            size,
            usage.into(),
            properties,
        )?;
        let mapped = if properties.contains(ash::vk::MemoryPropertyFlags::HOST_VISIBLE)
            && properties.contains(ash::vk::MemoryPropertyFlags::HOST_COHERENT)
        {
            unsafe {
                self.device
                    .map_memory(memory, 0, size, ash::vk::MemoryMapFlags::empty())
            }
            .map_err(|e| {
                GpuError::new(
                    format!("Failed to map buffer memory: {e:?}"),
                    GpuErrorKind::ResourceUpdate,
                )
            })?
        } else {
            std::ptr::null_mut()
        };

        Ok(VulkanBuffer {
            buffer,
            memory,
            mapped,
            flags: properties,
            size,
        })
    }
}

impl GpuBuffer for VulkanBuffer {
    fn size(&self) -> usize {
        self.size as usize
    }
}

impl Into<ash::vk::BufferUsageFlags> for BufferUsage {
    fn into(self) -> ash::vk::BufferUsageFlags {
        match self {
            BufferUsage::Vertex => ash::vk::BufferUsageFlags::VERTEX_BUFFER,
            BufferUsage::Index => ash::vk::BufferUsageFlags::INDEX_BUFFER,
            BufferUsage::Uniform => ash::vk::BufferUsageFlags::UNIFORM_BUFFER,
        }
    }
}
