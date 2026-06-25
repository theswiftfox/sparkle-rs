//! Backend abstraction layer for sparkle-rs.
//!
//! Defines platform- and API-agnostic traits and types for GPU rendering.
//! Backend implementations implement the [`GpuBackend`] trait
//! and its associated resource types.

use super::geometry::{AABB, Vertex};

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

// Error types

/// Categories of GPU errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuErrorKind {
    DeviceCreation,
    ResourceCreation,
    ResourceUpdate,
    ShaderCompilation,
    RenderPass,
    Present,
    Other,
}

/// A GPU error with a human-readable message and category.
#[derive(Debug, Clone)]
pub struct GpuError {
    pub message: String,
    pub kind: GpuErrorKind,
}

impl GpuError {
    pub fn new(message: impl Into<String>, kind: GpuErrorKind) -> Self {
        GpuError {
            message: message.into(),
            kind,
        }
    }
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[GPU {:?}] {}", self.kind, self.message)
    }
}

impl std::error::Error for GpuError {
    fn description(&self) -> &str {
        &self.message
    }
}

// Format and mode enums

/// Texture and render target pixel formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureFormat {
    // Color formats
    R8Unorm,
    Rg8Unorm,
    Rgba8Unorm,
    Rgba8UnormSrgb,
    Bgra8Unorm,
    Bgra8UnormSrgb,
    Rgba32Float,
    Rgba32Uint,
    R16g16b16a16Float,

    // hdr format
    Abgr10Unorm,

    // Depth formats
    Depth32Float,
    Depth24Stencil8,
}

/// Vertex attribute data types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
}

/// Blend mode for a render pipeline's color target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// No blending; output overwrites destination.
    None,
    /// Additive: src * 1 + dst * 1.
    Additive,
    /// Alpha: src * src_alpha + dst * (1 - src_alpha).
    Alpha,
}

/// Triangle face culling mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CullMode {
    None,
    Front,
    Back,
}

/// Depth/stencil comparison function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareFunc {
    Never,
    Less,
    LessEqual,
    Equal,
    GreaterEqual,
    Greater,
    Always,
}

/// GPU buffer usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferUsage {
    Vertex,
    Index,
    Uniform,
    Storage,
    Indirect,
}

/// Shader stage for uniform binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
}

/// Texture address (wrap) mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    Repeat,
    Mirror,
    Clamp,
}

/// Texture filter mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Nearest,
    Linear,
    Anisotropic,
}

/// Object transparency classification. Used to control draw order.
#[derive(Debug, Clone, Copy)]
pub enum ObjType {
    Opaque,
    Transparent,
    /// Matches any object type in draw filter comparisons. Never assign to an object.
    Any,
}

impl PartialEq for ObjType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (_, ObjType::Any) | (ObjType::Any, _) => true,
            (ObjType::Opaque, ObjType::Opaque) => true,
            (ObjType::Transparent, ObjType::Transparent) => true,
            _ => false,
        }
    }
}

// Descriptor structs

