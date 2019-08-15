use crate::utils;
use cgmath::*;
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
}

pub struct ConstantBuffer {
    pub view_matrix: Matrix4<f32>,
    pub projection_matrix: Matrix4<f32>,
    pub lights: [crate::drawing::geometry::Light; 2],
    _pad_1: Vector4<f32>,
    _pad_2: Vector4<f32>,
}

impl Default for ShaderProgram {
    fn default() -> ShaderProgram {
        ShaderProgram {
            vertex_shader: ptr::null_mut(),
            vertex_shader_byte_code: ptr::null_mut(),
            pixel_shader: ptr::null_mut(),
            pixel_shader_byte_code: ptr::null_mut(),
        }
    }
}

#[allow(dead_code)]
impl ShaderProgram {
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

    pub fn compile_shader(
        mut shader_data: *mut *mut dx::ID3DBlob,
        shader_file: &str,
        target: &str,
    ) -> Result<(), &'static str> {
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
            if shader_comp_err != ptr::null_mut() {
                let buffer_ptr = unsafe { (*shader_comp_err).GetBufferPointer() };
                let message_cstr = unsafe { std::ffi::CStr::from_ptr(buffer_ptr as *const i8) };
                return Err(message_cstr.to_str().unwrap());
            }
            return Err("Shader compilation failed!");
        }

        Ok(())
    }

    #[allow(unused_mut)]
    pub fn setup_shaders(
        &mut self,
        device: *mut dx11_1::ID3D11Device1,
    ) -> Result<(), &'static str> {
        let mut release = true;

        #[cfg(debug_assertions)]
        {
            let mut shader_dir = std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            shader_dir.push("shaders");
            let mut vtx_shader_file = std::path::PathBuf::from(shader_dir.to_str().unwrap());
            vtx_shader_file.push("vertex.hlsl");
            let target = "vs_5_0";
            ShaderProgram::compile_shader(
                &mut self.vertex_shader_byte_code as *mut *mut _,
                vtx_shader_file.as_path().to_str().unwrap(),
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
                return Err("Vertex Shader creation failed!");
            }

            let mut px_shader_file = std::path::PathBuf::from(shader_dir.to_str().unwrap());
            px_shader_file.push("pixel.hlsl");
            let target = "ps_5_0";
            ShaderProgram::compile_shader(
                &mut self.pixel_shader_byte_code as *mut *mut _,
                px_shader_file.as_path().to_str().unwrap(),
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
                return Err("Pixel Shader creation failed!");
            }

            release = false;
        }
        if release {
            // todo: load from precompiled file
        }

        Ok(())
    }
}