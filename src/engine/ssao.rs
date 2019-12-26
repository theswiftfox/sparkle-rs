use rand::{
    distributions::{Distribution, Uniform},
    thread_rng, Rng,
};
use winapi::shared::dxgiformat as dxfmt;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use crate::engine::d3d11::{
    cbuffer::CBuffer, shaders::ShaderProgram, textures::Texture2D, DxError, DxErrorType,
};

struct SSAOUniforms {
    kernel: [glm::Vec4; 16],
    proj: glm::Mat4,
    resolution: glm::Vec2,
    pad: glm::Vec2,
}
pub struct SSAO {
    program: ShaderProgram,
    cbuffer: CBuffer<SSAOUniforms>,
    noise: Texture2D,
    render_target: Texture2D,
    render_target_view: *mut dx11::ID3D11RenderTargetView,
}

fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + f * (b - a)
}

impl SSAO {
    pub fn prepare_draw(&mut self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        self.program.activate();
        unsafe {
            (*ctx).PSSetConstantBuffers(0, 1, &self.cbuffer.buffer_ptr() as *const *mut _);
        };
    }
    pub fn render_target_view(&self) -> *mut dx11::ID3D11RenderTargetView {
        self.render_target_view
    }
    pub fn render_target(&self) -> &Texture2D {
        &self.render_target
    }
    pub fn ssao_noise(&self) -> &Texture2D {
        &self.noise
    }
    pub fn set_proj(&mut self, proj: glm::Mat4, instant_update: bool) -> Result<(), DxError> {
        self.cbuffer.data.proj = proj;
        if instant_update {
            self.cbuffer.update()?
        }
        Ok(())
    }

    pub fn update(&mut self) -> Result<(), DxError> {
        self.cbuffer.update()
    }