/// Viewport rectangle and depth range.
#[derive(Debug, Clone, Copy)]
pub struct ViewportDesc {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

/// Texture sampler configuration.
#[derive(Debug, Clone, Copy)]
pub struct SamplerDesc {
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub filter: FilterMode,
    /// If set, creates a comparison sampler (used for shadow map PCF).
    pub compare: Option<CompareFunc>,
}

impl Default for SamplerDesc {
    fn default() -> Self {
        SamplerDesc {
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
            filter: FilterMode::Linear,
            compare: None,
        }
    }
}

/// Description for creating a 2D texture.
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub sampler: SamplerDesc,
    pub generate_mipmaps: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTargetUsage {
    Color,   // color attachment + sampled
    Depth,   // depth/stencil attachment + sampled
    Storage, // storage + transfer_src + sampled (RT output)
}

/// Description for creating a render target (usable as both render attachment and shader input).
pub struct RenderTargetDesc {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub sampler: SamplerDesc,
    pub usage: RenderTargetUsage,
}

/// Description for creating a GPU buffer.
pub struct BufferDesc {
    pub label: String,
    pub usage: BufferUsage,
    pub size: usize,
}

/// A single vertex attribute within a vertex layout.
#[derive(Debug, Clone)]
pub struct VertexAttribute {
    pub format: VertexFormat,
    pub offset: u32,
    pub shader_location: u32,
}

/// Describes the layout of vertex data in a vertex buffer.
#[derive(Debug, Clone)]
pub struct VertexLayout {
    pub stride: u32,
    pub attributes: Vec<VertexAttribute>,
}

/// Returns the standard vertex layout for this engine's [`Vertex`] struct.
pub fn standard_vertex_layout() -> VertexLayout {
    VertexLayout {
        stride: std::mem::size_of::<Vertex>() as u32,
        attributes: vec![
            VertexAttribute {
                format: VertexFormat::Float32x3, // position
                offset: 0,
                shader_location: 0,
            },
            VertexAttribute {
                format: VertexFormat::Float32x3, // normal
                offset: 12,
                shader_location: 1,
            },
            VertexAttribute {
                format: VertexFormat::Float32x3, // tangent
                offset: 24,
                shader_location: 2,
            },
            VertexAttribute {
                format: VertexFormat::Float32x3, // bitangent
                offset: 36,
                shader_location: 3,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2, // texcoord
                offset: 48,
                shader_location: 4,
            },
        ],
    }
}

/// Describes a single binding within a bind group layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingType {
    /// A uniform buffer.
    UniformBuffer,
    /// A storage buffer (SSBO).
    StorageBuffer,
    /// A 2D float texture (filterable).
    Texture2D,
    /// A 2D float texture (unfilterable, e.g. Rgba32Float G-buffer).
    Texture2DUnfilterable,
    /// A 2D unsigned integer texture (e.g., G-buffer packed data).
    Texture2DUint,
    /// A cubemap texture.
    TextureCube,
    /// A 2D depth texture (for shadow map sampling).
    TextureDepth2D,
    /// A regular (filtering) sampler.
    Sampler,
    /// A comparison sampler (used for PCF shadow mapping).
    SamplerComparison,
}

/// Description for creating a render pipeline (shaders + fixed-function state).
///
/// ## Bind group convention
///
/// The `bind_groups` field describes the layout of each bind group:
/// - **Group 0**: Per-frame vertex uniforms (view/proj or light-space matrix)
/// - **Group 1**: Per-object vertex uniforms (model matrix)
/// - **Group 2**: Material textures (diffuse, metallic-roughness, normal, or cubemap)
/// - **Group 3**: Per-pass fragment uniforms + pass-specific textures
///
/// Passes that don't use all groups should provide empty slices `&[]` for unused groups.
///
/// If `color_target_formats` is empty, no fragment stage is created (depth-only pass).
pub struct RenderPipelineDesc<'a, ShaderSource> {
    pub label: &'a str,
    pub shader_source: &'a ShaderSource,
    /// `None` for fullscreen / procedurally-generated-vertex shaders.
    pub vertex_layout: Option<VertexLayout>,
    pub blend_mode: BlendMode,
    pub cull_mode: CullMode,
    pub depth_write: bool,
    pub depth_compare: CompareFunc,
    pub color_target_formats: &'a [TextureFormat],
    pub depth_format: Option<TextureFormat>,
}

/// Description for creating a compute pipeline.
pub struct ComputePipelineDesc<'a, ShaderSource> {
    pub label: &'a str,
    pub shader_source: &'a ShaderSource,
    /// Value injected as specialization constant 0 (world dimension for scatter shader).
    pub world_dimension: Option<f32>,
}

/// Load operation for a render pass attachment.
#[derive(Debug, Clone, Copy)]
pub enum LoadOp {
    /// Clear the attachment to a specific value.
    Clear,
    /// Preserve existing contents.
    Load,
}

/// A color attachment within a render pass.
pub struct ColorAttachment<'a, B: GpuBackend> {
    pub target: &'a B::RenderTarget,
    pub load_op: LoadOp,
    pub clear_color: [f32; 4],
}

/// A depth/stencil attachment within a render pass.
pub struct DepthAttachment<'a, B: GpuBackend> {
    pub target: &'a B::RenderTarget,
    pub load_op: LoadOp,
    pub clear_depth: f32,
    pub write_enabled: bool,
}

/// Description for beginning a render pass.
pub struct RenderPassDesc<'a, B: GpuBackend> {
    pub label: &'a str,
    pub color_targets: Vec<ColorAttachment<'a, B>>,
    pub depth_target: Option<DepthAttachment<'a, B>>,
}

