use crate::engine::{
    backend::{
        BlendMode, BufferDesc, BufferUsage, GpuBackend, GpuError, GpuErrorKind, PipelineDesc,
        RenderPassDesc, RenderTargetDesc, SamplerDesc, ShaderStage, Shaders, TextureDesc,
        TextureFormat, ViewportDesc,
    },
    vulkan_backend::{
        CurrentFrame, FRAMES_IN_FLIGHT, SHADER_ENTRY_POINT, VulkanBackend, buffer::VulkanBuffer,
        create_shader_module, texture::VulkanTexture, util::gpu_error_out_of_range,
    },
};

pub struct Shader {
    label: &'static str,
    stage: ash::vk::ShaderStageFlags,
    code: &'static [u8],
}

impl GpuBackend for VulkanBackend {
    type Texture = VulkanTexture;

    type RenderTarget = VulkanTexture;

    type Buffer = VulkanBuffer;

    type Pipeline = VulkanPipeline;

    type ShaderSource = Vec<Shader>;

    fn load_shaders(&self) -> Shaders<Self> {
        let deferred_pre_vtx = Shader {
            label: "Deferred Pre VTX",
            stage: ash::vk::ShaderStageFlags::VERTEX,
            code: include_bytes!("../../shaders/spv/deferred/pre_vertex.spv"),
        };
        let deferred_pre_pxl = Shader {
            label: "Deferred Pre PXL",
            stage: ash::vk::ShaderStageFlags::FRAGMENT,
            code: include_bytes!("../../shaders/spv/deferred/pre_pixel_packing.spv"),
        };
        let deferred_light_vtx = Shader {
            label: "Deferred Light VTX",
            stage: ash::vk::ShaderStageFlags::VERTEX,
            code: include_bytes!("../../shaders/spv/deferred/light_vertex.spv"),
        };
        let deferred_light_pxl = Shader {
            label: "Deferred Light PXL",
            stage: ash::vk::ShaderStageFlags::FRAGMENT,
            code: include_bytes!("../../shaders/spv/deferred/light_pixel_packing.spv"),
        };
        let forward_vtx = Shader {
            label: "Forward Pass VTX",
            stage: ash::vk::ShaderStageFlags::VERTEX,
            code: include_bytes!("../../shaders/spv/main_pass/vertex.spv"),
        };
        let forward_pxl = Shader {
            label: "Forward Pass PXL",
            stage: ash::vk::ShaderStageFlags::FRAGMENT,
            code: include_bytes!("../../shaders/spv/main_pass/pixel.spv"),
        };
        let sky_vtx = Shader {
            label: "Skybox VTX",
            stage: ash::vk::ShaderStageFlags::VERTEX,
            code: include_bytes!("../../shaders/spv/skybox/sky_vertex.spv"),
        };
        let sky_pxl = Shader {
            label: "Skybox PXL",
            stage: ash::vk::ShaderStageFlags::FRAGMENT,
            code: include_bytes!("../../shaders/spv/skybox/sky_pixel.spv"),
        };
        let shadow_vtx = Shader {
            label: "Shadow VTX",
            stage: ash::vk::ShaderStageFlags::VERTEX,
            code: include_bytes!("../../shaders/spv/shadow_mapping/sm_vert.spv"),
        };
        let shadow_pixel = Shader {
            label: "Shadow PXL",
            stage: ash::vk::ShaderStageFlags::FRAGMENT,
            code: include_bytes!("../../shaders/spv/shadow_mapping/sm_pixel.spv"),
        };
        let blend = Shader {
            label: "Blend PXL",
            stage: ash::vk::ShaderStageFlags::FRAGMENT,
            code: include_bytes!("../../shaders/spv/blend.spv"),
        };
        let blend_vtx = Shader {
            label: "Blend VTX",
            stage: ash::vk::ShaderStageFlags::VERTEX,
            code: include_bytes!("../../shaders/spv/deferred/light_vertex.spv"),
        };

        Shaders {
            deferred_pre: vec![deferred_pre_vtx, deferred_pre_pxl],
            deferred_light: vec![deferred_light_vtx, deferred_light_pxl],
            forward: vec![forward_vtx, forward_pxl],
            shadow: vec![shadow_vtx, shadow_pixel],
            skybox: vec![sky_vtx, sky_pxl],
            output: vec![blend_vtx, blend],
            ssao: vec![],
            ssao_blur: vec![],
        }
    }

