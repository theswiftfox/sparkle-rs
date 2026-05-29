use std::sync::atomic::{AtomicUsize, Ordering};
use std::{cell::Cell, rc::Rc};

use crate::engine::{
    backend::{
        AddressMode, CompareFunc, FilterMode, GpuError, GpuErrorKind, GpuRenderTarget, GpuTexture,
        RenderTargetDesc, SamplerDesc, TextureDesc, TextureFormat,
    },
    vulkan_backend::VulkanBackend,
};

static TEXTURE_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone)]
pub struct VulkanTexture {
    pub image: ash::vk::Image,
    pub mem: ash::vk::DeviceMemory,
    pub image_view: ash::vk::ImageView,
    pub sampler: ash::vk::Sampler,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub mip_levels: u32,
    pub view_type: ash::vk::ImageViewType,
    pub compare_enabled: bool,
    pub id: usize,
    /// Permanent slot in the global bindless descriptor array.
    /// `u32::MAX` means not yet registered (e.g. swapchain images, depth-only textures).
    pub descriptor_index: u32,
    pub device_handle: ash::Device,
    pub current_layout: Rc<Cell<ash::vk::ImageLayout>>,
}

impl GpuTexture for VulkanTexture {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn format(&self) -> TextureFormat {
        self.format
    }

    fn id(&self) -> usize {
        self.id
    }
}

impl VulkanTexture {
    pub fn destroy(self) {
        let VulkanTexture {
            image,
            mem,
            image_view,
            sampler,
            device_handle,
            ..
        } = self;

        unsafe {
            if sampler != ash::vk::Sampler::null() {
                device_handle.destroy_sampler(sampler, None);
            }
            if image_view != ash::vk::ImageView::null() {
                device_handle.destroy_image_view(image_view, None);
            }
            if image != ash::vk::Image::null() {
                device_handle.destroy_image(image, None);
            }
            if mem != ash::vk::DeviceMemory::null() {
                device_handle.free_memory(mem, None);
            }
        }
    }
}

impl VulkanBackend {
    /// Assign a permanent bindless descriptor slot to a texture and write it
    /// into every per-frame descriptor set. Skips textures without a sampler
    /// (depth-only attachments).
    pub fn register_texture(&self, tex: &mut VulkanTexture) {
        if tex.sampler == ash::vk::Sampler::null() {
            return;
        }

        let mut reg = self.texture_registry.borrow_mut();

        if tex.compare_enabled {
            let slot = reg.allocate_shadow();
            tex.descriptor_index = slot;
            for set in &self.descriptors.sets {
                let image_info = ash::vk::DescriptorImageInfo {
                    image_view: tex.image_view,
                    image_layout: ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                };
                let sampler_info = ash::vk::DescriptorImageInfo {
                    sampler: tex.sampler,
                    ..Default::default()
                };
                let writes = [
                    ash::vk::WriteDescriptorSet {
                        dst_set: *set,
                        dst_binding: 8,
                        dst_array_element: slot,
                        descriptor_type: ash::vk::DescriptorType::SAMPLED_IMAGE,
                        descriptor_count: 1,
                        p_image_info: &image_info,
                        ..Default::default()
                    },
                    ash::vk::WriteDescriptorSet {
                        dst_set: *set,
                        dst_binding: 9,
                        dst_array_element: slot,
                        descriptor_type: ash::vk::DescriptorType::SAMPLER,
                        descriptor_count: 1,
                        p_image_info: &sampler_info,
                        ..Default::default()
                    },
                ];
                unsafe { self.device.update_descriptor_sets(&writes, &[]) };
            }
        } else if tex.view_type == ash::vk::ImageViewType::CUBE {
            let slot = reg.allocate_cube();
            tex.descriptor_index = slot;
            for set in &self.descriptors.sets {
                let info = ash::vk::DescriptorImageInfo {
                    image_view: tex.image_view,
                    image_layout: ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    sampler: tex.sampler,
                };
                let write = ash::vk::WriteDescriptorSet {
                    dst_set: *set,
                    dst_binding: 7,
                    dst_array_element: slot,
                    descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: 1,
                    p_image_info: &info,
                    ..Default::default()
                };
                unsafe { self.device.update_descriptor_sets(&[write], &[]) };
            }
        } else {
            let slot = reg.allocate_2d();
            tex.descriptor_index = slot;
            for set in &self.descriptors.sets {
                let info = ash::vk::DescriptorImageInfo {
                    image_view: tex.image_view,
                    image_layout: ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    sampler: tex.sampler,
                };
                let write = ash::vk::WriteDescriptorSet {
                    dst_set: *set,
                    dst_binding: 6,
                    dst_array_element: slot,
                    descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: 1,
                    p_image_info: &info,
                    ..Default::default()
                };
                unsafe { self.device.update_descriptor_sets(&[write], &[]) };
            }
        }
    }