// Resource traits

/// A GPU texture that can be sampled in shaders.
pub trait GpuTexture: Sized + Clone {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> TextureFormat;
    fn id(&self) -> usize;
}

/// A GPU texture that can be used as both a render target output and a shader input.
pub trait GpuRenderTarget: GpuTexture {}

/// A GPU buffer (vertex, index, or uniform).
pub trait GpuBuffer: Sized {
    fn size(&self) -> usize;
}

// GpuBackend trait

/// The main backend abstraction trait.
///
/// Implementations provide all GPU resource creation, command recording, and
/// frame lifecycle management. The engine is generic over this trait, allowing
/// different graphics API backends to be swapped in.
///
/// # Usage pattern
///
/// ```ignore
/// backend.begin_frame()?;
///
/// backend.begin_render_pass(&desc);
/// backend.set_pipeline(&pipeline);
/// backend.set_viewport(&viewport);
/// backend.bind_texture(0, &texture);
/// backend.draw_indexed(count, 0, 0);
/// backend.end_render_pass();
///
/// // ... more passes ...
///
/// backend.end_frame()?;
/// backend.present()?;
/// ```
pub trait GpuBackend: Sized + 'static {
    type Texture: GpuTexture;
    type RenderTarget: GpuRenderTarget;
    type Buffer: GpuBuffer;
    type Pipeline;
    type ShaderSource;
    type AccelerationStructure;

    /// load shaders
    fn load_shaders(&self) -> Shaders<Self>;

    /// load shaders for procedural generation
    fn load_proc_gen_shaders(&self) -> ProceduralShaders<Self>;

    /// Create a 2D texture from raw pixel data.
    fn create_texture(&self, desc: &TextureDesc, data: &[u8]) -> Result<Self::Texture, GpuError>;

    /// Create a cubemap texture from 6 face images (in +X, -X, +Y, -Y, +Z, -Z order).
    fn create_cubemap(
        &self,
        faces: [&[u8]; 6],
        width: u32,
        height: u32,
        format: TextureFormat,
        sampler: &SamplerDesc,
    ) -> Result<Self::Texture, GpuError>;

    /// Create a GPU buffer, optionally initialized with data.
    fn create_buffer(
        &self,
        desc: &BufferDesc,
        data: Option<&[u8]>,
    ) -> Result<Self::Buffer, GpuError>;

    /// Create a render target that can be rendered to and later sampled.
    fn create_render_target(&self, desc: &RenderTargetDesc)
    -> Result<Self::RenderTarget, GpuError>;

    /// Create a render pipeline from compiled shader bytecode and fixed-function state.
    fn create_render_pipeline(
        &self,
        desc: &RenderPipelineDesc<Self::ShaderSource>,
    ) -> Result<Self::Pipeline, GpuError>;

    /// Create a compute pipeline.
    fn create_compute_pipeline(
        &self,
        desc: &ComputePipelineDesc<Self::ShaderSource>,
    ) -> Result<Self::Pipeline, GpuError>;

    /// Execute a compute pipeline in a one-shot command submission with host synchronization and a pipeline memory barrier.
    fn execute_compute_one_shot(
        &self,
        pipeline: &Self::Pipeline,
        buffers: &[(u32, &Self::Buffer)],
        textures: &[(u32, &Self::Texture)],
        work_groups: (u32, u32, u32),
        max_instances: u32,
        asset_offset: u32,
        max_height: f32,
        spawn_height_min: f32,
        spawn_height_max: f32,
        slope_max: f32,
        scale_min: f32,
        scale_max: f32,
        tilt_factor: f32,
        terrain_segments_f: f32,
    ) -> Result<(), GpuError>;

    //  Buffer operations

    /// Upload new data to a uniform/dynamic buffer (CPU memcpy, immediate).
    fn update_buffer(&self, buffer: &Self::Buffer, data: &[u8]);

    /// Record a buffer update into the current command buffer.
    /// Data is baked into the command stream so each pass sees correct values
    /// even when the same buffer is updated multiple times per frame.
    /// Includes a pipeline barrier (transfer write -> uniform read).
    /// Must be called outside a render pass.
    fn cmd_update_buffer(&mut self, buffer: &Self::Buffer, data: &[u8]);

    //  Frame lifecycle

    /// Begin a new frame. Must be called before any render passes.
    fn begin_frame(&mut self) -> Result<(), GpuError>;

    /// Finish recording and submit all commands for the current frame.
    fn end_frame(&mut self) -> Result<(), GpuError>;

    /// Present the rendered frame to the display.
    fn present(&mut self) -> Result<(), GpuError>;

    //  Render pass management

    /// Begin a render pass with the specified attachments and load operations.
    fn begin_render_pass(&mut self, desc: &RenderPassDesc<Self>);

    /// End the current render pass.
    fn end_render_pass(&mut self);

    //  Draw commands (valid within a render pass)

    /// Bind a render pipeline (shaders + state) for subsequent draw calls.
    fn set_pipeline(&mut self, pipeline: &Self::Pipeline);

    /// Set the viewport rectangle and depth range.
    fn set_viewport(&mut self, viewport: &ViewportDesc);

    /// Bind a texture to a shader slot.
    fn bind_texture(&mut self, slot: u32, texture: &Self::Texture);

    /// Bind a render target as a texture input to a shader slot.
    fn bind_render_target_as_texture(&mut self, slot: u32, target: &Self::RenderTarget);

    /// Bind a uniform buffer to a shader stage and slot.
    fn bind_uniform(&mut self, stage: ShaderStage, slot: u32, buffer: &Self::Buffer);

    /// Permanently bind a UBO to a descriptor set binding slot.
    fn bind_buffer_to_descriptor(&self, binding: u32, buffer: &Self::Buffer);

    /// Set the vertex buffer for subsequent draw calls.
    fn set_vertex_buffer(&mut self, buffer: &Self::Buffer);

    /// Set the index buffer for subsequent draw calls.
    fn set_index_buffer(&mut self, buffer: &Self::Buffer);

    /// Issue an indexed draw call.
    fn draw_indexed(&mut self, index_count: u32, first_index: u32, base_vertex: i32);

    // Issue an indirect indexed draw call.
    fn draw_indexed_indirect(
        &mut self,
        indirect_commands_buffer: &Self::Buffer,
        offset: u64,
        draw_count: u32,
    );

    /// Set the per-draw model matrix
    fn set_model_matrix(&mut self, model: &glm::Mat4);

    fn set_material_properties(&mut self, props: MaterialProperties);

    //  Accessors

    /// Get the current frame's backbuffer render target.
    fn backbuffer(&self) -> Self::RenderTarget;

    /// Get the main depth buffer render target.
    fn main_depth_target(&self) -> Self::RenderTarget;

    /// Get the default full-window viewport.
    fn default_viewport(&self) -> ViewportDesc;

    /// Get the current framebuffer resolution (width, height).
    fn resolution(&self) -> (u32, u32);

    /// Handle a window resize by reconfiguring the surface and recreating
    /// resolution-dependent resources (depth buffer, etc.).
    fn resize(&mut self, width: u32, height: u32);

    /// wait for GPU to be idle
    fn wait_idle(&self) -> Result<(), GpuError>;

    /// Render egui overlay on top of the scene.
    ///
    /// Called by the editor after the scene has been rendered.
    /// The default implementation is a no-op (no overlay rendered).
    fn render_egui(
        &mut self,
        _textures_delta: &egui::TexturesDelta,
        _clipped_primitives: &[egui::ClippedPrimitive],
        _pixels_per_point: f32,
    ) {
    }

    //  Debug markers (default no-op implementations)

    /// Begin a named debug event region (e.g., for GPU profilers).
    fn begin_event(&self, _name: &str) {}

    /// End the current debug event region.
    fn end_event(&self) {}

    // raytracing
    fn has_rt_support(&self) -> bool;

    fn create_blas(
        &self,
        ty: AccelerationStructureType,
        render_items: &[RenderItem<'_, Self>],
    ) -> Result<Vec<Self::AccelerationStructure>, GpuError>;

    /// Build a top-level acceleration structure from a set of BLAS instances.
    ///
    /// `blas` and `transforms` must have the same length.
    /// Each entry pairs a BLAS with its world-space transform matrix.
    fn create_tlas(
        &self,
        blas: &[Self::AccelerationStructure],
        transforms: &[glm::Mat4],
    ) -> Result<Self::AccelerationStructure, GpuError>;

    /// create the raytracing pipeline
    fn create_rt_pipeline(&self, shaders: &RtShaders<Self>) -> Result<Self::Pipeline, GpuError>;

    /// create a render target for the RT output
    fn create_rt_output_target(
        &self,
        width: u32,
        height: u32,
    ) -> Result<Self::RenderTarget, GpuError>;

    /// call the RT pipeline on the given tlas
    fn dispatch_rays(
        &mut self,
        pipeline: &Self::Pipeline,
        tlas: &Self::AccelerationStructure,
        output: &Self::RenderTarget,
        light_buffer: &Self::Buffer,
        width: u32,
        height: u32,
        number_of_lights: u32,
    );

    /// load compiled RT shader sources (raygen, miss, closest_hit)
    fn load_rt_shaders(&self) -> RtShaders<Self>;
}