    fn create_texture(&self, desc: &TextureDesc, data: &[u8]) -> Result<Self::Texture, GpuError> {
        if matches!(
            desc.format,
            TextureFormat::Depth24Stencil8 | TextureFormat::Depth32Float
        ) {
            self.create_depth_texture(desc.width, desc.height, desc.format, &Some(desc.sampler))
        } else {
            self.create_vk_texture(desc, data)
        }
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
        let usage = match desc.usage {
            BufferUsage::Uniform => ash::vk::BufferUsageFlags::UNIFORM_BUFFER,
            BufferUsage::Index => ash::vk::BufferUsageFlags::INDEX_BUFFER,
            BufferUsage::Vertex => ash::vk::BufferUsageFlags::VERTEX_BUFFER,
        } | ash::vk::BufferUsageFlags::TRANSFER_DST;
        let flags = ash::vk::MemoryPropertyFlags::HOST_VISIBLE
            | ash::vk::MemoryPropertyFlags::HOST_COHERENT;
        let buffer = self.create_vulkan_buffer(desc.size as u64, usage, flags)?;
        if let Some(data) = data {
            self.update_buffer(&buffer, data);
        }
        Ok(buffer)
    }

    fn create_render_target(
        &self,
        desc: &RenderTargetDesc,
    ) -> Result<Self::RenderTarget, GpuError> {
        Self::create_vk_render_target(&self.instance, &self.device, self.phys_device, desc)
    }

