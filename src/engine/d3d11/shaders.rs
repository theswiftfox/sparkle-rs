use super::{DxError, DxErrorType};
use crate::utils;

use std::ptr;
use winapi::shared::winerror::S_OK;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;
use winapi::um::d3dcommon as dx;
use winapi::um::d3dcompiler as d3dcomp;

pub struct ShaderProgram {
    vertex_shader: *mut dx11::ID3D11VertexShader,
    vertex_shader_byte_code: *mut dx::ID3DBlob,
    pixel_shader: *mut dx11::ID3D11PixelShader,
    pixel_shader_byte_code: *mut dx::ID3DBlob,
    geometry_shader: *mut dx11::ID3D11GeometryShader,
    geometry_shader_byte_code: *mut dx::ID3DBlob,
    input_layout: *mut dx11::ID3D11InputLayout,

    pub context: *mut dx11_1::ID3D11DeviceContext1,
}

#[allow(dead_code)]
impl ShaderProgram {
    pub fn create(
        vs_file: &str,
        ps_file: &str,
        geom_file: Option<&str>,
        input_desc: &[dx11::D3D11_INPUT_ELEMENT_DESC],
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<ShaderProgram, DxError> {
        let mut shader_program = ShaderProgram {
            vertex_shader: ptr::null_mut(),
            vertex_shader_byte_code: ptr::null_mut(),
            pixel_shader: ptr::null_mut(),
            pixel_shader_byte_code: ptr::null_mut(),
            geometry_shader: ptr::null_mut(),
            geometry_shader_byte_code: ptr::null_mut(),
            input_layout: ptr::null_mut(),
            context: ptr::null_mut(),
        };

        shader_program.context = context;
        shader_program.setup_shaders(vs_file, ps_file, geom_file, device)?;

        let vertex_shader = shader_program.get_vertex_shader_byte_code();

        let res = unsafe {
            (*device).CreateInputLayout(
                input_desc.as_ptr(),
                input_desc.len() as u32,
                (*vertex_shader).GetBufferPointer(),
                (*vertex_shader).GetBufferSize(),
                &mut shader_program.input_layout as *mut *mut _,
            )
        };
        if res < S_OK {
            return Err(DxError::new(
                "Input Layout creation failed!",
                DxErrorType::ResourceCreation,
            ));
        }

        unsafe {
            (*context).IASetInputLayout(shader_program.input_layout);
        }

        Ok(shader_program)
    }
    pub fn get_vertex_shader(&self) -> *mut dx11::ID3D11VertexShader {
        self.vertex_shader
    }
    pub fn get_vertex_shader_byte_code(&self) -> *mut dx::ID3DBlob {
        self.vertex_shader_byte_code
    }
    pub fn get_pixel_shader(&self) -> *mut dx11::ID3D11PixelShader {
        self.pixel_shader
    }
    pub fn get_pixel_shader_byte_code(&self) -> *mut dx::ID3DBlob {
        self.pixel_shader_byte_code
    }

    pub fn activate(&mut self) {
        unsafe {
            (*self.context).VSSetShader(self.vertex_shader, ptr::null(), 0);
            (*self.context).GSSetShader(ptr::null_mut(), ptr::null(), 0);
            (*self.context).PSSetShader(self.pixel_shader, ptr::null(), 0);
        }
    }
    pub fn deactivate(&mut self) {
        // todo: useless?
        unsafe {
            (*self.context).VSSetShader(ptr::null_mut(), ptr::null(), 0);
            (*self.context).GSSetShader(ptr::null_mut(), ptr::null(), 0);
            (*self.context).PSSetShader(ptr::null_mut(), ptr::null(), 0);
        }
    }

    pub fn compile_shader(
        mut shader_data: *mut *mut dx::ID3DBlob,
        shader_file: &str,
        target: &str,
    ) -> Result<(), DxError> {
        #[cfg(debug_assertions)]
        println!("Compiling shader file: {}", shader_file);
        let entry = utils::to_lpc_str("main");
        let flags: u32 = d3dcomp::D3DCOMPILE_ENABLE_STRICTNESS | d3dcomp::D3DCOMPILE_DEBUG;

        let shader_file_cstr = utils::to_wide_str(shader_file);
        let target_cstr = utils::to_lpc_str(target);

        let mut shader_comp_err: *mut dx::ID3DBlob = ptr::null_mut();

        let result = unsafe {
            d3dcomp::D3DCompileFromFile(
                shader_file_cstr.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                entry.as_ptr(),
                target_cstr.as_ptr(),
                flags,
                0,
                shader_data as *mut *mut _,
                &mut shader_comp_err as *mut *mut _,
            )
        };
        if result < S_OK {
            shader_data = ptr::null_mut();
            let msg = match shader_comp_err != ptr::null_mut() {
                true => {
                    let buffer_ptr = unsafe { (*shader_comp_err).GetBufferPointer() };
                    let message_cstr = unsafe { std::ffi::CStr::from_ptr(buffer_ptr as *const i8) };
                    message_cstr.to_str().unwrap()
                }
                false => "Shader compilation failed!",
            };
            Err(DxError::new(&msg, DxErrorType::ShaderCompile))
        } else {
            Ok(())
        }
    }

    #[allow(unused_mut)]
    pub fn setup_shaders(
        &mut self,
        vs_file: &str,
        ps_file: &str,
        geom_file: Option<&str>,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<(), DxError> {
        let mut release = true;

        //#[cfg(debug_assertions)]
        {
            let mut shader_dir = std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            shader_dir.push("shaders");
            let mut vtx_shader_file = shader_dir.join(vs_file);
            let target = "vs_5_0";
            ShaderProgram::compile_shader(
                &mut self.vertex_shader_byte_code as *mut *mut _,
                vtx_shader_file.to_str().unwrap(),
                target,
            )?;

            let vtx_buffer_ptr = unsafe { (*self.vertex_shader_byte_code).GetBufferPointer() };
            let vtx_buffer_size = unsafe { (*self.vertex_shader_byte_code).GetBufferSize() };
            let res = unsafe {
                (*device).CreateVertexShader(
                    vtx_buffer_ptr as *const _,
                    vtx_buffer_size,
                    ptr::null_mut(),
                    &mut self.vertex_shader as *mut *mut _,
                )
            };

            if res < S_OK {
                return Err(DxError::new(
                    "Vertex Shader creation failed!",
                    DxErrorType::ShaderCreate,
                ));
            }
            if let Some(geom_file) = geom_file {
                let mut geom_shader_file = shader_dir.join(geom_file);
                let target = "gs_5_0";
                ShaderProgram::compile_shader(
                    &mut self.geometry_shader_byte_code as *mut *mut _,
                    geom_shader_file.to_str().unwrap(),
                    target,
                )?;

                let geom_buffer_ptr = unsafe { (*self.geometry_shader_byte_code).GetBufferPointer() };
                let geom_buffer_size = unsafe { (*self.geometry_shader_byte_code).GetBufferSize() };
                let res = unsafe {
                    (*device).CreateGeometryShader(
                        geom_buffer_ptr as *const _,
                        geom_buffer_size,
                        ptr::null_mut(),
                        &mut self.geometry_shader as *mut *mut _,
                    )                    
                };
                if res < S_OK {
                    return Err(DxError::new(
                        &format!("Geometry Shader creation failed! Err: {}", res),
                        DxErrorType::ShaderCreate,
                    ));
                }
            }

            let mut px_shader_file = shader_dir.join(ps_file);
            let target = "ps_5_0";
            ShaderProgram::compile_shader(
                &mut self.pixel_shader_byte_code as *mut *mut _,
                px_shader_file.to_str().unwrap(),
                target,
            )?;

            let pix_buffer_ptr = unsafe { (*self.pixel_shader_byte_code).GetBufferPointer() };
            let pix_buffer_size = unsafe { (*self.pixel_shader_byte_code).GetBufferSize() };
            let res = unsafe {
                (*device).CreatePixelShader(
                    pix_buffer_ptr as *const _,
                    pix_buffer_size,
                    ptr::null_mut(),
                    &mut self.pixel_shader as *mut *mut _,
                )
            };
            if res < S_OK {
                return Err(DxError::new(
                    "Pixel Shader creation failed!",
                    DxErrorType::ShaderCreate,
                ));
            }

            release = false;
        }
        if release {
            // todo: load from precompiled file
        }

        Ok(())
    }
}
