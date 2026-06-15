use std::os::raw::c_void;

use crate::engine::{
    backend::{BufferUsage, GpuBuffer, GpuError, GpuErrorKind},
    vulkan_backend::VulkanBackend,
};

pub struct PerFrameCopy {
    pub buffer: ash::vk::Buffer,
    pub memory: ash::vk::DeviceMemory,
    pub mapped: *mut c_void,
}

pub struct VulkanBuffer {
    pub buffer: ash::vk::Buffer,
    pub memory: ash::vk::DeviceMemory,
    pub mapped: *mut c_void,
    pub flags: ash::vk::MemoryPropertyFlags,
    pub size: ash::vk::DeviceSize,
    pub(crate) device_handle: ash::Device,
    pub per_frame_copies: Option<Vec<PerFrameCopy>>,
}

impl VulkanBuffer {
    pub fn destroy(&self) {
        let VulkanBuffer {
            buffer,
            memory,
            device_handle,
            ..
        } = self;

        unsafe {
            if *buffer != ash::vk::Buffer::null() {
                device_handle.destroy_buffer(*buffer, None);
            }
            if *memory != ash::vk::DeviceMemory::null() {
                device_handle.free_memory(*memory, None);
            }
        }

        if let Some(copies) = &self.per_frame_copies {
            for copy in copies {
                unsafe {
                    if copy.buffer != ash::vk::Buffer::null() {
                        device_handle.destroy_buffer(copy.buffer, None);
                    }
                    if copy.memory != ash::vk::DeviceMemory::null() {
                        device_handle.free_memory(copy.memory, None);
                    }
                }
            }
        }
    }

    pub fn is_host_mapable(&self) -> bool {
        host_mappable(self.flags)
    }

    pub fn frame_buffer(&self, frame_idx: usize) -> ash::vk::Buffer {
        match &self.per_frame_copies {
            None => self.buffer,
            Some(copies) => {
                if frame_idx == 0 {
                    self.buffer
                } else {
                    copies[frame_idx - 1].buffer
                }
            }
        }
    }

    pub fn frame_mapped(&self, frame_idx: usize) -> *mut c_void {
        match &self.per_frame_copies {
            None => self.mapped,
            Some(copies) => {
                if frame_idx == 0 {
                    self.mapped
                } else {
                    copies[frame_idx - 1].mapped
                }
            }
        }
    }
}

pub fn host_mappable(flags: ash::vk::MemoryPropertyFlags) -> bool {
    flags
        & (ash::vk::MemoryPropertyFlags::HOST_VISIBLE | ash::vk::MemoryPropertyFlags::HOST_COHERENT)
        == (ash::vk::MemoryPropertyFlags::HOST_VISIBLE
            | ash::vk::MemoryPropertyFlags::HOST_COHERENT)
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
        let has_host_flags = host_mappable(properties);
        // println!("Creating buffer with usage {usage:?}. Host mapped: {has_host_flags}");
        let mapped = if has_host_flags {
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
            device_handle: self.device.device.clone(),
            per_frame_copies: None,
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