    fn create_pipeline(
        &self,
        desc: &PipelineDesc<Self::ShaderSource>,
    ) -> Result<Self::Pipeline, GpuError> {
        let shader_modules = desc
            .shader_source
            .iter()
            .map(|source| {
                println!("Compiling shader: {}", desc.label);
                let module = create_shader_module(source.code, &self.device, source.label)?;
                Ok(ash::vk::PipelineShaderStageCreateInfo {
                    stage: source.stage,
                    module,
                    p_name: SHADER_ENTRY_POINT.as_ptr(),
                    ..Default::default()
                })
            })
            .collect::<Result<Vec<_>, GpuError>>()?;
        println!(
            "Compiled shader {} to #{} modules",
            desc.label,
            shader_modules.len()
        );

        let dynamic_states = [
            ash::vk::DynamicState::VIEWPORT,
            ash::vk::DynamicState::SCISSOR,
        ];
        let dynamic_state_create_info = ash::vk::PipelineDynamicStateCreateInfo {
            dynamic_state_count: dynamic_states.len() as u32,
            p_dynamic_states: dynamic_states.as_ptr(),
            ..Default::default()
        };

        let (vtx_input_state, attributes) = if let Some(layout) = &desc.vertex_layout {
            let attributes = layout
                .attributes
                .iter()
                .map(|it| ash::vk::VertexInputAttributeDescription {
                    binding: 0,
                    location: it.shader_location,
                    format: it.format.into(),
                    offset: it.offset,
                })
                .collect::<Vec<_>>();

            (
                ash::vk::VertexInputBindingDescription {
                    binding: 0,
                    stride: layout.stride,
                    input_rate: ash::vk::VertexInputRate::VERTEX,
                },
                attributes,
            )
        } else {
            (
                ash::vk::VertexInputBindingDescription::default(),
                Vec::new(),
            )
        };

        let pipeline_vtx_input_state = if desc.vertex_layout.is_none() {
            ash::vk::PipelineVertexInputStateCreateInfo {
                vertex_binding_description_count: 0,
                vertex_attribute_description_count: 0,
                p_vertex_binding_descriptions: std::ptr::null(),
                p_vertex_attribute_descriptions: std::ptr::null(),
                ..Default::default()
            }
        } else {
            ash::vk::PipelineVertexInputStateCreateInfo {
                vertex_binding_description_count: 1,
                p_vertex_binding_descriptions: &vtx_input_state,
                vertex_attribute_description_count: attributes.len() as _,
                p_vertex_attribute_descriptions: attributes.as_ptr(),
                ..Default::default()
            }
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

        let rasterization_state = ash::vk::PipelineRasterizationStateCreateInfo {
            depth_clamp_enable: ash::vk::FALSE,
            rasterizer_discard_enable: ash::vk::FALSE,
            polygon_mode: ash::vk::PolygonMode::FILL,
            cull_mode: desc.cull_mode.into(),
            front_face: ash::vk::FrontFace::CLOCKWISE,
            depth_bias_enable: ash::vk::FALSE,
            line_width: 1.0f32,
            ..Default::default()
        };

        let multisample_state = ash::vk::PipelineMultisampleStateCreateInfo {
            rasterization_samples: ash::vk::SampleCountFlags::TYPE_1,
            sample_shading_enable: ash::vk::FALSE,
            ..Default::default()
        };

        let blend_attachments = desc
            .color_target_formats
            .iter()
            .map(|_| ash::vk::PipelineColorBlendAttachmentState {
                blend_enable: match desc.blend_mode {
                    BlendMode::None => ash::vk::FALSE,
                    _ => ash::vk::TRUE,
                },
                color_write_mask: ash::vk::ColorComponentFlags::RGBA,
                ..Default::default()
            })
            .collect::<Vec<_>>();

        let blend_state = ash::vk::PipelineColorBlendStateCreateInfo {
            p_attachments: blend_attachments.as_ptr(),
            attachment_count: blend_attachments.len() as _,
            ..Default::default()
        };

        let depth_stencil = if let Some(_depth_stencil) = &desc.depth_format {
            ash::vk::PipelineDepthStencilStateCreateInfo {
                depth_write_enable: desc.depth_write as _,
                depth_compare_op: desc.depth_compare.into(),
                ..Default::default()
            }
        } else {
            ash::vk::PipelineDepthStencilStateCreateInfo::default()
        };
        let p_depth_stencil = if desc.depth_format.is_some() {
            &depth_stencil as *const _
        } else {
            std::ptr::null()
        };

        let color_formats: Vec<ash::vk::Format> = desc
            .color_target_formats
            .iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let depth_format = desc
            .depth_format
            .map(Into::into)
            .unwrap_or(ash::vk::Format::UNDEFINED);

        let rendering_create_info = ash::vk::PipelineRenderingCreateInfo {
            color_attachment_count: color_formats.len() as _,
            p_color_attachment_formats: color_formats.as_ptr(),
            depth_attachment_format: depth_format,
            ..Default::default()
        };

        let pipeline_info = ash::vk::GraphicsPipelineCreateInfo {
            stage_count: shader_modules.len() as _,
            p_stages: shader_modules.as_ptr(),
            p_vertex_input_state: &pipeline_vtx_input_state,
            p_input_assembly_state: &input_assembly,
            p_viewport_state: &viewport_state,
            p_rasterization_state: &rasterization_state,
            p_multisample_state: &multisample_state,
            p_color_blend_state: &blend_state,
            p_depth_stencil_state: p_depth_stencil,
            p_dynamic_state: &dynamic_state_create_info,
            layout: self.pipeline_layout,
            render_pass: ash::vk::RenderPass::null(),
            p_next: &rendering_create_info as *const _ as *const _,
            ..Default::default()
        };

        let pipeline = unsafe {
            self.device.create_graphics_pipelines(
                ash::vk::PipelineCache::null(),
                &[pipeline_info],
                None,
            )
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to create graphics pipeline: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        Ok(VulkanPipeline {
            label: desc.label.to_owned(),
            handle: pipeline[0],
            bind_point: ash::vk::PipelineBindPoint::GRAPHICS,
        })
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
                let (b_staging, m_staging) = VulkanBackend::create_buffer(
                    &backend.instance,
                    &backend.device,
                    backend.phys_device,
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
        let frame_idx = self.frame_idx;
        self.frame_idx = (self.frame_idx + 1) % (FRAMES_IN_FLIGHT as usize);

        self.current_frame = None;

        let fence = *self
            .sync_objects
            .draw_fences
            .get(frame_idx)
            .ok_or_else(|| {
                gpu_error_out_of_range(
                    "Frame Fence",
                    frame_idx,
                    self.sync_objects.draw_fences.len(),
                )
            })?;
        let present_semaphore = *self
            .sync_objects
            .present_completed_sems
            .get(frame_idx)
            .ok_or_else(|| {
                gpu_error_out_of_range(
                    "Present Completed Semaphore",
                    frame_idx,
                    self.sync_objects.present_completed_sems.len(),
                )
            })?;

        let command_buffer = *self.command_buffers.get(frame_idx).ok_or_else(|| {
            gpu_error_out_of_range("Command Buffer", frame_idx, self.command_buffers.len())
        })?;

        unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) }.map_err(|e| {
            GpuError::new(
                format!("Wait for Fences failed: {e:?}"),
                GpuErrorKind::Other,
            )
        })?;

        let (swapchain_idx, _optimal) = match unsafe {
            self.swapchain.fn_ptr.acquire_next_image(
                self.swapchain.swapchain,
                u64::MAX,
                present_semaphore,
                ash::vk::Fence::null(),
            )
        } {
            Ok(res) => res,
            Err(e) => {
                if e == ash::vk::Result::ERROR_OUT_OF_DATE_KHR {
                    return self.recreate_swapchain();
                }
                return Err(GpuError::new(
                    format!("Failed to get new swapchain image: {e:?}"),
                    GpuErrorKind::ResourceUpdate,
                ));
            }
        };

        let render_semaphore = *self
            .sync_objects
            .render_completed_sems
            .get(swapchain_idx as usize)
            .ok_or_else(|| {
                gpu_error_out_of_range(
                    "Render Completed Semaphore",
                    swapchain_idx as usize,
                    self.sync_objects.render_completed_sems.len(),
                )
            })?;

        unsafe { self.device.reset_fences(&[fence]) }.map_err(|e| {
            GpuError::new(format!("Reset Fences failed: {e:?}"), GpuErrorKind::Other)
        })?;

        unsafe {
            self.device
                .reset_command_buffer(command_buffer, ash::vk::CommandBufferResetFlags::empty())
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to reset command buffer: {e:?}"),
                GpuErrorKind::Other,
            )
        })?;

