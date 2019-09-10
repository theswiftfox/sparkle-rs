use crate::drawing::d3d11::backend::{DxError, DxErrorType};
use crate::utils;
use std::ptr;
use winapi::shared::dxgiformat as dxgifmt;
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
    input_layout: *mut dx11::ID3D11InputLayout,

    pub context: *mut dx11_1::ID3D11DeviceContext1,
    active: bool,
}

#[allow(dead_code)]
impl ShaderProgram {
    pub fn create(
        device: *mut dx11_1::ID3D11Device1,
        context: *mut dx11_1::ID3D11DeviceContext1,
    ) -> Result<ShaderProgram, DxError> {
        let mut shader_program = ShaderProgram {
            vertex_shader: ptr::null_mut(),
            vertex_shader_byte_code: ptr::null_mut(),
            pixel_shader: ptr::null_mut(),
            pixel_shader_byte_code: ptr::null_mut(),
            input_layout: ptr::null_mut(),
            context: ptr::null_mut(),
            active: false,
        };

        shader_program.context = context;
        shader_program.setup_shaders(device)?;

        let pos_name: &'static std::ffi::CStr = const_cstr!("SV_Position").as_cstr();
        let color_name: &'static std::ffi::CStr = const_cstr!("COLOR").as_cstr();
        let input_element_description: [dx11::D3D11_INPUT_ELEMENT_DESC; 2] = [
            dx11::D3D11_INPUT_ELEMENT_DESC {
                SemanticName: pos_name.as_ptr() as *const _,
                SemanticIndex: 0,
                Format: dxgifmt::DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: dx11::D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            dx11::D3D11_INPUT_ELEMENT_DESC {
                SemanticName: color_name.as_ptr() as *const _,
                SemanticIndex: 0,
                Format: dxgifmt::DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 16,
                InputSlotClass: dx11::D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let vertex_shader = shader_program.get_vertex_shader_byte_code();

        let res = unsafe {
            (*device).CreateInputLayout(
                input_element_description.as_ptr(),
                input_element_description.len() as u32,
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
        if self.active {
            return;
        }
        unsafe {
            (*self.context).VSSetShader(self.vertex_shader, ptr::null(), 0);
            (*self.context).GSSetShader(ptr::null_mut(), ptr::null(), 0);
            (*self.context).PSSetShader(self.pixel_shader, ptr::null(), 0);
        }
        self.active = true;
    }
    pub fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        unsafe {
            (*self.context).VSSetShader(ptr::null_mut(), ptr::null(), 0);
            (*self.context).GSSetShader(ptr::null_mut(), ptr::null(), 0);
            (*self.context).PSSetShader(ptr::null_mut(), ptr::null(), 0);
        }
        self.active = false;
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
    pub fn setup_shaders(&mut self, device: *mut dx11_1::ID3D11Device1) -> Result<(), DxError> {
        let mut release = true;

        //#[cfg(debug_assertions)]
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
                return Err(DxError::new(
                    "Vertex Shader creation failed!",
                    DxErrorType::ShaderCreate,
                ));
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
