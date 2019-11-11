use super::{DxError, DxErrorType};
use super::cbuffer::CBuffer;
use super::sampler::Texture2D;
use super::super::geometry::Vertex;
use super::super::scenegraph::drawable::Drawable;

use winapi::shared::dxgiformat::DXGI_FORMAT_R32_UINT;
use winapi::shared::winerror::S_OK;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

pub struct DxDrawable {
    context: *mut dx11_1::ID3D11DeviceContext1,
    vertex_buffer: *mut dx11::ID3D11Buffer,
    vertex_buffer_stride: u32,
    index_buffer: *mut dx11::ID3D11Buffer,
    index_buffer_stride: u32,
    index_count: u32,
    cbuffer: CBuffer<glm::Mat4>,
    textures: Vec<(u32, Texture2D)>, // (slot, tex)
}

impl Drawable for DxDrawable {
    fn draw(&mut self, model: glm::Mat4) {
        self.cbuffer.data = model;
        match self.cbuffer.update() {
            Ok(_) => {}
            Err(e) => println!("{}", e),
        };
        let offset = 0 as u32;
        unsafe {
            (*self.context).IASetVertexBuffers(
                0,
                1,
                &self.vertex_buffer as *const *mut _,
                &self.vertex_buffer_stride,
                &offset,
            );
            (*self.context).IASetIndexBuffer(self.index_buffer, DXGI_FORMAT_R32_UINT, 0);
            (*self.context).VSSetConstantBuffers(1, 1, &self.cbuffer.buffer_ptr() as *const *mut _);

            for (slot, tex) in &self.textures {
                (*self.context).PSSetSamplers(*slot, 1, &tex.get_sampler() as *const *mut _);
                (*self.context).PSSetShaderResources(*slot, 1, &tex.get_texture() as *const *mut _);
            }

            (*self.context).DrawIndexed(self.index_count, 0, 0);
        }
    }
}

impl DxDrawable {
    pub fn add_texture(&mut self, slot: u32, tex: Texture2D) {
        self.textures.push((slot, tex))
    }

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
        let cbuffer: CBuffer<glm::Mat4> = CBuffer::create(glm::identity(), context, device)?;
        Ok(std::rc::Rc::new(std::cell::RefCell::new(DxDrawable {
            context: context,
            vertex_buffer: vertex_buffer,
            vertex_buffer_stride: vtx_stride,
            index_buffer: index_buffer,
            index_buffer_stride: idx_stride,
            index_count: indices.len() as u32,
            cbuffer: cbuffer,
            textures: Vec::<(u32, Texture2D)>::new(),
        })))
    }
}
