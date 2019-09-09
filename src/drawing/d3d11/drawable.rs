use super::backend::{DxError, DxErrorType};
use crate::drawing::geometry::Vertex;
use crate::drawing::scenegraph::drawable::Drawable;

use winapi::shared::winerror::S_OK;
use winapi::shared::dxgiformat::{DXGI_FORMAT_R32_UINT};
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

pub struct DxDrawable {
    context: *mut dx11_1::ID3D11DeviceContext1,
    vertex_buffer: *mut dx11::ID3D11Buffer,
    vertex_buffer_stride: u32,
    index_buffer: *mut dx11::ID3D11Buffer,
    index_buffer_stride: u32,
    index_count: u32,
}

impl Drawable for DxDrawable {
    fn draw(&self, _model: cgmath::Matrix4<f32>) {
        let offset = 0 as u32;
        unsafe {
            (*self.context).IASetVertexBuffers(
                0,
                1,
                &self.vertex_buffer as *const *mut _,
                &self.vertex_buffer_stride,
                &offset,
            );
            (*self.context).IASetIndexBuffer(
                self.index_buffer,
                DXGI_FORMAT_R32_UINT,
                0
            );
            (*self.context).DrawIndexed(self.index_count, 0, 0);
        }
    }
}

impl DxDrawable {
    pub fn from_verts(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
    ) -> Result<std::rc::Rc<std::cell::RefCell<DxDrawable>>, DxError> {
        let mut vertex_buffer: *mut dx11::ID3D11Buffer = std::ptr::null_mut();
        let vtx_stride = std::mem::size_of::<Vertex>() as u32;
        {
            let vertex_buffer_data = vertices.as_ptr();
            let mut initial_data: dx11::D3D11_SUBRESOURCE_DATA = Default::default();
            initial_data.pSysMem = vertex_buffer_data as *const _;

            let mut buffer_desc: dx11::D3D11_BUFFER_DESC = Default::default();
            buffer_desc.ByteWidth = vtx_stride * vertices.len() as u32;
            buffer_desc.Usage = dx11::D3D11_USAGE_IMMUTABLE;
            buffer_desc.BindFlags = dx11::D3D11_BIND_VERTEX_BUFFER;
            buffer_desc.StructureByteStride = vtx_stride;

            let res = unsafe {
                (*device).CreateBuffer(
                    &buffer_desc,
                    &initial_data,
                    &mut vertex_buffer as *mut *mut _,
                )
            };

            if res < S_OK {
                return Err(DxError::new(
                    "Vertex Buffer creation failed!",
                    DxErrorType::ResourceCreation,
                ));
            }
        }

        let mut index_buffer: *mut dx11::ID3D11Buffer = std::ptr::null_mut();
        let idx_stride = std::mem::size_of::<u32>() as u32;
        {
            let index_buffer_data = indices.as_ptr();
            let mut initial_data: dx11::D3D11_SUBRESOURCE_DATA = Default::default();
            initial_data.pSysMem = index_buffer_data as *const _;

            let mut buffer_desc: dx11::D3D11_BUFFER_DESC = Default::default();
            buffer_desc.ByteWidth = idx_stride * indices.len() as u32;
            buffer_desc.Usage = dx11::D3D11_USAGE_IMMUTABLE;
            buffer_desc.BindFlags = dx11::D3D11_BIND_INDEX_BUFFER;
            buffer_desc.StructureByteStride = idx_stride;

            let res = unsafe {
                (*device).CreateBuffer(
                    &buffer_desc,
                    &initial_data,
                    &mut index_buffer as *mut *mut _,
                )
            };

            if res < S_OK {
                return Err(DxError::new(
                    "Index Buffer creation failed!",
                    DxErrorType::ResourceCreation,
                ));
            }
        }
        Ok(std::rc::Rc::new(std::cell::RefCell::new(DxDrawable {
            context: context,
            vertex_buffer: vertex_buffer,
            vertex_buffer_stride: vtx_stride,
            index_buffer: index_buffer,
            index_buffer_stride: idx_stride,
            index_count: indices.len() as u32,
        })))
    }
}
