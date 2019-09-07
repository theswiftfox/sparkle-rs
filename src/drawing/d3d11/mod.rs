use crate::drawing::Renderer;
use crate::window::Window;
use cgmath::conv::*;
use winapi::um::d3d11 as dx11;

mod backend;
mod shaders;

pub struct D3D11Renderer<W> {
    backend: backend::D3D11Backend,
    window: W,
}

impl<W> Renderer for D3D11Renderer<W>
where
    W: Window,
{
    fn create(width: i32, height: i32, title: &str) -> D3D11Renderer<W> {
        let window = W::create_window(width, height, "main", title);
        let backend = match backend::D3D11Backend::init(&window) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };
        let mut renderer = D3D11Renderer {
            backend: backend,
            window: window,
        };

        match renderer.init_draw_program() {
            Ok(_) => renderer,
            Err(e) => panic!(e),
        }
    }

    fn cleanup(&mut self) {
        self.backend.cleanup();
    }

    fn update(&mut self) -> Result<bool, &'static str> {
        let ok = self.window.update();

        if ok {
            self.render()?;
        }

        Ok(ok)
    }
}
impl<W> D3D11Renderer<W> {
    /**
     * Section: Render funcs
     */
    fn clear(&self) {
        self.backend.pix_begin_event("Clear");

        let ctx = self.backend.get_context();
        let render_target = self.backend.get_render_target_view();
        let depth_stencil = self.backend.get_depth_stencil_view();

        let color = array4(crate::drawing::colors_linear::BACKGROUND);
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
