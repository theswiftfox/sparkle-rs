use crate::drawing::scenegraph::Scenegraph;
use crate::drawing::scenegraph::node::Node;
use crate::drawing::Renderer;
use crate::window::Window;
use cgmath::conv::*;
use winapi::um::d3d11 as dx11;

mod backend;
mod shaders;

pub mod drawable;

pub struct D3D11Renderer<W> {
    backend: backend::D3D11Backend,
    draw_program: Option<shaders::ShaderProgram>,
    scene: Scenegraph,
    window: std::rc::Rc<std::cell::RefCell<W>>,
}

impl<W> Renderer for D3D11Renderer<W>
where
    W: Window,
{
    fn create(width: i32, height: i32, title: &str) -> D3D11Renderer<W> {
        let window = W::create_window(width, height, "main", title);
        let backend = match backend::D3D11Backend::init(window.clone()) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };
        let mut renderer = D3D11Renderer {
            backend: backend,
            window: window,
            draw_program: None,
            scene: Scenegraph::empty(),
        };

        let mut renderer = match renderer.init_draw_program() {
            Ok(_) => renderer,
            Err(e) => panic!(e),
        };

        // todo: cleanup this
        let mut vertex_buffer_data: Vec<crate::drawing::geometry::Vertex> = Vec::new();
        vertex_buffer_data.push(crate::drawing::geometry::Vertex::new_from_f32(
            0.0f32, 0.5f32, 0.5f32, 1.0f32, 1.0f32, 0.0f32, 0.0f32, 1.0f32,
        ));
        vertex_buffer_data.push(crate::drawing::geometry::Vertex::new_from_f32(
            0.5f32, -0.5f32, 0.5f32, 1.0f32, 0.0f32, 1.0f32, 0.0f32, 1.0f32,
        ));
        vertex_buffer_data.push(crate::drawing::geometry::Vertex::new_from_f32(
            -0.5f32, -0.5f32, 0.5f32, 1.0f32, 0.0f32, 0.0f32, 1.0f32, 1.0f32,
        ));
        let mut index_buffer_data: Vec<u32> = Vec::new();
        index_buffer_data.push(0);
        index_buffer_data.push(1);
        index_buffer_data.push(2);
        let drawable = match drawable::DxDrawable::from_verts(
                renderer.backend.get_device(),
                renderer.backend.get_context(),
                vertex_buffer_data,
                index_buffer_data,
            ) {
                Ok(d) => d,
                Err(e) =>panic!(e),
            };
        use cgmath::num_traits::One;
        let node = Node::create("Triangle", cgmath::Matrix4::one(), Some(drawable));
        renderer.scene.set_root(node);

        return renderer;
    }

    fn cleanup(&mut self) {
        self.backend.cleanup();
    }

    fn update(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let ok = self.window.borrow().update();

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

    fn render(&mut self) -> Result<(), backend::DxError> {
        self.clear();
        match &self.draw_program {
            Some(_) => self.draw_program.as_mut().unwrap().activate(),
            None => {}
        };
        self.backend.pix_begin_event("Render");

        // let ctx = self.backend.get_context();
        // unsafe { (*ctx).Draw(3, 0) };
        self.scene.draw();

        self.backend.pix_end_event();

        self.backend.present()?;

        Ok(())
    }

    fn init_draw_program(&mut self) -> Result<(), backend::DxError> {
        self.draw_program = Some(self.backend.create_shader_program()?);

        Ok(())
    }
}
