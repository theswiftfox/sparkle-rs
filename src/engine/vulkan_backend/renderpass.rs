use crate::engine::{
    backend::{GpuError, GpuErrorKind},
    vulkan_backend::{FRAMES_IN_FLIGHT, VulkanBackend, util::gpu_error_out_of_range},
};

impl VulkanBackend {
    pub fn wait_idle(&self) -> Result<(), GpuError> {
        unsafe { self.device.device_wait_idle() }.map_err(|e| {
            GpuError::new(
                format!("Failed to wait for device idle: {e:?}"),
                GpuErrorKind::Other,
            )
        })
    }

    pub fn draw(&mut self) -> Result<(), GpuError> {
        let frame_idx = self.frame_idx;
        self.frame_idx = (self.frame_idx + 1) % (FRAMES_IN_FLIGHT as usize);

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

        let (idx, _optimal) = match unsafe {
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
            .get(idx as usize)
            .ok_or_else(|| {
                gpu_error_out_of_range(
                    "Render Completed Semaphore",
                    idx as usize,
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

        self.render_pass(command_buffer, idx)?;

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
        })?;

        let present_info = ash::vk::PresentInfoKHR {
            wait_semaphore_count: 1,
            p_wait_semaphores: &render_semaphore,
            swapchain_count: 1,
            p_swapchains: &self.swapchain.swapchain,
            p_image_indices: &idx,
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
        Ok(())
    }

    fn render_pass(
        &self,
        command_buffer: ash::vk::CommandBuffer,
        image_idx: u32,
    ) -> Result<(), GpuError> {
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

        let (image, image_view) = self
            .swapchain
            .swapchain_images
            .get(image_idx as usize)
            .ok_or_else(|| {
                GpuError::new(
                    format!(
                        "ImageIdx {image_idx} outside of range for swapchaing images {}",
                        self.swapchain.swapchain_images.len()
                    ),
                    GpuErrorKind::RenderPass,
                )
            })
            .map(|tx| (tx.image, tx.image_view))?;

        self.transition_image_layout(
            command_buffer,
            image,
            ash::vk::ImageLayout::UNDEFINED,
            ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            1,
        )?;

        let clear_value = ash::vk::ClearValue {
            color: ash::vk::ClearColorValue {
                float32: [0.0f32, 0.0f32, 0.0f32, 1.0f32],
            },
        };
        let attachment_info = ash::vk::RenderingAttachmentInfo {
            image_view: image_view,
            image_layout: ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            load_op: ash::vk::AttachmentLoadOp::CLEAR,
            store_op: ash::vk::AttachmentStoreOp::STORE,
            clear_value,
            ..Default::default()
        };

        let rendering_info = ash::vk::RenderingInfo {
            render_area: ash::vk::Rect2D {
                offset: ash::vk::Offset2D { x: 0i32, y: 0i32 },
                extent: self.swapchain.swapchain_extent,
                ..Default::default()
            },
            layer_count: 1,
            color_attachment_count: 1,
            p_color_attachments: &attachment_info,
            ..Default::default()
        };

        unsafe {
            self.device
                .cmd_begin_rendering(command_buffer, &rendering_info);
        }

        unsafe {
            self.device.cmd_bind_pipeline(
                command_buffer,
                ash::vk::PipelineBindPoint::GRAPHICS,
                self.graphics_pipeline,
            );
        }

        let viewport = ash::vk::Viewport {
            x: 0.0f32,
            y: 0.0f32,
            width: self.swapchain.swapchain_extent.width as f32,
            height: self.swapchain.swapchain_extent.height as f32,
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        };

        unsafe { self.device.cmd_set_viewport(command_buffer, 0, &[viewport]) }

        let scissor = ash::vk::Rect2D {
            offset: ash::vk::Offset2D { x: 0i32, y: 0i32 },
            extent: self.swapchain.swapchain_extent,
        };

        unsafe {
            self.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
        }

        unsafe {
            self.device.cmd_draw(command_buffer, 3, 1, 0, 0);
        }

        unsafe {
            self.device.cmd_end_rendering(command_buffer);
        }

        self.transition_image_layout(
            command_buffer,
            image,
            ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ash::vk::ImageLayout::PRESENT_SRC_KHR,
            1,
        )?;

        unsafe { self.device.end_command_buffer(command_buffer) }.map_err(|e| {
            GpuError::new(
                format!("CommandBuffer recording failed: {e:?}"),
                GpuErrorKind::RenderPass,
            )
        })
    }

    pub fn transition_image_layout(
        &self,
        command_buffer: ash::vk::CommandBuffer,
        image: ash::vk::Image,
        old_layout: ash::vk::ImageLayout,
        new_layout: ash::vk::ImageLayout,
        layer_count: u32,
    ) -> Result<(), GpuError> {
        let mut src_access_mask = ash::vk::AccessFlags2::empty();
        let mut dst_access_mask = ash::vk::AccessFlags2::empty();
        let mut src_stage_mask = ash::vk::PipelineStageFlags2::empty();
        let mut dst_stage_mask = ash::vk::PipelineStageFlags2::empty();

        match (old_layout, new_layout) {
            (ash::vk::ImageLayout::UNDEFINED, ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) => {
                dst_access_mask = ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;
                src_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
                dst_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
            }
            (
                ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ash::vk::ImageLayout::PRESENT_SRC_KHR,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;

                src_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
                dst_stage_mask = ash::vk::PipelineStageFlags2::BOTTOM_OF_PIPE;
            }
            (ash::vk::ImageLayout::UNDEFINED, ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL) => {
                dst_access_mask = ash::vk::AccessFlags2::TRANSFER_WRITE;

                src_stage_mask = ash::vk::PipelineStageFlags2::TOP_OF_PIPE;
                dst_stage_mask = ash::vk::PipelineStageFlags2::TRANSFER;
            }
            (
                ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::TRANSFER_WRITE;
                dst_access_mask = ash::vk::AccessFlags2::SHADER_READ;

                src_stage_mask = ash::vk::PipelineStageFlags2::TRANSFER;
                dst_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
            }
            _ => {
                return Err(GpuError::new(
                    format!("Invalid layout transition {old_layout:?} -> {new_layout:?}"),
                    GpuErrorKind::ResourceUpdate,
                ));
            }
        };

        let barrier = ash::vk::ImageMemoryBarrier2 {
            src_stage_mask,
            src_access_mask,
            dst_stage_mask,
            dst_access_mask,
            old_layout,
            new_layout,
            src_queue_family_index: ash::vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: ash::vk::QUEUE_FAMILY_IGNORED,
            image,
            subresource_range: ash::vk::ImageSubresourceRange {
                aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count,
                ..Default::default()
            },
            ..Default::default()
        };
        let dependency_info = ash::vk::DependencyInfo {
            dependency_flags: ash::vk::DependencyFlags::empty(),
            image_memory_barrier_count: 1,
            p_image_memory_barriers: &barrier,
            ..Default::default()
        };

        unsafe {
            self.device
                .cmd_pipeline_barrier2(command_buffer, &dependency_info);
        };

        Ok(())
    }
}