    pub fn create_vk_render_target(
        instance: &ash::Instance,
        device: &ash::Device,
        phys_device: ash::vk::PhysicalDevice,
        info: &RenderTargetDesc,
    ) -> Result<VulkanTexture, GpuError> {
        let format: ash::vk::Format = info.format.into();

        let (base_usage, aspect_mask) = match info.format {
            TextureFormat::Depth32Float | TextureFormat::Depth24Stencil8 => (
                ash::vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                ash::vk::ImageAspectFlags::DEPTH,
            ),
            _ => (
                ash::vk::ImageUsageFlags::COLOR_ATTACHMENT,
                ash::vk::ImageAspectFlags::COLOR,
            ),
        };

        let (rt, rt_mem) = Self::create_image(
            instance,
            device,
            phys_device,
            info.width,
            info.height,
            format,
            1,
            ash::vk::ImageTiling::OPTIMAL,
            base_usage | ash::vk::ImageUsageFlags::SAMPLED,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        let view_create_info = ash::vk::ImageViewCreateInfo {
            image: rt,
            view_type: ash::vk::ImageViewType::TYPE_2D,
            format,
            subresource_range: ash::vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
            ..Default::default()
        };

        let image_view =
            unsafe { device.create_image_view(&view_create_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to create ImageView for texture: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;

        let sampler_info = info.sampler.into_vk(&instance, phys_device, None);

        let sampler = unsafe { device.create_sampler(&sampler_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create Sampler for texture: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        Ok(VulkanTexture {
            image: rt,
            mem: rt_mem,
            image_view,
            sampler,
            width: info.width,
            height: info.height,
            format: info.format,
            mip_levels: 1,
            id: TEXTURE_ID.fetch_add(1, Ordering::SeqCst),
            compare_enabled: info.sampler.compare.is_some(),
            view_type: view_create_info.view_type,
            descriptor_index: u32::MAX,
            device_handle: device.clone(),
            current_layout: Rc::new(Cell::new(ash::vk::ImageLayout::UNDEFINED)),
        })
    }

    pub fn create_vk_texture(
        &self,
        info: &TextureDesc,
        image_data: &[u8],
    ) -> Result<VulkanTexture, GpuError> {
        let image_size = image_data.len() as ash::vk::DeviceSize;
        let (staging_buff, staging_mem) = Self::create_buffer(
            &self.instance,
            &self.device,
            self.phys_device,
            image_size,
            ash::vk::BufferUsageFlags::TRANSFER_SRC,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        // copy image to staging memory
        self.copy_to_buffer(staging_mem, image_data.as_ptr() as *const _, image_size)?;

        let format: ash::vk::Format = info.format.into();
        let mip_levels = calculate_mip_levels(info.width, info.height);
        let (tex_image, tex_mem) = Self::create_image(
            &self.instance,
            &self.device,
            self.phys_device,
            info.width,
            info.height,
            format,
            mip_levels,
            ash::vk::ImageTiling::OPTIMAL,
            ash::vk::ImageUsageFlags::TRANSFER_SRC
                | ash::vk::ImageUsageFlags::TRANSFER_DST
                | ash::vk::ImageUsageFlags::SAMPLED,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        let command_buff = self.begin_single_time_commands()?;
        self.transition_image_layout(
            command_buff,
            tex_image,
            ash::vk::ImageLayout::UNDEFINED,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ash::vk::ImageAspectFlags::COLOR,
            1,
            mip_levels,
        )?;
        self.copy_buffer_to_image(
            command_buff,
            staging_buff,
            0,
            tex_image,
            info.width,
            info.height,
            0,
            1,
        );

        self.generate_mipmaps(
            command_buff,
            tex_image,
            format,
            info.width,
            info.height,
            mip_levels,
        )?;

        self.end_single_time_commands(command_buff)?;

        let view_create_info = ash::vk::ImageViewCreateInfo {
            image: tex_image,
            view_type: ash::vk::ImageViewType::TYPE_2D,
            format,
            subresource_range: ash::vk::ImageSubresourceRange {
                aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: mip_levels,
                base_array_layer: 0,
                layer_count: 1,
            },
            ..Default::default()
        };

        let image_view = unsafe { self.device.create_image_view(&view_create_info, None) }
            .map_err(|e| {
                GpuError::new(
                    format!("Failed to create ImageView for texture: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;

        let sampler_info = info
            .sampler
            .into_vk(&self.instance, self.phys_device, Some(mip_levels));

        let sampler = unsafe { self.device.create_sampler(&sampler_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create Sampler for texture: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        Ok(VulkanTexture {
            image: tex_image,
            mem: tex_mem,
            image_view,
            sampler,
            width: info.width,
            height: info.height,
            format: info.format,
            mip_levels,
            id: TEXTURE_ID.fetch_add(1, Ordering::SeqCst),
            compare_enabled: info.sampler.compare.is_some(),
            view_type: view_create_info.view_type,
            descriptor_index: u32::MAX,
            device_handle: self.device.device.clone(),
            current_layout: Rc::new(Cell::new(ash::vk::ImageLayout::UNDEFINED)),
        })
    }

    pub fn create_vk_cubemap(
        &self,
        faces: [&[u8]; 6],
        width: u32,
        height: u32,
        format: TextureFormat,
        sampler_desc: &SamplerDesc,
    ) -> Result<VulkanTexture, GpuError> {
        let vk_format: ash::vk::Format = format.into();
        let total_size = faces.iter().map(|f| f.len() as u64).sum();
        let (staging, staging_mem) = Self::create_buffer(
            &self.instance,
            &self.device,
            self.phys_device,
            total_size,
            ash::vk::BufferUsageFlags::TRANSFER_SRC,
            ash::vk::MemoryPropertyFlags::HOST_VISIBLE
                | ash::vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let data_ptr = unsafe {
            self.device
                .map_memory(staging_mem, 0, total_size, ash::vk::MemoryMapFlags::empty())
        }
        .map_err(|e| {
            GpuError::new(
                format!("Failed to map cubemap staging memory: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;
        let mut offset = 0u64;
        for face_data in &faces {
            unsafe {
                data_ptr
                    .add(offset as usize)
                    .copy_from(face_data.as_ptr() as *const _, face_data.len());
            }
            offset += face_data.len() as u64;
        }
        unsafe { self.device.unmap_memory(staging_mem) }

        let image_create_info = ash::vk::ImageCreateInfo {
            image_type: ash::vk::ImageType::TYPE_2D,
            format: vk_format,
            extent: ash::vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            mip_levels: 1,
            array_layers: 6,
            samples: ash::vk::SampleCountFlags::TYPE_1,
            tiling: ash::vk::ImageTiling::OPTIMAL,
            usage: ash::vk::ImageUsageFlags::TRANSFER_DST | ash::vk::ImageUsageFlags::SAMPLED,
            sharing_mode: ash::vk::SharingMode::EXCLUSIVE,
            flags: ash::vk::ImageCreateFlags::CUBE_COMPATIBLE,
            ..Default::default()
        };

        let cubemap =
            unsafe { self.device.create_image(&image_create_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to create cubemap image: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;

        let mem_reqs = unsafe { self.device.get_image_memory_requirements(cubemap) };
        let alloc_info = ash::vk::MemoryAllocateInfo {
            allocation_size: mem_reqs.size,
            memory_type_index: Self::find_memory_type(
                &self.instance,
                self.phys_device,
                mem_reqs.memory_type_bits,
                ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?,
            ..Default::default()
        };
        let cubemap_mem =
            unsafe { self.device.allocate_memory(&alloc_info, None) }.map_err(|e| {
                GpuError::new(
                    format!("Failed to allocate cubemap memory: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;
        unsafe { self.device.bind_image_memory(cubemap, cubemap_mem, 0) }.map_err(|e| {
            GpuError::new(
                format!("Failed to bind cubemap memory: {e:?}"),
                GpuErrorKind::ResourceUpdate,
            )
        })?;

        let cmd = self.begin_single_time_commands()?;
        self.transition_image_layout(
            cmd,
            cubemap,
            ash::vk::ImageLayout::UNDEFINED,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ash::vk::ImageAspectFlags::COLOR,
            6,
            1,
        )?;
        let mut face_offset = 0u64;
        for i in 0..6 {
            let face_size = faces[i as usize].len() as u64;
            self.copy_buffer_to_image(cmd, staging, face_offset, cubemap, width, height, i, 1);
            face_offset += face_size;
        }
        self.transition_image_layout(
            cmd,
            cubemap,
            ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ash::vk::ImageAspectFlags::COLOR,
            6,
            1,
        )?;
        self.end_single_time_commands(cmd)?;

        let view_create_info = ash::vk::ImageViewCreateInfo {
            image: cubemap,
            view_type: ash::vk::ImageViewType::CUBE,
            format: vk_format,
            subresource_range: ash::vk::ImageSubresourceRange {
                aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 6,
            },
            ..Default::default()
        };

        let image_view = unsafe { self.device.create_image_view(&view_create_info, None) }
            .map_err(|e| {
                GpuError::new(
                    format!("Failed to create cubemap ImageView: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;

        let sampler_info = sampler_desc.into_vk(&self.instance, self.phys_device, None);
        let sampler = unsafe { self.device.create_sampler(&sampler_info, None) }.map_err(|e| {
            GpuError::new(
                format!("Failed to create cubemap sampler: {e:?}"),
                GpuErrorKind::ResourceCreation,
            )
        })?;

        Ok(VulkanTexture {
            image: cubemap,
            mem: cubemap_mem,
            image_view,
            sampler,
            width,
            height,
            format,
            mip_levels: 1,
            id: TEXTURE_ID.fetch_add(1, Ordering::SeqCst),
            compare_enabled: sampler_desc.compare.is_some(),
            view_type: ash::vk::ImageViewType::CUBE,
            descriptor_index: u32::MAX,
            device_handle: self.device.device.clone(),
            current_layout: Rc::new(Cell::new(ash::vk::ImageLayout::UNDEFINED)),
        })
    }

    pub fn create_depth_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureFormat,
        sampler_desc: &Option<SamplerDesc>,
    ) -> Result<VulkanTexture, GpuError> {
        if !matches!(
            format,
            TextureFormat::Depth24Stencil8 | TextureFormat::Depth32Float
        ) {
            return Err(GpuError::new(
                format!("Invalid Format for depth texture: {format:?}"),
                GpuErrorKind::Other,
            ));
        }

        let vk_format: ash::vk::Format = format.into();

        let (depth_img, _mem) = Self::create_image(
            &self.instance,
            &self.device,
            self.phys_device,
            width,
            height,
            vk_format,
            1,
            ash::vk::ImageTiling::OPTIMAL,
            ash::vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        let view_create_info = ash::vk::ImageViewCreateInfo {
            image: depth_img,
            view_type: ash::vk::ImageViewType::TYPE_2D,
            format: vk_format,
            subresource_range: ash::vk::ImageSubresourceRange {
                aspect_mask: ash::vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
            ..Default::default()
        };

        let image_view = unsafe { self.device.create_image_view(&view_create_info, None) }
            .map_err(|e| {
                GpuError::new(
                    format!("Failed to create ImageView for texture: {e:?}"),
                    GpuErrorKind::ResourceCreation,
                )
            })?;

        let (sampler, compare_enabled) = if let Some(sampler_desc) = sampler_desc {
            let sampler_info = sampler_desc.into_vk(&self.instance, self.phys_device, None);

            let sampler =
                unsafe { self.device.create_sampler(&sampler_info, None) }.map_err(|e| {
                    GpuError::new(
                        format!("Failed to create Sampler for texture: {e:?}"),
                        GpuErrorKind::ResourceCreation,
                    )
                })?;

            (sampler, sampler_desc.compare.is_some())
        } else {
            (ash::vk::Sampler::null(), false)
        };

        Ok(VulkanTexture {
            image: depth_img,
            mem: ash::vk::DeviceMemory::null(),
            image_view,
            sampler,
            width,
            height,
            format,
            mip_levels: 1,
            id: TEXTURE_ID.fetch_add(1, Ordering::SeqCst),
            compare_enabled,
            view_type: view_create_info.view_type,
            descriptor_index: u32::MAX,
            device_handle: self.device.device.clone(),
            current_layout: Rc::new(Cell::new(ash::vk::ImageLayout::UNDEFINED)),
        })
    }

    fn generate_mipmaps(
        &self,
        command_buffer: ash::vk::CommandBuffer,
        image: ash::vk::Image,
        format: ash::vk::Format,
        width: u32,
        height: u32,
        mip_levels: u32,
    ) -> Result<(), GpuError> {
        let format_props = unsafe {
            self.instance
                .get_physical_device_format_properties(self.phys_device, format)
        };
        if (format_props.optimal_tiling_features
            & ash::vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR)
            != ash::vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
        {
            return Err(GpuError::new(
                "texture image format does not support linear blitting!",
                GpuErrorKind::ResourceCreation,
            ));
        }

        let mut barrier = ash::vk::ImageMemoryBarrier {
            src_access_mask: ash::vk::AccessFlags::TRANSFER_WRITE,
            dst_access_mask: ash::vk::AccessFlags::TRANSFER_READ,
            old_layout: ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            new_layout: ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            src_queue_family_index: ash::vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: ash::vk::QUEUE_FAMILY_IGNORED,
            image,
            subresource_range: ash::vk::ImageSubresourceRange {
                aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                base_array_layer: 0,
                layer_count: 1,
                level_count: 1,
                base_mip_level: 0,
            },
            ..Default::default()
        };

        let mut mip_width = width;
        let mut mip_height = height;

        for i in 1..mip_levels {
            barrier.subresource_range.base_mip_level = i - 1;
            barrier.old_layout = ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL;
            barrier.new_layout = ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
            barrier.src_access_mask = ash::vk::AccessFlags::TRANSFER_WRITE;
            barrier.dst_access_mask = ash::vk::AccessFlags::TRANSFER_READ;

            unsafe {
                self.device.cmd_pipeline_barrier(
                    command_buffer,
                    ash::vk::PipelineStageFlags::TRANSFER,
                    ash::vk::PipelineStageFlags::TRANSFER,
                    ash::vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                );
            }

            let offsets = [
                ash::vk::Offset3D { x: 0, y: 0, z: 0 },
                ash::vk::Offset3D {
                    x: mip_width as i32,
                    y: mip_height as i32,
                    z: 1,
                },
            ];
            let dst_offsets = [
                ash::vk::Offset3D { x: 0, y: 0, z: 0 },
                ash::vk::Offset3D {
                    x: if mip_width > 1 {
                        (mip_width / 2) as i32
                    } else {
                        1
                    },
                    y: if mip_height > 1 {
                        (mip_height / 2) as i32
                    } else {
                        1
                    },
                    z: 1,
                },
            ];
            let blit = ash::vk::ImageBlit {
                src_subresource: ash::vk::ImageSubresourceLayers {
                    aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                    mip_level: i - 1,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                dst_subresource: ash::vk::ImageSubresourceLayers {
                    aspect_mask: ash::vk::ImageAspectFlags::COLOR,
                    mip_level: i,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                src_offsets: offsets,
                dst_offsets,
            };

            unsafe {
                self.device.cmd_blit_image(
                    command_buffer,
                    image,
                    ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    image,
                    ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[blit],
                    ash::vk::Filter::LINEAR,
                );
            }

            barrier.old_layout = ash::vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
            barrier.new_layout = ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            barrier.src_access_mask = ash::vk::AccessFlags::TRANSFER_READ;
            barrier.dst_access_mask = ash::vk::AccessFlags::SHADER_READ;

            unsafe {
                self.device.cmd_pipeline_barrier(
                    command_buffer,
                    ash::vk::PipelineStageFlags::TRANSFER,
                    ash::vk::PipelineStageFlags::FRAGMENT_SHADER,
                    ash::vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                );
            }

            if mip_width > 1 {
                mip_width = mip_width / 2;
            }
            if mip_height > 1 {
                mip_height = mip_height / 2
            }
        }

        barrier.subresource_range.base_mip_level = mip_levels - 1;
        barrier.old_layout = ash::vk::ImageLayout::TRANSFER_DST_OPTIMAL;
        barrier.new_layout = ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        barrier.src_access_mask = ash::vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = ash::vk::AccessFlags::SHADER_READ;

        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                ash::vk::PipelineStageFlags::TRANSFER,
                ash::vk::PipelineStageFlags::FRAGMENT_SHADER,
                ash::vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }

        Ok(())
    }
}

fn calculate_mip_levels(width: u32, height: u32) -> u32 {
    width.max(height).ilog2() + 1
}

impl std::fmt::Debug for VulkanTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanTexture")
            .field("image", &self.image)
            .field("mem", &self.mem)
            .field("image_view", &self.image_view)
            .field("sampler", &self.sampler)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("view_type", &self.view_type)
            .field("compare_enabled", &self.compare_enabled)
            .field("id", &self.id)
            .field("descriptor_index", &self.descriptor_index)
            .field("device_handle", &self.device_handle.handle())
            .field("current_layout", &self.current_layout)
            .finish()
    }
}

impl Into<ash::vk::Format> for TextureFormat {
    fn into(self) -> ash::vk::Format {
        match self {
            TextureFormat::R8Unorm => ash::vk::Format::R8_UNORM,
            TextureFormat::Rg8Unorm => ash::vk::Format::R8G8_UNORM,
            TextureFormat::Rgba8Unorm => ash::vk::Format::R8G8B8A8_UNORM,
            TextureFormat::Rgba8UnormSrgb => ash::vk::Format::R8G8B8A8_SRGB,
            TextureFormat::Bgra8Unorm => ash::vk::Format::B8G8R8A8_UNORM,
            TextureFormat::Bgra8UnormSrgb => ash::vk::Format::B8G8R8A8_SRGB,
            TextureFormat::Rgba32Float => ash::vk::Format::R32G32B32A32_SFLOAT,
            TextureFormat::Rgba32Uint => ash::vk::Format::R32G32B32A32_UINT,
            TextureFormat::R16g16b16a16Float => ash::vk::Format::R16G16B16A16_SFLOAT,
            TextureFormat::Depth32Float => ash::vk::Format::D32_SFLOAT,
            TextureFormat::Depth24Stencil8 => ash::vk::Format::D24_UNORM_S8_UINT,
            TextureFormat::Abgr10Unorm => ash::vk::Format::A2B10G10R10_UNORM_PACK32,
        }
    }
}
impl Into<ash::vk::Format> for &TextureFormat {
    fn into(self) -> ash::vk::Format {
        (*self).into()
    }
}

impl TryFrom<ash::vk::Format> for TextureFormat {
    type Error = GpuError;
    fn try_from(value: ash::vk::Format) -> Result<Self, Self::Error> {
        let format = match value {
            ash::vk::Format::R8_UNORM => TextureFormat::R8Unorm,
            ash::vk::Format::R8G8_UNORM => TextureFormat::Rg8Unorm,
            ash::vk::Format::R8G8B8A8_UNORM => TextureFormat::Rgba8Unorm,
            ash::vk::Format::R8G8B8A8_SRGB => TextureFormat::Rgba8UnormSrgb,
            ash::vk::Format::B8G8R8A8_UNORM => TextureFormat::Bgra8Unorm,
            ash::vk::Format::B8G8R8A8_SRGB => TextureFormat::Bgra8UnormSrgb,
            ash::vk::Format::R32G32B32A32_SFLOAT => TextureFormat::Rgba32Float,
            ash::vk::Format::R32G32B32A32_UINT => TextureFormat::Rgba32Uint,
            ash::vk::Format::R16G16B16A16_SFLOAT => TextureFormat::R16g16b16a16Float,
            ash::vk::Format::D32_SFLOAT => TextureFormat::Depth32Float,
            ash::vk::Format::D24_UNORM_S8_UINT => TextureFormat::Depth24Stencil8,
            ash::vk::Format::A2B10G10R10_UNORM_PACK32 => TextureFormat::Abgr10Unorm,
            f => {
                return Err(GpuError::new(
                    format!("VK Format {f:?} is not yet supported"),
                    GpuErrorKind::Other,
                ));
            }
        };
        Ok(format)
    }
}

impl GpuRenderTarget for VulkanTexture {}

impl SamplerDesc {
    fn into_vk(
        &self,
        instance: &ash::Instance,
        device: ash::vk::PhysicalDevice,
        mip_levels: Option<u32>,
    ) -> ash::vk::SamplerCreateInfo<'_> {
        let anisotropy = if self.filter == FilterMode::Anisotropic {
            ash::vk::TRUE
        } else {
            ash::vk::FALSE
        };

        let device_properties = unsafe { instance.get_physical_device_properties(device) };

        let (compare_enable, compare_op) = match self.compare {
            None => (ash::vk::FALSE, ash::vk::CompareOp::ALWAYS),
            Some(CompareFunc::Always) => (ash::vk::TRUE, ash::vk::CompareOp::ALWAYS),
            Some(CompareFunc::Equal) => (ash::vk::TRUE, ash::vk::CompareOp::EQUAL),
            Some(CompareFunc::Greater) => (ash::vk::TRUE, ash::vk::CompareOp::GREATER),
            Some(CompareFunc::GreaterEqual) => {
                (ash::vk::TRUE, ash::vk::CompareOp::GREATER_OR_EQUAL)
            }
            Some(CompareFunc::Less) => (ash::vk::TRUE, ash::vk::CompareOp::LESS),
            Some(CompareFunc::LessEqual) => (ash::vk::TRUE, ash::vk::CompareOp::LESS_OR_EQUAL),
            Some(CompareFunc::Never) => (ash::vk::TRUE, ash::vk::CompareOp::NEVER),
        };

        let mut info = ash::vk::SamplerCreateInfo {
            mag_filter: ash::vk::Filter::LINEAR,
            min_filter: ash::vk::Filter::LINEAR,
            anisotropy_enable: anisotropy,
            max_anisotropy: device_properties.limits.max_sampler_anisotropy,
            mipmap_mode: ash::vk::SamplerMipmapMode::LINEAR,
            address_mode_u: self.address_u.into(),
            address_mode_v: self.address_v.into(),
            address_mode_w: ash::vk::SamplerAddressMode::REPEAT,
            compare_enable,
            compare_op,
            ..Default::default()
        };
        if let Some(mip_levels) = mip_levels {
            info.min_lod = 0f32;
            info.max_lod = mip_levels as f32;
        }
        info
    }
}

impl Into<ash::vk::SamplerAddressMode> for AddressMode {
    fn into(self) -> ash::vk::SamplerAddressMode {
        match self {
            AddressMode::Repeat => ash::vk::SamplerAddressMode::REPEAT,
            AddressMode::Mirror => ash::vk::SamplerAddressMode::MIRRORED_REPEAT,
            AddressMode::Clamp => ash::vk::SamplerAddressMode::CLAMP_TO_EDGE,
        }
    }
}

impl Into<ash::vk::Filter> for FilterMode {
    fn into(self) -> ash::vk::Filter {
        match self {
            FilterMode::Nearest => ash::vk::Filter::NEAREST,
            FilterMode::Linear => ash::vk::Filter::LINEAR,
            // vk sets up anisotropic differently, so just return linear
            FilterMode::Anisotropic => ash::vk::Filter::LINEAR,
        }
    }
}
