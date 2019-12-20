//pub mod generate;
pub mod geometry;
pub mod scenegraph;

pub(crate) mod d3d11;
pub(crate) mod settings;

mod draw_programs;

use crate::import;
use crate::input::first_person::FPSController;
use crate::input::input_handler::InputHandler;
use crate::input::Camera;
use crate::window::Window;

use d3d11::{drawable::ObjType, skybox::SkyBox, D3D11Backend, DxError};
use scenegraph::Scenegraph;
use std::time::Instant;
use winapi::um::d3d11 as dx11;

pub struct Renderer {
    scene: Scenegraph,
    skybox: Option<SkyBox>,
    screen_quad: d3d11::drawable::ScreenQuad,
    forward_program: Option<draw_programs::ForwardPass>,
    deferred_program_pre: Option<draw_programs::DeferredPassPre>,
    deferred_program_light: Option<draw_programs::DeferredPassLight>,
    shadow_program: Option<draw_programs::ShadowPass>,
    skybox_program: Option<draw_programs::SkyBoxPass>,
    input_handler: Option<std::rc::Rc<std::cell::RefCell<dyn InputHandler>>>,
    camera: Option<std::rc::Rc<std::cell::RefCell<dyn Camera>>>,
    backend: D3D11Backend,
    window: std::rc::Rc<std::cell::RefCell<Window>>,
    clock: Instant,
    frame_counter: u32,
    frame_time: f32,
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
        let quad = d3d11::drawable::ScreenQuad::create(backend.get_device())
            .expect("Error generating ScreenQuad");
        let mut renderer = Renderer {
            backend: backend,
            window: window,
            forward_program: None,
            deferred_program_pre: None,
            deferred_program_light: None,
            shadow_program: None,
            skybox_program: None,
            scene: Scenegraph::empty(),
            skybox: None,
            screen_quad: quad,
            input_handler: None,
            camera: None,
            clock: Instant::now(),
            frame_counter: 0,
            frame_time: 0.0f32,
        };

        let mut renderer = match renderer.init_draw_program() {
            Ok(_) => renderer,
            Err(e) => {
                // use std::error::Error; // put error description() trait in scope
                // println!(
                //     "Error during draw program initialization:\n{}",
                //     e.description()
                // );
                panic!(format!("{}", e))
            }
        };

        renderer.change_input_handler(input_handler.clone());
        renderer.change_camera(input_handler.clone());

        // TODO: handle optional values here correctly?
        renderer
            .forward_program
            .as_mut()
            .unwrap()
            .set_proj(input_handler.borrow().projection_mat(), false)
            .expect("Impossible");
        renderer
            .deferred_program_pre
            .as_mut()
            .unwrap()
            .set_camera_planes(input_handler.borrow().near_far(), false)
            .expect("Error updating shader constants");
        renderer
            .deferred_program_pre
            .as_mut()
            .unwrap()
            .set_proj(input_handler.borrow().projection_mat(), false)
            .expect("Error setting projection matrix");
        renderer
            .skybox_program
            .as_mut()
            .unwrap()
            .set_proj(input_handler.borrow().projection_mat(), false)
            .expect("Error setting projection matrix [SkyBox]");