pub struct Shaders<B: GpuBackend> {
    pub deferred_pre: B::ShaderSource,
    pub shadow: B::ShaderSource,
    pub deferred_light: B::ShaderSource,
    pub forward: B::ShaderSource,
    pub output: B::ShaderSource,
    pub skybox: B::ShaderSource,
}

pub struct ProceduralShaders<B: GpuBackend> {
    pub scattering: B::ShaderSource,
}

#[derive(Clone, Copy)]
pub enum AccelerationStructureType {
    Blas,
    Tlas,
}

pub struct RtShaders<B: GpuBackend> {
    pub raygen: B::ShaderSource,
    pub miss: B::ShaderSource,
    pub miss_shadow: B::ShaderSource,
    pub closest_hit: B::ShaderSource,
}

// Material

pub struct MaterialProperties {
    pub has_parallax: bool,
}

static MATERIAL_ID: AtomicUsize = AtomicUsize::new(0);

/// A collection of textures bound to shader slots, representing a surface material.
pub struct Material<B: GpuBackend> {
    textures: HashMap<u32, Rc<B::Texture>>,
    has_parallax: bool,
    id: usize,
}

impl<B: GpuBackend> Clone for Material<B> {
    fn clone(&self) -> Self {
        Self {
            textures: self.textures.clone(),
            has_parallax: self.has_parallax.clone(),
            id: self.id.clone(),
        }
    }
}

