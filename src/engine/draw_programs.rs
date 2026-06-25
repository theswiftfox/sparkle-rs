//! Render pass programs for the sparkle-rs deferred+forward pipeline.
//!
//! Each pass struct bundles a GPU pipeline, uniform buffers, and render targets.
//! All types are generic over [`GpuBackend`] for backend-agnostic rendering.
//!
//! The rendering pipeline consists of:
//! 1. **DeferredPassPre** — G-buffer fill (position, normal+roughness, albedo+metallic MRT)
//! 2. **SsaoPass** — Screen-space ambient occlusion (SSAO + blur sub-passes)
//! 3. **ShadowPass** — Directional shadow map generation
//! 4. **DeferredPassLight** — Fullscreen deferred lighting (reads SSAO result)
//! 5. **ForwardPass** — Transparent object rendering with forward lighting
//! 6. **OutputPass** — Composite deferred + forward results to backbuffer
//! 7. **SkyBoxPass** — Skybox rendering
//!
//! Each pass exposes:
//! - `create(backend, ...)` — construct from a backend and shader bytecode
//! - `prepare_draw(backend)` — bind the pipeline and uniforms for drawing
//! - `update(backend)` — upload CPU-side uniform data to GPU buffers
//! - Setters for CPU-side uniform data (view, proj, light, etc.)
//! - Accessors for render targets used by subsequent passes

use super::backend::*;
use super::geometry::{Light, LightType};

// Uniform data structs (CPU-side, #[repr(C)] for GPU upload)

/// View and projection matrices — used by vertex shaders of most passes.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ViewProjUniforms {
    pub view: glm::Mat4,
    pub proj: glm::Mat4,
    pub inv_view: glm::Mat4,
    pub inv_proj: glm::Mat4,
}

/// Camera position and SSAO toggle — used by lighting pixel shaders.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CameraUniforms {
    pub camera_pos: glm::Vec3,
    pub ssao: u32,
}

/// Near/far plane distances — used by deferred pre-pass pixel shader.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct NearFarUniforms {
    pub near_plane: f32,
    pub far_plane: f32,
    pub _pad: f32,
    pub _pad2: f32,
}

/// Light-space matrix — used by shadow pass vertex shader.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct LightSpaceUniforms {
    pub light_space_matrix: glm::Mat4,
}

/// GPU-side light data, matching the shader cbuffer layout.
///
/// Layout (96 bytes):
/// - `position: Vec3` (12 bytes) + `t: u32` (4 bytes) = 16 bytes
/// - `color: Vec3` (12 bytes) + `radius: f32` (4 bytes) = 16 bytes
/// - `light_space: Mat4` (64 bytes)
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct GpuLight {
    position: glm::Vec3,
    t: u32,
    color: glm::Vec3,
    radius: f32,
    light_space: glm::Mat4,
}

impl GpuLight {
    pub(crate) fn from_light(light: &Light) -> GpuLight {
        let t = match &light.t {
            LightType::Ambient => 0u32,
            LightType::Directional => 1u32,
            LightType::Area => 2u32,
        };
        GpuLight {
            position: light.position,
            t,
            color: light.color,
            radius: light.radius,
            light_space: light.light_proj,
        }
    }
}

// SSAO helpers (kernel + noise generation)

fn hash_u32(mut x: u32) -> u32 {
    x = ((x >> 16) ^ x).wrapping_mul(0x45d9f3b);
    x = ((x >> 16) ^ x).wrapping_mul(0x45d9f3b);
    x = (x >> 16) ^ x;
    x
}

fn hash_float(i: u32) -> f32 {
    (hash_u32(i) as f32) / (u32::MAX as f32)
}

