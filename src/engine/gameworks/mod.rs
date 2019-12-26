use winapi::shared::dxgiformat as dxgifmt;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

#[allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
pub(crate) mod hbao_plus;

use crate::engine::d3d11::{textures, DxError, DxErrorType};

pub struct SSAO {
    backend: hbao_plus::HBAO,
    render_target: textures::Texture2D,
    render_target_view: *mut dx11::ID3D11RenderTargetView,
}

impl SSAO {
    pub fn get_render_target(&self) -> &textures::Texture2D {
        &self.render_target
    }
    pub fn get_render_target_view(&self) -> *mut dx11::ID3D11RenderTargetView {
        self.render_target_view
    }
    pub fn new(
        (res_x, res_y): (u32, u32),
        depth_view: *mut dx11::ID3D11ShaderResourceView,
        projection: glm::Mat4,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<SSAO, DxError> {
        let mut proj = [0.0f32; 16];
        let mut idx = 0;
        for i in 0..4 {
            idx = 4 * i;
            let col = projection.column(i);
            proj[idx] = col[0];
            proj[idx + 1] = col[1];
            proj[idx + 2] = col[2];
            proj[idx + 3] = col[3];
        }

        // render target for ssao out
        let mut tv: *mut dx11::ID3D11RenderTargetView = std::ptr::null_mut();
        let mut tex = textures::Texture2D::create_mutable_render_target(
            res_x,
            res_y,
            dxgifmt::DXGI_FORMAT_R32G32B32A32_FLOAT,
            dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
            dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
            dx11::D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            1,
            dx11::D3D11_BIND_RENDER_TARGET | dx11::D3D11_BIND_SHADER_RESOURCE,
            0,
            0,
            device,
        )?;
        {
            let mut dt_desc: dx11::D3D11_RENDER_TARGET_VIEW_DESC = Default::default();
            dt_desc.Format = dxgifmt::DXGI_FORMAT_R32G32B32A32_FLOAT;
            dt_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            unsafe { dt_desc.u.Texture2D_mut().MipSlice = 0 };
            let res = unsafe {
                (*device).CreateRenderTargetView(
                    tex.get_texture_handle() as *mut _,
                    &dt_desc,
                    &mut tv as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Error creating depth target view for texture",
                    DxErrorType::ResourceCreation,
                ));
            }
            let mut pos_tar_rv_desc: dx11::D3D11_SHADER_RESOURCE_VIEW_DESC = Default::default();
            pos_tar_rv_desc.Format = dxgifmt::DXGI_FORMAT_R32G32B32A32_FLOAT;
            pos_tar_rv_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            unsafe {
                pos_tar_rv_desc.u.Texture2D_mut().MostDetailedMip = 0;
                pos_tar_rv_desc.u.Texture2D_mut().MipLevels = 1;
            };
            let res = unsafe {
                (*device).CreateShaderResourceView(
                    tex.get_texture_handle() as *mut _,
                    &pos_tar_rv_desc as *const _,
                    &mut tex.shader_view as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Error creating depth shader view for texture",
                    DxErrorType::ResourceCreation,
                ));
            }
        }
        let hbao = unsafe {
            hbao_plus::HBAO::new(
                device as *mut _,
                depth_view as *mut _,
                tv as *mut _,
                proj.as_mut_ptr(),
            )
        };

        Ok(SSAO {
            backend: hbao,
            render_target: tex,
            render_target_view: tv,
        })
    }
    pub fn render(&mut self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        let status = unsafe { self.backend.RenderAO(ctx as *mut _) };
        if status != hbao_plus::GFSDK_SSAO_Status_GFSDK_SSAO_OK {
            panic!("GFSDK returned {}", status);
        }
    }
}
