// use std::any::TypeId;

use std::ffi::CString;

use crate::engine::{
    backend::{
        BlendMode, BufferDesc, BufferUsage, Drawable, GpuBackend, GpuError, GpuErrorKind,
        PipelineDesc, RenderPassDesc, RenderTargetDesc, SamplerDesc, ShaderStage, Shaders,
        TextureDesc, TextureFormat, ViewportDesc,
    },
    vulkan_backend::{
        CurrentFrame, FRAMES_IN_FLIGHT, PushConstants, SHADER_ENTRY_POINT, VulkanBackend,
        buffer::VulkanBuffer, create_shader_module, texture::VulkanTexture,
        util::gpu_error_out_of_range,
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
            code: include_bytes!("../../shaders/spv/deferred/pre_pixel.spv"),
        };
        let deferred_light_vtx = Shader {
            label: "Deferred Light VTX",
            stage: ash::vk::ShaderStageFlags::VERTEX,
            code: include_bytes!("../../shaders/spv/deferred/light_vertex.spv"),
        };
        let deferred_light_pxl = Shader {
            label: "Deferred Light PXL",
            stage: ash::vk::ShaderStageFlags::FRAGMENT,
            code: include_bytes!("../../shaders/spv/deferred/light_pixel.spv"),
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
        let mut tex = if matches!(
            desc.format,
            TextureFormat::Depth24Stencil8 | TextureFormat::Depth32Float
        ) {
            self.create_depth_texture(desc.width, desc.height, desc.format, &Some(desc.sampler))?
        } else {
            self.create_vk_texture(desc, data)?
        };
        self.register_texture(&mut tex);
        Ok(tex)
    }

    fn create_cubemap(
        &self,
        faces: [&[u8]; 6],
        width: u32,
        height: u32,
        format: TextureFormat,
        sampler: &SamplerDesc,
    ) -> Result<Self::Texture, GpuError> {
        let mut tex = self.create_vk_cubemap(faces, width, height, format, sampler)?;
        self.register_texture(&mut tex);
        Ok(tex)
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
        let mut flags = ash::vk::MemoryPropertyFlags::DEVICE_LOCAL;
        if desc.usage == BufferUsage::Uniform {
            flags = flags
                | ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT;
        }
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
        let mut rt =
            Self::create_vk_render_target(&self.instance, &self.device, self.phys_device, desc)?;
        self.register_texture(&mut rt);
        Ok(rt)
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

        let pipeline_vtx_input_state = ash::vk::PipelineVertexInputStateCreateInfo {
            vertex_binding_description_count: 1,
            p_vertex_binding_descriptions: &vtx_input_state,
            vertex_attribute_description_count: attributes.len() as _,
            p_vertex_attribute_descriptions: attributes.as_ptr(),
            ..Default::default()
        };
        let p_vertex_input_state = if desc.vertex_layout.is_none() {
            std::ptr::null()
        } else {
            &pipeline_vtx_input_state
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
            front_face: ash::vk::FrontFace::COUNTER_CLOCKWISE,
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
            .map(|_| {
                let (enable, src_color, dst_color, src_alpha, dst_alpha) = match desc.blend_mode {
                    BlendMode::None => (
                        ash::vk::FALSE,
                        ash::vk::BlendFactor::ZERO,
                        ash::vk::BlendFactor::ZERO,
                        ash::vk::BlendFactor::ZERO,
                        ash::vk::BlendFactor::ZERO,
                    ),
                    BlendMode::Additive => (
                        ash::vk::TRUE,
                        ash::vk::BlendFactor::ONE,
                        ash::vk::BlendFactor::ONE,
                        ash::vk::BlendFactor::ONE,
                        ash::vk::BlendFactor::ONE,
                    ),
                    BlendMode::Alpha => (
                        ash::vk::TRUE,
                        ash::vk::BlendFactor::SRC_ALPHA,
                        ash::vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                        ash::vk::BlendFactor::ONE,
                        ash::vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                    ),
                };
                ash::vk::PipelineColorBlendAttachmentState {
                    blend_enable: enable,
                    src_color_blend_factor: src_color,
                    dst_color_blend_factor: dst_color,
                    src_alpha_blend_factor: src_alpha,
                    dst_alpha_blend_factor: dst_alpha,
                    color_write_mask: ash::vk::ColorComponentFlags::RGBA,
                    ..Default::default()
                }
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
                depth_test_enable: ash::vk::TRUE,
                depth_compare_op: desc.depth_compare.into(),
                depth_bounds_test_enable: ash::vk::FALSE,
                stencil_test_enable: ash::vk::FALSE,
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
            p_vertex_input_state,
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
            if buffer.is_host_mapable() {
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
                let command_buffer = backend.begin_single_time_commands().unwrap();
                let (b_staging, m_staging) = VulkanBackend::create_buffer(
                    &backend.instance,
                    &backend.device,
                    backend.phys_device,
                    size,
                    ash::vk::BufferUsageFlags::TRANSFER_SRC,
                    ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                        | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
                )
                .unwrap();
                backend
                    .copy_to_buffer(m_staging, data.as_ptr() as *const _, size)
                    .unwrap();
                backend.copy_buffer(command_buffer, b_staging, 0, buffer.buffer, 0, size);
                backend.end_single_time_commands(command_buffer).unwrap();

                unsafe {
                    backend.device.destroy_buffer(b_staging, None);
                    backend.device.free_memory(m_staging, None);
                }
                Ok(())
            }
        }
        if let Err(e) = update_buffer_safe(&self, buffer, data) {
            panic!("Failed to update buffer: {e:?}")
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

        let swapchain_texture = self
            .swapchain
            .swapchain_images
            .get(swapchain_idx as usize)
            .ok_or_else(|| {
                gpu_error_out_of_range(
                    "Swapchain Image",
                    swapchain_idx as usize,
                    self.swapchain.swapchain_images.len(),
                )
            })?;

        let depth_texture = &self.depth_targets[frame_idx];

        unsafe {
            self.device.begin_command_buffer(
                command_buffer,
                &ash::vk::CommandBufferBeginInfo {
                    ..Default::default()
                },
            )
        }
        .map_err(|e| {
            GpuError::new(
                format!("CommandBuffer::begin failed {e:?}"),
                GpuErrorKind::RenderPass,
            )
        })?;

        self.transition_image_layout(
            command_buffer,
            swapchain_texture.image,
            swapchain_texture.current_layout.get(),
            ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ash::vk::ImageAspectFlags::COLOR,
            1,
        )?;
        swapchain_texture
            .current_layout
            .set(ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

        self.transition_image_layout(
            command_buffer,
            depth_texture.image,
            depth_texture.current_layout.get(),
            ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            ash::vk::ImageAspectFlags::DEPTH,
            1,
        )?;
        depth_texture
            .current_layout
            .set(ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL);

        unsafe {
            self.device.cmd_bind_descriptor_sets(
                command_buffer,
                ash::vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptors.sets[frame_idx]],
                &[],
            );
        }

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

        let swapchain_tex = &self.swapchain.swapchain_images[render_idx as usize];
        self.transition_image_layout(
            command_buffer,
            swapchain_tex.image,
            swapchain_tex.current_layout.get(),
            ash::vk::ImageLayout::PRESENT_SRC_KHR,
            ash::vk::ImageAspectFlags::COLOR,
            1,
        )?;
        swapchain_tex
            .current_layout
            .set(ash::vk::ImageLayout::PRESENT_SRC_KHR);

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

        desc.color_targets.iter().for_each(|target| {
            let old_layout = target.target.current_layout.get();
            self.transition_image_layout(
                command_buffer,
                target.target.image,
                old_layout,
                ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ash::vk::ImageAspectFlags::COLOR,
                1,
            )
            .unwrap();
            target
                .target
                .current_layout
                .set(ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        });
        if let Some(depth_target) = &desc.depth_target {
            let old_layout = depth_target.target.current_layout.get();
            self.transition_image_layout(
                command_buffer,
                depth_target.target.image,
                old_layout,
                ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                ash::vk::ImageAspectFlags::DEPTH,
                1,
            )
            .unwrap();
            depth_target
                .target
                .current_layout
                .set(ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL);
        }

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

        self.current_pass_targets.clear();
        for target in &desc.color_targets {
            self.current_pass_targets.push((
                target.target.image,
                ash::vk::ImageAspectFlags::COLOR,
                target.target.current_layout.clone(),
            ));
        }
        if let Some(depth) = &desc.depth_target {
            self.current_pass_targets.push((
                depth.target.image,
                ash::vk::ImageAspectFlags::DEPTH,
                depth.target.current_layout.clone(),
            ));
        }

        let area = if let Some(att) = desc.color_targets.first() {
            ash::vk::Rect2D {
                offset: ash::vk::Offset2D { x: 0i32, y: 0i32 },
                extent: ash::vk::Extent2D {
                    width: att.target.width,
                    height: att.target.height,
                },
                ..Default::default()
            }
        } else if let Some(att) = &desc.depth_target {
            ash::vk::Rect2D {
                offset: ash::vk::Offset2D { x: 0i32, y: 0i32 },
                extent: ash::vk::Extent2D {
                    width: att.target.width,
                    height: att.target.height,
                },
                ..Default::default()
            }
        } else {
            ash::vk::Rect2D {
                offset: ash::vk::Offset2D { x: 0i32, y: 0i32 },
                extent: self.swapchain.swapchain_extent,
                ..Default::default()
            }
        };

        let rendering_info = ash::vk::RenderingInfo {
            render_area: area,
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
        for &(image, aspect, ref layout_cell) in &self.current_pass_targets {
            // Skip swapchain images — they transition to PRESENT_SRC_KHR
            // in end_frame, not SHADER_READ_ONLY_OPTIMAL
            // (swapchain images lack SAMPLED_BIT)
            if self
                .swapchain
                .swapchain_images
                .iter()
                .any(|tex| tex.image == image)
            {
                continue;
            }
            let old_layout = layout_cell.get();
            if self
                .transition_image_layout(
                    command_buffer,
                    image,
                    old_layout,
                    ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    aspect,
                    1,
                )
                .is_ok()
            {
                layout_cell.set(ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
            }
        }
        self.current_pass_targets.clear();
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

        let scissor = ash::vk::Rect2D {
            offset: ash::vk::Offset2D {
                x: viewport.x as _,
                y: viewport.y as _,
            },
            extent: ash::vk::Extent2D {
                width: viewport.width as _,
                height: viewport.height as _,
            },
        };

        unsafe {
            self.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
        }
    }

    fn bind_texture(&mut self, slot: u32, texture: &Self::Texture) {
        if texture.descriptor_index == u32::MAX {
            return;
        }
        match slot {
            0 => self.pending_push.tex0 = texture.descriptor_index,
            1 => self.pending_push.tex1 = texture.descriptor_index,
            2 => self.pending_push.tex2 = texture.descriptor_index,
            3 => self.pending_push.tex3 = texture.descriptor_index,
            _ => {}
        }
    }

    fn bind_render_target_as_texture(&mut self, slot: u32, target: &Self::RenderTarget) {
        if target.descriptor_index == u32::MAX {
            return;
        }
        match slot {
            0 => self.pending_push.tex0 = target.descriptor_index,
            1 => self.pending_push.tex1 = target.descriptor_index,
            2 => self.pending_push.tex2 = target.descriptor_index,
            3 => self.pending_push.tex3 = target.descriptor_index,
            _ => {}
        }
    }

    fn bind_uniform(&mut self, _stage: ShaderStage, _slot: u32, _buffer: &Self::Buffer) {
        // No-op: UBOs are permanently bound at bindings 0-5.
        // Model matrix goes through push constants via set_model_matrix().
    }

    fn bind_ubo_to_descriptor(&self, binding: u32, buffer: &Self::Buffer) {
        let info = ash::vk::DescriptorBufferInfo {
            buffer: buffer.buffer,
            offset: 0,
            range: ash::vk::WHOLE_SIZE,
        };
        for set in &self.descriptors.sets {
            let write = ash::vk::WriteDescriptorSet {
                dst_set: *set,
                dst_binding: binding,
                dst_array_element: 0,
                descriptor_type: ash::vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
                p_buffer_info: &info,
                ..Default::default()
            };
            unsafe { self.device.update_descriptor_sets(&[write], &[]) };
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
            self.device.cmd_push_constants(
                command_buffer,
                self.pipeline_layout,
                ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::FRAGMENT,
                0,
                std::slice::from_raw_parts(
                    &self.pending_push as *const PushConstants as *const u8,
                    std::mem::size_of::<PushConstants>(),
                ),
            );
            self.device.cmd_draw_indexed(
                command_buffer,
                index_count,
                1,
                first_index,
                base_vertex,
                0,
            );
        }
        self.pending_push = PushConstants::default();
    }

    fn set_model_matrix(&mut self, model: &glm::Mat4) {
        let data = crate::engine::backend::as_bytes(std::slice::from_ref(model));
        self.pending_push.model.copy_from_slice(unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const f32, 16)
        });
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
        ViewportDesc {
            x: 0.0,
            y: 0.0,
            width: self.swapchain.swapchain_extent.width as _,
            height: self.swapchain.swapchain_extent.height as _,
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
            println!("Failed to recreate swapchain on resize: {e:?}")
        }
    }

    fn begin_event(&self, name: &str) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            return;
        };
        let (Some(debug_utils_ext), true) = (
            &self.device.debug_utils_ext,
            self.instance.validation_enabled,
        ) else {
            return;
        };
        let name = format!("{name} - BEGIN");
        let Ok(c_str) = CString::new(name) else {
            return;
        };
        let c_str = c_str.into_raw();

        let marker_info = ash::vk::DebugUtilsLabelEXT {
            p_label_name: c_str,
            ..Default::default()
        };

        unsafe {
            debug_utils_ext.cmd_insert_debug_utils_label(command_buffer, &marker_info);
        }

        let _ = unsafe { CString::from_raw(c_str) }; // ensure its cleaned up again
    }

    fn end_event(&self) {
        let Some(CurrentFrame { command_buffer, .. }) = self.current_frame else {
            return;
        };
        let (Some(debug_utils_ext), true) = (
            &self.device.debug_utils_ext,
            self.instance.validation_enabled,
        ) else {
            return;
        };
        let name = format!("END EVENT");
        let Ok(c_str) = CString::new(name) else {
            return;
        };
        let c_str = c_str.into_raw();

        let marker_info = ash::vk::DebugUtilsLabelEXT {
            p_label_name: c_str,
            ..Default::default()
        };

        unsafe {
            debug_utils_ext.cmd_insert_debug_utils_label(command_buffer, &marker_info);
        }
        let _ = unsafe { CString::from_raw(c_str) }; // ensure its cleaned up again
    }
}

pub struct VulkanPipeline {
    label: String,
    handle: ash::vk::Pipeline,
    bind_point: ash::vk::PipelineBindPoint,
}

impl<B: GpuBackend> Drop for Drawable<B> {
    fn drop(&mut self) {
        // if TypeId::of::<B>() == TypeId::of::<VulkanBackend>() {
        //     let vtx_buffer = &mut self.vertex_buffer as *mut _ as *mut VulkanBuffer;
        //     unsafe { &(*vtx_buffer) }.destroy();
        //     let idx_buffer = &mut self.index_buffer as *mut _ as *mut VulkanBuffer;
        //     unsafe { &(*idx_buffer) }.destroy();
        //     let model_buffer = &mut self.model_buffer as *mut _ as *mut VulkanBuffer;
        //     unsafe { &(*model_buffer) }.destroy();
        // }
    }
}
