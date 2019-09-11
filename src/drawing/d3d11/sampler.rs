use crate::drawing::d3d11::backend::{DxError, DxErrorType};

use winapi::shared::dxgiformat as fmt;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use image::ColorType;
use image::ImageBuffer;
use image::Pixel;

pub struct Texture2D {
    sampler: *mut dx11::ID3D11SamplerState,
    handle: *mut dx11::ID3D11Texture2D,
    shader_view: *mut dx11::ID3D11ShaderResourceView,
}

impl Texture2D {
    pub fn create_from_image<P: Pixel, C: std::ops::DerefMut>(
        image: &ImageBuffer<P, C>,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<Texture2D, DxError> {
        // fucking bullshit that the subtype is private...
        /*
        let format = match Pixel::Subtype {
            u8 => {
                match channels {
                    1 => R8 UNORM,
                    2 => RG8 UNORM,
                    ...
                }
            },
            f32 => {
                match channels {
                    1 => R32 FLOAT,
                    2 => RG32 FLOAT,
                    ...
                }
            },
            _ => Err(Unsupported),
        };
        */
        Err(DxError::new("Unimplemented", DxErrorType::Generic))
    }
    pub fn create_empty_mutable(
        format: u32,
        miplevels: u32,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<Texture2D, DxError> {
        Texture2D::create(format, miplevels, device, std::ptr::null())
    }

    fn create(
        format: u32,
        miplevels: u32,
        device: *mut dx11_1::ID3D11Device1,
        image: *const dx11::D3D11_SUBRESOURCE_DATA,
    ) -> Result<Texture2D, DxError> {
        let mut tex = Texture2D {
            sampler: std::ptr::null_mut(),
            handle: std::ptr::null_mut(),
            shader_view: std::ptr::null_mut(),
        };
        {
            let mut desc: dx11::D3D11_SAMPLER_DESC = Default::default();
            desc.Filter = dx11::D3D11_FILTER_MIN_MAG_MIP_LINEAR;
            desc.AddressU = dx11::D3D11_TEXTURE_ADDRESS_WRAP;
            desc.AddressV = dx11::D3D11_TEXTURE_ADDRESS_WRAP;
            desc.AddressW = dx11::D3D11_TEXTURE_ADDRESS_WRAP;
            desc.ComparisonFunc = dx11::D3D11_COMPARISON_NEVER;
            desc.MinLOD = 0.0f32;
            desc.MaxLOD = dx11::D3D11_FLOAT32_MAX;
            let res =
                unsafe { (*device).CreateSamplerState(&desc, &mut tex.sampler as *mut *mut _) };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Sampler creation failed",
                    DxErrorType::ResourceCreation,
                ));
            }
        }
        {
            let usage = match image {
                i if i.is_null() => dx11::D3D11_USAGE_DYNAMIC,
                _ => dx11::D3D11_USAGE_IMMUTABLE,
            };
            let mut desc: dx11::D3D11_TEXTURE2D_DESC = Default::default();
            desc.MipLevels = miplevels;
            desc.ArraySize = 1;
            desc.Format = format;
            desc.SampleDesc.Count = 1;
            desc.Usage = usage;
            desc.BindFlags = dx11::D3D11_BIND_SHADER_RESOURCE;

            let res =
                unsafe { (*device).CreateTexture2D(&desc, image, &mut tex.handle as *mut *mut _) };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Texture creation failed",
                    DxErrorType::ResourceCreation,
                ));
            }
            let res = unsafe {
                (*device).CreateShaderResourceView(
                    tex.handle as *mut _,
                    std::ptr::null(),
                    &mut tex.shader_view as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "ShaderView creation failed",
                    DxErrorType::ResourceCreation,
                ));
            }
        }
        Ok(tex)
    }
}
