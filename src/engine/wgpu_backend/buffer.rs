use super::super::backend::*;

/// A GPU buffer backed by wgpu (vertex, index, or uniform).
#[derive(Clone)]
pub struct WgpuBuffer {
    pub(crate) buffer: wgpu::Buffer,
    size: usize,
}

impl WgpuBuffer {
    pub(crate) fn new(buffer: wgpu::Buffer, size: usize) -> Self {
        WgpuBuffer { buffer, size }
    }
}

impl GpuBuffer for WgpuBuffer {
    fn size(&self) -> usize {
        self.size
    }
}