/// Generate 32 hemisphere-distributed sample points for SSAO.
/// Samples are weighted toward the center for better quality.
fn generate_ssao_kernel() -> [[f32; 4]; 32] {
    let mut kernel = [[0.0f32; 4]; 32];
    for i in 0..32 {
        let theta = 2.0 * std::f32::consts::PI * hash_float(i as u32 * 2);
        let cos_phi = hash_float(i as u32 * 2 + 1);
        let sin_phi = (1.0 - cos_phi * cos_phi).sqrt();

        let mut x = sin_phi * theta.cos();
        let mut y = sin_phi * theta.sin();
        let mut z = cos_phi;

        // Scale: more samples closer to the surface
        let scale = 0.1 + 0.9 * (i as f32 / 32.0) * (i as f32 / 32.0);
        x *= scale;
        y *= scale;
        z *= scale;

        kernel[i] = [x, y, z, 0.0];
    }
    kernel
}

/// Generate a 4x4 noise texture for SSAO (random rotation vectors).
/// Each pixel is (randX, randY, 0, 1) in [0, 255] Rgba8Unorm.
fn generate_ssao_noise() -> Vec<u8> {
    let mut noise = Vec::with_capacity(4 * 4 * 4);
    for i in 0..16 {
        let x = hash_float(i as u32 * 3 + 100);
        let y = hash_float(i as u32 * 3 + 101);
        noise.push((x * 255.0) as u8);
        noise.push((y * 255.0) as u8);
        noise.push(0u8);
        noise.push(255u8);
    }
    noise
}

// SsaoPass

/// SSAO uniforms matching the WGSL struct layout.
///
/// Layout (656 bytes):
/// - `projection: Mat4` (64 bytes)
/// - `view: Mat4` (64 bytes)
/// - `resolution: [f32; 2]` (8 bytes)
/// - `radius: f32` (4 bytes)
/// - `bias: f32` (4 bytes)
/// - `kernel: [[f32; 4]; 32]` (512 bytes)
#[repr(C)]
#[derive(Clone, Copy)]
struct SsaoUniforms {
    projection: glm::Mat4,
    view: glm::Mat4,
    resolution: [f32; 2],
    radius: f32,
    bias: f32,
    kernel: [[f32; 4]; 32],
}

// ForwardPass

/// Forward rendering pass: renders transparent objects with full lighting.
///
/// Vertex uniforms (slot 0): view + projection matrices.
/// Pixel uniforms (slot 0): camera position + SSAO flag.
/// Pixel uniforms (slot 1): light data.
/// Output: `Rgba32Float` render target.
pub(crate) struct ForwardPass<B: GpuBackend> {
    pipeline: B::Pipeline,
    pipeline_double_sided: B::Pipeline, // Shared UBOs are bound globally
    render_target: B::RenderTarget,
}

impl<B: GpuBackend> ForwardPass<B> {
    pub fn render_target(&self) -> &B::RenderTarget {
        &self.render_target
    }

    /// Bind pipeline and all uniform buffers for drawing.
    pub fn prepare_draw(&self, backend: &mut B) {
        backend.set_pipeline(&self.pipeline);
    }

    /// Switch pipeline based on whether the drawable is double-sided.
    /// Rebinds pass uniforms since set_pipeline() clears all pending bindings.
    pub fn set_pipeline_for(&self, backend: &mut B, double_sided: bool) {
        if double_sided {
            backend.set_pipeline(&self.pipeline_double_sided);
        } else {
            backend.set_pipeline(&self.pipeline);
        }
    }

    pub fn create(
        backend: &B,
        resolution: (u32, u32),
        shader_source: &B::ShaderSource,
    ) -> Result<Self, GpuError> {
        let pipeline = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "forward_pass",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::Alpha,
            cull_mode: CullMode::Back,
            depth_write: true,
            depth_compare: CompareFunc::LessEqual,
            color_target_formats: &[TextureFormat::R16g16b16a16Float],
            depth_format: Some(TextureFormat::Depth32Float),
        })?;

        let pipeline_double_sided = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "forward_pass_double_sided",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::Alpha,
            cull_mode: CullMode::None,
            depth_write: true,
            depth_compare: CompareFunc::LessEqual,
            color_target_formats: &[TextureFormat::R16g16b16a16Float],
            depth_format: Some(TextureFormat::Depth32Float),
        })?;

        let render_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::R16g16b16a16Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;

        Ok(ForwardPass {
            pipeline,
            pipeline_double_sided,
            render_target,
        })
    }

    /// Recreate resolution-dependent render targets after a window resize.
    pub fn resize(&mut self, backend: &B, resolution: (u32, u32)) -> Result<(), GpuError> {
        self.render_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::R16g16b16a16Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;
        Ok(())
    }
}