impl<B: GpuBackend> Material<B> {
    pub fn new() -> Self {
        Material {
            textures: HashMap::new(),
            has_parallax: false,
            id: MATERIAL_ID.fetch_add(1, Ordering::SeqCst),
        }
    }

    /// Bind all material textures to their respective shader slots.
    pub fn bind(&self, backend: &mut B) {
        for (slot, tex) in &self.textures {
            backend.bind_texture(*slot, tex);
            backend.set_material_properties(MaterialProperties {
                has_parallax: self.has_parallax,
            });
        }
    }

    /// Add or replace a texture at the given slot.
    pub fn add_texture(&mut self, slot: u32, tex: Rc<B::Texture>) {
        self.textures.insert(slot, tex);
        // Regenerate id when material changes so sorting picks up the difference.
        self.id = MATERIAL_ID.fetch_add(1, Ordering::SeqCst);
    }

    pub fn set_parallax(&mut self, parallax: bool) {
        self.has_parallax = parallax;
    }
}

impl<B: GpuBackend> PartialEq for Material<B> {
    fn eq(&self, other: &Material<B>) -> bool {
        if self.textures.len() != other.textures.len() {
            return false;
        }
        for (slot, tex) in &self.textures {
            match other.textures.get(slot) {
                Some(o_tex) if tex.id() == o_tex.id() => continue,
                _ => return false,
            }
        }
        true
    }
}

// Drawable

static DRAWABLE_ID: AtomicUsize = AtomicUsize::new(0);

/// A renderable mesh: vertex/index buffers, a per-object uniform buffer (model matrix),
/// a material, and a transparency classification.
pub struct Drawable<B: GpuBackend> {
    id: usize,
    pub(crate) vertex_buffer: Rc<B::Buffer>,
    pub(crate) vertex_count: u32,
    pub(crate) index_buffer: Rc<B::Buffer>,
    pub(crate) index_count: u32,
    pub(crate) model_buffer: Rc<B::Buffer>,
    model_matrix: glm::Mat4,
    material: Material<B>,
    object_type: ObjType,
    double_sided: bool,
    /// Local-space axis-aligned bounding box computed from vertex positions.
    aabb: AABB,
}

