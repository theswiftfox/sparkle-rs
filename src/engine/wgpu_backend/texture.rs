use super::super::backend::*;
use std::sync::atomic::{AtomicUsize, Ordering};

// WgpuTexture

static TEXTURE_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct WgpuTexture {
    pub(crate) _texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    format: TextureFormat,
    id: usize,
}

impl WgpuTexture {
    pub(crate) fn new(
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        sampler: wgpu::Sampler,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Self {
        WgpuTexture {
            _texture: texture,
            view,
            sampler,
            width,
            height,
            format,
            id: TEXTURE_ID.fetch_add(1, Ordering::SeqCst),
        }
    }
}

impl GpuTexture for WgpuTexture {
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

// WgpuRenderTarget

static RENDER_TARGET_ID: AtomicUsize = AtomicUsize::new(0);

/// A render target backed by a wgpu texture. Can be rendered to and sampled.
///
/// For the backbuffer, `_texture` is `None` and `view` is updated each frame
/// in `begin_frame()`.
#[derive(Clone)]
pub struct WgpuRenderTarget {
    pub(crate) _texture: Option<wgpu::Texture>,
    pub(crate) view: wgpu::TextureView,
    pub(crate) sampler: Option<wgpu::Sampler>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TextureFormat,
    id: usize,
}

impl WgpuRenderTarget {
    /// Create a render target from an owned texture.
    pub(crate) fn new(
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        sampler: Option<wgpu::Sampler>,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Self {
        WgpuRenderTarget {
            _texture: Some(texture),
            view,
            sampler,
            width,
            height,
            format,
            id: RENDER_TARGET_ID.fetch_add(1, Ordering::SeqCst),
        }
    }

    /// Create a render target proxy for the backbuffer.
    /// The `view` will be replaced each frame in `begin_frame()`.
    pub(crate) fn backbuffer_proxy(
        view: wgpu::TextureView,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Self {
        WgpuRenderTarget {
            _texture: None,
            view,
            sampler: None,
            width,
            height,
            format,
            id: RENDER_TARGET_ID.fetch_add(1, Ordering::SeqCst),
        }
    }
}

impl GpuTexture for WgpuRenderTarget {
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

impl GpuRenderTarget for WgpuRenderTarget {}
