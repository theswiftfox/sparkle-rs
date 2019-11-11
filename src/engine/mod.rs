//pub mod generate;
pub mod geometry;
pub mod scenegraph;

pub(crate) mod d3d11;
pub(crate) mod settings;

use crate::import;
use crate::input::first_person::FPSController;
use crate::input::input_handler::InputHandler;
use crate::input::Camera;
use crate::window::Window;
use d3d11::{cbuffer, shaders, D3D11Backend, DxError};
use scenegraph::Scenegraph;
use std::time::Instant;
use winapi::um::d3d11 as dx11;

struct VertexShaderUniforms {
    pub view: glm::Mat4,
    pub proj: glm::Mat4,
}
struct PixelShaderUniforms {
    pub camera_pos: glm::Vec4,
}

pub struct Renderer {
    scene: Scenegraph,
    draw_program: Option<shaders::ShaderProgram>,
    input_handler: Option<std::rc::Rc<std::cell::RefCell<dyn InputHandler>>>,
    camera: Option<std::rc::Rc<std::cell::RefCell<dyn Camera>>>,
    vertex_shader_uniforms: cbuffer::CBuffer<VertexShaderUniforms>,
    pixel_shader_uniforms: cbuffer::CBuffer<PixelShaderUniforms>,
    backend: D3D11Backend,
    window: std::rc::Rc<std::cell::RefCell<Window>>,
    clock: Instant,
}

impl Renderer {
    pub fn create(title: &str) -> Renderer {
        let settings = settings::Settings::load();
        let (width, height) = settings.resolution;
        let input_handler = FPSController::create_ptr(
            (width as f32) / (height as f32),
            settings.camera_fov,
            0.1f32,
            settings.view_distance,
        );

        let window = Window::create_window(width, height, "main", title);
        let backend = match D3D11Backend::init(window.clone()) {
            Ok(b) => b,
            Err(e) => panic!(format!("{}", e)),
        };
        let vtx_uniforms = VertexShaderUniforms {
            view: glm::identity(),
            proj: glm::identity(),
        };
        let pxl_uniforms = PixelShaderUniforms {
            camera_pos: glm::zero(),
        };
        let vtx_cbuff = match cbuffer::CBuffer::create(
            vtx_uniforms,
            backend.get_context(),
            backend.get_device(),
        ) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };
        let pxl_cbuff = match cbuffer::CBuffer::create(
            pxl_uniforms,
            backend.get_context(),
            backend.get_device(),
        ) {
            Ok(b) => b,
            Err(e) => panic!(e),
        };
        let mut renderer = Renderer {
            backend: backend,
            window: window,
            draw_program: None,
            scene: Scenegraph::empty(),
            input_handler: None,
            camera: None,
            vertex_shader_uniforms: vtx_cbuff,
            pixel_shader_uniforms: pxl_cbuff,
            clock: Instant::now(),
        };

        let mut renderer = match renderer.init_draw_program() {
            Ok(_) => renderer,
            Err(e) => panic!(format!("{}", e)),
        };

        renderer.change_input_handler(input_handler.clone());
        renderer.change_camera(input_handler.clone());
        renderer.vertex_shader_uniforms.data.proj = input_handler.borrow().projection_mat();

