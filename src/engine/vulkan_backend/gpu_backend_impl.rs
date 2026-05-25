use crate::engine::{
    backend::{
        BufferDesc, BufferUsage, GpuBackend, GpuError, PipelineDesc, RenderPassDesc,
        RenderTargetDesc, SamplerDesc, ShaderStage, TextureDesc, TextureFormat, ViewportDesc,
    },
    vulkan_backend::{VulkanBackend, buffer::VulkanBuffer, texture::VulkanTexture},
};

impl GpuBackend for VulkanBackend {
    type Texture = VulkanTexture;

    type RenderTarget = VulkanTexture;

    type Buffer = VulkanBuffer;

    type Pipeline = ash::vk::Pipeline;

    fn create_texture(&self, desc: &TextureDesc, data: &[u8]) -> Result<Self::Texture, GpuError> {
        self.create_vk_texture(desc, data)
    }

    fn create_cubemap(
        &self,
        faces: [&[u8]; 6],
        width: u32,
        height: u32,
        format: TextureFormat,
        sampler: &SamplerDesc,
    ) -> Result<Self::Texture, GpuError> {
        self.create_vk_cubemap(faces, width, height, format, sampler)
    }

    fn create_buffer(
        &self,
        desc: &BufferDesc,
        data: Option<&[u8]>,
    ) -> Result<Self::Buffer, GpuError> {
        let flags = match desc.usage {
            BufferUsage::Uniform => {
                ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                    | ash::vk::MemoryPropertyFlags::HOST_COHERENT
            }
            BufferUsage::Index | BufferUsage::Vertex => ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
        };
        let buffer = self.create_vulkan_buffer(desc.size as u64, desc.usage, flags)?;
        if let Some(data) = data {
            self.update_buffer(&buffer, data);
        }
        Ok(buffer)
    }

    fn create_render_target(
        &self,
        desc: &RenderTargetDesc,
    ) -> Result<Self::RenderTarget, GpuError> {
        self.create_vk_render_target(desc)
    }

    fn create_pipeline(&self, desc: &PipelineDesc) -> Result<Self::Pipeline, GpuError> {
        todo!()
    }

    fn update_buffer(&self, buffer: &Self::Buffer, data: &[u8]) {
        fn update_buffer_safe(
            backend: &VulkanBackend,
            buffer: &VulkanBuffer,
            data: &[u8],
        ) -> Result<(), GpuError> {
            let size = (data.len() as ash::vk::DeviceSize).min(buffer.size);
            if buffer.flags
                == (ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                    | ash::vk::MemoryPropertyFlags::HOST_COHERENT)
            {
                if buffer.mapped != std::ptr::null_mut() {
                    unsafe {
                        buffer
                            .mapped
                            .copy_from(data.as_ptr() as *const _, size as usize);
                    }
                    Ok(())
                } else {
                    backend.copy_to_buffer(buffer.memory, data.as_ptr() as *const _, size)
                }
            } else {
                let command_buffer = backend.begin_single_time_commands()?;
                let (b_staging, m_staging) = backend.create_buffer(
                    size,
                    ash::vk::BufferUsageFlags::TRANSFER_SRC,
                    ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                        | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
                )?;
                backend.copy_to_buffer(m_staging, data.as_ptr() as *const _, size)?;
                backend.copy_buffer(command_buffer, b_staging, 0, buffer.buffer, 0, size);
                backend.end_single_time_commands(command_buffer)
            }
        }
        if let Err(e) = update_buffer_safe(&self, buffer, data) {
            println!("Failed to update buffer: {e:?}")
        }
    }

    fn begin_frame(&mut self) -> Result<(), GpuError> {
        todo!()
    }

    fn end_frame(&mut self) -> Result<(), GpuError> {
        todo!()
    }

    fn present(&mut self) -> Result<(), GpuError> {
        todo!()
    }

    fn begin_render_pass(&mut self, desc: &RenderPassDesc<Self>) {
        todo!()
    }

    fn end_render_pass(&mut self) {
        todo!()
    }

    fn set_pipeline(&mut self, pipeline: &Self::Pipeline) {
        todo!()
    }

    fn set_viewport(&mut self, viewport: &ViewportDesc) {
        todo!()
    }

    fn bind_texture(&mut self, slot: u32, texture: &Self::Texture) {
        let binding = if texture.view_type == ash::vk::ImageViewType::CUBE {
            7u32
        } else if texture.compare_enabled {
            5u32
        } else {
            4u32
        };
        todo!()
    }

    fn bind_render_target_as_texture(&mut self, slot: u32, target: &Self::RenderTarget) {
        let binding = if target.view_type == ash::vk::ImageViewType::CUBE {
            7u32
        } else if target.compare_enabled {
            5u32
        } else {
            4u32
        };
        todo!()
    }

    fn bind_uniform(&mut self, stage: ShaderStage, slot: u32, buffer: &Self::Buffer) {
        let binding = slot;
        todo!()
    }

    fn set_vertex_buffer(&mut self, buffer: &Self::Buffer) {
        todo!()
    }

    fn set_index_buffer(&mut self, buffer: &Self::Buffer) {
        todo!()
    }

    fn draw_indexed(&mut self, index_count: u32, first_index: u32, base_vertex: i32) {
        todo!()
    }

    fn backbuffer(&self) -> &Self::RenderTarget {
        todo!()
    }

    fn main_depth_target(&self) -> &Self::RenderTarget {
        todo!()
    }

    fn default_viewport(&self) -> ViewportDesc {
        todo!()
    }

    fn resolution(&self) -> (u32, u32) {
        todo!()
    }

    fn resize(&mut self, width: u32, height: u32) {
        todo!()
    }
}
