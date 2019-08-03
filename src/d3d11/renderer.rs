use std::*;
use super::super::{window, utils};
use super::{backend};
use cgmath::conv::*;
use winapi::um::d3d11 as dx11;
#[cfg(debug_assertions)]
use winapi::um::d3dcommon as dx;
#[cfg(debug_assertions)]
use winapi::um::d3dcompiler as d3dcomp;

#[allow(dead_code)] // we don't want warnings if some color is not used..
mod colors_linear {
    pub const BACKGROUND : cgmath::Vector4<f32> = cgmath::Vector4 { x: 0.052860655f32, y: 0.052860655f32, z: 0.052860655f32, w: 1.0f32 };
    pub const GREEN : cgmath::Vector4<f32> = cgmath::Vector4 { x: 0.005181516f32, y: 0.201556236f32, z: 0.005181516f32, w: 1.0f32 };
    pub const BLUE : cgmath::Vector4<f32> = cgmath::Vector4 { x: 0.001517635f32, y: 0.114435382f32, z: 0.610495627f32, w: 1.0f32 };
    pub const RED : cgmath::Vector4<f32> = cgmath::Vector4 { x: 0.545724571f32, y: 0.026241219f32, z: 0.001517635f32, w: 1.0f32 };
    pub const WHITE : cgmath::Vector4<f32> = cgmath::Vector4 { x: 0.052860655f32, y: 0.052860655f32, z: 0.052860655f32, w: 1.0f32 };
}

pub struct D3D11Renderer {
    backend : backend::D3D11Backend,
    window : window::Window
}

impl D3D11Renderer {
    pub fn create(width: i32, height: i32, title: &str) -> Result<D3D11Renderer, &'static str> {
        let window = window::Window::create_window(width, height, "main", title)?;
        let backend = backend::D3D11Backend::init(&window)?;
        let renderer = D3D11Renderer {
            backend: backend,
            window: window
        };
        
        Ok(renderer)
    }

    pub fn cleanup(&mut self) {
        self.backend.cleanup();
    }

    pub fn update(&mut self) -> Result<bool, &'static str> {
        let ok = self.window.update();

        if ok {
            self.render()?;
        }

        Ok(ok)
    }

    /**
     * Section: Render funcs
     */
    fn clear(& self) {
        self.backend.pix_begin_event("Clear");

        let ctx = self.backend.get_context();
        let render_target = self.backend.get_render_target_view();
        let depth_stencil = self.backend.get_depth_stencil_view();

        let color = array4(colors_linear::BACKGROUND);
        unsafe { 
            (*ctx).ClearRenderTargetView(render_target, &color);
            (*ctx).ClearDepthStencilView(depth_stencil, dx11::D3D11_CLEAR_DEPTH | dx11::D3D11_CLEAR_STENCIL, 1.0f32, 0);

            (*ctx).OMSetRenderTargets(1, &render_target, depth_stencil);     
        };
        let viewport = self.backend.get_viewport();
        unsafe { (*ctx).RSSetViewports(1, viewport) };

        self.backend.pix_end_event();
    }

    fn render(&mut self) -> Result<(), &'static str> {
        self.clear();

        self.backend.pix_begin_event("Render");

		// let ctx = self.backend.get_context();
		// unsafe { (*ctx).IASetInputLayout() };

        self.backend.pix_end_event();

        self.backend.present()?;

        Ok(())
    }

    fn setupShaders(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut release = true;
        #[cfg(debug_assertions)]
        {
            let entry = utils::to_lpc_str("main");
            let mut vertex_shader_data : *mut dx::ID3DBlob = ptr::null_mut();
            let mut vertex_shader_error : *mut dx::ID3DBlob = ptr::null_mut();
            let flags : u32 = d3dcomp::D3DCOMPILE_ENABLE_STRICTNESS | d3dcomp::D3DCOMPILE_DEBUG;

            let vtx_shader_file = utils::to_wide_str("shaders/vertex.hlsl");
            let target = utils::to_lpc_str("vs_5_0");
            let result = unsafe { d3dcomp::D3DCompileFromFile(
                vtx_shader_file.as_ptr(), 
                ptr::null(), 
                ptr::null_mut(), 
                entry.as_ptr(), 
                target.as_ptr(),
                flags,
                0, 
                &mut vertex_shader_data,
                &mut vertex_shader_error
            )};      

            release = false;
        }
        if (release) {
            // todo: load from precompiled file
        }


        Ok(())
    }
}