// DeferredPassPre

/// Deferred pre-pass: fills the G-buffer with position, normal, roughness, albedo, and metallic data.
///
/// Vertex uniforms (slot 0): view + projection matrices.
/// Pixel uniforms (slot 0): near/far plane distances.
/// Output: three float MRT targets (position, normal+roughness, albedo+metallic).
pub(crate) struct DeferredPassPre<B: GpuBackend> {
    pipeline: B::Pipeline,
    pipeline_double_sided: B::Pipeline,
    positions_target: B::RenderTarget,
    normal_roughness_target: B::RenderTarget,
    albedo_metallic_target: B::RenderTarget,
}

impl<B: GpuBackend> DeferredPassPre<B> {
    pub fn positions(&self) -> &B::RenderTarget {
        &self.positions_target
    }

    pub fn normal_roughness(&self) -> &B::RenderTarget {
        &self.normal_roughness_target
    }

    pub fn albedo_metallic(&self) -> &B::RenderTarget {
        &self.albedo_metallic_target
    }

    /// Bind pipeline and all uniform buffers for drawing.
    pub fn prepare_draw(&self, backend: &mut B) {
        backend.set_pipeline(&self.pipeline);
    }

    /// Switch pipeline based on whether the drawable is double-sided.
    /// Call after `prepare_draw()` to override the default pipeline.
    /// Rebinds pass uniforms since set_pipeline() clears all pending bindings.
    pub fn set_pipeline_for(&self, backend: &mut B, double_sided: bool) {
        if double_sided {
            backend.set_pipeline(&self.pipeline_double_sided);
        } else {
            backend.set_pipeline(&self.pipeline);
        }
    }

    pub fn create(
        backend: &B,
        resolution: (u32, u32),
        shader_source: &B::ShaderSource,
    ) -> Result<Self, GpuError> {
        let pipeline = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "deferred_pre",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::None,
            cull_mode: CullMode::Back,
            depth_write: true,
            depth_compare: CompareFunc::LessEqual,
            color_target_formats: &[
                TextureFormat::Rgba32Float,
                TextureFormat::R16g16b16a16Float,
                TextureFormat::R16g16b16a16Float,
            ],
            depth_format: Some(TextureFormat::Depth32Float),
        })?;

        let pipeline_double_sided = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "deferred_pre_double_sided",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::None,
            cull_mode: CullMode::None,
            depth_write: true,
            depth_compare: CompareFunc::LessEqual,
            color_target_formats: &[
                TextureFormat::Rgba32Float,
                TextureFormat::R16g16b16a16Float,
                TextureFormat::R16g16b16a16Float,
            ],
            depth_format: Some(TextureFormat::Depth32Float),
        })?;

        let positions_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::Rgba32Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;
        let normal_roughness_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::R16g16b16a16Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;
        let albedo_metallic_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::R16g16b16a16Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;

        Ok(DeferredPassPre {
            pipeline,
            pipeline_double_sided,
            positions_target,
            normal_roughness_target,
            albedo_metallic_target,
        })
    }

    /// Recreate resolution-dependent render targets after a window resize.
    pub fn resize(&mut self, backend: &B, resolution: (u32, u32)) -> Result<(), GpuError> {
        self.positions_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::Rgba32Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;
        self.normal_roughness_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::R16g16b16a16Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;
        self.albedo_metallic_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::R16g16b16a16Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;
        Ok(())
    }
}

// DeferredPassLight