        println!("DX Setup took {} ms", renderer.clock.elapsed().as_millis());
        renderer.clock = Instant::now();
        match settings.level {
            Some(l) => match renderer.load_scene(&l) {
                Ok(_) => {
                    println!(
                        "Loaded scene in {} ms",
                        renderer.clock.elapsed().as_millis()
                    );
                    let light = renderer.scene.get_directional_light();
                    let shadow_dist = 10.0;
                    let light_proj = glm::ortho_zo(
                        -shadow_dist,
                        shadow_dist,
                        -shadow_dist,
                        shadow_dist,
                        1.0,
                        70.0,
                    );
                    let dir = light.direction.xyz() * (-1.0);
                    let mut up = glm::vec3(0.0, 1.0, 0.0);
                    if (up.dot(&dir.normalize()) - 1.0).abs() <= 0.0000001 {
                        up = glm::vec3(0.0, 0.0, 1.0);
                    }
                    //println!("{}", light_proj);
                    let light_view = glm::look_at(&dir, &glm::zero(), &up);
                    // println!("{}", light_view);
                    let light_space_mat = light_proj * light_view;
                    renderer
                        .forward_program
                        .as_mut()
                        .unwrap()
                        .set_directional_light((*light).clone(), false)
                        .expect("Impossible");
                    renderer
                        .forward_program
                        .as_mut()
                        .unwrap()
                        .set_light_space_matrix(light_space_mat, false)
                        .expect("Impossible");
                    match &mut renderer.shadow_program {
                        Some(p) => {
                            p.set_light_space(light_space_mat, true)
                                .expect("Internal error when setting light space matrix");
                        }
                        None => (),
                    };

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

        let mut ok = true;
        if self.frame_time >= 1.0f32 {
            ok = self
                .window
                .borrow_mut()
                .set_title(&format!("{} FPS", self.frame_counter));
            self.frame_counter = 0;
            self.frame_time = 0.0f32;
        }
        if ok {
            ok = self.window.borrow_mut().update();
        }

        if ok {
            match &self.input_handler {
                Some(i) => i.borrow_mut().update(dt),
                None => {}
            };
            match &self.camera {
                Some(c) => {
                    c.borrow_mut().update(dt);
                    let light = self.scene.get_directional_light();
                    let dir = light.direction.xyz() * (-1.0);
                    let mut up = glm::vec3(0.0, 1.0, 0.0);
                    if (up.dot(&dir.normalize()) - 1.0).abs() <= 0.0000001 {
                        up = glm::vec3(0.0, 0.0, 1.0);
                    }
                    //println!("{}", light_proj);
                    let pos = &c.borrow().position();
                    let light_view = glm::look_at(&(pos + dir), &pos, &up);
                    //println!("{}", light_view);
                    let light_space_mat = self.scene.get_light_proj() * light_view;

                    match &self.forward_program {
                        Some(_) => {
                            &self
                                .forward_program
                                .as_mut()
                                .unwrap()
                                .set_view(c.borrow().view_mat(), false)
                                .expect("Error setting view mat");
                            &self
                                .forward_program
                                .as_mut()
                                .unwrap()
                                .set_camera_pos(glm::vec3_to_vec4(&c.borrow().position()), false)
                                .expect("Error setting camera pos");

                            &self
                                .forward_program
                                .as_mut()
                                .unwrap()
                                .set_directional_light((*light).clone(), false)
                                .expect("Impossible");
                            &self
                                .forward_program
                                .as_mut()
                                .unwrap()
                                .set_light_space_matrix(light_space_mat, false)
                                .expect("Impossible");
                            &self.forward_program.as_mut().unwrap().update();
                        }
                        _ => (),
                    };
                    if let Some(deferred_pre) = &mut self.deferred_program_pre {
                        deferred_pre
                            .set_view(c.borrow().view_mat(), false)
                            .expect("Error updating view matrix");
                        deferred_pre.update()?;
                    }
                    if let Some(deferred_light) = &mut self.deferred_program_light {
                        deferred_light
                            .set_camera_pos(glm::vec3_to_vec4(&c.borrow().position()), false)
                            .expect("Error setting camera pos");
                        deferred_light
                            .set_directional_light((*light).clone(), false)
                            .expect("Impossible");
                        deferred_light
                            .set_light_space_matrix(light_space_mat, false)
                            .expect("Error updating LS matrix");
                        deferred_light.update()?;
                    }
                    if let Some(p) = &mut self.shadow_program {
                        p.set_light_space(light_space_mat, true)
                            .expect("Internal error when setting light space matrix");
                    };
                    if let Some(sky_prog) = &mut self.skybox_program {
                        sky_prog
                            .set_view(
                                glm::mat3_to_mat4(&glm::mat4_to_mat3(&c.borrow().view_mat())),
                                true,
                            )
                            .expect("Error updating view matrix [SkyBox]");
                    }
                    // if let Some(sky) = &mut self.skybox {
                    //     let model = glm::translate(&glm::identity(), &c.borrow().position());
                    //     sky.update(&model);
                    // }
                }
                None => {}
            };
            self.render()?;
        }
        self.frame_counter = self.frame_counter + 1;
        self.frame_time = self.frame_time + dt;
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
        println!("Reading scene file...");
        let node = import::load_gltf(
            //"assets/sponza_glTF/Sponza.gltf",
            scene_file,
            self.backend.get_device(),
            self.backend.get_context(),
        )
        .expect("Unable to load scene");
        //let node = import::load_gltf("assets/gltf_uv_test/TextureCoordinateTest.gltf", renderer.backend.get_device(), renderer.backend.get_context()).expect("Unable to load scene");
        println!("Processing scene...");
        self.scene.set_root(node);
        self.scene.set_directional_light(geometry::Light {
            direction: glm::vec4(-15.0, -50.0, -5.0, 1.0),
            color: glm::vec4(0.3, 0.3, 0.3, 1.0),
        });
        self.scene.build_matrices();

        println!("Loading skybox...");
        self.skybox = Some(
            SkyBox::new(self.backend.get_device(), self.backend.get_context())
                .expect("Error loading skybox"),
        );

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
        let shadow_map_dt = match &self.shadow_program {
            Some(sp) => sp.get_depth_stencil_view(),
            None => std::ptr::null_mut(),
        };

        let color: [f32; 4] = colors_linear::background().into();
        //let shadow_map_clear_color: [f32; 4] = glm::zero::<glm::Vec4>().into();
        unsafe {
            (*ctx).ClearRenderTargetView(render_target, &color);
            if !shadow_map_dt.is_null() {
                //(*ctx).ClearRenderTargetView(shadow_map_rt, &shadow_map_clear_color);
                (*ctx).ClearDepthStencilView(
                    shadow_map_dt,
                    dx11::D3D11_CLEAR_DEPTH | dx11::D3D11_CLEAR_STENCIL,
                    1.0f32,
                    0,
                );
            }
        }
        if let Some(dp) = &self.deferred_program_pre {
            let targets = dp.get_render_targets();
            for tv in &targets {
                unsafe { (*ctx).ClearRenderTargetView(*tv, &color) };
            }
        }
        unsafe {
            (*ctx).ClearDepthStencilView(
                depth_stencil,
                dx11::D3D11_CLEAR_DEPTH | dx11::D3D11_CLEAR_STENCIL,
                1.0f32,
                0,
            );
        };

        self.backend.pix_end_event();
    }

    fn render(&mut self) -> Result<(), DxError> {
        self.clear();

        let ctx = self.backend.get_context();
        self.backend.disable_blend();
        match &mut self.shadow_program {
            Some(sp) => {
                self.backend.pix_begin_event("Shadow Mapping");
                let viewport = sp.get_shadow_map_viewport();
                unsafe { (*ctx).RSSetViewports(1, viewport) };
                unsafe {
                    let null_sampler: [*mut dx11::ID3D11SamplerState; 1] = [std::ptr::null_mut()];
                    let null_srv: [*mut dx11::ID3D11ShaderResourceView; 1] = [std::ptr::null_mut()];
                    (*ctx).PSSetSamplers(5, 1, null_sampler.as_ptr());
                    (*ctx).PSSetShaderResources(5, 1, null_srv.as_ptr());
                };
                let depth_stencil = sp.get_depth_stencil_view();
                unsafe { (*ctx).OMSetRenderTargets(0, std::ptr::null(), depth_stencil) };
                self.shadow_program.as_mut().unwrap().prepare_draw(ctx);

                self.scene.draw(ObjType::Any);
                self.backend.pix_end_event();
            }
            None => {}
        }

        let viewport = self.backend.get_viewport();
        unsafe { (*ctx).RSSetViewports(1, viewport) };
        let depth_stencil = self.backend.get_depth_stencil_view();

        if let Some(deferred_pre) = &mut self.deferred_program_pre {
            self.backend.pix_begin_event("Deferred Pre Pass");
            deferred_pre.prepare_draw(ctx);

            let null_srv: [*mut dx11::ID3D11ShaderResourceView; 2] =
                [std::ptr::null_mut(), std::ptr::null_mut()];
            unsafe {
                (*ctx).PSSetShaderResources(0, 2, null_srv.as_ptr());
            }
            let targets = deferred_pre.get_render_targets();
            unsafe {
                (*ctx).OMSetRenderTargets(targets.len() as _, targets.as_ptr(), depth_stencil)
            };

            self.scene.draw(ObjType::Opaque);

            self.backend.pix_end_event();
        }

        let render_target = self.backend.get_render_target_view();
        unsafe { (*ctx).OMSetRenderTargets(1, &render_target, std::ptr::null_mut()) };
        if let Some(deferred_light) = &mut self.deferred_program_light {
            self.backend.pix_begin_event("Deferred Light Pass");
            deferred_light.prepare_draw(ctx);

            if let Some(sp) = &self.shadow_program {
                let tex = sp.get_shadow_map();
                unsafe {
                    (*ctx).PSSetSamplers(5, 1, &tex.get_sampler() as *const *mut _);
                    (*ctx).PSSetShaderResources(5, 1, &tex.get_texture_view() as *const *mut _);
                }
            }

            if let Some(dp) = &self.deferred_program_pre {
                let pos = dp.positions();
                let albedo = dp.albedo();

                let texs = [pos.get_texture_view(), albedo.get_texture_view()];
                unsafe {
                    (*ctx).PSSetShaderResources(0, 2, texs.as_ptr());
                }

                self.screen_quad.draw(ctx);
                self.backend.pix_end_event();
            }
        }

        if let Some(fwd) = &mut self.forward_program {
            self.backend.enable_blend();
            self.backend.pix_begin_event("Main Pass");

            unsafe { (*ctx).OMSetRenderTargets(1, &render_target, depth_stencil) };
            fwd.prepare_draw(ctx);
            match &self.shadow_program {
                Some(sp) => {
                    let tex = sp.get_shadow_map();
                    unsafe {
                        (*ctx).PSSetSamplers(5, 1, &tex.get_sampler() as *const *mut _);
                        (*ctx).PSSetShaderResources(5, 1, &tex.get_texture_view() as *const *mut _);
                    }
                }
                None => (),
            };
            self.scene.draw(ObjType::Transparent);
            self.backend.pix_end_event();
        };

        if let Some(skbp) = &mut self.skybox_program {
            if let Some(sky) = &self.skybox {
                self.backend.pix_begin_event("Skybox");
                skbp.prepare_draw(ctx);

                sky.draw();
                self.backend.pix_end_event();
            }
        }
        self.backend.present()?;

        Ok(())
    }

    fn init_draw_program(&mut self) -> Result<(), DxError> {
        self.forward_program = Some(draw_programs::ForwardPass::create(
            self.backend.get_device(),
            self.backend.get_context(),
        )?);
        self.deferred_program_pre = Some(draw_programs::DeferredPassPre::create(
            self.window.borrow().get_resolution(),
            self.backend.get_device(),
            self.backend.get_context(),
        )?);
        self.deferred_program_light = Some(draw_programs::DeferredPassLight::create(
            self.backend.get_device(),
            self.backend.get_context(),
        )?);
        self.shadow_program = Some(draw_programs::ShadowPass::create_simple(
            self.backend.get_device(),
            self.backend.get_context(),
        )?);
        self.skybox_program = Some(draw_programs::SkyBoxPass::create(
            self.backend.get_device(),
            self.backend.get_context(),
        )?);

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
