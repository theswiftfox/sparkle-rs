//! Backend abstraction layer for sparkle-rs.
//!
//! Defines platform- and API-agnostic traits and types for GPU rendering.
//! Backend implementations (wgpu, Vulkan, etc.) implement the [`GpuBackend`] trait
//! and its associated resource types.

use super::geometry::Vertex;

use std::cell::RefCell;
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
            address_u: AddressMode::Clamp,
            address_v: AddressMode::Clamp,
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

/// Description for creating a render target (usable as both render attachment and shader input).
pub struct RenderTargetDesc {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub sampler: SamplerDesc,
}

/// Description for creating a GPU buffer.
pub struct BufferDesc {
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
/// ## Shader source
///
/// `shader_source` is WGSL text (UTF-8 bytes) for the wgpu backend.
/// Entry points must be named `vs_main` (vertex) and `fs_main` (fragment).
/// If `color_target_formats` is empty, no fragment stage is created (depth-only pass).
pub struct PipelineDesc<'a> {
    pub label: &'a str,
    pub shader_source: &'a [u8],
    /// `None` for fullscreen / procedurally-generated-vertex shaders.
    pub vertex_layout: Option<VertexLayout>,
    pub blend_mode: BlendMode,
    pub cull_mode: CullMode,
    pub depth_write: bool,
    pub depth_compare: CompareFunc,
    pub color_target_formats: &'a [TextureFormat],
    pub depth_format: Option<TextureFormat>,
    /// Bind group layout descriptions. Index = group number (0..3).
    pub bind_groups: &'a [&'a [BindingType]],
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
/// different graphics API backends (wgpu, Vulkan, etc.) to be swapped in.
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
pub trait GpuBackend: Sized {
    type Texture: GpuTexture;
    type RenderTarget: GpuRenderTarget;
    type Buffer: GpuBuffer;
    type Pipeline;

    // --- Resource creation ---

    /// Create a 2D texture from raw pixel data.
    fn create_texture(
        &self,
        desc: &TextureDesc,
        data: &[u8],
    ) -> Result<Self::Texture, GpuError>;

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
    fn create_render_target(
        &self,
        desc: &RenderTargetDesc,
    ) -> Result<Self::RenderTarget, GpuError>;

    /// Create a render pipeline from compiled shader bytecode and fixed-function state.
    fn create_pipeline(
        &self,
        desc: &PipelineDesc,
    ) -> Result<Self::Pipeline, GpuError>;

    // --- Buffer operations ---

    /// Upload new data to a uniform/dynamic buffer.
    fn update_buffer(&self, buffer: &Self::Buffer, data: &[u8]);

    // --- Frame lifecycle ---

    /// Begin a new frame. Must be called before any render passes.
    fn begin_frame(&mut self) -> Result<(), GpuError>;

    /// Finish recording and submit all commands for the current frame.
    fn end_frame(&mut self) -> Result<(), GpuError>;

    /// Present the rendered frame to the display.
    fn present(&mut self) -> Result<(), GpuError>;

    // --- Render pass management ---

    /// Begin a render pass with the specified attachments and load operations.
    fn begin_render_pass(&mut self, desc: &RenderPassDesc<Self>);

    /// End the current render pass.
    fn end_render_pass(&mut self);

    // --- Draw commands (valid within a render pass) ---

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

    /// Set the vertex buffer for subsequent draw calls.
    fn set_vertex_buffer(&mut self, buffer: &Self::Buffer);

    /// Set the index buffer for subsequent draw calls.
    fn set_index_buffer(&mut self, buffer: &Self::Buffer);

    /// Issue an indexed draw call.
    fn draw_indexed(&mut self, index_count: u32, first_index: u32, base_vertex: i32);

    // --- Accessors ---

    /// Get the current frame's backbuffer render target.
    fn backbuffer(&self) -> &Self::RenderTarget;

    /// Get the main depth buffer render target.
    fn main_depth_target(&self) -> &Self::RenderTarget;

    /// Get the default full-window viewport.
    fn default_viewport(&self) -> ViewportDesc;

    /// Get the current framebuffer resolution (width, height).
    fn resolution(&self) -> (u32, u32);

    /// Handle a window resize by reconfiguring the surface and recreating
    /// resolution-dependent resources (depth buffer, etc.).
    fn resize(&mut self, width: u32, height: u32);

