use winapi::shared::dxgiformat as dxgifmt;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;

use super::d3d11::{cbuffer, sbuffer, shaders, textures, DxError, DxErrorType};
use super::geometry::Light;

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
        // dx11::D3D11_INPUT_ELEMENT_DESC {
        //     SemanticName: uv_name.as_ptr() as *const _,
        //     SemanticIndex: 1,
        //     Format: dxgifmt::DXGI_FORMAT_R32G32_FLOAT,
        //     InputSlot: 0,
        //     AlignedByteOffset: dx11::D3D11_APPEND_ALIGNED_ELEMENT,
        //     InputSlotClass: dx11::D3D11_INPUT_PER_VERTEX_DATA,
        //     InstanceDataStepRate: 0,
        // },
    ]
}

/**
 * Section ForwardPass
 */
struct ConstantsVtxMP {
    pub view: glm::Mat4,
    pub proj: glm::Mat4,
    pub light_space: glm::Mat4,
}

pub(crate) struct ForwardPass {
    program: shaders::ShaderProgram,
    vertex_shader_uniforms: cbuffer::CBuffer<ConstantsVtxMP>,
    pixel_shader_uniforms: cbuffer::CBuffer<ConstantsDefLight>,
    lights_buffer: sbuffer::SBuffer<Light>,
}
impl ForwardPass {
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
            (*ctx).PSSetShaderResources(3, 1, &self.lights_buffer.shader_view() as *const *mut _);
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
    pub fn set_light_space_matrix(
        &mut self,
        ls_mat: glm::Mat4,
        instant_update: bool,
    ) -> Result<(), DxError> {
        self.vertex_shader_uniforms.data.light_space = ls_mat;
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

    pub fn set_camera_pos(&mut self, cpos: glm::Vec3, instant_update: bool) -> Result<(), DxError> {
        self.pixel_shader_uniforms.data.camera_pos = cpos;
        if instant_update {
            self.pixel_shader_uniforms.update()?
        }
        Ok(())
    }
    pub fn set_lights(&mut self, light: Vec<Light>) -> Result<(), DxError> {
        self.lights_buffer.update(light)
    }
    pub fn set_ssao(&mut self, ssao: u32, instant_update: bool) -> Result<(), DxError> {
        self.pixel_shader_uniforms.data.ssao = ssao;
        if instant_update {
            self.pixel_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn create(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<ForwardPass, DxError> {
        let vtx_shader = "mp_vertex.cso";
        let ps_shader = "mp_pixel.cso";
        let input_element_description = vertex_input_desc();

        let vtx_uniforms = ConstantsVtxMP {
            view: glm::identity(),
            proj: glm::identity(),
            light_space: glm::identity(),
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
        let light = Light {
            position: glm::zero(),
            t: 0,
            color: glm::zero(),
            radius: 0.0,
        };
        let sbuff = match sbuffer::SBuffer::create(vec![light], context, device) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };

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
            lights_buffer: sbuff,
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
        let mut position_tv: *mut dx11::ID3D11RenderTargetView = std::ptr::null_mut();
        let mut position_tex = textures::Texture2D::create_mutable_render_target(
            res_x,
            res_y,
            dxgifmt::DXGI_FORMAT_R32G32B32A32_UINT,
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
            dt_desc.Format = dxgifmt::DXGI_FORMAT_R32G32B32A32_UINT;
            dt_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            unsafe { dt_desc.u.Texture2D_mut().MipSlice = 0 };
            let res = unsafe {
                (*device).CreateRenderTargetView(
                    position_tex.get_texture_handle() as *mut _,
                    &dt_desc,
                    &mut position_tv as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Error creating depth target view for texture",
                    DxErrorType::ResourceCreation,
                ));
            }
            let mut pos_tar_rv_desc: dx11::D3D11_SHADER_RESOURCE_VIEW_DESC = Default::default();
            pos_tar_rv_desc.Format = dxgifmt::DXGI_FORMAT_R32G32B32A32_UINT;
            pos_tar_rv_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            unsafe {
                pos_tar_rv_desc.u.Texture2D_mut().MostDetailedMip = 0;
                pos_tar_rv_desc.u.Texture2D_mut().MipLevels = 1;
            };
            let res = unsafe {
                (*device).CreateShaderResourceView(
                    position_tex.get_texture_handle() as *mut _,
                    &pos_tar_rv_desc as *const _,
                    &mut position_tex.shader_view as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Error creating depth shader view for texture",
                    DxErrorType::ResourceCreation,
                ));
            }
        }
        // render target albedo
        let mut albedo_tv: *mut dx11::ID3D11RenderTargetView = std::ptr::null_mut();
        let mut albedo_tex = textures::Texture2D::create_mutable_render_target(
            res_x,
            res_y,
            dxgifmt::DXGI_FORMAT_R32G32B32A32_UINT,
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
            dt_desc.Format = dxgifmt::DXGI_FORMAT_R32G32B32A32_UINT;
            dt_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            unsafe { dt_desc.u.Texture2D_mut().MipSlice = 0 };
            let res = unsafe {
                (*device).CreateRenderTargetView(
                    albedo_tex.get_texture_handle() as *mut _,
                    &dt_desc,
                    &mut albedo_tv as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Error creating depth target view for texture",
                    DxErrorType::ResourceCreation,
                ));
            }
            let mut pos_tar_rv_desc: dx11::D3D11_SHADER_RESOURCE_VIEW_DESC = Default::default();
            pos_tar_rv_desc.Format = dxgifmt::DXGI_FORMAT_R32G32B32A32_UINT;
            pos_tar_rv_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            unsafe {
                pos_tar_rv_desc.u.Texture2D_mut().MostDetailedMip = 0;
                pos_tar_rv_desc.u.Texture2D_mut().MipLevels = 1;
            };
            let res = unsafe {
                (*device).CreateShaderResourceView(
                    albedo_tex.get_texture_handle() as *mut _,
                    &pos_tar_rv_desc as *const _,
                    &mut albedo_tex.shader_view as *mut *mut _,
                )
            };
            if res < winapi::shared::winerror::S_OK {
                return Err(DxError::new(
                    "Error creating depth shader view for texture",
                    DxErrorType::ResourceCreation,
                ));
            }
        }

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
struct MatricesDefLight {
    light_space: glm::Mat4,
}

pub(crate) struct DeferredPassLight {
    program: shaders::ShaderProgram,
    pixel_shader_uniforms: cbuffer::CBuffer<ConstantsDefLight>,
    pixel_shader_matrices: cbuffer::CBuffer<MatricesDefLight>,
    lights_buffer: sbuffer::SBuffer<Light>,
}
impl DeferredPassLight {
    pub fn prepare_draw(&mut self, ctx: *mut dx11_1::ID3D11DeviceContext1) {
        self.program.activate();

        unsafe {
            (*ctx).PSSetConstantBuffers(
                0,
                1,
                &self.pixel_shader_uniforms.buffer_ptr() as *const *mut _,
            );
            (*ctx).PSSetConstantBuffers(
                1,
                1,
                &self.pixel_shader_matrices.buffer_ptr() as *const *mut _,
            );
            (*ctx).PSSetShaderResources(3, 1, &self.lights_buffer.shader_view() as *const *mut _);
        };
    }

    pub fn update(&mut self) -> Result<(), DxError> {
        self.pixel_shader_uniforms.update()?;
        self.pixel_shader_matrices.update()?;
        Ok(())
    }
    pub fn set_light_space_matrix(
        &mut self,
        ls_mat: glm::Mat4,
        instant_update: bool,
    ) -> Result<(), DxError> {
        self.pixel_shader_matrices.data.light_space = ls_mat;
        if instant_update {
            self.pixel_shader_matrices.update()?
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
    pub fn set_lights(&mut self, light: Vec<Light>) -> Result<(), DxError> {
        self.lights_buffer.update(light)
    }

    pub fn set_ssao(&mut self, ssao: u32, instant_update: bool) -> Result<(), DxError> {
        self.pixel_shader_uniforms.data.ssao = ssao;
        if instant_update {
            self.pixel_shader_uniforms.update()?
        }
        Ok(())
    }

    pub fn create(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<DeferredPassLight, DxError> {
        let vtx_shader = "deferred_light_vertex.cso";
        let ps_shader = "deferred_light_pixel_packing.cso";

        let pxl_uniforms = ConstantsDefLight {
            camera_pos: glm::zero(),
            ssao: 1,
        };
        let pxl_matrices = MatricesDefLight {
            light_space: glm::zero(),
        };

        let pxl_cbuff = match cbuffer::CBuffer::create(pxl_uniforms, context, device) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };
        let pxl_cbuff_1 = match cbuffer::CBuffer::create(pxl_matrices, context, device) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };
        let light = Light {
            position: glm::zero(),
            t: 0,
            color: glm::zero(),
            radius: 0.0,
        };
        let pxl_sbuff = match sbuffer::SBuffer::create(vec![light], context, device) {
            Ok(b) => b,
            Err(e) => return Err(e),
        };

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
            pixel_shader_matrices: pxl_cbuff_1,
            lights_buffer: pxl_sbuff,
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
    //shadow_map_render_target: *mut dx11::ID3D11RenderTargetView,
    shadow_map_depth_target: *mut dx11::ID3D11DepthStencilView,
    shadow_viewport: dx11::D3D11_VIEWPORT,
}

// todo: destructor that cleans up resources

impl ShadowPass {
    // pub fn get_render_target_view(&self) -> *mut dx11::ID3D11RenderTargetView {
    //     self.shadow_map_render_target
    // }
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
