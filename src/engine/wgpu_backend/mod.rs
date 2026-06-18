mod buffer;
mod texture;

pub use buffer::WgpuBuffer;
pub use texture::{WgpuRenderTarget, WgpuTexture};

use super::backend::*;
use std::sync::Arc;

// Pipeline wrapper

pub struct WgpuPipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) bind_group_layouts: Vec<Option<wgpu::BindGroupLayout>>,
    pub(crate) bind_group_descriptors: Vec<Vec<BindingType>>,
    /// Pre-created bind groups for groups with empty descriptors.
    pub(crate) empty_bind_groups: Vec<Option<wgpu::BindGroup>>,
}

// Deferred render commands

/// Commands recorded between begin_render_pass() and end_render_pass(),
/// replayed within a scoped wgpu::RenderPass to avoid self-referential borrows.
#[allow(dead_code)]
enum RenderCommand {
    SetPipeline(wgpu::RenderPipeline),
    SetViewport {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        min_depth: f32,
        max_depth: f32,
    },
    SetVertexBuffer(wgpu::Buffer),
    SetIndexBuffer(wgpu::Buffer),
    BindGroup(u32, wgpu::BindGroup),
    DrawIndexed {
        index_count: u32,
        first_index: u32,
        base_vertex: i32,
    },
}

// Binding accumulation state

/// A pending binding resource, accumulated between set_pipeline() and draw_indexed().
#[allow(dead_code)]
enum PendingResource {
    Buffer(wgpu::Buffer),
    TextureView(wgpu::TextureView),
    Sampler(wgpu::Sampler),
}

struct PendingColorAttachment {
    view: wgpu::TextureView,
    ops: wgpu::Operations<wgpu::Color>,
}

struct PendingDepthAttachment {
    view: wgpu::TextureView,
    depth_ops: wgpu::Operations<f32>,
}

struct PendingPass {
    label: String,
    color_attachments: Vec<PendingColorAttachment>,
    depth_attachment: Option<PendingDepthAttachment>,
    commands: Vec<RenderCommand>,
}

// WgpuBackend

pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    _window: Arc<winit::window::Window>,

    // Persistent render targets
    backbuffer_target: WgpuRenderTarget,
    depth_target: WgpuRenderTarget,

    // Per-frame state
    surface_texture: Option<wgpu::SurfaceTexture>,
    command_buffers: Vec<wgpu::CommandBuffer>,
    pending_pass: Option<PendingPass>,

    // Binding accumulation state (active during a render pass)
    current_pipeline_layouts: Vec<Option<wgpu::BindGroupLayout>>,
    current_pipeline_descriptors: Vec<Vec<BindingType>>,
    current_empty_bind_groups: Vec<Option<wgpu::BindGroup>>,
    /// Per-group pending bindings. Index = binding index within that group.
    pending_bindings: [Vec<Option<PendingResource>>; 4],

    width: u32,
    height: u32,

    /// Egui GPU renderer, lazily created on first `render_egui` call.
    egui_renderer: Option<egui_wgpu::Renderer>,
}

// Format conversion helpers

fn to_wgpu_format(format: TextureFormat) -> wgpu::TextureFormat {
    match format {
        TextureFormat::R8Unorm => wgpu::TextureFormat::R8Unorm,
        TextureFormat::Rg8Unorm => wgpu::TextureFormat::Rg8Unorm,
        TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        TextureFormat::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureFormat::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
        TextureFormat::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
        TextureFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        TextureFormat::Rgba32Uint => wgpu::TextureFormat::Rgba32Uint,
        TextureFormat::R16g16b16a16Float => wgpu::TextureFormat::Rgba16Float,
        TextureFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
        TextureFormat::Depth24Stencil8 => wgpu::TextureFormat::Depth24PlusStencil8,
        TextureFormat::Abgr10Unorm => wgpu::TextureFormat::Bgra8UnormSrgb, // fallback panic!("Not supported in wgpu"),
    }
}

fn from_wgpu_format(format: wgpu::TextureFormat) -> TextureFormat {
    match format {
        wgpu::TextureFormat::R8Unorm => TextureFormat::R8Unorm,
        wgpu::TextureFormat::Rg8Unorm => TextureFormat::Rg8Unorm,
        wgpu::TextureFormat::Rgba8Unorm => TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb => TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm => TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb => TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba32Float => TextureFormat::Rgba32Float,
        wgpu::TextureFormat::Rgba32Uint => TextureFormat::Rgba32Uint,
        wgpu::TextureFormat::Rgba16Float => TextureFormat::R16g16b16a16Float,
        wgpu::TextureFormat::Depth32Float => TextureFormat::Depth32Float,
        wgpu::TextureFormat::Depth24PlusStencil8 => TextureFormat::Depth24Stencil8,
        _ => TextureFormat::Bgra8UnormSrgb, // fallback
    }
}