    // --- Debug markers (default no-op implementations) ---

    /// Begin a named debug event region (e.g., for GPU profilers).
    fn begin_event(&self, _name: &str) {}

    /// End the current debug event region.
    fn end_event(&self) {}
}

// Material

static MATERIAL_ID: AtomicUsize = AtomicUsize::new(0);

/// A collection of textures bound to shader slots, representing a surface material.
pub struct Material<B: GpuBackend> {
    textures: HashMap<u32, Rc<B::Texture>>,
    id: usize,
}

impl<B: GpuBackend> Material<B> {
    pub fn new() -> Self {
        Material {
            textures: HashMap::new(),
            id: MATERIAL_ID.fetch_add(1, Ordering::SeqCst),
        }
    }

    /// Bind all material textures to their respective shader slots.
    pub fn bind(&self, backend: &mut B) {
        for (slot, tex) in &self.textures {
            backend.bind_texture(*slot, tex);
        }
    }

    /// Add or replace a texture at the given slot.
    pub fn add_texture(&mut self, slot: u32, tex: Rc<B::Texture>) {
        self.textures.insert(slot, tex);
        // Regenerate id when material changes so sorting picks up the difference.
        self.id = MATERIAL_ID.fetch_add(1, Ordering::SeqCst);
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
    vertex_buffer: B::Buffer,
    index_buffer: B::Buffer,
    index_count: u32,
    model_buffer: B::Buffer,
    material: Material<B>,
    object_type: ObjType,
    double_sided: bool,
}

impl<B: GpuBackend> Drawable<B> {
    /// Create a drawable from vertex and index data.
    pub fn from_verts(
        backend: &B,
        vertices: &[Vertex],
        indices: &[u32],
        object_type: ObjType,
    ) -> Result<Rc<RefCell<Drawable<B>>>, GpuError> {
        let vertex_data = as_bytes(vertices);
        let index_data = as_bytes(indices);

        let vertex_buffer = backend.create_buffer(
            &BufferDesc {
                usage: BufferUsage::Vertex,
                size: vertex_data.len(),
            },
            Some(vertex_data),
        )?;

        let index_buffer = backend.create_buffer(
            &BufferDesc {
                usage: BufferUsage::Index,
                size: index_data.len(),
            },
            Some(index_data),
        )?;

        let identity: glm::Mat4 = glm::identity();
        let model_data = as_bytes(std::slice::from_ref(&identity));
        let model_buffer = backend.create_buffer(
            &BufferDesc {
                usage: BufferUsage::Uniform,
                size: model_data.len(),
            },
            Some(model_data),
        )?;

        Ok(Rc::new(RefCell::new(Drawable {
            id: DRAWABLE_ID.fetch_add(1, Ordering::SeqCst),
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            model_buffer,
            material: Material::new(),
            object_type,
            double_sided: false,
        })))
    }

    /// Upload a new model matrix to the GPU.
    pub fn update_model(&self, backend: &B, model: &glm::Mat4) {
        let data = as_bytes(std::slice::from_ref(model));
        backend.update_buffer(&self.model_buffer, data);
    }

    /// Issue draw commands for this mesh.
    pub fn draw(&self, backend: &mut B, bind_material: bool) {
        backend.set_vertex_buffer(&self.vertex_buffer);
        backend.set_index_buffer(&self.index_buffer);
        backend.bind_uniform(ShaderStage::Vertex, 1, &self.model_buffer);

        if bind_material {
            self.material.bind(backend);
        }

        backend.draw_indexed(self.index_count, 0, 0);
    }

    pub fn material(&self) -> &Material<B> {
        &self.material
    }

    pub fn object_type(&self) -> ObjType {
        self.object_type
    }

    pub fn is_double_sided(&self) -> bool {
        self.double_sided
    }

    pub fn set_double_sided(&mut self, val: bool) {
        self.double_sided = val;
    }

    /// Add or replace a texture on this drawable's material.
    pub fn add_texture(&mut self, slot: u32, tex: Rc<B::Texture>) {
        self.material.add_texture(slot, tex);
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
                usage: BufferUsage::Vertex,
                size: vertex_data.len(),
            },
            Some(vertex_data),
        )?;

        let index_buffer = backend.create_buffer(
            &BufferDesc {
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
