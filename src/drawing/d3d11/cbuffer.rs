use crate::drawing::d3d11::backend::{DxError, DxErrorType};

use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

pub struct CBuffer<T> {
    pub data: T,
    size: usize,
    gpu_buffer: *mut dx11::ID3D11Buffer,
    context: *mut dx11_1::ID3D11DeviceContext1,
}

impl<T> CBuffer<T> {
    pub fn create(
        data: T,
        context: *mut dx11_1::ID3D11DeviceContext1,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<CBuffer<T>, DxError> {
        let mut buffer = CBuffer {
            data: data,
            size: std::mem::size_of::<T>(),
            context: context,
            gpu_buffer: std::ptr::null_mut(),
        };
        buffer.create_gpu_resource(device)?;
        return Ok(buffer);
    }

    pub fn update(&mut self) -> Result<(), DxError> {
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
                "CBuffer update failed",
                DxErrorType::ResourceUpdate,
            ));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(&self.data as *const T, mapped.pData as *mut T, 1);
            (*self.context).Unmap(self.gpu_buffer as *mut _, 0);
        };
        Ok(())
    }

    pub fn buffer_ptr(&self) -> *mut dx11::ID3D11Buffer {
        self.gpu_buffer
    }

    fn create_gpu_resource(&mut self, device: *mut dx11_1::ID3D11Device1) -> Result<(), DxError> {
        let mut desc: dx11::D3D11_BUFFER_DESC = Default::default();
        desc.ByteWidth = self.size as u32;
        desc.Usage = dx11::D3D11_USAGE_DYNAMIC;
        desc.BindFlags = dx11::D3D11_BIND_CONSTANT_BUFFER;
        desc.CPUAccessFlags = dx11::D3D11_CPU_ACCESS_WRITE;
        let mut initial_data: dx11::D3D11_SUBRESOURCE_DATA = Default::default();
        initial_data.pSysMem = (&self.data as *const T) as *const _;
        let res = unsafe {
            (*device).CreateBuffer(&desc, &initial_data, &mut self.gpu_buffer as *mut *mut _)
        };
        if res < winapi::shared::winerror::S_OK {
            return Err(DxError::new(
                "CBuffer create failed",
                DxErrorType::ResourceCreation,
            ));
        }
        Ok(())
    }
}