        let image = self
            .swapchain
            .swapchain_images
            .get(swapchain_idx as usize)
            .ok_or_else(|| {
                gpu_error_out_of_range(
                    "Swapchain Image",
                    swapchain_idx as usize,
                    self.swapchain.swapchain_images.len(),
                )
            })
            .map(|tx| tx.image)?;

        self.transition_image_layout(
            command_buffer,
            image,
            ash::vk::ImageLayout::UNDEFINED,
            ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            1,
        )?;

        self.current_frame = Some(CurrentFrame {
            idx: frame_idx,
            render_idx: swapchain_idx,
            command_buffer,
            fence,
            present_sem: present_semaphore,
            render_sem: render_semaphore,
        });

        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), GpuError> {
        let Some(CurrentFrame {
            render_idx,
            command_buffer,
            fence,
            present_sem: present_semaphore,
            render_sem: render_semaphore,
            ..
        }) = self.current_frame
        else {
            return Err(GpuError::new(
                "Cannot end frame if no frame was started!",
                GpuErrorKind::Other,
            ));
        };

        self.transition_image_layout(
            command_buffer,
            self.swapchain.swapchain_images[render_idx as usize].image,
            ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ash::vk::ImageLayout::PRESENT_SRC_KHR,
            1,
        )?;

        unsafe { self.device.end_command_buffer(command_buffer) }.map_err(|e| {
            GpuError::new(
                format!("CommandBuffer recording failed: {e:?}"),
                GpuErrorKind::RenderPass,
            )
        })?;

        let wait_flags = ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        let submit_info = ash::vk::SubmitInfo {
            wait_semaphore_count: 1,
            p_wait_semaphores: &present_semaphore,
            p_wait_dst_stage_mask: &wait_flags,
            command_buffer_count: 1,
            p_command_buffers: &command_buffer,
            signal_semaphore_count: 1,
            p_signal_semaphores: &render_semaphore,
            ..Default::default()
        };

        unsafe { self.device.queue_submit(self.queue, &[submit_info], fence) }.map_err(|e| {
            GpuError::new(
                format!("Failed to submit render pass to queue: {e:?}"),
                GpuErrorKind::Present,
            )
        })
    }

    fn present(&mut self) -> Result<(), GpuError> {
        let Some(CurrentFrame {
            render_idx,
            render_sem: render_semaphore,
            ..
        }) = self.current_frame
        else {
            return Err(GpuError::new(
                "Cannot present frame if no frame was started!",
                GpuErrorKind::Other,
            ));
        };

        let present_info = ash::vk::PresentInfoKHR {
            wait_semaphore_count: 1,
            p_wait_semaphores: &render_semaphore,
            swapchain_count: 1,
            p_swapchains: &self.swapchain.swapchain,
            p_image_indices: &render_idx,
            ..Default::default()
        };

        match unsafe {
            self.swapchain
                .fn_ptr
                .queue_present(self.queue, &present_info)
        } {
            Ok(suboptimal) => {
                if suboptimal {
                    return self.recreate_swapchain();
                }
            }
            Err(e) => {
                if e == ash::vk::Result::SUBOPTIMAL_KHR
                    || e == ash::vk::Result::ERROR_OUT_OF_DATE_KHR
                {
                    return self.recreate_swapchain();
                }
                return Err(GpuError::new(
                    format!("Queue Present failed: {e:?}"),
                    GpuErrorKind::Present,
                ));
            }
        }
        self.current_frame = None;
        Ok(())
    }

    fn begin_render_pass(&mut self, desc: &RenderPassDesc<Self>) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            println!("Cannot begin render pass if no frame was started..");
            return;
        };

        let color_attachments = desc
            .color_targets
            .iter()
            .map(|target| {
                let clear_value = ash::vk::ClearValue {
                    color: ash::vk::ClearColorValue {
                        float32: target.clear_color,
                    },
                };
                let load_op: ash::vk::AttachmentLoadOp = target.load_op.into();
                let store_op = ash::vk::AttachmentStoreOp::STORE;

                let image_view = target.target.image_view;
                ash::vk::RenderingAttachmentInfo {
                    image_view,
                    image_layout: ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    load_op,
                    store_op,
                    clear_value,
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();

        let (depth_target, depth_target_count) = if let Some(target) = &desc.depth_target {
            let clear_value = ash::vk::ClearValue {
                depth_stencil: ash::vk::ClearDepthStencilValue {
                    depth: target.clear_depth,
                    ..Default::default()
                },
            };
            let load_op: ash::vk::AttachmentLoadOp = target.load_op.into();
            let store_op = ash::vk::AttachmentStoreOp::STORE;

            let image_view = target.target.image_view;
            (
                ash::vk::RenderingAttachmentInfo {
                    image_view,
                    image_layout: ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                    load_op,
                    store_op,
                    clear_value,
                    ..Default::default()
                },
                1,
            )
        } else {
            (ash::vk::RenderingAttachmentInfo::default(), 0)
        };

        let depth_target_ptr = if depth_target_count == 1 {
            &depth_target
        } else {
            std::ptr::null()
        };
        let rendering_info = ash::vk::RenderingInfo {
            render_area: ash::vk::Rect2D {
                offset: ash::vk::Offset2D { x: 0i32, y: 0i32 },
                extent: self.swapchain.swapchain_extent,
                ..Default::default()
            },
            layer_count: 1,
            color_attachment_count: color_attachments.len() as u32,
            p_color_attachments: color_attachments.as_ptr(),
            p_depth_attachment: depth_target_ptr,
            ..Default::default()
        };

        unsafe {
            self.device
                .cmd_begin_rendering(command_buffer, &rendering_info);
        }
    }

    fn end_render_pass(&mut self) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            println!("Cannot end render pass without begin_frame called first");
            return;
        };
        unsafe {
            self.device.cmd_end_rendering(command_buffer);
        }
    }

    fn set_pipeline(&mut self, pipeline: &Self::Pipeline) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            println!("Cannot begin render pass if no frame was started..");
            return;
        };
        unsafe {
            self.device
                .cmd_bind_pipeline(command_buffer, pipeline.bind_point, pipeline.handle);
        }
    }

    fn set_viewport(&mut self, viewport: &ViewportDesc) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            println!("Cannot begin render pass if no frame was started..");
            return;
        };

        unsafe {
            self.device
                .cmd_set_viewport(command_buffer, 0, &[viewport.into()]);
        }
    }

    fn bind_texture(&mut self, slot: u32, texture: &Self::Texture) {
        let Some(CurrentFrame { idx, .. }) = self.current_frame else {
            return;
        };
        let dst_set = self.descriptors.sets[idx];
        let writes = map_slot_to_descriptor_writes(slot, BindingType::Texture(texture), dst_set);
        unsafe {
            self.device.update_descriptor_sets(writes.as_slice(), &[]);
        }
    }

    fn bind_render_target_as_texture(&mut self, slot: u32, target: &Self::RenderTarget) {
        let Some(CurrentFrame { idx, .. }) = self.current_frame else {
            return;
        };
        let dst_set = self.descriptors.sets[idx];
        let writes = map_slot_to_descriptor_writes(slot, BindingType::Texture(target), dst_set);
        unsafe {
            self.device.update_descriptor_sets(writes.as_slice(), &[]);
        }
    }

    fn bind_uniform(&mut self, _stage: ShaderStage, slot: u32, buffer: &Self::Buffer) {
        let Some(CurrentFrame { idx, .. }) = self.current_frame else {
            return;
        };
        let dst_set = self.descriptors.sets[idx];
        let writes = map_slot_to_descriptor_writes(slot, BindingType::Buffer(buffer), dst_set);
        unsafe {
            self.device.update_descriptor_sets(writes.as_slice(), &[]);
        }
    }

    fn set_vertex_buffer(&mut self, buffer: &Self::Buffer) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            return;
        };

        unsafe {
            self.device
                .cmd_bind_vertex_buffers(command_buffer, 0, &[buffer.buffer], &[0u64]);
        }
    }

    fn set_index_buffer(&mut self, buffer: &Self::Buffer) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            return;
        };
        unsafe {
            self.device.cmd_bind_index_buffer(
                command_buffer,
                buffer.buffer,
                0u64,
                ash::vk::IndexType::UINT32,
            );
        }
    }

    fn draw_indexed(&mut self, index_count: u32, first_index: u32, base_vertex: i32) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            return;
        };
        unsafe {
            self.device.cmd_draw_indexed(
                command_buffer,
                index_count,
                1,
                first_index,
                base_vertex,
                0,
            );
        }
    }

    fn backbuffer(&self) -> &Self::RenderTarget {
        let Some(CurrentFrame { render_idx, .. }) = self.current_frame else {
            return &self.swapchain.swapchain_images[0];
        };
        &self.swapchain.swapchain_images[render_idx as usize]
    }

    fn main_depth_target(&self) -> &Self::RenderTarget {
        let Some(CurrentFrame { idx, .. }) = self.current_frame else {
            return &self.depth_targets[0];
        };
        &self.depth_targets[idx]
    }

    fn default_viewport(&self) -> ViewportDesc {
        let (w, h) = self.resolution();
        ViewportDesc {
            x: 0.0,
            y: 0.0,
            width: w as f32,
            height: h as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }

    fn resolution(&self) -> (u32, u32) {
        let extent = self.swapchain.swapchain_extent;
        (extent.width, extent.height)
    }

    // recreate swapchain reads new width & height from window
    fn resize(&mut self, _width: u32, _height: u32) {
        if let Err(e) = self.recreate_swapchain() {
            println!("Failed to recreate swapchain on resize")
        }
    }
}