impl<B: GpuBackend> Clone for Drawable<B> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            vertex_buffer: self.vertex_buffer.clone(),
            vertex_count: self.vertex_count,
            index_buffer: self.index_buffer.clone(),
            index_count: self.index_count.clone(),
            model_buffer: self.model_buffer.clone(),
            model_matrix: self.model_matrix.clone(),
            material: self.material.clone(),
            object_type: self.object_type.clone(),
            double_sided: self.double_sided.clone(),
            aabb: self.aabb.clone(),
        }
    }
}

impl<B: GpuBackend> Drawable<B> {
    /// Create a drawable from vertex and index data.
    pub fn from_verts(
        backend: &B,
        vertices: &[Vertex],
        indices: &[u32],
        object_type: ObjType,
    ) -> Result<Drawable<B>, GpuError> {
        let vertex_data = as_bytes(vertices);
        let index_data = as_bytes(indices);

        let vertex_buffer = backend.create_buffer(
            &BufferDesc {
                label: "Drawable Vertex Buffer".into(),
                usage: BufferUsage::Vertex,
                size: vertex_data.len(),
            },
            Some(vertex_data),
        )?;

        let index_buffer = backend.create_buffer(
            &BufferDesc {
                label: "Drawable Index Buffer".into(),
                usage: BufferUsage::Index,
                size: index_data.len(),
            },
            Some(index_data),
        )?;

        let identity: glm::Mat4 = glm::identity();
        let model_data = as_bytes(std::slice::from_ref(&identity));
        let model_buffer = backend.create_buffer(
            &BufferDesc {
                label: "Drawable Model Uniform".into(),
                usage: BufferUsage::Uniform,
                size: model_data.len(),
            },
            Some(model_data),
        )?;

        Ok(Drawable {
            id: DRAWABLE_ID.fetch_add(1, Ordering::SeqCst),
            vertex_buffer: Rc::new(vertex_buffer),
            vertex_count: vertices.len() as u32,
            index_buffer: Rc::new(index_buffer),
            index_count: indices.len() as u32,
            model_buffer: Rc::new(model_buffer),
            model_matrix: identity,
            material: Material::new(),
            object_type,
            double_sided: false,
            aabb: AABB::from_vertices(vertices),
        })
    }

    /// Upload a new model matrix to the GPU.
    pub fn update_model(&mut self, backend: &B, model: &glm::Mat4) {
        self.model_matrix = *model;
        let data = as_bytes(std::slice::from_ref(model));
        backend.update_buffer(&self.model_buffer, data);
    }

    /// Issue draw commands for this mesh.
    pub fn draw(&self, backend: &mut B, bind_material: bool) {
        backend.set_vertex_buffer(&self.vertex_buffer);
        backend.set_index_buffer(&self.index_buffer);
        backend.set_model_matrix(&self.model_matrix);
        backend.bind_uniform(ShaderStage::Vertex, 1, &self.model_buffer);

        if bind_material {
            self.material.bind(backend);
        }
        backend.set_material_properties(MaterialProperties {
            has_parallax: self.material.has_parallax,
        });

        backend.draw_indexed(self.index_count, 0, 0);
    }

    pub fn is_double_sided(&self) -> bool {
        self.double_sided
    }

    pub fn set_double_sided(&mut self, val: bool) {
        self.double_sided = val;
    }

    /// Returns the local-space AABB of this drawable's mesh.
    pub fn aabb(&self) -> &AABB {
        &self.aabb
    }

    pub fn set_parallax(&mut self, parallax: bool) {
        self.material.set_parallax(parallax);
    }

    /// Add or replace a texture on this drawable's material.
    pub fn add_texture(&mut self, slot: u32, tex: Rc<B::Texture>) {
        self.material.add_texture(slot, tex);
    }

    /// Returns the current world-space model matrix for this drawable.
    pub fn model_matrix(&self) -> &glm::Mat4 {
        &self.model_matrix
    }
}

impl<B: GpuBackend> PartialEq for Drawable<B> {
    fn eq(&self, other: &Drawable<B>) -> bool {
        self.id == other.id
    }
}

