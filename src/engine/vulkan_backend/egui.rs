use std::{collections::HashMap, ffi::c_void};

use crate::engine::backend::{GpuError, GpuErrorKind};

use super::{FRAMES_IN_FLIGHT, SHADER_ENTRY_POINT, buffer::VulkanBuffer, create_shader_module};
use crate::engine::vulkan_backend::VulkanBackend;

// Egui vertex format
//
// layout(location=0) vec2 pos    (offset 0,  R32G32_SFLOAT)
// layout(location=1) vec2 uv     (offset 8,  R32G32_SFLOAT)
// layout(location=2) vec4 color  (offset 16, R8G8B8A8_UNORM)
// stride: 20 bytes (matches egui::Vertex repr(C))

const EGUI_VERTEX_STRIDE: u32 = 20;
const EGUI_PUSH_CONSTANTS_SIZE: u32 = 16; // vec2 scale + vec2 translate

struct EguiTextureInfo {
    image: ash::vk::Image,
    memory: ash::vk::DeviceMemory,
    image_view: ash::vk::ImageView,
    sampler: ash::vk::Sampler,
}

pub(crate) struct EguiBatch {
    texture_id: egui::TextureId,
    vertex_offset: u32,
    index_offset: u32,
    index_count: u32,
    clip_min_x: i32,
    clip_min_y: i32,
    clip_max_x: i32,
    clip_max_y: i32,
}

pub(crate) struct EguiRenderer {
    device: ash::Device,
    phys_device: ash::vk::PhysicalDevice,
    instance: ash::Instance,
    queue: ash::vk::Queue,
    queue_family: u32,

    pipeline: ash::vk::Pipeline,
    pipeline_layout: ash::vk::PipelineLayout,
    descriptor_set_layout: ash::vk::DescriptorSetLayout,
    descriptor_pool: ash::vk::DescriptorPool,
    /// One descriptor set per in-flight frame (single CIS at binding 0).
    descriptor_sets: Vec<ash::vk::DescriptorSet>,

    textures: HashMap<egui::TextureId, EguiTextureInfo>,

    vertex_buffers: Vec<Option<VulkanBuffer>>,
    index_buffers: Vec<Option<VulkanBuffer>>,
    vertex_caps: Vec<usize>,
    index_caps: Vec<usize>,
}

impl EguiRenderer {
    pub fn create(
        instance: &ash::Instance,
        device: &ash::Device,
        phys_device: ash::vk::PhysicalDevice,
        queue: ash::vk::Queue,
        queue_family: u32,
        swapchain_format: ash::vk::Format,
    ) -> Result<Self, GpuError> {
        let device = device.clone();
        let instance = instance.clone();

        let desc_set_layout = Self::create_descriptor_set_layout(&device)?;
        let pipeline_layout = Self::create_pipeline_layout(&device, desc_set_layout)?;
        let pipeline = Self::create_pipeline(&device, pipeline_layout, swapchain_format)?;
        let (pool, sets) = Self::create_descriptor_pool_and_sets(&device, desc_set_layout)?;

        Ok(EguiRenderer {
            device,
            phys_device,
            instance,
            queue,
            queue_family,
            pipeline,
            pipeline_layout,
            descriptor_set_layout: desc_set_layout,
            descriptor_pool: pool,
            descriptor_sets: sets,
            textures: HashMap::new(),
            vertex_buffers: (0..FRAMES_IN_FLIGHT).map(|_| None).collect(),
            index_buffers: (0..FRAMES_IN_FLIGHT).map(|_| None).collect(),
            vertex_caps: vec![0; FRAMES_IN_FLIGHT as usize],
            index_caps: vec![0; FRAMES_IN_FLIGHT as usize],
        })
    }

    pub fn destroy(&self) {
        unsafe {
            if self.pipeline != ash::vk::Pipeline::null() {
                self.device.destroy_pipeline(self.pipeline, None);
            }
            if self.pipeline_layout != ash::vk::PipelineLayout::null() {
                self.device
                    .destroy_pipeline_layout(self.pipeline_layout, None);
            }
            if self.descriptor_set_layout != ash::vk::DescriptorSetLayout::null() {
                self.device
                    .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            }
            if self.descriptor_pool != ash::vk::DescriptorPool::null() {
                self.device
                    .destroy_descriptor_pool(self.descriptor_pool, None);
            }
        }
        for (_, tex) in &self.textures {
            Self::destroy_texture(&self.device, tex);
        }
        for buf in &self.vertex_buffers {
            if let Some(b) = buf {
                b.destroy();
            }
        }
        for buf in &self.index_buffers {
            if let Some(b) = buf {
                b.destroy();
            }
        }
    }