fn to_wgpu_address_mode(mode: AddressMode) -> wgpu::AddressMode {
    match mode {
        AddressMode::Repeat => wgpu::AddressMode::Repeat,
        AddressMode::Mirror => wgpu::AddressMode::MirrorRepeat,
        AddressMode::Clamp => wgpu::AddressMode::ClampToEdge,
    }
}

fn to_wgpu_filter(filter: FilterMode) -> wgpu::FilterMode {
    match filter {
        FilterMode::Nearest => wgpu::FilterMode::Nearest,
        FilterMode::Linear | FilterMode::Anisotropic => wgpu::FilterMode::Linear,
    }
}

fn to_wgpu_compare(func: CompareFunc) -> wgpu::CompareFunction {
    match func {
        CompareFunc::Never => wgpu::CompareFunction::Never,
        CompareFunc::Less => wgpu::CompareFunction::Less,
        CompareFunc::LessEqual => wgpu::CompareFunction::LessEqual,
        CompareFunc::Equal => wgpu::CompareFunction::Equal,
        CompareFunc::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
        CompareFunc::Greater => wgpu::CompareFunction::Greater,
        CompareFunc::Always => wgpu::CompareFunction::Always,
    }
}

fn to_wgpu_vertex_format(format: VertexFormat) -> wgpu::VertexFormat {
    match format {
        VertexFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
        VertexFormat::Float32x3 => wgpu::VertexFormat::Float32x3,
        VertexFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
    }
}

/// Convert a BindingType to a wgpu BindGroupLayoutEntry.
/// `group_idx` determines shader stage visibility (0,1=VERTEX; 2,3=FRAGMENT).
fn binding_type_to_layout_entry(
    binding: u32,
    bt: &BindingType,
    group_idx: usize,
) -> wgpu::BindGroupLayoutEntry {
    let visibility = match group_idx {
        0 | 1 => wgpu::ShaderStages::VERTEX,
        _ => wgpu::ShaderStages::FRAGMENT,
    };

    // UniformBuffers may be accessed from both stages in some configurations
    let visibility = match bt {
        BindingType::UniformBuffer => wgpu::ShaderStages::VERTEX_FRAGMENT,
        _ => visibility,
    };

    let ty = match bt {
        BindingType::UniformBuffer => wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        BindingType::Texture2D => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        BindingType::Texture2DUnfilterable => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        BindingType::Texture2DUint => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Uint,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        BindingType::TextureCube => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::Cube,
            multisampled: false,
        },
        BindingType::TextureDepth2D => wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        BindingType::Sampler => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        BindingType::SamplerComparison => {
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)
        }
    };

    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    }
}

/// Find the binding index of the Nth texture-type entry in a bind group descriptor.
/// Skips UniformBuffer and Sampler entries. Returns None if N is out of bounds.
fn nth_texture_binding_index(descriptors: &[BindingType], n: u32) -> Option<u32> {
    let mut count = 0u32;
    for (i, bt) in descriptors.iter().enumerate() {
        match bt {
            BindingType::Texture2D
            | BindingType::Texture2DUnfilterable
            | BindingType::Texture2DUint
            | BindingType::TextureCube
            | BindingType::TextureDepth2D => {
                if count == n {
                    return Some(i as u32);
                }
                count += 1;
            }
            _ => {}
        }
    }
    None
}

/// Find the binding index of the Nth UniformBuffer entry in a bind group descriptor.
fn nth_uniform_binding_index(descriptors: &[BindingType], n: u32) -> Option<u32> {
    let mut count = 0u32;
    for (i, bt) in descriptors.iter().enumerate() {
        if *bt == BindingType::UniformBuffer {
            if count == n {
                return Some(i as u32);
            }
            count += 1;
        }
    }
    None
}

// WgpuBackend initialization