impl<B: GpuBackend> PartialOrd for Drawable<B> {
    fn partial_cmp(&self, other: &Drawable<B>) -> Option<std::cmp::Ordering> {
        if self.id == other.id {
            return Some(std::cmp::Ordering::Equal);
        }
        if self.material.eq(&other.material) {
            return Some(std::cmp::Ordering::Equal);
        }
        if self.id < other.id {
            return Some(std::cmp::Ordering::Less);
        }
        Some(std::cmp::Ordering::Greater)
    }
}

pub struct IndirectDrawable<B: GpuBackend> {
    id: usize,
    pub(crate) vertex_buffer: Rc<B::Buffer>,
    pub(crate) vertex_count: u32,
    pub(crate) index_buffer: Rc<B::Buffer>,
    pub(crate) index_count: u32,

    /// The SSBO filled by the compute shader containing `Vec<glm::Mat4>`
    pub(crate) instance_matrix_buffer: Rc<B::Buffer>,
    /// The buffer matching `VkDrawIndexedIndirectCommand` filled by the compute shader
    pub(crate) indirect_command_buffer: Rc<B::Buffer>,

    material: Material<B>,
    object_type: ObjType,
    double_sided: bool,
    // Global bounding box for the entire zone containing these assets (for coarse frustum culling)
    aabb: AABB,
}

impl<B: GpuBackend> Clone for IndirectDrawable<B> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            vertex_buffer: self.vertex_buffer.clone(),
            vertex_count: self.vertex_count,
            index_buffer: self.index_buffer.clone(),
            index_count: self.index_count.clone(),
            instance_matrix_buffer: self.instance_matrix_buffer.clone(),
            indirect_command_buffer: self.indirect_command_buffer.clone(),
            material: self.material.clone(),
            object_type: self.object_type.clone(),
            double_sided: self.double_sided.clone(),
            aabb: self.aabb.clone(),
        }
    }
}

impl<B: GpuBackend> IndirectDrawable<B> {
    pub fn from_drawable(
        drawable: Drawable<B>,
        instance_matrix_buffer: B::Buffer,
        indirect_command_buffer: B::Buffer,
    ) -> Self {
        let Drawable {
            id,
            vertex_buffer,
            vertex_count,
            index_buffer,
            index_count,
            material,
            object_type,
            double_sided,
            aabb,
            ..
        } = drawable;
        Self {
            id: id,
            vertex_buffer: vertex_buffer,
            vertex_count,
            index_buffer: index_buffer,
            index_count: index_count,
            instance_matrix_buffer: Rc::new(instance_matrix_buffer),
            indirect_command_buffer: Rc::new(indirect_command_buffer),
            material: material,
            object_type: object_type,
            double_sided: double_sided,
            aabb: aabb,
        }
    }
    pub fn draw_indirect(&self, backend: &mut B, bind_material: bool) {
        backend.set_vertex_buffer(&self.vertex_buffer);
        backend.set_index_buffer(&self.index_buffer);

        if bind_material {
            self.material.bind(backend);
        }
        backend.set_material_properties(MaterialProperties {
            has_parallax: self.material.has_parallax,
        });

        // Execute indirect draw — instance SSBO (binding 10) is bound once at load time
        // via execute_compute_one_shot, not per-frame (avoids MoltenVK descriptor race).
        backend.draw_indexed_indirect(&self.indirect_command_buffer, 0, 1);
    }
}

pub enum RenderItem<'a, B: GpuBackend> {
    Standard(&'a Drawable<B>),
    Indirect(&'a IndirectDrawable<B>),
}

// Implement partial_cmp directly on the enum wrapper so your sorting doesn't change
impl<'a, B: GpuBackend> PartialEq for RenderItem<'a, B> {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl<'a, B: GpuBackend> PartialOrd for RenderItem<'a, B> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.key().partial_cmp(&other.key())
    }
}

impl<'a, B: GpuBackend> RenderItem<'a, B> {
    // Helper function to extract sorting keys (e.g., sort by material ID to minimize PBR state shifts)
    fn key(&self) -> usize {
        match self {
            RenderItem::Standard(d) => d.id, // mapping your drawable ID or material ID
            RenderItem::Indirect(id) => id.id, // Assuming IndirectDrawable also has an ID
        }
    }

    pub fn object_type(&self) -> ObjType {
        match self {
            RenderItem::Standard(drawable) => drawable.object_type,
            RenderItem::Indirect(indirect_drawable) => indirect_drawable.object_type,
        }
    }