/// Deferred lighting pass: fullscreen quad that reads G-buffer and computes lighting.
///
/// Pixel uniforms (slot 0): camera position + SSAO flag.
/// Pixel uniforms (slot 1): light data.
/// Inputs (bound by Renderer): G-buffer positions (slot 0), normal+roughness (slot 1),
///   albedo+metallic (slot 2), shadow map (slot 3), SSAO blurred texture (slot 4).
/// Output: `R16g16b16a16Float` render target (accumulated light).
pub(crate) struct DeferredPassLight<B: GpuBackend> {
    pipeline: B::Pipeline,
    render_target: B::RenderTarget,
    pixel_uniforms: CameraUniforms,
}

impl<B: GpuBackend> DeferredPassLight<B> {
    pub fn render_target(&self) -> &B::RenderTarget {
        &self.render_target
    }

    /// Bind pipeline and all uniform buffers for drawing.
    pub fn prepare_draw(&self, backend: &mut B) {
        backend.set_pipeline(&self.pipeline);
    }

    /// Upload all CPU-side uniform data to GPU buffers.
    // This method is no longer needed as the Renderer will directly update shared UBOs.
    // pub fn update(&self, backend: &B) {
    //     backend.update_buffer(&self.pixel_uniforms_buf, as_bytes(std::slice::from_ref(&self.pixel_uniforms)));
    //     backend.update_buffer(&self.light_buf, as_bytes(std::slice::from_ref(&self.light_data)));
    // }
    pub fn set_camera_pos(&mut self, pos: glm::Vec3) {
        // This updates the CPU-side struct
        self.pixel_uniforms.camera_pos = pos;
    }

    pub fn set_ssao(&mut self, enabled: bool) {
        self.pixel_uniforms.ssao = if enabled { 1 } else { 0 };
    }

    pub fn create(
        backend: &B,
        resolution: (u32, u32),
        shader_source: &B::ShaderSource,
    ) -> Result<Self, GpuError> {
        let pipeline = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "deferred_light",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::Additive,
            cull_mode: CullMode::None,
            depth_write: false,
            depth_compare: CompareFunc::Always,
            color_target_formats: &[TextureFormat::R16g16b16a16Float],
            depth_format: None,
        })?;

        let render_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::R16g16b16a16Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;

        Ok(DeferredPassLight {
            pixel_uniforms: CameraUniforms {
                camera_pos: glm::zero(),
                ssao: 1,
            },
            pipeline,
            render_target,
        })
    }

    /// Recreate resolution-dependent render targets after a window resize.
    pub fn resize(&mut self, backend: &B, resolution: (u32, u32)) -> Result<(), GpuError> {
        self.render_target = backend.create_render_target(&RenderTargetDesc {
            width: resolution.0,
            height: resolution.1,
            format: TextureFormat::R16g16b16a16Float,
            sampler: SamplerDesc::default(),
            usage: RenderTargetUsage::Color,
        })?;
        Ok(())
    }
}

// ShadowPass

pub const SHADOW_MAP_SIZE: u32 = 2048;

/// Shadow mapping pass: renders the scene from the light's perspective into a depth map.
///
/// Vertex uniforms (slot 0): light-space matrix.
/// Output: `Depth32Float` shadow map render target (4096x4096).
/// The shadow map has a comparison sampler for PCF filtering.
pub(crate) struct ShadowPass<B: GpuBackend> {
    pipeline: B::Pipeline,              // Shared UBOs are bound globally
    pipeline_double_sided: B::Pipeline, // Shared UBOs are bound globally
    shadow_map: B::RenderTarget,
    shadow_viewport: ViewportDesc,
}

impl<B: GpuBackend> ShadowPass<B> {
    pub fn shadow_map(&self) -> &B::RenderTarget {
        &self.shadow_map
    }

    pub fn viewport(&self) -> &ViewportDesc {
        &self.shadow_viewport
    }

    /// Bind pipeline and vertex uniform buffer for drawing.
    pub fn prepare_draw(&self, backend: &mut B) {
        backend.set_pipeline(&self.pipeline);
    }

    /// Switch pipeline based on whether the drawable is double-sided.
    /// Rebinds pass uniforms since set_pipeline() clears all pending bindings.
    pub fn set_pipeline_for(&self, backend: &mut B, double_sided: bool) {
        if double_sided {
            backend.set_pipeline(&self.pipeline_double_sided);
        } else {
            backend.set_pipeline(&self.pipeline);
        }
    }