impl WgpuBackend {
    pub fn init(window: Arc<winit::window::Window>) -> Result<Self, GpuError> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        let surface = instance.create_surface(window.clone()).map_err(|e| {
            GpuError::new(
                format!("Surface creation failed: {}", e),
                GpuErrorKind::DeviceCreation,
            )
        })?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| {
            GpuError::new(
                format!("No suitable GPU adapter found: {e}"),
                GpuErrorKind::DeviceCreation,
            )
        })?;

        println!("wgpu adapter: {:?}", adapter.get_info().name);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("sparkle-rs"),
            ..Default::default()
        }))
        .map_err(|e| {
            GpuError::new(
                format!("Device creation failed: {}", e),
                GpuErrorKind::DeviceCreation,
            )
        })?;

        // Configure surface
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // Create depth texture
        let (depth_target, _) = Self::create_depth_texture_inner(&device, width, height);

        // Create a dummy backbuffer proxy (view is replaced each frame in begin_frame)
        let dummy_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("backbuffer_dummy"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let dummy_view = dummy_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let backbuffer_target = WgpuRenderTarget::backbuffer_proxy(
            dummy_view,
            width,
            height,
            from_wgpu_format(surface_format),
        );

        println!(
            "wgpu backend initialized: {}x{}, format: {:?}",
            width, height, surface_format
        );

        Ok(WgpuBackend {
            device,
            queue,
            surface,
            surface_config,
            _window: window,
            backbuffer_target,
            depth_target,
            surface_texture: None,
            command_buffers: Vec::new(),
            pending_pass: None,
            current_pipeline_layouts: Vec::new(),
            current_pipeline_descriptors: Vec::new(),
            current_empty_bind_groups: Vec::new(),
            pending_bindings: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            width,
            height,
            egui_renderer: None,
        })
    }

    fn create_depth_texture_inner(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (WgpuRenderTarget, wgpu::TextureFormat) {
        let depth_format = wgpu::TextureFormat::Depth32Float;
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let target = WgpuRenderTarget::new(
            depth_texture,
            depth_view,
            None,
            width,
            height,
            TextureFormat::Depth32Float,
        );
        (target, depth_format)
    }

    //  Accessors for editor / egui integration

    /// Raw wgpu device handle (needed by egui-wgpu renderer).
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Raw wgpu queue handle (needed by egui-wgpu renderer).
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The surface texture format used for the backbuffer.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    /// The current frame's backbuffer texture view.
    /// Only valid between begin_frame() and present().
    pub fn backbuffer_view(&self) -> &wgpu::TextureView {
        &self.backbuffer_target.view
    }
}

// GpuBackend trait implementation

impl GpuBackend for WgpuBackend {
    type Texture = WgpuTexture;
    type RenderTarget = WgpuRenderTarget;
    type Buffer = WgpuBuffer;
    type Pipeline = WgpuPipeline;
    type ShaderSource = &'static [u8];

    fn set_material_properties(&mut self, _props: MaterialProperties) {
        unimplemented!()
    }

    fn load_shaders(&self) -> Shaders<Self> {
        // Load WGSL shaders
        let deferred_pre_wgsl = include_bytes!("../../shaders/wgsl/deferred_pre.wgsl");
        let ssao_wgsl = include_bytes!("../../shaders/wgsl/ssao.wgsl");
        let ssao_blur_wgsl = include_bytes!("../../shaders/wgsl/ssao_blur.wgsl");
        let shadow_wgsl = include_bytes!("../../shaders/wgsl/shadow.wgsl");
        let deferred_light_wgsl = include_bytes!("../../shaders/wgsl/deferred_light.wgsl");
        let forward_wgsl = include_bytes!("../../shaders/wgsl/forward.wgsl");
        let output_wgsl = include_bytes!("../../shaders/wgsl/output.wgsl");
        let skybox_wgsl = include_bytes!("../../shaders/wgsl/skybox.wgsl");

        Shaders {
            deferred_pre: deferred_pre_wgsl,
            ssao: ssao_wgsl,
            ssao_blur: ssao_blur_wgsl,
            shadow: shadow_wgsl,
            deferred_light: deferred_light_wgsl,
            forward: forward_wgsl,
            output: output_wgsl,
            skybox: skybox_wgsl,
        }
    }

