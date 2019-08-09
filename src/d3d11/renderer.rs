use super::super::window;
use super::backend;
use cgmath::conv::*;
use std::*;
use winapi::um::d3d11 as dx11;

#[allow(dead_code)] // we don't want warnings if some color is not used..
mod colors_linear {
    pub const BACKGROUND: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.052860655f32,
        y: 0.052860655f32,
        z: 0.052860655f32,
        w: 1.0f32,
    };
    pub const GREEN: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.005181516f32,
        y: 0.201556236f32,
        z: 0.005181516f32,
        w: 1.0f32,
    };
    pub const BLUE: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.001517635f32,
        y: 0.114435382f32,
        z: 0.610495627f32,
        w: 1.0f32,
    };
    pub const RED: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.545724571f32,
        y: 0.026241219f32,
        z: 0.001517635f32,
        w: 1.0f32,
    };
    pub const WHITE: cgmath::Vector4<f32> = cgmath::Vector4 {
        x: 0.052860655f32,
        y: 0.052860655f32,
        z: 0.052860655f32,
        w: 1.0f32,
    };
}

pub struct D3D11Renderer {
    backend: backend::D3D11Backend,
    window: window::Window,
}

impl D3D11Renderer {
    pub fn create(width: i32, height: i32, title: &str) -> Result<D3D11Renderer, &'static str> {
        let window = window::Window::create_window(width, height, "main", title)?;
        let backend = backend::D3D11Backend::init(&window)?;
        let mut renderer = D3D11Renderer {
            backend: backend,
            window: window,
        };

        renderer.init_draw_program()?;

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
    fn clear(&self) {
        self.backend.pix_begin_event("Clear");

        let ctx = self.backend.get_context();
        let render_target = self.backend.get_render_target_view();
        let depth_stencil = self.backend.get_depth_stencil_view();

        let color = array4(colors_linear::BACKGROUND);
        unsafe {
            (*ctx).ClearRenderTargetView(render_target, &color);
            (*ctx).ClearDepthStencilView(
                depth_stencil,
                dx11::D3D11_CLEAR_DEPTH | dx11::D3D11_CLEAR_STENCIL,
                1.0f32,
                0,
            );

            (*ctx).OMSetRenderTargets(1, &render_target, depth_stencil);
        };
        let viewport = self.backend.get_viewport();
        unsafe { (*ctx).RSSetViewports(1, viewport) };

        self.backend.pix_end_event();
    }

    fn render(&mut self) -> Result<(), &'static str> {
        self.clear();

        self.backend.pix_begin_event("Render");

        let ctx = self.backend.get_context();
        unsafe { (*ctx).Draw(3, 0) };

        self.backend.pix_end_event();

        self.backend.present()?;

        Ok(())
    }

    fn init_draw_program(&mut self) -> Result<(), &'static str> {
        self.backend.initialize_shader_program()?;

        Ok(())
    }
}