    pub fn create(backend: &B, shader_source: &B::ShaderSource) -> Result<Self, GpuError> {
        let pipeline = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "shadow_pass",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::None,
            cull_mode: CullMode::Front, // Front-face culling reduces shadow acne
            depth_write: true,
            depth_compare: CompareFunc::Less,
            color_target_formats: &[],
            depth_format: Some(TextureFormat::Depth32Float),
        })?;

        let pipeline_double_sided = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "shadow_pass_double_sided",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::None,
            cull_mode: CullMode::None, // Both faces cast shadows for double-sided materials
            depth_write: true,
            depth_compare: CompareFunc::Less,
            color_target_formats: &[],
            depth_format: Some(TextureFormat::Depth32Float),
        })?;

        let shadow_map = backend.create_render_target(&RenderTargetDesc {
            width: SHADOW_MAP_SIZE,
            height: SHADOW_MAP_SIZE,
            format: TextureFormat::Depth32Float,
            sampler: SamplerDesc {
                address_u: AddressMode::Clamp,
                address_v: AddressMode::Clamp,
                filter: FilterMode::Linear,
                compare: Some(CompareFunc::LessEqual),
            },
            usage: RenderTargetUsage::Depth,
        })?;

        let shadow_viewport = ViewportDesc {
            x: 0.0,
            y: 0.0,
            width: SHADOW_MAP_SIZE as f32,
            height: SHADOW_MAP_SIZE as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        Ok(ShadowPass {
            pipeline,
            pipeline_double_sided,
            shadow_map,
            shadow_viewport,
        })
    }
}

// OutputPass

/// Output compositing pass: blends deferred and forward results to the backbuffer.
///
/// No uniforms. Inputs are bound by the Renderer:
///   - Deferred light result (slot 0)
///   - Forward result (slot 1)
pub(crate) struct OutputPass<B: GpuBackend> {
    pipeline: B::Pipeline,
}

impl<B: GpuBackend> OutputPass<B> {
    /// Bind pipeline for drawing.
    pub fn prepare_draw(&self, backend: &mut B) {
        backend.set_pipeline(&self.pipeline);
    }

    pub fn create(
        backend: &B,
        shader_source: &B::ShaderSource,
        backbuffer_format: TextureFormat,
    ) -> Result<Self, GpuError> {
        let pipeline = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "output_pass",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::None,
            cull_mode: CullMode::None,
            depth_write: false,
            depth_compare: CompareFunc::Always,
            color_target_formats: &[backbuffer_format],
            depth_format: None,
        })?;

        Ok(OutputPass { pipeline })
    }
}

// SkyBoxPass

/// Skybox rendering pass: draws a cubemap skybox behind all scene geometry.
///
/// Vertex uniforms (slot 0): view + projection matrices.
/// The view matrix should have its translation component removed
/// (mat3→mat4 conversion) so the skybox moves with the camera.
pub(crate) struct SkyBoxPass<B: GpuBackend> {
    pipeline: B::Pipeline,
}

impl<B: GpuBackend> SkyBoxPass<B> {
    /// Bind pipeline and vertex uniform buffer for drawing.
    pub fn prepare_draw(&self, backend: &mut B) {
        backend.set_pipeline(&self.pipeline);
    }

    pub fn create(
        backend: &B,
        shader_source: &B::ShaderSource,
        backbuffer_format: TextureFormat,
    ) -> Result<Self, GpuError> {
        let pipeline = backend.create_render_pipeline(&RenderPipelineDesc {
            label: "skybox_pass",
            shader_source,
            vertex_layout: Some(standard_vertex_layout()),
            blend_mode: BlendMode::None,
            cull_mode: CullMode::None,
            depth_write: false,
            depth_compare: CompareFunc::LessEqual,
            color_target_formats: &[backbuffer_format],
            depth_format: Some(TextureFormat::Depth32Float),
        })?;

        Ok(SkyBoxPass { pipeline })
    }
}
