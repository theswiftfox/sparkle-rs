use crate::engine::{
    backend::{GpuError, GpuErrorKind},
    vulkan_backend::VulkanBackend,
};

impl VulkanBackend {
    pub fn draw(&self) -> Result<(), GpuError> {
        unsafe {
            self.device
                .wait_for_fences(&[self.sync_objects.draw_fence], true, u64::MAX)
        }
        .map_err(|e| {
            GpuError::new(
                format!("Wait for Fences failed: {e:?}"),
                GpuErrorKind::Other,
            )
        })?;
        unsafe { self.device.reset_fences(&[self.sync_objects.draw_fence]) }.map_err(|e| {
            GpuError::new(format!("Reset Fences failed: {e:?}"), GpuErrorKind::Other)
        })?;

        let (idx, optimal) = unsafe {
            self.swapchain.fn_ptr.acquire_next_image(
                self.swapchain.swapchain,
                u64::MAX,
                self.sync_objects.present_sem,
                ash::vk::Fence::null(),
            )
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to get new swapchain image: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        if !optimal {
            println!("Suboptimal Swapchain image. Still continuing..");
        }

        self.render_pass(idx)?;

        let wait_flags = ash::vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        let submit_info = ash::vk::SubmitInfo {
            wait_semaphore_count: 1,
            p_wait_semaphores: &self.sync_objects.present_sem,
            p_wait_dst_stage_mask: &wait_flags,
            command_buffer_count: 1,
            p_command_buffers: &self.command_buffer,
            signal_semaphore_count: 1,
            p_signal_semaphores: &self.sync_objects.render_sem,
            ..Default::default()
        };

        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], self.sync_objects.draw_fence)
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to submit render pass to queue: {e:?}"),
                GpuErrorKind::Present,
            )
        })?;

        let present_info = ash::vk::PresentInfoKHR {
            wait_semaphore_count: 1,
            p_wait_semaphores: &self.sync_objects.render_sem,
            swapchain_count: 1,
            p_swapchains: &self.swapchain.swapchain,
            p_image_indices: &idx,
            ..Default::default()
        };

        unsafe {
            self.swapchain
                .fn_ptr
                .queue_present(self.queue, &present_info)
        }
        .map_err(|e| {
            GpuError::new(
                format!("Queue Present failed: {e:?}"),
                GpuErrorKind::Present,
            )
        })?;

        Ok(())
    }

    fn render_pass(&self, image_idx: u32) -> Result<(), GpuError> {
        unsafe {
            self.device.begin_command_buffer(
                self.command_buffer,
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

        let image = *self
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
            })?;

        let image_view = *self.image_views.get(image_idx as usize).ok_or_else(|| {
            GpuError::new(
                format!(
                    "ImageIdx {image_idx} outside of range for swapchain image views {}",
                    self.image_views.len()
                ),
                GpuErrorKind::RenderPass,
            )
        })?;

        self.transition_image_layout(
            image,
            ash::vk::ImageLayout::UNDEFINED,
            ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ash::vk::AccessFlags2::empty(),
            ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        );

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
                .cmd_begin_rendering(self.command_buffer, &rendering_info);
        }

        unsafe {
            self.device.cmd_bind_pipeline(
                self.command_buffer,
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

        unsafe {
            self.device
                .cmd_set_viewport(self.command_buffer, 0, &[viewport])
        }

        let scissor = ash::vk::Rect2D {
            offset: ash::vk::Offset2D { x: 0i32, y: 0i32 },
            extent: self.swapchain.swapchain_extent,
        };

        unsafe {
            self.device
                .cmd_set_scissor(self.command_buffer, 0, &[scissor]);
        }

        unsafe {
            self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
        }

        unsafe {
            self.device.cmd_end_rendering(self.command_buffer);
        }

        self.transition_image_layout(
            image,
            ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ash::vk::ImageLayout::PRESENT_SRC_KHR,
            ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            ash::vk::AccessFlags2::empty(),
            ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            ash::vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        );

        unsafe { self.device.end_command_buffer(self.command_buffer) }.map_err(|e| {
            GpuError::new(
                format!("CommandBuffer recording failed: {e:?}"),
                GpuErrorKind::RenderPass,
            )
        })
    }

    pub fn transition_image_layout(
        &self,
        image: ash::vk::Image,
        old_layout: ash::vk::ImageLayout,
        new_layout: ash::vk::ImageLayout,
        src_access_mask: ash::vk::AccessFlags2,
        dst_access_mask: ash::vk::AccessFlags2,
        src_stage_mask: ash::vk::PipelineStageFlags2,
        dst_stage_mask: ash::vk::PipelineStageFlags2,
    ) {
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
                layer_count: 1,
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

        // unsafe {
        //     self.device
        //         .cmd_pipeline_barrier2(self.command_buffer, &dependency_info);
        // };
        unsafe {
            self.khr_sync
                .cmd_pipeline_barrier2(self.command_buffer, &dependency_info);
        }
    }
}
