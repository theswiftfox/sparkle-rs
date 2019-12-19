use super::{DxError, DxErrorType};

use winapi::shared::dxgiformat as fmt;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use image::ColorType;
use image::DynamicImage;
use image::GenericImageView;

pub struct Texture2D {
    pub format: u32,
    sampler: *mut dx11::ID3D11SamplerState,
    handle: *mut dx11::ID3D11Texture2D,
    pub shader_view: *mut dx11::ID3D11ShaderResourceView,
}

impl Texture2D {
    pub fn get_sampler(&self) -> *mut dx11::ID3D11SamplerState {
        self.sampler
    }
    pub fn get_texture_view(&self) -> *mut dx11::ID3D11ShaderResourceView {
        self.shader_view
    }

    pub fn get_texture_handle(&self) -> *mut dx11::ID3D11Texture2D {
        self.handle
    }

    pub fn create_from_image_obj(
        image: DynamicImage,
        address_u: u32,
        address_v: u32,
        filter: u32,
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<Texture2D, DxError> {
        let mut img: (
            (Option<image::RgbaImage>, fmt::DXGI_FORMAT),
            (Option<image::GrayImage>, fmt::DXGI_FORMAT),
        ) = ((None, 0), (None, 0));
        let mut data: dx11::D3D11_SUBRESOURCE_DATA = Default::default();
        if let Some(_) = match image.color() {
            ColorType::Gray(_) => {
                let format = fmt::DXGI_FORMAT_R8_UNORM;
                img.1 = (Some(image.to_luma()), format);
                data.SysMemPitch = image.dimensions().0 * 4; //std::mem::size_of_val(&dtype) as u32;
                Some(())
            }
            ColorType::BGR(_) | ColorType::BGRA(_) | ColorType::RGB(_) | ColorType::RGBA(_) => {
                let format = fmt::DXGI_FORMAT_R8G8B8A8_UNORM;
                img.0 = (Some(image.to_rgba()), format);
                data.SysMemPitch = image.dimensions().0 * 4; //std::mem::size_of_val(&dtype) as u32;
                Some(())
            }
            _ => None,
        } {
            match img {
                ((Some(rgba), fmt), (None, _)) => {
                    let (width, height) = rgba.dimensions();
                    let rgba_data = rgba.into_raw();
                    data.pSysMem = rgba_data.as_ptr() as *const _;
                    let mut tex = Texture2D::create(
                        width,
                        height,
                        fmt,
                        address_u,
                        address_v,
                        filter,
                        0,
                        dx11::D3D11_BIND_SHADER_RESOURCE,
                        dx11::D3D11_USAGE_DEFAULT,
                        0,
                        device,
                        context,
                        &data as *const _,
                    )?;
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
                    unsafe { (*context).GenerateMips(tex.shader_view) };
                    return Ok(tex);
                }
                ((None, _), (Some(gray), fmt)) => {
                    let (width, height) = gray.dimensions();
                    let gray_data = gray.into_raw();
                    data.pSysMem = gray_data.as_ptr() as *const _;
                    let mut tex = Texture2D::create(
                        width,
                        height,
                        fmt,
                        address_u,
                        address_v,
                        filter,
                        0,
                        dx11::D3D11_BIND_SHADER_RESOURCE,
                        dx11::D3D11_USAGE_DEFAULT,
                        0,
                        device,
                        context,
                        &data as *const _,
                    )?;
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
                    unsafe { (*context).GenerateMips(tex.shader_view) };
                    return Ok(tex);
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
    pub fn create_from_image_data(
        image_data: &Vec<u8>,
        width: u32,
        height: u32,
        format: winapi::shared::dxgiformat::DXGI_FORMAT,
        channels: u32,
        address_u: u32,
        address_v: u32,
        filter: u32,
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<Texture2D, DxError> {
        let mut data: dx11::D3D11_SUBRESOURCE_DATA = Default::default();
        data.pSysMem = image_data.as_ptr() as *const _;
        data.SysMemPitch = width * channels;
        let mut tex = Texture2D::create(
            width,
            height,
            format,
            address_u,
            address_v,
            filter,
            0,
            dx11::D3D11_BIND_SHADER_RESOURCE,
            dx11::D3D11_USAGE_DEFAULT,
            0,
            device,
            context,
            &data as *const _,
        )?;
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
        unsafe { (*context).GenerateMips(tex.shader_view) };
        return Ok(tex);
    }

    pub fn create_empty_mutable(
        width: u32,
        height: u32,
        format: u32,
        address_u: u32,
        address_v: u32,
        filter: u32,
        miplevels: u32,
        bind_flags: u32,
        usage: u32,
        sampler_type: u32,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<Texture2D, DxError> {
        Texture2D::create(
            width,
            height,
            format,
            address_u,
            address_v,
            filter,
            miplevels,
            bind_flags,
            usage,
            sampler_type,
            device,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    }
    pub fn create_mutable_render_target(
        width: u32,
        height: u32,
        format: u32,
        address_u: u32,
        address_v: u32,
        filter: u32,
        miplevels: u32,
        bind_flags: u32,
        sampler_type: u32,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<Texture2D, DxError> {
        Texture2D::create(
            width,
            height,
            format,
            address_u,
            address_v,
            filter,
            miplevels,
            bind_flags,
            dx11::D3D11_USAGE_DEFAULT,
            sampler_type,
            device,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    }

    fn create(
        width: u32,
        height: u32,
        format: u32,
        address_u: u32,
        address_v: u32,
        filter: u32,
        miplevels: u32,
        bind_flags: u32,
        usage: u32,
        sampler_type: u32,
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
        image: *const dx11::D3D11_SUBRESOURCE_DATA,
    ) -> Result<Texture2D, DxError> {
        let mut tex = Texture2D {
            format: format,
            sampler: std::ptr::null_mut(),
            handle: std::ptr::null_mut(),
            shader_view: std::ptr::null_mut(),
        };
        if bind_flags & dx11::D3D11_BIND_SHADER_RESOURCE == dx11::D3D11_BIND_SHADER_RESOURCE {
            //is_shader_resource
            let mut desc: dx11::D3D11_SAMPLER_DESC = Default::default();
            desc.Filter = filter;
            if filter == dx11::D3D11_FILTER_ANISOTROPIC {
                desc.MaxAnisotropy = 16; // todo: settings?
            }
            desc.AddressU = address_u;
            desc.AddressV = address_v;
            desc.AddressW = address_v;
            desc.ComparisonFunc = match sampler_type {
                1 => dx11::D3D11_COMPARISON_LESS,
                _ => dx11::D3D11_COMPARISON_NEVER,
            };
            desc.MinLOD = 0.0f32;
            desc.MaxLOD = dx11::D3D11_FLOAT32_MAX;
            let res =
                unsafe { (*device).CreateSamplerState(&desc, &mut tex.sampler as *mut *mut _) };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    format!(
                        "Sampler creation failed\n ModeU: {}, ModeV: {}, Filter: {}",
                        address_u, address_v, filter
                    )
                    .as_str(),
                    DxErrorType::ResourceCreation,
                ));
            }
        }
        {
            let mut desc: dx11::D3D11_TEXTURE2D_DESC = Default::default();
            desc.Usage = usage;
            desc.Width = width;
            desc.Height = height;
            desc.MipLevels = miplevels;
            desc.ArraySize = 1;
            desc.Format = format;
            desc.SampleDesc.Count = 1;
            desc.BindFlags = bind_flags;
            if miplevels == 0 {
                desc.MiscFlags = dx11::D3D11_RESOURCE_MISC_GENERATE_MIPS;
                desc.BindFlags = desc.BindFlags | dx11::D3D11_BIND_RENDER_TARGET;
                let res = unsafe {
                    (*device).CreateTexture2D(
                        &desc,
                        std::ptr::null(),
                        &mut tex.handle as *mut *mut _,
                    )
                };
                if res < winapi::shared::winerror::S_OK {
                    return Err(DxError::new(
                        "Texture creation failed",
                        DxErrorType::ResourceCreation,
                    ));
                }
                unsafe {
                    (*context).UpdateSubresource(
                        tex.handle as *mut _,
                        0,
                        std::ptr::null(),
                        (*image).pSysMem as *mut _,
                        (*image).SysMemPitch,
                        (*image).SysMemPitch * height,
                    )
                };
            } else {
                let res = unsafe {
                    (*device).CreateTexture2D(&desc, image, &mut tex.handle as *mut *mut _)
                };
                if res < winapi::shared::winerror::S_OK {
                    return Err(DxError::new(
                        "Texture creation failed",
                        DxErrorType::ResourceCreation,
                    ));
                }
            }
        }
        Ok(tex)
    }
}
