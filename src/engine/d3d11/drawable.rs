use super::super::geometry::Vertex;
use super::cbuffer::CBuffer;
use super::textures::Texture2D;
use super::{DxError, DxErrorType};

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;

use winapi::shared::dxgiformat::DXGI_FORMAT_R32_UINT;
use winapi::shared::winerror::S_OK;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

static DRAWABLE_ID: AtomicUsize = AtomicUsize::new(0);

pub struct Material {
    textures: HashMap<u32, Rc<RefCell<Texture2D>>>,
}

impl Material {
    fn new() -> Material {
        Material {
            textures: HashMap::new(),
        }
    }

    pub fn bind(&self, context: *mut dx11_1::ID3D11DeviceContext1) {
        for (slot, tex) in &self.textures {
            unsafe {
                (*context).PSSetSamplers(*slot, 1, &tex.borrow().get_sampler() as *const *mut _);
                (*context).PSSetShaderResources(
                    *slot,
                    1,
                    &tex.borrow().get_texture_view() as *const *mut _,
                );
            }
        }
    }
}

impl PartialEq for Material {
    fn eq(&self, other: &Material) -> bool {
        let mut eq = true;
        for (slot, tex) in &self.textures {
            if let Some(o_tex) = other.textures.get(slot) {
                if tex.borrow().id != o_tex.borrow().id {
                    eq = false;
                    break;
                }
            } else {
                eq = false;
                break;
            }
        }
        return eq;
    }
}

pub struct DxDrawable {
    id: usize,
    context: *mut dx11_1::ID3D11DeviceContext1,
    vertex_buffer: *mut dx11::ID3D11Buffer,
    vertex_buffer_stride: u32,
    index_buffer: *mut dx11::ID3D11Buffer,
    index_buffer_stride: u32,
    index_count: u32,
    cbuffer: CBuffer<glm::Mat4>,
    material: Material,
    object_type: ObjType,
}

impl DxDrawable {
    pub fn update_model(&mut self, model: &glm::Mat4) {
        self.cbuffer.data = model.clone();
        match self.cbuffer.update() {
            Ok(_) => {}
            Err(e) => println!("{}", e),
        };
    }
    pub fn draw(&self, bind_material: bool) {
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

            if bind_material {
                self.material.bind(self.context);
            }

            (*self.context).DrawIndexed(self.index_count, 0, 0);
        }
    }

    pub fn material(&self) -> &Material {
        &self.material
    }

    pub fn object_type(&self) -> ObjType {
        self.object_type
    }

    pub fn add_texture(&mut self, slot: u32, tex: Rc<RefCell<Texture2D>>) {
        self.material.textures.insert(slot, tex);
    }

    pub fn from_verts(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        object_type: ObjType,
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
            id: DRAWABLE_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            context: context,
            vertex_buffer: vertex_buffer,
            vertex_buffer_stride: vtx_stride,
            index_buffer: index_buffer,
            index_buffer_stride: idx_stride,
            index_count: indices.len() as u32,
            cbuffer: cbuffer,
            material: Material::new(),
            object_type: object_type,
        })))
    }
}

impl PartialEq for DxDrawable {
    fn eq(&self, other: &DxDrawable) -> bool {
        self.id == other.id
    }
}
use std::cmp::Ordering;
impl PartialOrd for DxDrawable {
    fn partial_cmp(&self, other: &DxDrawable) -> Option<Ordering> {
        if self.id == other.id {
            return Some(Ordering::Equal);
        }
        if self.material.eq(&other.material) {
            return Some(Ordering::Equal);
        }
        if self.id < other.id {
            return Some(Ordering::Less);
        }
        Some(Ordering::Greater)
    }
}

pub struct ScreenQuad {
    vertex_buffer: *mut dx11::ID3D11Buffer,
    vertex_buffer_stride: u32,
    index_buffer: *mut dx11::ID3D11Buffer,
    index_buffer_stride: u32,
    index_count: u32,
}

impl ScreenQuad {
    pub fn create(device: *mut dx11_1::ID3D11Device1) -> Result<ScreenQuad, DxError> {
        let mut vertices = vec![
            Vertex::default(),
            Vertex::default(),
            Vertex::default(),
            Vertex::default(),
        ];
        vertices[1].position.x = 1.0f32;
        vertices[2].position.y = 1.0f32;
        vertices[3].position.x = 1.0f32;
        vertices[3].position.y = 1.0f32;
        let indices = vec![2, 1, 0, 1, 2, 3];
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
        Ok(ScreenQuad {
            vertex_buffer: vertex_buffer,
            vertex_buffer_stride: vtx_stride,
            index_buffer: index_buffer,
            index_buffer_stride: idx_stride,
            index_count: indices.len() as u32,
        })
    }

    pub fn draw(&self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        let offset = 0 as u32;
        unsafe {
            (*ctx).IASetVertexBuffers(
                0,
                1,
                &self.vertex_buffer as *const *mut _,
                &self.vertex_buffer_stride,
                &offset,
            );
            (*ctx).IASetIndexBuffer(self.index_buffer, DXGI_FORMAT_R32_UINT, 0);
            (*ctx).DrawIndexed(self.index_count, 0, 0);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ObjType {
    Opaque,
    Transparent,
    Any, // use only for draw call to match all objects. never assign to object!
}

impl PartialEq for ObjType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (_, ObjType::Any) => true,
            (ObjType::Opaque, ObjType::Opaque) => true,
            (ObjType::Transparent, ObjType::Transparent) => true,
            _ => false,
        }
    }
}