    fn create_descriptor_set_layout(
        device: &ash::Device,
    ) -> Result<ash::vk::DescriptorSetLayout, GpuError> {
        let binding = ash::vk::DescriptorSetLayoutBinding {
            binding: 0,
            descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
            stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
            ..Default::default()
        };
        let create_info = ash::vk::DescriptorSetLayoutCreateInfo {
            binding_count: 1,
            p_bindings: &binding,
            ..Default::default()
        };
        unsafe { device.create_descriptor_set_layout(&create_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create egui descriptor set layout: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })
    }

    fn create_descriptor_pool_and_sets(
        device: &ash::Device,
        layout: ash::vk::DescriptorSetLayout,
    ) -> Result<(ash::vk::DescriptorPool, Vec<ash::vk::DescriptorSet>), GpuError> {
        let pool_size = ash::vk::DescriptorPoolSize {
            ty: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: FRAMES_IN_FLIGHT,
        };
        let pool_info = ash::vk::DescriptorPoolCreateInfo {
            max_sets: FRAMES_IN_FLIGHT,
            pool_size_count: 1,
            p_pool_sizes: &pool_size,
            ..Default::default()
        };
        let pool = unsafe { device.create_descriptor_pool(&pool_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create egui descriptor pool: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        let layouts = vec![layout; FRAMES_IN_FLIGHT as usize];
        let alloc_info = ash::vk::DescriptorSetAllocateInfo {
            descriptor_pool: pool,
            descriptor_set_count: layouts.len() as u32,
            p_set_layouts: layouts.as_ptr(),
            ..Default::default()
        };
        let sets = unsafe { device.allocate_descriptor_sets(&alloc_info) }.map_err(|e| {
            GpuError::new(
                format!("Failed to allocate egui descriptor sets: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        Ok((pool, sets))
    }

    fn create_pipeline_layout(
        device: &ash::Device,
        desc_layout: ash::vk::DescriptorSetLayout,
    ) -> Result<ash::vk::PipelineLayout, GpuError> {
        let push_range = ash::vk::PushConstantRange {
            stage_flags: ash::vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: EGUI_PUSH_CONSTANTS_SIZE,
        };
        let create_info = ash::vk::PipelineLayoutCreateInfo {
            set_layout_count: 1,
            p_set_layouts: &desc_layout,
            push_constant_range_count: 1,
            p_push_constant_ranges: &push_range,
            ..Default::default()
        };
        unsafe { device.create_pipeline_layout(&create_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create egui pipeline layout: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })
    }

    fn create_pipeline(
        device: &ash::Device,
        pipeline_layout: ash::vk::PipelineLayout,
        color_format: ash::vk::Format,
    ) -> Result<ash::vk::Pipeline, GpuError> {
        let vert_code = include_bytes!("../../shaders/spv/egui/egui.vert.spv");
        let frag_code = include_bytes!("../../shaders/spv/egui/egui.frag.spv");

        let vert_module = create_shader_module(vert_code, device, "egui vertex")?;
        let frag_module = create_shader_module(frag_code, device, "egui fragment")?;

        let stages = [
            ash::vk::PipelineShaderStageCreateInfo {
                stage: ash::vk::ShaderStageFlags::VERTEX,
                module: vert_module,
                p_name: SHADER_ENTRY_POINT.as_ptr(),
                ..Default::default()
            },
            ash::vk::PipelineShaderStageCreateInfo {
                stage: ash::vk::ShaderStageFlags::FRAGMENT,
                module: frag_module,
                p_name: SHADER_ENTRY_POINT.as_ptr(),
                ..Default::default()
            },
        ];

        let vertex_binding = ash::vk::VertexInputBindingDescription {
            binding: 0,
            stride: EGUI_VERTEX_STRIDE,
            input_rate: ash::vk::VertexInputRate::VERTEX,
        };
        let vertex_attributes = [
            ash::vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: ash::vk::Format::R32G32_SFLOAT,
                offset: 0,
            },
            ash::vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: ash::vk::Format::R32G32_SFLOAT,
                offset: 8,
            },
            ash::vk::VertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: ash::vk::Format::R8G8B8A8_UNORM,
                offset: 16,
            },
        ];
        let vtx_input = ash::vk::PipelineVertexInputStateCreateInfo {
            vertex_binding_description_count: 1,
            p_vertex_binding_descriptions: &vertex_binding,
            vertex_attribute_description_count: vertex_attributes.len() as u32,
            p_vertex_attribute_descriptions: vertex_attributes.as_ptr(),
            ..Default::default()
        };

        let input_assembly = ash::vk::PipelineInputAssemblyStateCreateInfo {
            topology: ash::vk::PrimitiveTopology::TRIANGLE_LIST,
            ..Default::default()
        };

        let viewport_state = ash::vk::PipelineViewportStateCreateInfo {
            viewport_count: 1,
            scissor_count: 1,
            ..Default::default()
        };

        let rasterization = ash::vk::PipelineRasterizationStateCreateInfo {
            depth_clamp_enable: ash::vk::FALSE,
            rasterizer_discard_enable: ash::vk::FALSE,
            polygon_mode: ash::vk::PolygonMode::FILL,
            cull_mode: ash::vk::CullModeFlags::NONE,
            front_face: ash::vk::FrontFace::COUNTER_CLOCKWISE,
            depth_bias_enable: ash::vk::FALSE,
            line_width: 1.0,
            ..Default::default()
        };

        let multisample = ash::vk::PipelineMultisampleStateCreateInfo {
            rasterization_samples: ash::vk::SampleCountFlags::TYPE_1,
            sample_shading_enable: ash::vk::FALSE,
            ..Default::default()
        };

        let blend_attachment = ash::vk::PipelineColorBlendAttachmentState {
            blend_enable: ash::vk::TRUE,
            src_color_blend_factor: ash::vk::BlendFactor::ONE,
            dst_color_blend_factor: ash::vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            color_blend_op: ash::vk::BlendOp::ADD,
            src_alpha_blend_factor: ash::vk::BlendFactor::ONE,
            dst_alpha_blend_factor: ash::vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            alpha_blend_op: ash::vk::BlendOp::ADD,
            color_write_mask: ash::vk::ColorComponentFlags::RGBA,
            ..Default::default()
        };
        let blend = ash::vk::PipelineColorBlendStateCreateInfo {
            logic_op_enable: ash::vk::FALSE,
            attachment_count: 1,
            p_attachments: &blend_attachment,
            ..Default::default()
        };

        let dynamic_states = [
            ash::vk::DynamicState::VIEWPORT,
            ash::vk::DynamicState::SCISSOR,
        ];
        let dynamic = ash::vk::PipelineDynamicStateCreateInfo {
            dynamic_state_count: dynamic_states.len() as u32,
            p_dynamic_states: dynamic_states.as_ptr(),
            ..Default::default()
        };

        let rendering = ash::vk::PipelineRenderingCreateInfo {
            color_attachment_count: 1,
            p_color_attachment_formats: &color_format,
            ..Default::default()
        };

        let info = ash::vk::GraphicsPipelineCreateInfo {
            stage_count: stages.len() as u32,
            p_stages: stages.as_ptr(),
            p_vertex_input_state: &vtx_input,
            p_input_assembly_state: &input_assembly,
            p_viewport_state: &viewport_state,
            p_rasterization_state: &rasterization,
            p_multisample_state: &multisample,
            p_color_blend_state: &blend,
            p_dynamic_state: &dynamic,
            layout: pipeline_layout,
            render_pass: ash::vk::RenderPass::null(),
            p_next: &rendering as *const _ as *const std::ffi::c_void,
            ..Default::default()
        };

        let pipelines = unsafe {
            device.create_graphics_pipelines(ash::vk::PipelineCache::null(), &[info], None)
        }
        .map_err(|(_, e)| {
            GpuError::new(
                format!("Failed to create egui pipeline: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        unsafe {
            device.destroy_shader_module(vert_module, None);
            device.destroy_shader_module(frag_module, None);
        }

        Ok(pipelines[0])
    }

    pub fn create_or_update_texture(
        &mut self,
        id: egui::TextureId,
        delta: &egui::epaint::ImageDelta,
    ) {
        // Partial update: new glyphs added to an existing font atlas texture
        if let Some(pos) = delta.pos {
            if let Some(tex) = self.textures.get(&id) {
                let egui::ImageData::Color(image) = &delta.image;
                let w = image.size[0] as u32;
                let h = image.size[1] as u32;
                let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Err(e) =
                    self.upload_texture_sub_region(tex, pos[0] as u32, pos[1] as u32, w, h, &raw)
                {
                    eprintln!("Failed to update egui texture sub-region: {e:?}");
                }
                return;
            }
        }

        // egui sends texture deltas every frame for reference counting.
        // Skip if the texture is already uploaded.
        if self.textures.contains_key(&id) {
            return;
        }

        let (width, height, pixels) = match &delta.image {
            egui::ImageData::Color(image) => {
                let w = image.size[0] as u32;
                let h = image.size[1] as u32;
                let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                (w, h, raw)
            }
        };

        let format = ash::vk::Format::R8G8B8A8_UNORM;

        match self.upload_texture(width, height, format, &pixels) {
            Ok(info) => {
                self.textures.insert(id, info);
            }
            Err(e) => {
                eprintln!("Failed to create egui texture: {e:?}");
            }
        }
    }

    fn upload_texture(
        &self,
        width: u32,
        height: u32,
        format: ash::vk::Format,
        pixels: &[u8],
    ) -> Result<EguiTextureInfo, GpuError> {
        let data_size = (width as u64) * (height as u64) * 4;

        let (staging_buf, staging_mem) = VulkanBackend::create_buffer(
            &self.instance,
            &self.device,
            self.phys_device,
            data_size,
            ash::vk::BufferUsageFlags::TRANSFER_SRC,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let mapped = unsafe {
            self.device
                .map_memory(staging_mem, 0, data_size, ash::vk::MemoryMapFlags::empty())
                .map_err(|e| {
                    GpuError::new(
                        format!("Failed to map staging: {e:?}"),
                        GpuErrorKind::ResourceUpdate,
                    )
                })?
        };
        unsafe {
            mapped.copy_from(pixels.as_ptr() as *const c_void, data_size as usize);
        }
        unsafe { self.device.unmap_memory(staging_mem) };

        let (image, memory) = VulkanBackend::create_image(
            &self.instance,
            &self.device,
            self.phys_device,
            width,
            height,
            format,
            1,
            ash::vk::ImageTiling::OPTIMAL,
            ash::vk::ImageUsageFlags::TRANSFER_DST | ash::vk::ImageUsageFlags::SAMPLED,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        let (upload_pool, cmd) = self.create_upload_command_buffer()?;

        layout_transition(
            &self.device,
            cmd,
            image,
            ash::vk::ImageLayout::UNDEFINED,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ash::vk::ImageAspectFlags::COLOR,
        );

        let copy = ash::vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: ash::vk::ImageSubresourceLayers {
                aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
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
                cmd,
                staging_buf,
                image,
                ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy],
            );
        }

        layout_transition(
            &self.device,
            cmd,
            image,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ash::vk::ImageAspectFlags::COLOR,
        );

        self.submit_and_cleanup_upload(cmd, upload_pool, staging_buf, staging_mem)?;

        let view_info = ash::vk::ImageViewCreateInfo {
            image,
            view_type: ash::vk::ImageViewType::TYPE_2D,
            format,
            subresource_range: ash::vk::ImageSubresourceRange {
                aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
            ..Default::default()
        };
        let image_view =
            unsafe { self.device.create_image_view(&view_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to create egui image view: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;

        let sampler_info = ash::vk::SamplerCreateInfo {
            mag_filter: ash::vk::Filter::LINEAR,
            min_filter: ash::vk::Filter::LINEAR,
            mipmap_mode: ash::vk::SamplerMipmapMode::LINEAR,
            address_mode_u: ash::vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_v: ash::vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_w: ash::vk::SamplerAddressMode::CLAMP_TO_EDGE,
            anisotropy_enable: ash::vk::FALSE,
            compare_enable: ash::vk::FALSE,
            ..Default::default()
        };
        let sampler = unsafe { self.device.create_sampler(&sampler_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create egui sampler: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        Ok(EguiTextureInfo {
            image,
            memory,
            image_view,
            sampler,
        })
    }

    /// Upload pixel data into a sub-region of an existing texture.
    /// Uses a one-shot command buffer with queue_wait_idle.
    fn upload_texture_sub_region(
        &self,
        tex: &EguiTextureInfo,
        offset_x: u32,
        offset_y: u32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(), GpuError> {
        let data_size = (width as u64) * (height as u64) * 4;

        let (staging_buf, staging_mem) = VulkanBackend::create_buffer(
            &self.instance,
            &self.device,
            self.phys_device,
            data_size,
            ash::vk::BufferUsageFlags::TRANSFER_SRC,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let mapped = unsafe {
            self.device
                .map_memory(staging_mem, 0, data_size, ash::vk::MemoryMapFlags::empty())
                .map_err(|e| {
                    GpuError::new(
                        format!("Failed to map staging: {e:?}"),
                        GpuErrorKind::ResourceUpdate,
                    )
                })?
        };
        unsafe {
            mapped.copy_from(pixels.as_ptr() as *const c_void, data_size as usize);
        }
        unsafe { self.device.unmap_memory(staging_mem) };

        let (upload_pool, cmd) = self.create_upload_command_buffer()?;

        // SHADER_READ_ONLY → TRANSFER_DST
        layout_transition(
            &self.device,
            cmd,
            tex.image,
            ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ash::vk::ImageAspectFlags::COLOR,
        );

        // Copy
        let copy = ash::vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: ash::vk::ImageSubresourceLayers {
                aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: ash::vk::Offset3D {
                x: offset_x as i32,
                y: offset_y as i32,
                z: 0,
            },
            image_extent: ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            },
        };
        unsafe {
            self.device.cmd_copy_buffer_to_image(
                cmd,
                staging_buf,
                tex.image,
                ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy],
            );
        }

        // TRANSFER_DST → SHADER_READ_ONLY
        layout_transition(
            &self.device,
            cmd,
            tex.image,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ash::vk::ImageAspectFlags::COLOR,
        );

        self.submit_and_cleanup_upload(cmd, upload_pool, staging_buf, staging_mem)
    }

    /// Helper: create a transient one-shot command buffer for uploads.
    fn create_upload_command_buffer(
        &self,
    ) -> Result<(ash::vk::CommandPool, ash::vk::CommandBuffer), GpuError> {
        let pool_info = ash::vk::CommandPoolCreateInfo {
            flags: ash::vk::CommandPoolCreateFlags::TRANSIENT,
            queue_family_index: self.queue_family,
            ..Default::default()
        };
        let upload_pool =
            unsafe { self.device.create_command_pool(&pool_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to create upload pool: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;

        let alloc_info = ash::vk::CommandBufferAllocateInfo {
            command_pool: upload_pool,
            level: ash::vk::CommandBufferLevel::PRIMARY,
            command_buffer_count: 1,
            ..Default::default()
        };
        let cmd_bufs =
            unsafe { self.device.allocate_command_buffers(&alloc_info) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to allocate upload cmd buf: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;
        let cmd = cmd_bufs[0];

        unsafe {
            self.device.begin_command_buffer(
                cmd,
                &ash::vk::CommandBufferBeginInfo {
                    flags: ash::vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
                    ..Default::default()
                },
            )
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to begin upload cmd buf: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        Ok((upload_pool, cmd))
    }

    /// Helper: submit a one-shot upload command buffer, wait for completion, clean up resources.
    fn submit_and_cleanup_upload(
        &self,
        cmd: ash::vk::CommandBuffer,
        upload_pool: ash::vk::CommandPool,
        staging_buf: ash::vk::Buffer,
        staging_mem: ash::vk::DeviceMemory,
    ) -> Result<(), GpuError> {
        unsafe { self.device.end_command_buffer(cmd) }.map_err(|e| {
            GpuError::new(
                format!("Failed to end upload cmd buf: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        let submit = ash::vk::SubmitInfo {
            command_buffer_count: 1,
            p_command_buffers: &cmd,
            ..Default::default()
        };
        unsafe {
            self.device
                .queue_submit(self.queue, &[submit], ash::vk::Fence::null())
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to submit upload: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;
        unsafe { self.device.queue_wait_idle(self.queue) }.map_err(|e| {
            GpuError::new(
                format!("Failed to wait for upload: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        unsafe {
            self.device.destroy_command_pool(upload_pool, None);
            self.device.destroy_buffer(staging_buf, None);
            self.device.free_memory(staging_mem, None);
        }

        Ok(())
    }

    pub fn free_texture(&mut self, id: &egui::TextureId) {
        if let Some(tex) = self.textures.remove(id) {
            Self::destroy_texture(&self.device, &tex);
        }
    }

    fn destroy_texture(device: &ash::Device, tex: &EguiTextureInfo) {
        unsafe {
            if tex.sampler != ash::vk::Sampler::null() {
                device.destroy_sampler(tex.sampler, None);
            }
            if tex.image_view != ash::vk::ImageView::null() {
                device.destroy_image_view(tex.image_view, None);
            }
            if tex.image != ash::vk::Image::null() {
                device.destroy_image(tex.image, None);
            }
            if tex.memory != ash::vk::DeviceMemory::null() {
                device.free_memory(tex.memory, None);
            }
        }
    }

    pub fn ensure_buffer_capacity(
        &mut self,
        frame_idx: usize,
        needed_vertices: usize,
        needed_indices: usize,
    ) {
        let buf_idx = frame_idx % FRAMES_IN_FLIGHT as usize;

        if needed_vertices > self.vertex_caps[buf_idx] {
            self.grow_vertex_buffer(buf_idx, needed_vertices);
        }
        if needed_indices > self.index_caps[buf_idx] {
            self.grow_index_buffer(buf_idx, needed_indices);
        }
    }

    fn grow_vertex_buffer(&mut self, buf_idx: usize, needed: usize) {
        let new_cap = (needed.max(1) as f64 * 1.5) as usize;
        let size = (new_cap as u64) * (EGUI_VERTEX_STRIDE as u64);

        if let Some(old) = &self.vertex_buffers[buf_idx] {
            old.destroy();
        }

        let (buf, mem) = match VulkanBackend::create_buffer(
            &self.instance,
            &self.device,
            self.phys_device,
            size,
            ash::vk::BufferUsageFlags::VERTEX_BUFFER,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to create egui vtx buffer: {e:?}");
                self.vertex_caps[buf_idx] = 0;
                return;
            }
        };

        let mapped = unsafe {
            self.device
                .map_memory(mem, 0, size, ash::vk::MemoryMapFlags::empty())
                .ok()
        }
        .unwrap_or(std::ptr::null_mut());

        self.vertex_buffers[buf_idx] = Some(VulkanBuffer {
            buffer: buf,
            memory: mem,
            mapped,
            flags: ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
            size,
            device_handle: self.device.clone(),
            per_frame_copies: None,
        });
        self.vertex_caps[buf_idx] = new_cap;
    }

    fn grow_index_buffer(&mut self, buf_idx: usize, needed: usize) {
        let new_cap = (needed.max(1) as f64 * 1.5) as usize;
        let size = (new_cap as u64) * 4; // u32 indices

        if let Some(old) = &self.index_buffers[buf_idx] {
            old.destroy();
        }

        let (buf, mem) = match VulkanBackend::create_buffer(
            &self.instance,
            &self.device,
            self.phys_device,
            size,
            ash::vk::BufferUsageFlags::INDEX_BUFFER,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to create egui idx buffer: {e:?}");
                self.index_caps[buf_idx] = 0;
                return;
            }
        };

        let mapped = unsafe {
            self.device
                .map_memory(mem, 0, size, ash::vk::MemoryMapFlags::empty())
                .ok()
        }
        .unwrap_or(std::ptr::null_mut());

        self.index_buffers[buf_idx] = Some(VulkanBuffer {
            buffer: buf,
            memory: mem,
            mapped,
            flags: ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
            size,
            device_handle: self.device.clone(),
            per_frame_copies: None,
        });
        self.index_caps[buf_idx] = new_cap;
    }

    // Main draw
    pub fn cmd_draw(
        &self,
        cmd: ash::vk::CommandBuffer,
        frame_idx: usize,
        width: u32,
        height: u32,
        pixels_per_point: f32,
        batches: &[EguiBatch],
        vertices: &[egui::epaint::Vertex],
        indices: &[u32],
    ) {
        // eprintln!("[egui::cmd_draw] batches={}, vertices={}, indices={}", batches.len(), vertices.len(), indices.len());
        if batches.is_empty() {
            eprintln!("[egui::cmd_draw] no batches, returning");
            return;
        }

        unsafe {
            self.device
                .cmd_bind_pipeline(cmd, ash::vk::PipelineBindPoint::GRAPHICS, self.pipeline);
        }
        // eprintln!("[egui::cmd_draw] pipeline bound");

        let buf_idx = frame_idx % FRAMES_IN_FLIGHT as usize;
        let vtx_buf = match &self.vertex_buffers[buf_idx] {
            Some(b) => b,
            None => {
                eprintln!("[egui::cmd_draw] vertex buffer None, returning");
                return;
            }
        };
        let idx_buf = match &self.index_buffers[buf_idx] {
            Some(b) => b,
            None => {
                eprintln!("[egui::cmd_draw] index buffer None, returning");
                return;
            }
        };

        // Upload vertices
        let vtx_bytes = vertex_slice_as_bytes(vertices);
        if !vtx_bytes.is_empty() && vtx_buf.mapped != std::ptr::null_mut() {
            // eprintln!("[egui::cmd_draw] uploading {} bytes of verts", vtx_bytes.len());
            unsafe {
                std::ptr::copy_nonoverlapping(
                    vtx_bytes.as_ptr(),
                    vtx_buf.mapped as *mut u8,
                    vtx_bytes.len(),
                );
            }
        }

        // Upload indices
        let idx_bytes = index_slice_as_bytes(indices);
        if !idx_bytes.is_empty() && idx_buf.mapped != std::ptr::null_mut() {
            // eprintln!("[egui::cmd_draw] uploading {} bytes of indices", idx_bytes.len());
            unsafe {
                std::ptr::copy_nonoverlapping(
                    idx_bytes.as_ptr(),
                    idx_buf.mapped as *mut u8,
                    idx_bytes.len(),
                );
            }
        }

        // Viewport (Vulkan Y-flip)
        let vp = ash::vk::Viewport {
            x: 0.0,
            y: height as f32,
            width: width as f32,
            height: -(height as f32),
            min_depth: 0.0,
            max_depth: 1.0,
        };
        unsafe {
            self.device.cmd_set_viewport(cmd, 0, &[vp]);
        }

        // Push constants: scale + translate (screen → NDC)
        let push_data: [f32; 4] = [
            2.0 * pixels_per_point / width as f32,
            -2.0 * pixels_per_point / height as f32,
            -1.0,
            1.0,
        ];
        unsafe {
            self.device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                ash::vk::ShaderStageFlags::VERTEX,
                0,
                std::slice::from_raw_parts(
                    push_data.as_ptr() as *const u8,
                    EGUI_PUSH_CONSTANTS_SIZE as usize,
                ),
            );
        }

        let frame_set = self.descriptor_sets.get(buf_idx).copied();
        let frame_set = match frame_set {
            Some(s) => s,
            None => {
                eprintln!("[egui::cmd_draw] no descriptor set, returning");
                return;
            }
        };

        for (_i, batch) in batches.iter().enumerate() {
            // eprintln!("[egui::cmd_draw] batch {}: texture_id={:?}, vert_offset={}, idx_offset={}, idx_count={}, clip=[{},{} - {},{}]",
            //     i, batch.texture_id, batch.vertex_offset, batch.index_offset, batch.index_count,
            //     batch.clip_min_x, batch.clip_min_y, batch.clip_max_x, batch.clip_max_y,
            // );

            // Scissor
            let sc = ash::vk::Rect2D {
                offset: ash::vk::Offset2D {
                    x: batch.clip_min_x.max(0),
                    y: batch.clip_min_y.max(0),
                },
                extent: ash::vk::Extent2D {
                    width: (batch.clip_max_x - batch.clip_min_x).max(0) as u32,
                    height: (batch.clip_max_y - batch.clip_min_y).max(0) as u32,
                },
            };
            // eprintln!(
            //     "[egui::cmd_draw]   scissor: offset=({},{}), extent=({},{})",
            //     sc.offset.x, sc.offset.y, sc.extent.width, sc.extent.height
            // );
            unsafe {
                self.device.cmd_set_scissor(cmd, 0, &[sc]);
            }

            // Find texture and update descriptor
            let tex = match self.textures.get(&batch.texture_id) {
                Some(t) => t,
                None => {
                    eprintln!("[egui::cmd_draw]   texture not found, skipping batch");
                    continue;
                }
            };

            let img_info = ash::vk::DescriptorImageInfo {
                image_view: tex.image_view,
                image_layout: ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                sampler: tex.sampler,
            };
            let write = ash::vk::WriteDescriptorSet {
                dst_set: frame_set,
                dst_binding: 0,
                descriptor_count: 1,
                descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                p_image_info: &img_info,
                ..Default::default()
            };
            unsafe {
                self.device.update_descriptor_sets(&[write], &[]);
            }

            // Bind descriptor set
            unsafe {
                self.device.cmd_bind_descriptor_sets(
                    cmd,
                    ash::vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    0,
                    &[frame_set],
                    &[],
                );
            }

            // Bind vertex/index buffers
            unsafe {
                self.device.cmd_bind_vertex_buffers(
                    cmd,
                    0,
                    &[vtx_buf.buffer],
                    &[batch.vertex_offset as u64 * EGUI_VERTEX_STRIDE as u64],
                );
                self.device.cmd_bind_index_buffer(
                    cmd,
                    idx_buf.buffer,
                    batch.index_offset as u64 * 4,
                    ash::vk::IndexType::UINT32,
                );
            }

            // Draw
            // eprintln!("[egui::cmd_draw]   cmd_draw_indexed(count={})", batch.index_count);
            unsafe {
                self.device
                    .cmd_draw_indexed(cmd, batch.index_count, 1, 0, 0, 0);
            }
        }
    }

    pub fn has_texture(&self, id: &egui::TextureId) -> bool {
        self.textures.contains_key(id)
    }
}

/// Helper: build vertex/index arrays from egui primitives
pub fn build_egui_batches(
    clipped_primitives: &[egui::ClippedPrimitive],
    pixel_per_point: f32,
) -> (Vec<egui::epaint::Vertex>, Vec<u32>, Vec<EguiBatch>) {
    let mut vertices: Vec<egui::epaint::Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut batches: Vec<EguiBatch> = Vec::new();

    for clipped in clipped_primitives {
        let egui::ClippedPrimitive {
            clip_rect,
            primitive,
        } = clipped;

        let mesh = match primitive {
            egui::epaint::Primitive::Mesh(m) => m,
            egui::epaint::Primitive::Callback(_) => continue,
        };

        let clip_min_x = (clip_rect.min.x * pixel_per_point) as i32;
        let clip_min_y = (clip_rect.min.y * pixel_per_point) as i32;
        let clip_max_x = (clip_rect.max.x * pixel_per_point) as i32;
        let clip_max_y = (clip_rect.max.y * pixel_per_point) as i32;

        if clip_min_x >= clip_max_x || clip_min_y >= clip_max_y {
            continue;
        }

        let vertex_offset = vertices.len() as u32;
        let index_offset = indices.len() as u32;

        vertices.extend_from_slice(&mesh.vertices);
        indices.extend_from_slice(&mesh.indices);

        batches.push(EguiBatch {
            texture_id: mesh.texture_id,
            vertex_offset,
            index_offset,
            index_count: mesh.indices.len() as u32,
            clip_min_x,
            clip_min_y,
            clip_max_x,
            clip_max_y,
        });
    }

    (vertices, indices, batches)
}

fn vertex_slice_as_bytes(vertices: &[egui::epaint::Vertex]) -> &[u8] {
    if vertices.is_empty() {
        return &[];
    }
    unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            vertices.len() * std::mem::size_of::<egui::epaint::Vertex>(),
        )
    }
}

fn index_slice_as_bytes(indices: &[u32]) -> &[u8] {
    if indices.is_empty() {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(indices.as_ptr() as *const u8, indices.len() * 4) }
}

fn layout_transition(
    device: &ash::Device,
    cmd: ash::vk::CommandBuffer,
    image: ash::vk::Image,
    old: ash::vk::ImageLayout,
    new: ash::vk::ImageLayout,
    aspect: ash::vk::ImageAspectFlags,
) {
    let (src_access, dst_access, src_stage, dst_stage) = match (old, new) {
        (ash::vk::ImageLayout::UNDEFINED, ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
            ash::vk::AccessFlags2::empty(),
            ash::vk::AccessFlags2::TRANSFER_WRITE,
            ash::vk::PipelineStageFlags2::TOP_OF_PIPE,
            ash::vk::PipelineStageFlags2::TRANSFER,
        ),
        (
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        ) => (
            ash::vk::AccessFlags2::TRANSFER_WRITE,
            ash::vk::AccessFlags2::SHADER_READ,
            ash::vk::PipelineStageFlags2::TRANSFER,
            ash::vk::PipelineStageFlags2::FRAGMENT_SHADER,
        ),
        (
            ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        ) => (
            ash::vk::AccessFlags2::SHADER_READ,
            ash::vk::AccessFlags2::TRANSFER_WRITE,
            ash::vk::PipelineStageFlags2::FRAGMENT_SHADER,
            ash::vk::PipelineStageFlags2::TRANSFER,
        ),
        _ => return,
    };

    let barrier = ash::vk::ImageMemoryBarrier2 {
        src_stage_mask: src_stage,
        src_access_mask: src_access,
        dst_stage_mask: dst_stage,
        dst_access_mask: dst_access,
        old_layout: old,
        new_layout: new,
        src_queue_family_index: ash::vk::QUEUE_FAMILY_IGNORED,
        dst_queue_family_index: ash::vk::QUEUE_FAMILY_IGNORED,
        image,
        subresource_range: ash::vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        },
        ..Default::default()
    };
    let dep = ash::vk::DependencyInfo {
        image_memory_barrier_count: 1,
        p_image_memory_barriers: &barrier,
        ..Default::default()
    };
    unsafe {
        device.cmd_pipeline_barrier2(cmd, &dep);
    }
}