        println!("DX Setup took {} ms", renderer.clock.elapsed().as_millis());
        renderer.clock = Instant::now();
        match settings.level {
            Some(l) => match renderer.load_scene(&l) {
                Ok(_) => {
                    println!(
                        "Loaded scene in {} ms",
                        renderer.clock.elapsed().as_millis()
                    );
                    renderer.clock = Instant::now();
                }
                Err(_) => {
                    println!("Error loading scene from {}", &l);
                }
            },
            _ => (),
        };
        return renderer;
    }

    pub fn cleanup(&mut self) {
        self.backend.cleanup();
    }

    pub fn update(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let dt = self.clock.elapsed().as_millis() as f32 / 1000f32;
        self.clock = Instant::now();
        let ok = self.window.borrow_mut().update();

        if ok {
            match &self.input_handler {
                Some(i) => i.borrow_mut().update(dt),
                None => {}
            };
            match &self.camera {
                Some(c) => {
                    c.borrow_mut().update(dt);
                    self.vertex_shader_uniforms.data.view = c.borrow().view_mat();
                    self.pixel_shader_uniforms.data.camera_pos =
                        glm::vec3_to_vec4(&c.borrow().position());
                }
                None => {}
            };
            self.vertex_shader_uniforms.update()?;
            self.pixel_shader_uniforms.update()?;
            self.render()?;
        }
        Ok(ok)
    }

    pub fn unload_scene(&mut self) -> Result<(), ()> {
        // todo: error handling
        match self.scene.clear() {
            Ok(_) => Ok(()),
            Err(_) => Err(()),
        }
    }

    pub fn load_scene(&mut self, scene_file: &str) -> Result<(), ()> {
        let node = import::load_gltf(
            //"assets/sponza_glTF/Sponza.gltf",
            scene_file,
            self.backend.get_device(),
            self.backend.get_context(),
        )
        .expect("Unable to load scene");
        //let node = import::load_gltf("assets/gltf_uv_test/TextureCoordinateTest.gltf", renderer.backend.get_device(), renderer.backend.get_context()).expect("Unable to load scene");
        self.scene.set_root(node);
        // todo: err handling
        Ok(())
    }

    pub fn change_input_handler(
        &mut self,
        handler: std::rc::Rc<std::cell::RefCell<dyn InputHandler>>,
    ) {
        match &self.input_handler {
            Some(i) => {
                std::mem::drop(i);
                self.input_handler = Some(handler.clone());
            }
            None => self.input_handler = Some(handler.clone()),
        };
        self.window.borrow_mut().set_input_handler(handler.clone())
    }
    pub fn change_camera(&mut self, cam: std::rc::Rc<std::cell::RefCell<dyn Camera>>) {
        match &self.camera {
            Some(c) => {
                std::mem::drop(c);
                self.camera = Some(cam.clone());
            }
            None => self.camera = Some(cam.clone()),
        }
    }

    /**
     * Section: Render funcs
     */
    fn clear(&self) {
        self.backend.pix_begin_event("Clear");

        let ctx = self.backend.get_context();
        let render_target = self.backend.get_render_target_view();
        let depth_stencil = self.backend.get_depth_stencil_view();

        let color: [f32; 4] = colors_linear::background().into();
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

    fn render(&mut self) -> Result<(), DxError> {
        self.clear();
        match &self.draw_program {
            Some(_) => self.draw_program.as_mut().unwrap().activate(),
            None => {}
        };
        self.backend.pix_begin_event("Render");

        let ctx = self.backend.get_context();
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

        self.scene.draw();

        self.backend.pix_end_event();

        self.backend.present()?;

        Ok(())
    }

    fn init_draw_program(&mut self) -> Result<(), DxError> {
        self.draw_program = Some(self.backend.create_shader_program()?);

        Ok(())
    }
}

#[allow(dead_code)] // we don't want warnings if some color is not used..
pub mod colors_linear {
    pub fn background() -> glm::Vec4 {
        glm::vec4(0.052860655f32, 0.052860655f32, 0.052860655f32, 1.0f32)
    }
    pub fn green() -> glm::Vec4 {
        glm::vec4(0.005181516f32, 0.201556236f32, 0.005181516f32, 1.0f32)
    }
    pub fn blue() -> glm::Vec4 {
        glm::vec4(0.001517635f32, 0.114435382f32, 0.610495627f32, 1.0f32)
    }
    pub fn red() -> glm::Vec4 {
        glm::vec4(0.545724571f32, 0.026241219f32, 0.001517635f32, 1.0f32)
    }
    pub fn white() -> glm::Vec4 {
        glm::vec4(0.052860655f32, 0.052860655f32, 0.052860655f32, 1.0f32)
    }
}
