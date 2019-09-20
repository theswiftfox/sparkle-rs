use crate::drawing::d3d11::backend::{DxError, DxErrorType};

use winapi::shared::dxgiformat as fmt;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use image::ColorType;
use image::DynamicImage;
use image::GenericImageView;

pub struct Texture2D {
    sampler: *mut dx11::ID3D11SamplerState,
    handle: *mut dx11::ID3D11Texture2D,
    shader_view: *mut dx11::ID3D11ShaderResourceView,
}

impl Texture2D {
    pub fn get_sampler(&self) -> *mut dx11::ID3D11SamplerState {
        self.sampler
    }
    pub fn get_texture(&self) -> *mut dx11::ID3D11ShaderResourceView {
        self.shader_view
    }

    pub fn create_from_image(
        image: DynamicImage,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<Texture2D, DxError> {
        let mut img: (
            (Option<image::RgbaImage>, fmt::DXGI_FORMAT),
            (Option<image::GrayImage>, fmt::DXGI_FORMAT),
        ) = ((None, 0), (None, 0));
        let mut data: dx11::D3D11_SUBRESOURCE_DATA = Default::default();
        if let Some(_) = match image.color() {
            ColorType::Gray(dtype) => {
                let format = fmt::DXGI_FORMAT_R8_UNORM;
                /*match typeof(dtype) {
                    u8 => fmt::DXGI_FORMAT_R8_UNORM,
                    u16 => fmt::DXGI_FORMAT_D16_UNORM,
                    f32 => fmt::DXGI_FORMAT_D32_FLOAT,
                    _ => return Err(DxError::new("Invalid format for grayscale texture", DxErrorType::Generic))
                };
                TODO: other datatpyes?
                */
                img.1 = (Some(image.to_luma()), format);
                data.SysMemPitch = image.dimensions().0 * std::mem::size_of_val(&dtype) as u32;
                Some(())
            }
            ColorType::BGR(dtype)
            | ColorType::BGRA(dtype)
            | ColorType::RGB(dtype)
            | ColorType::RGBA(dtype) => {
                let format = fmt::DXGI_FORMAT_R8G8B8A8_UNORM;
                /* match dtype {
                    u8 => fmt::DXGI_FORMAT_R8G8B8A8_UNORM,
                    u16 => fmt::DXGI_FORMAT_R16G16B16A16_UINT,
                    u32 => fmt::DXGI_FORMAT_R32G32B32A32_UINT,
                    f32 => fmt::DXGI_FORMAT_R32G32B32A32_FLOAT,
                };
                */
                img.0 = (Some(image.to_rgba()), format);
                data.SysMemPitch = image.dimensions().0 * std::mem::size_of_val(&dtype) as u32;
                Some(())
            }
            _ => None,
        } {
            match img {
                ((Some(rgba), fmt), (None, _)) => {
                    let (width, height) = rgba.dimensions();
                    let rgba_data = rgba.into_raw();
                    data.pSysMem = rgba_data.as_ptr() as *const _;
                    return Texture2D::create(fmt, 1, width, height, device, &data as *const _);
                }
                ((None, _), (Some(gray), fmt)) => {
                    let (width, height) = gray.dimensions();
                    let gray_data = gray.into_raw();
                    data.pSysMem = gray_data.as_ptr() as *const _;
                    return Texture2D::create(fmt, 1, width, height, device, &data as *const _);
                }
                _ => {
                    return Err(DxError::new(
                        "Unable to read image data",
                        DxErrorType::Generic,
                    ))
                }
            }
        } else {
            Err(DxError::new("Unimplemented", DxErrorType::Generic))
        }
    }
    pub fn create_empty_mutable(
        format: u32,
        miplevels: u32,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<Texture2D, DxError> {
        Texture2D::create(format, miplevels, 0, 0, device, std::ptr::null())
    }

    fn create(
        format: u32,
        miplevels: u32,
        width: u32,
        height: u32,
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
            let mut desc: dx11::D3D11_TEXTURE2D_DESC = Default::default();
            match image {
                i if i.is_null() => {
                    desc.Usage = dx11::D3D11_USAGE_DYNAMIC;
                }
                _ => {
                    desc.Usage = dx11::D3D11_USAGE_IMMUTABLE;
                    desc.Width = width;
                    desc.Height = height;
                }
            };
            desc.MipLevels = miplevels;
            desc.ArraySize = 1;
            desc.Format = format;
            desc.SampleDesc.Count = 1;
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
