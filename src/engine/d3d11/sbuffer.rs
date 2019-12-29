use super::{DxError, DxErrorType};

use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;
use winapi::um::d3dcommon as d3d;

pub struct SBuffer<T> {
    data: Vec<T>,
    size: usize,
    gpu_buffer: *mut dx11::ID3D11Buffer,
    shader_view: *mut dx11::ID3D11ShaderResourceView,
    context: *mut dx11_1::ID3D11DeviceContext1,
    device: *mut dx11_1::ID3D11Device1,
}

impl<T> SBuffer<T> {
    pub fn create(
        data: Vec<T>,
        context: *mut dx11_1::ID3D11DeviceContext1,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<SBuffer<T>, DxError> {
        let size = data.len();
        let mut buffer = SBuffer {
            data: data,
            size: std::mem::size_of::<T>() * size,
            context: context,
            shader_view: std::ptr::null_mut(),
            gpu_buffer: std::ptr::null_mut(),
            device: device,
        };
        buffer.create_gpu_resource()?;
        return Ok(buffer);
    }

    pub fn update(&mut self, data: Vec<T>) -> Result<(), DxError> {
        if self.data.len() != data.len() {
            // return Err(DxError::new(
            //     "Sizes differ. Currently unsupported",
            //     DxErrorType::ResourceUpdate,
            // ));
            unsafe {
                (*self.shader_view).Release();
                (*self.gpu_buffer).Release();
            }
            self.size = data.len() * std::mem::size_of::<T>();
            self.data = data;
            return self.create_gpu_resource()
        }
        self.data = data;
        let mut mapped: dx11::D3D11_MAPPED_SUBRESOURCE = Default::default();
        let res = unsafe {
            (*self.context).Map(
                self.gpu_buffer as *mut _,
                0,
                dx11::D3D11_MAP_WRITE_DISCARD,
                0,
                &mut mapped as *mut _,
            )
        };
        if res < winapi::shared::winerror::S_OK {
            return Err(DxError::new(
                "SBuffer update failed",
                DxErrorType::ResourceUpdate,
            ));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.data.as_ptr() as *const T,
                mapped.pData as *mut T,
                self.data.len(),
            );
            (*self.context).Unmap(self.gpu_buffer as *mut _, 0);
        };
        Ok(())
    }

    pub fn buffer_ptr(&self) -> *mut dx11::ID3D11Buffer {
        self.gpu_buffer
    }

    pub fn shader_view(&self) -> *mut dx11::ID3D11ShaderResourceView {
        self.shader_view
    }

    fn create_gpu_resource(&mut self) -> Result<(), DxError> {
        let mut desc: dx11::D3D11_BUFFER_DESC = Default::default();
        desc.ByteWidth = self.size as u32;
        desc.StructureByteStride = std::mem::size_of::<T>() as u32;
        desc.Usage = dx11::D3D11_USAGE_DYNAMIC;
        desc.BindFlags = dx11::D3D11_BIND_SHADER_RESOURCE;
        desc.MiscFlags = dx11::D3D11_RESOURCE_MISC_BUFFER_STRUCTURED;
        desc.CPUAccessFlags = dx11::D3D11_CPU_ACCESS_WRITE;
        let mut initial_data: dx11::D3D11_SUBRESOURCE_DATA = Default::default();
        initial_data.pSysMem = self.data.as_ptr() as *const _;
        let res = unsafe {
            (*self.device).CreateBuffer(&desc, &initial_data, &mut self.gpu_buffer as *mut *mut _)
        };
        if res < winapi::shared::winerror::S_OK {
            return Err(DxError::new(
                "SBuffer create failed",
                DxErrorType::ResourceCreation,
            ));
        }
        let mut desc = dx11::D3D11_SHADER_RESOURCE_VIEW_DESC::default();
        desc.ViewDimension = d3d::D3D11_SRV_DIMENSION_BUFFER;
        desc.Format = winapi::shared::dxgiformat::DXGI_FORMAT_UNKNOWN;
        unsafe {
            let buffer_desc = desc.u.Buffer_mut();
            *buffer_desc.u1.FirstElement_mut() = 0;
            *buffer_desc.u2.NumElements_mut() = self.data.len() as u32;

            let res = (*self.device).CreateShaderResourceView(
                self.gpu_buffer as *mut _,
                &desc,
                &mut self.shader_view as *mut *mut _,
            );
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "SBuffer shader view create failed",
                    DxErrorType::ResourceCreation,
                ));
            }
        }
        Ok(())
    }
}
