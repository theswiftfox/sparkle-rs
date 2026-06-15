use crate::engine::{
    backend::{GpuError, GpuErrorKind},
    vulkan_backend::{ENABLE_MARKER, VulkanBackend},
};

impl VulkanBackend {
    pub fn transition_image_layout(
        &self,
        command_buffer: ash::vk::CommandBuffer,
        image: ash::vk::Image,
        old_layout: ash::vk::ImageLayout,
        new_layout: ash::vk::ImageLayout,
        aspect: ash::vk::ImageAspectFlags,
        layer_count: u32,
        mip_map_levels: u32,
    ) -> Result<(), GpuError> {
        let mut src_access_mask = ash::vk::AccessFlags2::empty();
        let mut dst_access_mask = ash::vk::AccessFlags2::empty();
        let mut src_stage_mask = ash::vk::PipelineStageFlags2::empty();
        let mut dst_stage_mask = ash::vk::PipelineStageFlags2::empty();

        match (old_layout, new_layout) {
            (
                ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;
                dst_access_mask = ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                    | ash::vk::AccessFlags2::COLOR_ATTACHMENT_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
                dst_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
            }
            (
                ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE;
                dst_access_mask = ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | ash::vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
                dst_stage_mask = ash::vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | ash::vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
            }
            (ash::vk::ImageLayout::GENERAL, ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            | (ash::vk::ImageLayout::UNDEFINED, ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            | (
                ash::vk::ImageLayout::PRESENT_SRC_KHR,
                ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ) => {
                dst_access_mask = ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                    | ash::vk::AccessFlags2::COLOR_ATTACHMENT_READ;
                // src_stage_mask MUST include COLOR_ATTACHMENT_OUTPUT to sync with swapchain acquire
                src_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
                dst_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
            }
            (layout, _)
                if layout == new_layout
                    && layout != ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
                    && layout != ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL =>
            {
                // Skip only if not an attachment; attachments require a barrier even if the layout matches
                return Ok(());
            }
            (ash::vk::ImageLayout::GENERAL, ash::vk::ImageLayout::PRESENT_SRC_KHR)
            | (
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
            (ash::vk::ImageLayout::UNDEFINED, ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL) => {
                src_access_mask = ash::vk::AccessFlags2::empty();
                dst_access_mask = ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;

                src_stage_mask = ash::vk::PipelineStageFlags2::TOP_OF_PIPE;
                dst_stage_mask = ash::vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | ash::vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
            }
            (
                ash::vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL,
                ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::SHADER_READ;
                dst_access_mask = ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;

                src_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
                dst_stage_mask = ash::vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | ash::vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
            }
            (
                ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;
                dst_access_mask = ash::vk::AccessFlags2::SHADER_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
                dst_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
            }
            (ash::vk::ImageLayout::UNDEFINED, ash::vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL) => {
                src_access_mask = ash::vk::AccessFlags2::empty();
                dst_access_mask = ash::vk::AccessFlags2::SHADER_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::TOP_OF_PIPE;
                dst_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
            }
            (
                ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                ash::vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;
                dst_access_mask = ash::vk::AccessFlags2::SHADER_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | ash::vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
                dst_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
            }
            (
                ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE;
                dst_access_mask = ash::vk::AccessFlags2::SHADER_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | ash::vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
                dst_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
            }
            (
                ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ash::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::SHADER_READ;
                dst_access_mask = ash::vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                    | ash::vk::AccessFlags2::COLOR_ATTACHMENT_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
                dst_stage_mask = ash::vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
            }
            (
                ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ash::vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            ) => {
                src_access_mask = ash::vk::AccessFlags2::SHADER_READ;
                dst_access_mask = ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | ash::vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
                dst_stage_mask = ash::vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                    | ash::vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS;
            }
            (ash::vk::ImageLayout::UNDEFINED, ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => {
                src_access_mask = ash::vk::AccessFlags2::empty();
                dst_access_mask = ash::vk::AccessFlags2::SHADER_READ;
                src_stage_mask = ash::vk::PipelineStageFlags2::TOP_OF_PIPE;
                dst_stage_mask = ash::vk::PipelineStageFlags2::FRAGMENT_SHADER;
            }
            _ => {
                // panic!("Invalid layout transition {old_layout:?} -> {new_layout:?}");
                return Err(GpuError::new(
                    format!("Invalid layout transition {old_layout:?} -> {new_layout:?}"),
                    GpuErrorKind::ResourceUpdate,
                ));
            }
        };

        if ENABLE_MARKER {
            println!("MARKER ==== TRANSITION IMAGE\n  {old_layout:?} -> {new_layout:?}");
        }

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
                aspect_mask: aspect,
                base_mip_level: 0,
                level_count: mip_map_levels,
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