    pub fn new(
        (width, height): (u32, u32),
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<SSAO, DxError> {
        let vtx_shader = "deferred_light_vertex.cso";
        let ps_shader = "ssao_pixel.cso";
        let mut buffer_data = SSAOUniforms {
            kernel: [glm::zero(); 16],
            proj: glm::identity(),
            resolution: glm::vec2(width as f32, height as f32),
            pad: glm::zero(),
        };
        let range = Uniform::new(0.0, 1.0);
        let mut rng = thread_rng();
        for i in 0..16 {
            let f = (i as f32) / 16.0;
            let scale = lerp(0.1, 1.0, f * f);
            let sample = glm::vec4(
                range.sample(&mut rng) * 2.0 - 1.0,
                range.sample(&mut rng) * 2.0 - 1.0,
                range.sample(&mut rng), // * 2.0 - 1.0,
                0.0,
            )
            .normalize()
                * scale;
            buffer_data.kernel[i] = sample;
        }
        let cbuf = CBuffer::create(buffer_data, context, device)?;

        let mut noise = Vec::<glm::Vec3>::new();
        for _ in 0..64 {
            let n = glm::vec3(
                range.sample(&mut rng) * 2.0 - 1.0,
                range.sample(&mut rng) * 2.0 - 1.0,
                0.0,
            )
            .normalize();
            noise.push(n);
        }

        // let mut noise = Vec::<glm::Vec4>::new();
        // for _ in 0..256 {
        //     for _ in 0..256 {
        //         let sample = glm::vec4(
        //             rng.gen_range(0.0, 1.0) * 2.0 - 1.0,
        //             rng.gen_range(0.0, 1.0) * 2.0 - 1.0,
        //             rng.gen_range(0.0, 1.0) * 2.0 - 1.0,
        //             0.0,
        //         );
        //         //.normalize();
        //         noise.push(sample);
        //     }
        // }
        let mut data: dx11::D3D11_SUBRESOURCE_DATA = Default::default();
        data.pSysMem = noise.as_ptr() as *const _;
        data.SysMemPitch = 8 * 12;
        let mut tex = Texture2D::create(
            8,
            8,
            dxfmt::DXGI_FORMAT_R32G32B32_FLOAT,
            dx11::D3D11_TEXTURE_ADDRESS_WRAP,
            dx11::D3D11_TEXTURE_ADDRESS_WRAP,
            dx11::D3D11_FILTER_MIN_MAG_MIP_POINT,
            1,
            dx11::D3D11_BIND_SHADER_RESOURCE,
            0,
            dx11::D3D11_USAGE_IMMUTABLE,
            0,
            device,
            context,
            &data,
            1,
        )?;
        // unsafe {
        //     (*context).UpdateSubresource(
        //         tex.get_texture_handle() as *mut _,
        //         0,
        //         std::ptr::null(),
        //         noise.as_ptr() as *const _,
        //         256,
        //         256 * 4,
        //     )
        // };
        let res = unsafe {
            (*device).CreateShaderResourceView(
                tex.get_texture_handle() as *mut _,
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

        // buffer_data.kernel[0] = glm::vec4(1.0, 1.0, 1.0, 0.0);
        // buffer_data.kernel[1] = glm::vec4(-1.0, -1.0, -1.0, 0.0);

        // buffer_data.kernel[2] = glm::vec4(-1.0, 1.0, 1.0, 0.0);
        // buffer_data.kernel[3] = glm::vec4(1.0, -1.0, -1.0, 0.0);

        // buffer_data.kernel[4] = glm::vec4(1.0, 1.0, -1.0, 0.0);
        // buffer_data.kernel[5] = glm::vec4(-1.0, -1.0, 1.0, 0.0);

        // buffer_data.kernel[6] = glm::vec4(-1.0, 1.0, -1.0, 0.0);
        // buffer_data.kernel[7] = glm::vec4(1.0, -1.0, 1.0, 0.0);

        // buffer_data.kernel[8] = glm::vec4(-1.0, 0.0, 0.0, 0.0);
        // buffer_data.kernel[9] = glm::vec4(1.0, 0.0, 0.0, 0.0);

        // buffer_data.kernel[10] = glm::vec4(0.0, -1.0, 0.0, 0.0);
        // buffer_data.kernel[11] = glm::vec4(0.0, 1.0, 0.0, 0.0);

        // buffer_data.kernel[12] = glm::vec4(0.0, 0.0, -1.0, 0.0);
        // buffer_data.kernel[13] = glm::vec4(0.0, 0.0, 1.0, 0.0);

        // //let range = Uniform::new(0.25, 1.0);
        // for i in 0..14 {
        //     buffer_data.kernel[i] = rng.gen_range(0.25, 1.0) * buffer_data.kernel[i].normalize();
        // }

        // let cbuf = CBuffer::create(buffer_data, context, device)?;

        let mut ssao_view: *mut dx11::ID3D11RenderTargetView = std::ptr::null_mut();
        let mut ssao_target = Texture2D::create_mutable_render_target(
            width,
            height,
            dxfmt::DXGI_FORMAT_R32G32B32A32_FLOAT,
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
            let mut rt_desc: dx11::D3D11_RENDER_TARGET_VIEW_DESC = Default::default();
            rt_desc.Format = dxfmt::DXGI_FORMAT_R32G32B32A32_FLOAT;
            rt_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            unsafe { rt_desc.u.Texture2D_mut().MipSlice = 0 };
            let res = unsafe {
                (*device).CreateRenderTargetView(
                    ssao_target.get_texture_handle() as *mut _,
                    &rt_desc,
                    &mut ssao_view as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Error creating depth target view for texture",
                    DxErrorType::ResourceCreation,
                ));
            }
            let mut ssao_shader_view: dx11::D3D11_SHADER_RESOURCE_VIEW_DESC = Default::default();
            ssao_shader_view.Format = dxfmt::DXGI_FORMAT_R32G32B32A32_FLOAT;
            ssao_shader_view.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            unsafe {
                ssao_shader_view.u.Texture2D_mut().MostDetailedMip = 0;
                ssao_shader_view.u.Texture2D_mut().MipLevels = 1;
            };
            let res = unsafe {
                (*device).CreateShaderResourceView(
                    ssao_target.get_texture_handle() as *mut _,
                    &ssao_shader_view as *const _,
                    &mut ssao_target.shader_view as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Error creating shader view for texture",
                    DxErrorType::ResourceCreation,
                ));
            }
        }

        Ok(SSAO {
            program: ShaderProgram::create(&vtx_shader, &ps_shader, None, None, device, context)?,
            cbuffer: cbuf,
            noise: tex,
            render_target: ssao_target,
            render_target_view: ssao_view,
        })
    }
}