pub struct VulkanPipeline {
    label: String,
    handle: ash::vk::Pipeline,
    bind_point: ash::vk::PipelineBindPoint,
}

enum BindingType<'a> {
    Texture(&'a VulkanTexture),
    Buffer(&'a VulkanBuffer),
}

struct DescriptorWrites<'a> {
    inner: Vec<ash::vk::WriteDescriptorSet<'a>>,
    image_infos: Vec<ash::vk::DescriptorImageInfo>,
    buffer_infos: Vec<ash::vk::DescriptorBufferInfo>,
}

impl<'a> DescriptorWrites<'a> {
    pub fn as_slice(&self) -> &[ash::vk::WriteDescriptorSet<'a>] {
        &self.inner
    }
}

fn map_slot_to_descriptor_writes(
    slot: u32,
    binding_type: BindingType,
    dst_set: ash::vk::DescriptorSet,
) -> DescriptorWrites<'_> {
    let mut writes = DescriptorWrites {
        inner: Vec::new(),
        image_infos: Vec::new(),
        buffer_infos: Vec::new(),
    };
    match binding_type {
        BindingType::Texture(VulkanTexture {
            view_type,
            image_view,
            sampler,
            compare_enabled,
            ..
        }) => {
            if *compare_enabled {
                // slots 24-27 → comparison texture: 2 writes (image + sampler)
                // array offset = slot - 24
                let offset = slot;

                let sampled_image_info = ash::vk::DescriptorImageInfo {
                    image_view: *image_view,
                    image_layout: ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                };
                let sampler_info = ash::vk::DescriptorImageInfo {
                    sampler: *sampler,
                    ..Default::default()
                };
                writes.image_infos.push(sampled_image_info);
                writes.image_infos.push(sampler_info);

                writes.inner.push(ash::vk::WriteDescriptorSet {
                    dst_set,
                    dst_binding: 6u32,
                    dst_array_element: offset,
                    descriptor_type: ash::vk::DescriptorType::SAMPLED_IMAGE,
                    descriptor_count: 1,
                    p_image_info: &writes.image_infos[0],
                    ..Default::default()
                });
                writes.inner.push(ash::vk::WriteDescriptorSet {
                    dst_set,
                    dst_binding: 7u32,
                    dst_array_element: offset,
                    descriptor_type: ash::vk::DescriptorType::SAMPLER,
                    descriptor_count: 1,
                    p_image_info: &writes.image_infos[1],
                    ..Default::default()
                });
            } else if *view_type == ash::vk::ImageViewType::CUBE {
                // slots 20-23 → cubemap: binding 5, array offset = slot - 20
                let offset = slot;
                let info = ash::vk::DescriptorImageInfo {
                    image_view: *image_view,
                    image_layout: ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    sampler: *sampler,
                };
                writes.image_infos.push(info);
                writes.inner.push(ash::vk::WriteDescriptorSet {
                    dst_set,
                    dst_binding: 5,
                    dst_array_element: offset,
                    descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: 1,
                    p_image_info: &writes.image_infos[0],
                    ..Default::default()
                });
            } else {
                // slots 4-19 → regular textures: binding 4, array offset = slot - 4
                let offset = slot;
                let info = ash::vk::DescriptorImageInfo {
                    image_view: *image_view,
                    image_layout: ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    sampler: *sampler,
                };
                writes.image_infos.push(info);
                writes.inner.push(ash::vk::WriteDescriptorSet {
                    dst_set,
                    dst_binding: 4,
                    dst_array_element: offset,
                    descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: 1,
                    p_image_info: &writes.image_infos[0],
                    ..Default::default()
                });
            }
        }
        BindingType::Buffer(buffer) => {
            let info = ash::vk::DescriptorBufferInfo {
                buffer: buffer.buffer,
                offset: 0,
                range: ash::vk::WHOLE_SIZE,
            };
            writes.buffer_infos.push(info);
            writes.inner.push(ash::vk::WriteDescriptorSet {
                dst_set,
                dst_binding: slot,
                dst_array_element: 0,
                descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
                p_buffer_info: &writes.buffer_infos[0],
                ..Default::default()
            });
        }
    }
    writes
}