    fn create_texture(&self, desc: &TextureDesc, data: &[u8]) -> Result<Self::Texture, GpuError> {
        let wgpu_format = to_wgpu_format(desc.format);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: desc.width,
                height: desc.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload pixel data
        let bytes_per_row = desc.width * wgpu_format.block_copy_size(None).unwrap_or(4);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(desc.height),
            },
            wgpu::Extent3d {
                width: desc.width,
                height: desc.height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let aniso = match desc.sampler.filter {
            FilterMode::Anisotropic => 16,
            _ => 1,
        };
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: to_wgpu_address_mode(desc.sampler.address_u),
            address_mode_v: to_wgpu_address_mode(desc.sampler.address_v),
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: to_wgpu_filter(desc.sampler.filter),
            min_filter: to_wgpu_filter(desc.sampler.filter),
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            compare: desc.sampler.compare.map(to_wgpu_compare),
            anisotropy_clamp: aniso,
            ..Default::default()
        });

        Ok(WgpuTexture::new(
            texture,
            view,
            sampler,
            desc.width,
            desc.height,
            desc.format,
        ))
    }

    fn create_cubemap(
        &self,
        faces: [&[u8]; 6],
        width: u32,
        height: u32,
        format: TextureFormat,
        sampler: &SamplerDesc,
    ) -> Result<Self::Texture, GpuError> {
        let wgpu_format = to_wgpu_format(format);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cubemap"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload each face to its array layer
        let bytes_per_pixel = wgpu_format.block_copy_size(None).unwrap_or(4);
        let bytes_per_row = width * bytes_per_pixel;
        for (i, face_data) in faces.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                face_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }

        // Create a cube view over the 6-layer 2D texture
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("cubemap_view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let aniso = match sampler.filter {
            FilterMode::Anisotropic => 16,
            _ => 1,
        };
        let wgpu_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cubemap_sampler"),
            address_mode_u: to_wgpu_address_mode(sampler.address_u),
            address_mode_v: to_wgpu_address_mode(sampler.address_v),
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: to_wgpu_filter(sampler.filter),
            min_filter: to_wgpu_filter(sampler.filter),
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            compare: sampler.compare.map(to_wgpu_compare),
            anisotropy_clamp: aniso,
            ..Default::default()
        });

        Ok(WgpuTexture::new(
            texture,
            view,
            wgpu_sampler,
            width,
            height,
            format,
        ))
    }

    fn create_buffer(
        &self,
        desc: &BufferDesc,
        data: Option<&[u8]>,
    ) -> Result<Self::Buffer, GpuError> {
        let usage = match desc.usage {
            BufferUsage::Vertex => wgpu::BufferUsages::VERTEX,
            BufferUsage::Index => wgpu::BufferUsages::INDEX,
            BufferUsage::Uniform => wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        };

        let buffer = if let Some(data) = data {
            wgpu::util::DeviceExt::create_buffer_init(
                &self.device,
                &wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: data,
                    usage,
                },
            )
        } else {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: desc.size as u64,
                usage,
                mapped_at_creation: false,
            })
        };

        Ok(WgpuBuffer::new(buffer, desc.size))
    }

    fn create_render_target(
        &self,
        desc: &RenderTargetDesc,
    ) -> Result<Self::RenderTarget, GpuError> {
        let wgpu_format = to_wgpu_format(desc.format);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: desc.width,
                height: desc.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: to_wgpu_address_mode(desc.sampler.address_u),
            address_mode_v: to_wgpu_address_mode(desc.sampler.address_v),
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: to_wgpu_filter(desc.sampler.filter),
            min_filter: to_wgpu_filter(desc.sampler.filter),
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            compare: desc.sampler.compare.map(to_wgpu_compare),
            ..Default::default()
        });

        Ok(WgpuRenderTarget::new(
            texture,
            view,
            Some(sampler),
            desc.width,
            desc.height,
            desc.format,
        ))
    }

    fn create_pipeline(
        &self,
        desc: &PipelineDesc<Self::ShaderSource>,
    ) -> Result<Self::Pipeline, GpuError> {
        // Parse WGSL source
        let wgsl_str = std::str::from_utf8(desc.shader_source).map_err(|e| {
            GpuError::new(
                format!("Shader source is not valid UTF-8: {}", e),
                GpuErrorKind::ShaderCompilation,
            )
        })?;

        let shader_module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(desc.label),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(wgsl_str)),
            });

        // Build bind group layouts (one per group, up to 4)
        let mut bind_group_layouts: Vec<Option<wgpu::BindGroupLayout>> = Vec::new();
        let mut bind_group_descriptors: Vec<Vec<BindingType>> = Vec::new();
        let mut empty_bind_groups: Vec<Option<wgpu::BindGroup>> = Vec::new();

        for group_idx in 0..4 {
            let bindings: &[BindingType] = if group_idx < desc.bind_groups.len() {
                desc.bind_groups[group_idx]
            } else {
                &[]
            };

            let entries: Vec<wgpu::BindGroupLayoutEntry> = bindings
                .iter()
                .enumerate()
                .map(|(i, bt)| binding_type_to_layout_entry(i as u32, bt, group_idx))
                .collect();

            let layout = self
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(&format!("{}_group{}", desc.label, group_idx)),
                    entries: &entries,
                });

            // Pre-create empty bind group for groups with no entries
            let empty_bg = if bindings.is_empty() {
                Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("{}_group{}_empty", desc.label, group_idx)),
                    layout: &layout,
                    entries: &[],
                }))
            } else {
                None
            };

            bind_group_layouts.push(Some(layout));
            bind_group_descriptors.push(bindings.to_vec());
            empty_bind_groups.push(empty_bg);
        }

        // Create pipeline layout
        let layout_refs: Vec<Option<&wgpu::BindGroupLayout>> =
            bind_group_layouts.iter().map(|x| x.as_ref()).collect();
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(desc.label),
                bind_group_layouts: &layout_refs,
                immediate_size: 0,
            });

        // Build vertex buffer layout
        let wgpu_attributes: Vec<wgpu::VertexAttribute>;
        let vertex_buffers: Vec<wgpu::VertexBufferLayout>;

        if let Some(ref vl) = desc.vertex_layout {
            wgpu_attributes = vl
                .attributes
                .iter()
                .map(|a| wgpu::VertexAttribute {
                    format: to_wgpu_vertex_format(a.format),
                    offset: a.offset as u64,
                    shader_location: a.shader_location,
                })
                .collect();
            vertex_buffers = vec![wgpu::VertexBufferLayout {
                array_stride: vl.stride as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu_attributes,
            }];
        } else {
            wgpu_attributes = Vec::new();
            vertex_buffers = Vec::new();
        }

        // Determine if we have a fragment stage
        let has_fragment = !desc.color_target_formats.is_empty();

        // Build color target states
        let blend_state = match desc.blend_mode {
            BlendMode::None => None,
            BlendMode::Additive => Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            BlendMode::Alpha => Some(wgpu::BlendState::ALPHA_BLENDING),
        };

        let color_targets: Vec<Option<wgpu::ColorTargetState>> = desc
            .color_target_formats
            .iter()
            .map(|fmt| {
                Some(wgpu::ColorTargetState {
                    format: to_wgpu_format(*fmt),
                    blend: blend_state,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            })
            .collect();

        // Depth/stencil state
        let depth_stencil = desc.depth_format.map(|fmt| wgpu::DepthStencilState {
            format: to_wgpu_format(fmt),
            depth_write_enabled: Some(desc.depth_write),
            depth_compare: Some(to_wgpu_compare(desc.depth_compare)),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });

        // Cull mode
        let cull_mode = match desc.cull_mode {
            CullMode::None => None,
            CullMode::Front => Some(wgpu::Face::Front),
            CullMode::Back => Some(wgpu::Face::Back),
        };

        // Fragment state
        let fragment_state = if has_fragment {
            Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &color_targets,
                compilation_options: Default::default(),
            })
        } else {
            None
        };

        // Create render pipeline
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(desc.label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader_module,
                    entry_point: Some("vs_main"),
                    buffers: &vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: fragment_state,
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                cache: None,
                multiview_mask: None,
            });

        Ok(WgpuPipeline {
            pipeline,
            bind_group_layouts,
            bind_group_descriptors,
            empty_bind_groups,
        })
    }

    //  Buffer operations

    fn update_buffer(&self, buffer: &Self::Buffer, data: &[u8]) {
        self.queue.write_buffer(&buffer.buffer, 0, data);
    }

    fn cmd_update_buffer(&mut self, buffer: &Self::Buffer, data: &[u8]) {
        self.queue.write_buffer(&buffer.buffer, 0, data);
    }

    //  Frame lifecycle

    fn begin_frame(&mut self) -> Result<(), GpuError> {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
                println!("Surface texture is suboptimal, but will try to use it anyway");
                surface_texture
            }
            reason => {
                return Err(GpuError::new(
                    format!("Failed to acquire surface texture: {reason:?}"),
                    GpuErrorKind::Present,
                ));
            }
        };
        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Update backbuffer proxy with this frame's surface view
        self.backbuffer_target.view = surface_view;

        self.surface_texture = Some(surface_texture);
        self.command_buffers.clear();

        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), GpuError> {
        if !self.command_buffers.is_empty() {
            self.queue.submit(self.command_buffers.drain(..));
        }
        Ok(())
    }

    fn present(&mut self) -> Result<(), GpuError> {
        if let Some(surface_texture) = self.surface_texture.take() {
            surface_texture.present();
        }
        Ok(())
    }

    fn render_egui(
        &mut self,
        textures_delta: &egui::TexturesDelta,
        clipped_primitives: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
    ) {
        let egui_renderer = self.egui_renderer.get_or_insert_with(|| {
            egui_wgpu::Renderer::new(
                &self.device,
                self.surface_config.format,
                egui_wgpu::RendererOptions::PREDICTABLE,
            )
        });

        for (id, image_delta) in &textures_delta.set {
            egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
        }

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.width, self.height],
            pixels_per_point,
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui_encoder"),
            });

        egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            clipped_primitives,
            &screen_descriptor,
        );

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.backbuffer_target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            egui_renderer.render(
                &mut render_pass.forget_lifetime(),
                clipped_primitives,
                &screen_descriptor,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        for id in &textures_delta.free {
            egui_renderer.free_texture(id);
        }
    }

    //  Render pass management

    fn begin_render_pass(&mut self, desc: &RenderPassDesc<Self>) {
        let color_attachments: Vec<PendingColorAttachment> = desc
            .color_targets
            .iter()
            .map(|att| {
                let load = match att.load_op {
                    LoadOp::Clear => wgpu::LoadOp::Clear(wgpu::Color {
                        r: att.clear_color[0] as f64,
                        g: att.clear_color[1] as f64,
                        b: att.clear_color[2] as f64,
                        a: att.clear_color[3] as f64,
                    }),
                    LoadOp::Load => wgpu::LoadOp::Load,
                };
                PendingColorAttachment {
                    view: att.target.view.clone(),
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                }
            })
            .collect();

        let depth_attachment = desc.depth_target.as_ref().map(|d| {
            let load = match d.load_op {
                LoadOp::Clear => wgpu::LoadOp::Clear(d.clear_depth),
                LoadOp::Load => wgpu::LoadOp::Load,
            };
            let store = if d.write_enabled {
                wgpu::StoreOp::Store
            } else {
                wgpu::StoreOp::Discard
            };
            PendingDepthAttachment {
                view: d.target.view.clone(),
                depth_ops: wgpu::Operations { load, store },
            }
        });

        self.pending_pass = Some(PendingPass {
            label: desc.label.to_string(),
            color_attachments,
            depth_attachment,
            commands: Vec::new(),
        });
    }

    fn end_render_pass(&mut self) {
        if let Some(pass) = self.pending_pass.take() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some(&pass.label),
                });

            {
                let color_attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = pass
                    .color_attachments
                    .iter()
                    .map(|att| {
                        Some(wgpu::RenderPassColorAttachment {
                            view: &att.view,
                            resolve_target: None,
                            ops: att.ops,
                            depth_slice: None,
                        })
                    })
                    .collect();

                let depth_attachment = pass.depth_attachment.as_ref().map(|d| {
                    wgpu::RenderPassDepthStencilAttachment {
                        view: &d.view,
                        depth_ops: Some(d.depth_ops),
                        stencil_ops: None,
                    }
                });

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&pass.label),
                    color_attachments: &color_attachments,
                    depth_stencil_attachment: depth_attachment,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                // Replay deferred commands
                for cmd in &pass.commands {
                    match cmd {
                        RenderCommand::SetPipeline(p) => {
                            render_pass.set_pipeline(p);
                        }
                        RenderCommand::SetViewport {
                            x,
                            y,
                            w,
                            h,
                            min_depth,
                            max_depth,
                        } => {
                            render_pass.set_viewport(*x, *y, *w, *h, *min_depth, *max_depth);
                        }
                        RenderCommand::SetVertexBuffer(buf) => {
                            render_pass.set_vertex_buffer(0, buf.slice(..));
                        }
                        RenderCommand::SetIndexBuffer(buf) => {
                            render_pass.set_index_buffer(buf.slice(..), wgpu::IndexFormat::Uint32);
                        }
                        RenderCommand::BindGroup(index, bg) => {
                            render_pass.set_bind_group(*index, bg, &[]);
                        }
                        RenderCommand::DrawIndexed {
                            index_count,
                            first_index,
                            base_vertex,
                        } => {
                            render_pass.draw_indexed(
                                *first_index..(*first_index + *index_count),
                                *base_vertex,
                                0..1,
                            );
                        }
                    }
                }
            } // render_pass dropped here, releasing encoder borrow

            self.command_buffers.push(encoder.finish());

            // Submit immediately so that any preceding queue.write_buffer()
            // calls take effect for THIS pass before the next write_buffer()
            // overwrites the same uniform buffer with a different light's data.
            self.queue.submit(self.command_buffers.drain(..));
        }
    }

    //  Draw commands (recorded into pending pass)

    fn set_pipeline(&mut self, pipeline: &Self::Pipeline) {
        if let Some(ref mut pass) = self.pending_pass {
            pass.commands
                .push(RenderCommand::SetPipeline(pipeline.pipeline.clone()));
        }

        // Store active pipeline metadata for binding accumulation
        self.current_pipeline_layouts = pipeline.bind_group_layouts.clone();
        self.current_pipeline_descriptors = pipeline.bind_group_descriptors.clone();
        self.current_empty_bind_groups = pipeline
            .empty_bind_groups
            .iter()
            .map(|bg| bg.clone())
            .collect();

        // Reset pending bindings to match the new pipeline's layout
        for group_idx in 0..4 {
            let num_bindings = if group_idx < self.current_pipeline_descriptors.len() {
                self.current_pipeline_descriptors[group_idx].len()
            } else {
                0
            };
            self.pending_bindings[group_idx].clear();
            self.pending_bindings[group_idx].resize_with(num_bindings, || None);
        }
    }

    fn set_viewport(&mut self, viewport: &ViewportDesc) {
        if let Some(ref mut pass) = self.pending_pass {
            pass.commands.push(RenderCommand::SetViewport {
                x: viewport.x,
                y: viewport.y,
                w: viewport.width,
                h: viewport.height,
                min_depth: viewport.min_depth,
                max_depth: viewport.max_depth,
            });
        }
    }

    fn bind_texture(&mut self, slot: u32, texture: &Self::Texture) {
        // Material textures go into group 2.
        // slot N maps to the Nth texture entry in group 2's descriptor.
        if self.current_pipeline_descriptors.len() <= 2 {
            return;
        }
        let descriptors = &self.current_pipeline_descriptors[2];
        if let Some(binding_idx) = nth_texture_binding_index(descriptors, slot) {
            let idx = binding_idx as usize;
            if idx < self.pending_bindings[2].len() {
                self.pending_bindings[2][idx] =
                    Some(PendingResource::TextureView(texture.view.clone()));
            }
            // Check if next binding is a Sampler
            let next = idx + 1;
            if next < descriptors.len() {
                match descriptors[next] {
                    BindingType::Sampler | BindingType::SamplerComparison => {
                        if next < self.pending_bindings[2].len() {
                            self.pending_bindings[2][next] =
                                Some(PendingResource::Sampler(texture.sampler.clone()));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn bind_render_target_as_texture(&mut self, slot: u32, target: &Self::RenderTarget) {
        // Render targets bound as textures go into group 3.
        // slot N maps to the Nth texture entry in group 3's descriptor.
        if self.current_pipeline_descriptors.len() <= 3 {
            return;
        }
        let descriptors = &self.current_pipeline_descriptors[3];
        if let Some(binding_idx) = nth_texture_binding_index(descriptors, slot) {
            let idx = binding_idx as usize;
            if idx < self.pending_bindings[3].len() {
                self.pending_bindings[3][idx] =
                    Some(PendingResource::TextureView(target.view.clone()));
            }
            // Check if next binding is a Sampler/SamplerComparison
            let next = idx + 1;
            if next < descriptors.len() {
                match descriptors[next] {
                    BindingType::Sampler | BindingType::SamplerComparison => {
                        if next < self.pending_bindings[3].len() {
                            if let Some(ref sampler) = target.sampler {
                                self.pending_bindings[3][next] =
                                    Some(PendingResource::Sampler(sampler.clone()));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn bind_uniform(&mut self, stage: ShaderStage, slot: u32, buffer: &Self::Buffer) {
        // Mapping convention:
        //   Vertex, slot 0 → Group 0, binding 0 (per-frame)
        //   Vertex, slot 1 → Group 1, binding 0 (per-object)
        //   Fragment, slot N → Group 3, Nth UniformBuffer entry
        match stage {
            ShaderStage::Vertex => {
                let group_idx = slot as usize; // slot 0 → group 0, slot 1 → group 1
                if group_idx < 2 && !self.pending_bindings[group_idx].is_empty() {
                    // Vertex uniforms always go to binding 0 within their group
                    self.pending_bindings[group_idx][0] =
                        Some(PendingResource::Buffer(buffer.buffer.clone()));
                }
            }
            ShaderStage::Fragment => {
                // Fragment uniforms go into group 3
                if self.current_pipeline_descriptors.len() > 3 {
                    let descriptors = &self.current_pipeline_descriptors[3];
                    if let Some(binding_idx) = nth_uniform_binding_index(descriptors, slot) {
                        let idx = binding_idx as usize;
                        if idx < self.pending_bindings[3].len() {
                            self.pending_bindings[3][idx] =
                                Some(PendingResource::Buffer(buffer.buffer.clone()));
                        }
                    }
                }
            }
        }
    }

    fn bind_ubo_to_descriptor(&self, _binding: u32, _buffer: &Self::Buffer) {
        // No-op: wgpu uses bind groups, not bindless descriptors.
    }

    fn set_vertex_buffer(&mut self, buffer: &Self::Buffer) {
        if let Some(ref mut pass) = self.pending_pass {
            pass.commands
                .push(RenderCommand::SetVertexBuffer(buffer.buffer.clone()));
        }
    }

    fn set_index_buffer(&mut self, buffer: &Self::Buffer) {
        if let Some(ref mut pass) = self.pending_pass {
            pass.commands
                .push(RenderCommand::SetIndexBuffer(buffer.buffer.clone()));
        }
    }

    fn draw_indexed(&mut self, index_count: u32, first_index: u32, base_vertex: i32) {
        if let Some(ref mut pass) = self.pending_pass {
            // Create and bind groups from accumulated state
            for group_idx in 0..self.current_pipeline_layouts.len() {
                let descriptors = &self.current_pipeline_descriptors[group_idx];

                // For empty groups, use the pre-created empty bind group
                if descriptors.is_empty() {
                    if let Some(ref bg) = self.current_empty_bind_groups[group_idx] {
                        pass.commands
                            .push(RenderCommand::BindGroup(group_idx as u32, bg.clone()));
                    }
                    continue;
                }

                // Build bind group entries from pending bindings
                let bindings = &self.pending_bindings[group_idx];
                let mut entries: Vec<wgpu::BindGroupEntry> = Vec::new();
                let mut complete = true;

                for (binding_idx, pending) in bindings.iter().enumerate() {
                    match pending {
                        Some(PendingResource::Buffer(buf)) => {
                            entries.push(wgpu::BindGroupEntry {
                                binding: binding_idx as u32,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: buf,
                                    offset: 0,
                                    size: None,
                                }),
                            });
                        }
                        Some(PendingResource::TextureView(view)) => {
                            entries.push(wgpu::BindGroupEntry {
                                binding: binding_idx as u32,
                                resource: wgpu::BindingResource::TextureView(view),
                            });
                        }
                        Some(PendingResource::Sampler(sampler)) => {
                            entries.push(wgpu::BindGroupEntry {
                                binding: binding_idx as u32,
                                resource: wgpu::BindingResource::Sampler(sampler),
                            });
                        }
                        None => {
                            // Missing binding — skip this draw call
                            complete = false;
                            break;
                        }
                    }
                }

                if !complete {
                    // Incomplete bindings for this group — skip the draw
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "wgpu: incomplete bindings for group {}, skipping draw",
                        group_idx
                    );
                    return;
                }

                if let Some(ref layout) = self.current_pipeline_layouts[group_idx] {
                    let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: None,
                        layout,
                        entries: &entries,
                    });

                    pass.commands
                        .push(RenderCommand::BindGroup(group_idx as u32, bind_group));
                }
            }

            pass.commands.push(RenderCommand::DrawIndexed {
                index_count,
                first_index,
                base_vertex,
            });
        }
    }

    fn set_model_matrix(&mut self, _model: &glm::Mat4) {
        // wgpu backend still uses bind_uniform for model matrix
    }

    //  Accessors

    fn backbuffer(&self) -> Self::RenderTarget {
        self.backbuffer_target.clone()
    }

    fn main_depth_target(&self) -> Self::RenderTarget {
        self.depth_target.clone()
    }

    fn default_viewport(&self) -> ViewportDesc {
        ViewportDesc {
            x: 0.0,
            y: 0.0,
            width: self.width as f32,
            height: self.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        let (depth_target, _) = Self::create_depth_texture_inner(&self.device, width, height);
        self.depth_target = depth_target;

        self.backbuffer_target.width = width;
        self.backbuffer_target.height = height;
    }

    fn wait_idle(&self) -> Result<(), GpuError> {
        Ok(())
    }
}
