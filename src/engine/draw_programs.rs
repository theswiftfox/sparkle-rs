use winapi::shared::dxgiformat as dxgifmt;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use super::d3d11::{cbuffer, shaders, textures, DxError, DxErrorType};
use super::geometry::{Light, LightType};

pub(crate) fn vertex_input_desc() -> [dx11::D3D11_INPUT_ELEMENT_DESC; 5] {
    let pos_name: &'static std::ffi::CStr = const_cstr!("SV_Position").as_cstr();
    let norm_name: &'static std::ffi::CStr = const_cstr!("NORMAL").as_cstr();
    let tang_name: &'static std::ffi::CStr = const_cstr!("TANGENT").as_cstr();
    let bitang_name: &'static std::ffi::CStr = const_cstr!("BITANGENT").as_cstr();
    let uv_name: &'static std::ffi::CStr = const_cstr!("TEXCOORD").as_cstr();
    [
        dx11::D3D11_INPUT_ELEMENT_DESC {
            SemanticName: pos_name.as_ptr() as *const _,
            SemanticIndex: 0,
            Format: dxgifmt::DXGI_FORMAT_R32G32B32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: dx11::D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        dx11::D3D11_INPUT_ELEMENT_DESC {
            SemanticName: norm_name.as_ptr() as *const _,
            SemanticIndex: 0,
            Format: dxgifmt::DXGI_FORMAT_R32G32B32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: dx11::D3D11_APPEND_ALIGNED_ELEMENT,
            InputSlotClass: dx11::D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        dx11::D3D11_INPUT_ELEMENT_DESC {
            SemanticName: tang_name.as_ptr() as *const _,
            SemanticIndex: 0,
            Format: dxgifmt::DXGI_FORMAT_R32G32B32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: dx11::D3D11_APPEND_ALIGNED_ELEMENT,
            InputSlotClass: dx11::D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        dx11::D3D11_INPUT_ELEMENT_DESC {
            SemanticName: bitang_name.as_ptr() as *const _,
            SemanticIndex: 0,
            Format: dxgifmt::DXGI_FORMAT_R32G32B32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: dx11::D3D11_APPEND_ALIGNED_ELEMENT,
            InputSlotClass: dx11::D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        dx11::D3D11_INPUT_ELEMENT_DESC {
            SemanticName: uv_name.as_ptr() as *const _,
            SemanticIndex: 0,
            Format: dxgifmt::DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: dx11::D3D11_APPEND_ALIGNED_ELEMENT,
            InputSlotClass: dx11::D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
    ]
}

fn create_render_target(
    (width, height): (u32, u32),
    format: u32,
    device: *mut dx11_1::ID3D11Device1,
) -> Result<(textures::Texture2D, *mut dx11::ID3D11RenderTargetView), DxError> {
    let mut tv: *mut dx11::ID3D11RenderTargetView = std::ptr::null_mut();
    let mut tex = textures::Texture2D::create_mutable_render_target(
        width,
        height,
        format,
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
        let mut desc: dx11::D3D11_RENDER_TARGET_VIEW_DESC = Default::default();
        desc.Format = format;
        desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
        unsafe { desc.u.Texture2D_mut().MipSlice = 0 };
        let res = unsafe {
            (*device).CreateRenderTargetView(
                tex.get_texture_handle() as *mut _,
                &desc,
                &mut tv as *mut *mut _,
            )
        };
        if res < winapi::shared::winerror::S_OK {
            return Err(DxError::new(
                "Error creating depth target view for texture",
                DxErrorType::ResourceCreation,
            ));
        }
        let mut rv_desc: dx11::D3D11_SHADER_RESOURCE_VIEW_DESC = Default::default();
        rv_desc.Format = format;
        rv_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
        unsafe {
            rv_desc.u.Texture2D_mut().MostDetailedMip = 0;
            rv_desc.u.Texture2D_mut().MipLevels = 1;
        };
        let res = unsafe {
            (*device).CreateShaderResourceView(
                tex.get_texture_handle() as *mut _,
                &rv_desc as *const _,
                &mut tex.shader_view as *mut *mut _,
            )
        };
        if res < winapi::shared::winerror::S_OK {
            return Err(DxError::new(
                "Error creating depth shader view for texture",
                DxErrorType::ResourceCreation,
            ));
        }
        Ok((tex, tv))
    }
}

/**
 * Section ForwardPass
 */
struct ConstantsVtxMP {
    pub view: glm::Mat4,
    pub proj: glm::Mat4,
}

pub(crate) struct ForwardPass {
    program: shaders::ShaderProgram,
    vertex_shader_uniforms: cbuffer::CBuffer<ConstantsVtxMP>,
    pixel_shader_uniforms: cbuffer::CBuffer<ConstantsDefLight>,
    light_buffer: cbuffer::CBuffer<DxLight>,
    render_target: textures::Texture2D,
    render_target_view: *mut dx11::ID3D11RenderTargetView,
}
impl ForwardPass {
    pub fn get_render_target(&self) -> &textures::Texture2D {
        &self.render_target
    }
    pub fn get_render_target_view(&self) -> *mut dx11::ID3D11RenderTargetView {
        self.render_target_view
    }
    pub fn prepare_draw(&mut self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        self.program.activate();

        unsafe {
            (*ctx).VSSetConstantBuffers(
                0,
                1,
                &self.vertex_shader_uniforms.buffer_ptr() as *const *mut _,
            );
            let cbuffs = [
                self.pixel_shader_uniforms.buffer_ptr(),
                self.light_buffer.buffer_ptr(),
            ];
            (*ctx).PSSetConstantBuffers(0, 2, cbuffs.as_ptr());
        };
    }

    pub fn update(&mut self) -> Result<(), DxError> {
        self.vertex_shader_uniforms.update()?;
        self.pixel_shader_uniforms.update()?;
        self.light_buffer.update()?;

        Ok(())
    }

    pub fn set_view(&mut self, view: glm::Mat4, instant_update: bool) -> Result<(), DxError> {
        self.vertex_shader_uniforms.data.view = view;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }
    pub fn set_proj(&mut self, proj: glm::Mat4, instant_update: bool) -> Result<(), DxError> {
        self.vertex_shader_uniforms.data.proj = proj;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }
    pub fn set_light(&mut self, light: &Light, instant_update: bool) -> Result<(), DxError> {
        self.light_buffer.data = DxLight::from_sparkle_light(light);
        if instant_update {
            self.light_buffer.update()?
        }
        Ok(())
    }

    pub fn set_view_proj(
        &mut self,
        view: glm::Mat4,
        proj: glm::Mat4,
        instant_update: bool,
    ) -> Result<(), DxError> {
        self.set_view(view, false)?;
        self.set_proj(proj, false)?;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn set_camera_pos(&mut self, cpos: glm::Vec3, instant_update: bool) -> Result<(), DxError> {
        self.pixel_shader_uniforms.data.camera_pos = cpos;
        if instant_update {
            self.pixel_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn set_ssao(&mut self, ssao: u32, instant_update: bool) -> Result<(), DxError> {
        self.pixel_shader_uniforms.data.ssao = ssao;
        if instant_update {
            self.pixel_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn create(
        (width, height): (u32, u32),
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<ForwardPass, DxError> {
        let vtx_shader = "mp_vertex.cso";
        let ps_shader = "mp_pixel.cso";
        let input_element_description = vertex_input_desc();

        let vtx_uniforms = ConstantsVtxMP {
            view: glm::identity(),
            proj: glm::identity(),
        };
        let pxl_uniforms = ConstantsDefLight {
            camera_pos: glm::zero(),
            ssao: 1,
        };
        let vtx_cbuff = match cbuffer::CBuffer::create(vtx_uniforms, context, device) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };
        let pxl_cbuff = match cbuffer::CBuffer::create(pxl_uniforms, context, device) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };
        let light = DxLight {
            position: glm::zero(),
            t: 0,
            color: glm::zero(),
            radius: 0.0,
            light_space: glm::identity(),
        };
        let pxl_cbuff2 = match cbuffer::CBuffer::create(light, context, device) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };

        let (tex, tv) = create_render_target(
            (width, height),
            dxgifmt::DXGI_FORMAT_R32G32B32A32_FLOAT,
            device,
        )?;

        Ok(ForwardPass {
            program: shaders::ShaderProgram::create(
                &vtx_shader,
                &ps_shader,
                None,
                Some(&input_element_description),
                device,
                context,
            )?,
            vertex_shader_uniforms: vtx_cbuff,
            pixel_shader_uniforms: pxl_cbuff,
            light_buffer: pxl_cbuff2,
            render_target: tex,
            render_target_view: tv,
        })
    }
}

/**
 * Section Deferred Renderer
 */
struct ConstantsPxDeferredPre {
    near_plane: f32,
    far_plane: f32,
    _pad: f32,
    _pad2: f32,
}
struct ConstantsVtxDeferredPre {
    view: glm::Mat4,
    proj: glm::Mat4,
}

pub(crate) struct DeferredPassPre {
    program: shaders::ShaderProgram,
    vertex_shader_uniforms: cbuffer::CBuffer<ConstantsVtxDeferredPre>,
    pixel_shader_uniforms: cbuffer::CBuffer<ConstantsPxDeferredPre>,
    positions: textures::Texture2D,
    positions_render_target: *mut dx11::ID3D11RenderTargetView,
    albedo: textures::Texture2D,
    albedo_render_target: *mut dx11::ID3D11RenderTargetView,
}
impl DeferredPassPre {
    pub fn get_render_targets(&self) -> [*mut dx11::ID3D11RenderTargetView; 2] {
        [self.positions_render_target, self.albedo_render_target]
    }

    pub fn positions(&self) -> &textures::Texture2D {
        &self.positions
    }
    pub fn albedo(&self) -> &textures::Texture2D {
        &self.albedo
    }

    pub fn prepare_draw(&mut self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        self.program.activate();

        unsafe {
            (*ctx).VSSetConstantBuffers(
                0,
                1,
                &self.vertex_shader_uniforms.buffer_ptr() as *const *mut _,
            );
            (*ctx).PSSetConstantBuffers(
                0,
                1,
                &self.pixel_shader_uniforms.buffer_ptr() as *const *mut _,
            );
        };
    }

    pub fn update(&mut self) -> Result<(), DxError> {
        self.vertex_shader_uniforms.update()?;
        self.pixel_shader_uniforms.update()?;

        Ok(())
    }

    pub fn set_view(&mut self, view: glm::Mat4, instant_update: bool) -> Result<(), DxError> {
        self.vertex_shader_uniforms.data.view = view;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }
    pub fn set_proj(&mut self, proj: glm::Mat4, instant_update: bool) -> Result<(), DxError> {
        self.vertex_shader_uniforms.data.proj = proj;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn set_view_proj(
        &mut self,
        view: glm::Mat4,
        proj: glm::Mat4,
        instant_update: bool,
    ) -> Result<(), DxError> {
        self.set_view(view, false)?;
        self.set_proj(proj, false)?;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn set_camera_planes(
        &mut self,
        (near, far): (f32, f32),
        instant_update: bool,
    ) -> Result<(), DxError> {
        self.pixel_shader_uniforms.data.near_plane = near;
        self.pixel_shader_uniforms.data.far_plane = far;
        if instant_update {
            self.pixel_shader_uniforms.update()?
        }
        Ok(())
    }
    pub fn create(
        (res_x, res_y): (u32, u32),
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<DeferredPassPre, DxError> {
        let vtx_shader = "deferred_pre_vertex.cso";
        let ps_shader = "deferred_pre_pixel_packing.cso";
        let input_element_description = vertex_input_desc();

        let vtx_uniforms = ConstantsVtxDeferredPre {
            view: glm::identity(),
            proj: glm::identity(),
        };
        let pxl_uniforms = ConstantsPxDeferredPre {
            near_plane: 0.0f32,
            far_plane: 0.0f32,
            _pad: 0.0f32,
            _pad2: 0.0f32,
        };
        let vtx_cbuff = match cbuffer::CBuffer::create(vtx_uniforms, context, device) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };
        let pxl_cbuff = match cbuffer::CBuffer::create(pxl_uniforms, context, device) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };

        // render target positions
        let (position_tex, position_tv) = create_render_target(
            (res_x, res_y),
            dxgifmt::DXGI_FORMAT_R32G32B32A32_UINT,
            device,
        )?;
        // render target albedo
        let (albedo_tex, albedo_tv) = create_render_target(
            (res_x, res_y),
            dxgifmt::DXGI_FORMAT_R32G32B32A32_UINT,
            device,
        )?;

        Ok(DeferredPassPre {
            program: shaders::ShaderProgram::create(
                &vtx_shader,
                &ps_shader,
                None,
                Some(&input_element_description),
                device,
                context,
            )?,
            vertex_shader_uniforms: vtx_cbuff,
            pixel_shader_uniforms: pxl_cbuff,
            positions: position_tex,
            positions_render_target: position_tv,
            albedo: albedo_tex,
            albedo_render_target: albedo_tv,
        })
    }
}

struct ConstantsDefLight {
    camera_pos: glm::Vec3,
    ssao: u32,
}

struct DxLight {
    position: glm::Vec3,
    t: u32,
    color: glm::Vec3,
    radius: f32,
    light_space: glm::Mat4,
}
impl DxLight {
    #[allow(unreachable_patterns)]
    fn from_sparkle_light(light: &Light) -> DxLight {
        let t = match &light.t {
            LightType::Ambient => 0u32,
            LightType::Directional => 1u32,
            LightType::Area => 2u32,
            _ => std::u32::MAX,
        };
        DxLight {
            position: light.position.clone(),
            t: t,
            color: light.color.clone(),
            radius: light.radius.clone(),
            light_space: light.light_proj.clone(),
        }
    }
}

pub(crate) struct DeferredPassLight {
    program: shaders::ShaderProgram,
    pixel_shader_uniforms: cbuffer::CBuffer<ConstantsDefLight>,
    light_buffer: cbuffer::CBuffer<DxLight>,
    render_target: textures::Texture2D,
    render_target_view: *mut dx11::ID3D11RenderTargetView,
}
impl DeferredPassLight {
    pub fn get_render_target(&self) -> &textures::Texture2D {
        &self.render_target
    }
    pub fn get_render_target_view(&self) -> *mut dx11::ID3D11RenderTargetView {
        self.render_target_view
    }
    pub fn prepare_draw(&mut self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        self.program.activate();

        unsafe {
            let cbuffs = [
                self.pixel_shader_uniforms.buffer_ptr(),
                self.light_buffer.buffer_ptr(),
            ];
            (*ctx).PSSetConstantBuffers(0, 2, cbuffs.as_ptr());
        };
    }

    pub fn update(&mut self) -> Result<(), DxError> {
        self.pixel_shader_uniforms.update()?;
        self.light_buffer.update()?;
        Ok(())
    }
    pub fn set_light(&mut self, light: &Light, instant_update: bool) -> Result<(), DxError> {
        self.light_buffer.data = DxLight::from_sparkle_light(light);
        if instant_update {
            self.light_buffer.update()?
        }
        Ok(())
    }

    pub fn set_camera_pos(&mut self, cpos: glm::Vec3, instant_update: bool) -> Result<(), DxError> {
        self.pixel_shader_uniforms.data.camera_pos = cpos;
        if instant_update {
            self.pixel_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn set_ssao(&mut self, ssao: u32, instant_update: bool) -> Result<(), DxError> {
        self.pixel_shader_uniforms.data.ssao = ssao;
        if instant_update {
            self.pixel_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn create(
        (width, height): (u32, u32),
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<DeferredPassLight, DxError> {
        let vtx_shader = "deferred_light_vertex.cso";
        let ps_shader = "deferred_light_pixel_packing.cso";

        let pxl_uniforms = ConstantsDefLight {
            camera_pos: glm::zero(),
            ssao: 1,
        };

        let pxl_cbuff = match cbuffer::CBuffer::create(pxl_uniforms, context, device) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };
        let light = DxLight {
            position: glm::zero(),
            t: 0,
            color: glm::zero(),
            radius: 0.0,
            light_space: glm::identity(),
        };
        let pxl_cbuff2 = match cbuffer::CBuffer::create(light, context, device) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };
        let (tex, tv) = create_render_target(
            (width, height),
            dxgifmt::DXGI_FORMAT_R32G32B32A32_FLOAT,
            device,
        )?;
        Ok(DeferredPassLight {
            program: shaders::ShaderProgram::create(
                &vtx_shader,
                &ps_shader,
                None,
                None,
                device,
                context,
            )?,
            pixel_shader_uniforms: pxl_cbuff,
            light_buffer: pxl_cbuff2,
            render_target: tex,
            render_target_view: tv,
        })
    }
}

/**
 * Section Shadow Mapping
 */
const SHADOW_MAP_SIZE: u32 = /*2048; */4096;

struct ConstantsVtxSM {
    pub light_space_matrix: glm::Mat4,
}

pub(crate) struct ShadowPass {
    program: shaders::ShaderProgram,
    vertex_shader_uniforms: cbuffer::CBuffer<ConstantsVtxSM>,
    shadow_map: textures::Texture2D,
    shadow_map_depth_target: *mut dx11::ID3D11DepthStencilView,
    shadow_viewport: dx11::D3D11_VIEWPORT,
}

// todo: destructor that cleans up resources

impl ShadowPass {
    pub fn get_depth_stencil_view(&self) -> *mut dx11::ID3D11DepthStencilView {
        self.shadow_map_depth_target
    }
    pub fn get_shadow_map(&self) -> &textures::Texture2D {
        &self.shadow_map
    }
    pub fn get_shadow_map_viewport(&self) -> &dx11::D3D11_VIEWPORT {
        &self.shadow_viewport
    }

    pub fn prepare_draw(&mut self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        self.program.activate();
        unsafe {
            (*ctx).VSSetConstantBuffers(
                0,
                1,
                &self.vertex_shader_uniforms.buffer_ptr() as *const *mut _,
            );
        };
    }

    pub fn update(&mut self) -> Result<(), DxError> {
        self.vertex_shader_uniforms.update()?;

        Ok(())
    }

    pub fn set_light_space(
        &mut self,
        light_space_matrix: glm::Mat4,
        instant_update: bool,
    ) -> Result<(), DxError> {
        self.vertex_shader_uniforms.data.light_space_matrix = light_space_matrix;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn create_simple(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<ShadowPass, DxError> {
        let vt_file = "sm_vertex.cso";
        let ps_file = "sm_pixel.cso";

        ShadowPass::create(device, context, vt_file, None, ps_file)
    }

    fn create(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
        vertex_shader: &str,
        geometry_shader: Option<&str>,
        pixel_shader: &str,
    ) -> Result<ShadowPass, DxError> {
        let input_element_description = vertex_input_desc();

        let vtx_uniforms = ConstantsVtxSM {
            light_space_matrix: glm::identity(),
        };
        let vtx_cbuff = match cbuffer::CBuffer::create(vtx_uniforms, context, device) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };

        let mut depth_tex = textures::Texture2D::create_mutable_render_target(
            SHADOW_MAP_SIZE,
            SHADOW_MAP_SIZE,
            dxgifmt::DXGI_FORMAT_R24G8_TYPELESS, // depth component only
            dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
            dx11::D3D11_TEXTURE_ADDRESS_CLAMP,
            dx11::D3D11_FILTER_COMPARISON_MIN_MAG_LINEAR_MIP_POINT,
            1,
            dx11::D3D11_BIND_DEPTH_STENCIL | dx11::D3D11_BIND_SHADER_RESOURCE,
            0,
            1,
            device,
        )?;
        let mut dt_desc: dx11::D3D11_DEPTH_STENCIL_VIEW_DESC = Default::default();
        dt_desc.Flags = 0;
        dt_desc.Format = dxgifmt::DXGI_FORMAT_D24_UNORM_S8_UINT;
        dt_desc.ViewDimension = dx11::D3D11_DSV_DIMENSION_TEXTURE2D;
        unsafe { dt_desc.u.Texture2D_mut().MipSlice = 0 };
        let mut dtv: *mut dx11::ID3D11DepthStencilView = std::ptr::null_mut();
        let res = unsafe {
            (*device).CreateDepthStencilView(
                depth_tex.get_texture_handle() as *mut _,
                &dt_desc,
                &mut dtv as *mut *mut _,
            )
        };
        if res < winapi::shared::winerror::S_OK {
            return Err(DxError::new(
                "Error creating depth target view for texture",
                DxErrorType::ResourceCreation,
            ));
        }
        let mut dt_rv_desc: dx11::D3D11_SHADER_RESOURCE_VIEW_DESC = Default::default();
        dt_rv_desc.Format = dxgifmt::DXGI_FORMAT_R24_UNORM_X8_TYPELESS;
        dt_rv_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
        unsafe {
            dt_rv_desc.u.Texture2D_mut().MostDetailedMip = 0;
            dt_rv_desc.u.Texture2D_mut().MipLevels = 1;
        };
        let res = unsafe {
            (*device).CreateShaderResourceView(
                depth_tex.get_texture_handle() as *mut _,
                &dt_rv_desc as *const _,
                &mut depth_tex.shader_view as *mut *mut _,
            )
        };
        if res < winapi::shared::winerror::S_OK {
            return Err(DxError::new(
                "Error creating depth shader view for texture",
                DxErrorType::ResourceCreation,
            ));
        }

        Ok(ShadowPass {
            program: shaders::ShaderProgram::create(
                &vertex_shader,
                &pixel_shader,
                geometry_shader,
                Some(&input_element_description),
                device,
                context,
            )?,
            vertex_shader_uniforms: vtx_cbuff,
            shadow_map: depth_tex,
            //shadow_map_render_target: rtv,
            shadow_map_depth_target: dtv,
            shadow_viewport: dx11::D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: SHADOW_MAP_SIZE as f32,
                Height: SHADOW_MAP_SIZE as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            },
        })
    }
}

pub struct OutputPass {
    program: shaders::ShaderProgram,
}

impl OutputPass {
    pub fn prepare_draw(&mut self) {
        self.program.activate();
    }
    pub fn create(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<OutputPass, DxError> {
        let vtx_shader = "deferred_light_vertex.cso";
        let ps_shader = "blend_pixel.cso";

        Ok(OutputPass {
            program: shaders::ShaderProgram::create(
                &vtx_shader,
                &ps_shader,
                None,
                None,
                device,
                context,
            )?,
        })
    }
}

pub struct SkyBoxPass {
    program: shaders::ShaderProgram,
    vertex_shader_uniforms: cbuffer::CBuffer<ConstantsVtxDeferredPre>,
}

impl SkyBoxPass {
    pub fn prepare_draw(&mut self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        self.program.activate();
        unsafe {
            (*ctx).VSSetConstantBuffers(
                0,
                1,
                &self.vertex_shader_uniforms.buffer_ptr() as *const *mut _,
            );
        };
    }

    pub fn update(&mut self) -> Result<(), DxError> {
        self.vertex_shader_uniforms.update()
    }

    pub fn set_view(&mut self, view: glm::Mat4, instant_update: bool) -> Result<(), DxError> {
        self.vertex_shader_uniforms.data.view = view;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }
    pub fn set_proj(&mut self, proj: glm::Mat4, instant_update: bool) -> Result<(), DxError> {
        self.vertex_shader_uniforms.data.proj = proj;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }
    pub fn set_view_proj(
        &mut self,
        view: glm::Mat4,
        proj: glm::Mat4,
        instant_update: bool,
    ) -> Result<(), DxError> {
        self.set_view(view, false)?;
        self.set_proj(proj, false)?;
        if instant_update {
            self.vertex_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn create(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<SkyBoxPass, DxError> {
        let vtx_shader = "skybox_vertex.cso";
        let ps_shader = "skybox_pixel.cso";

        let vtx_uniforms = ConstantsVtxDeferredPre {
            view: glm::identity(),
            proj: glm::identity(),
        };
        let vtx_cbuff = match cbuffer::CBuffer::create(vtx_uniforms, context, device) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };

        Ok(SkyBoxPass {
            program: shaders::ShaderProgram::create(
                &vtx_shader,
                &ps_shader,
                None,
                None,
                device,
                context,
            )?,
            vertex_shader_uniforms: vtx_cbuff,
        })
    }
}