    pub fn material(&self) -> &Material<B> {
        match self {
            RenderItem::Standard(drawable) => &drawable.material,
            RenderItem::Indirect(indirect_drawable) => &indirect_drawable.material,
        }
    }

    pub fn is_double_sided(&self) -> bool {
        match self {
            RenderItem::Standard(drawable) => drawable.double_sided,
            RenderItem::Indirect(indirect_drawable) => indirect_drawable.double_sided,
        }
    }

    pub fn draw(&self, backend: &mut B, rebind_material: bool) {
        match self {
            RenderItem::Standard(drawable) => drawable.draw(backend, rebind_material),
            RenderItem::Indirect(indirect_drawable) => {
                indirect_drawable.draw_indirect(backend, rebind_material)
            }
        }
    }

    pub fn vertex_buffer(&self) -> Rc<B::Buffer> {
        match self {
            RenderItem::Standard(drawable) => drawable.vertex_buffer.clone(),
            RenderItem::Indirect(indirect_drawable) => indirect_drawable.vertex_buffer.clone(),
        }
    }
    pub fn index_buffer(&self) -> Rc<B::Buffer> {
        match self {
            RenderItem::Standard(drawable) => drawable.index_buffer.clone(),
            RenderItem::Indirect(indirect_drawable) => indirect_drawable.index_buffer.clone(),
        }
    }
    pub fn vertex_count(&self) -> u32 {
        match self {
            RenderItem::Standard(drawable) => drawable.vertex_count,
            RenderItem::Indirect(indirect_drawable) => indirect_drawable.vertex_count,
        }
    }
    pub fn index_count(&self) -> u32 {
        match self {
            RenderItem::Standard(drawable) => drawable.index_count,
            RenderItem::Indirect(indirect_drawable) => indirect_drawable.index_count,
        }
    }

    /// Returns the world-space model matrix for this render item.
    /// For Indirect drawables, returns identity (no per-instance matrix at this level).
    pub fn model_matrix(&self) -> glm::Mat4 {
        match self {
            RenderItem::Standard(drawable) => *drawable.model_matrix(),
            RenderItem::Indirect(ind) => glm::identity(),
        }
    }
}

// ScreenQuad

/// A fullscreen quad used for post-processing and fullscreen passes.
pub struct ScreenQuad<B: GpuBackend> {
    vertex_buffer: B::Buffer,
    index_buffer: B::Buffer,
    index_count: u32,
}

impl<B: GpuBackend> ScreenQuad<B> {
    /// Create a screen-space quad covering [0,0] to [1,1].
    pub fn create(backend: &B) -> Result<Self, GpuError> {
        let mut vertices = [Vertex::default(); 4];
        vertices[1].position.x = 1.0;
        vertices[2].position.y = 1.0;
        vertices[3].position.x = 1.0;
        vertices[3].position.y = 1.0;
        let indices: [u32; 6] = [2, 1, 0, 1, 2, 3];

        let vertex_data = as_bytes(&vertices);
        let index_data = as_bytes(&indices);

        let vertex_buffer = backend.create_buffer(
            &BufferDesc {
                label: "Screen Quad Vertex Buffer".into(),
                usage: BufferUsage::Vertex,
                size: vertex_data.len(),
            },
            Some(vertex_data),
        )?;

        let index_buffer = backend.create_buffer(
            &BufferDesc {
                label: "Screen Quad Index Buffer".into(),
                usage: BufferUsage::Index,
                size: index_data.len(),
            },
            Some(index_data),
        )?;

        Ok(ScreenQuad {
            vertex_buffer,
            index_buffer,
            index_count: 6,
        })
    }

    /// Issue draw commands for the screen quad.
    pub fn draw(&self, backend: &mut B) {
        backend.set_vertex_buffer(&self.vertex_buffer);
        backend.set_index_buffer(&self.index_buffer);
        backend.draw_indexed(self.index_count, 0, 0);
    }
}

// Helpers

/// Safely reinterpret a slice of `T` as a byte slice.
///
/// # Safety
///
/// The caller must ensure `T` is a plain-old-data type with no padding that
/// would cause undefined behavior when read as bytes. All types used in this
/// engine (Vertex, Mat4, Vec3, f32, u32) satisfy this requirement.
pub fn as_bytes<T>(data: &[T]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            std::mem::size_of::<T>() * data.len(),
        )
    }
}
