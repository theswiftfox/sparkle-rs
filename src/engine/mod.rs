//pub mod generate;
pub(crate) mod d3d11;
pub(crate) mod draw_programs;
pub(crate) mod gameworks;
pub(crate) mod geometry;
pub(crate) mod scenegraph;
pub(crate) mod settings;

use crate::import;
use crate::input::first_person::FPSController;
use crate::input::input_handler::InputHandler;
use crate::input::Camera;
use crate::window::Window;

use d3d11::{drawable::ObjType, skybox::SkyBox, D3D11Backend, DxError};
use scenegraph::Scenegraph;
use scenegraph::Scenegraph;
use std::time::Instant;
use winapi::um::d3d11 as dx11;

pub struct Renderer {
    settings: settings::Settings,
    scene: Scenegraph,
    skybox: Option<SkyBox>,
    shadow_dist: f32,
    screen_quad: d3d11::drawable::ScreenQuad,
    forward_program: Option<draw_programs::ForwardPass>,
    deferred_program_pre: Option<draw_programs::DeferredPassPre>,
    deferred_program_light: Option<draw_programs::DeferredPassLight>,
    shadow_program: Option<draw_programs::ShadowPass>,
    skybox_program: Option<draw_programs::SkyBoxPass>,
    ssao_program: Option<gameworks::SSAO>,
    output_program: Option<draw_programs::OutputPass>,
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
            settings: settings,
            shadow_dist: 50.0,
            backend: backend,
            window: window,
            forward_program: None,
            deferred_program_pre: None,
            deferred_program_light: None,
            shadow_program: None,
            skybox_program: None,
            ssao_program: None,
            output_program: None,
            scene: Scenegraph::empty(),
            skybox: None,
            screen_quad: quad,
            input_handler: None,
            camera: Some(input_handler.clone()),
            clock: Instant::now(),
            frame_counter: 0,
            frame_time: 0.0f32,
        };

        let mut renderer = match renderer.init_draw_program() {
            Ok(_) => renderer,
            Err(e) => panic!(format!("{}", e)),
        };

        renderer.change_input_handler(input_handler.clone());

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
        match renderer.settings.level.clone() {
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
                Some(i) => i.borrow_mut().update(dt, &mut self.settings),
                None => {}
            };
            match &self.camera {
                Some(c) => {
                    c.borrow_mut().update(dt);

                    if let Some(forward_program) = &mut self.forward_program {
                        forward_program
                            .set_view(c.borrow().view_mat(), false)
                            .expect("Error setting view mat");
                        forward_program
                            .set_camera_pos(c.borrow().position(), false)
                            .expect("Error setting camera pos");
                        let ssao = match self.settings.ssao {
                            true => 1,
                            false => 0,
                        };
                        forward_program
                            .set_ssao(ssao, false)
                            .expect("Error changing ssao state");
                        forward_program.update()?;
                    }
                    if let Some(deferred_pre) = &mut self.deferred_program_pre {
                        deferred_pre
                            .set_view(c.borrow().view_mat(), false)
                            .expect("Error updating view matrix");
                        deferred_pre.update()?;
                    }
                    if let Some(deferred_light) = &mut self.deferred_program_light {
                        deferred_light
                            .set_camera_pos(c.borrow().position(), false)
                            .expect("Error setting camera pos");
                        let ssao = match self.settings.ssao {
                            true => 1,
                            false => 0,
                        };
                        deferred_light
                            .set_ssao(ssao, false)
                            .expect("Error setting ssao state");
                        deferred_light.update()?;
                    }
                    if let Some(sky_prog) = &mut self.skybox_program {
                        sky_prog
                            .set_view(
                                glm::mat3_to_mat4(&glm::mat4_to_mat3(&c.borrow().view_mat())),
                                true,
                            )
                            .expect("Error updating view matrix [SkyBox]");
                    }
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
            scene_file,
            self.backend.get_device(),
            self.backend.get_context(),
        )
        .expect("Unable to load scene");
        println!("Processing scene...");
        self.scene.set_root(node);
        self.scene.add_light(geometry::Light::default()); // ambient light; TODO: maybe use this to allow for changing light params on the fly
        let shadow_dist = 20.0;
        self.scene.add_light(geometry::Light {
            position: glm::vec3(-0.15, -0.5, -0.05).normalize(),
            // direction: glm::vec4(0.0, -1.5, -1.5, 1.0).normalize(),
            t: geometry::LightType::Directional,
            color: glm::vec3(23.47, 21.31, 20.79),
            radius: 1.0,
            light_proj: glm::ortho_zo(
                -shadow_dist,
                shadow_dist,
                -shadow_dist,
                shadow_dist,
                1.0,
                2.5 * self.shadow_dist,
            ),
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

        let bg_color: [f32; 4] = colors_linear::background().into();
        unsafe {
            (*ctx).ClearRenderTargetView(render_target, &bg_color);
        }
        let color: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
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
        if let Some(ssao) = &self.ssao_program {
            let target = ssao.get_render_target_view();
            unsafe { (*ctx).ClearRenderTargetView(target, &color) };
        }
        if let Some(def) = &self.deferred_program_light {
            let tv = def.get_render_target_view();
            unsafe { (*ctx).ClearRenderTargetView(tv, &color) };
        }
        if let Some(fwd) = &self.forward_program {
            let tv = fwd.get_render_target_view();
            unsafe { (*ctx).ClearRenderTargetView(tv, &color) };
        }

        self.backend.pix_end_event();
    }

    fn render(&mut self) -> Result<(), DxError> {
        self.clear();

        let ctx = self.backend.get_context();
        self.backend.disable_blend();

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

        if self.settings.ssao {
            if let Some(ssao) = &mut self.ssao_program {
                unsafe {
                    (*ctx).PSSetShaderResources(4, 1, &std::ptr::null_mut() as *const *mut _);
                }
                let tv = ssao.get_render_target_view();
                self.backend.pix_begin_event("HBAO+");
                unsafe { (*ctx).OMSetRenderTargets(1, &tv, std::ptr::null_mut()) };
                ssao.render(ctx);
                self.backend.pix_end_event();
            }
        }

        for light in self.scene.get_lights() {
            self.backend.enable_add_blend();
            let mut l = geometry::Light::default();
            if light.t != geometry::LightType::Ambient {
                if let Some(c) = &self.camera {
                    l = light.clone();
                    let dir = light.position.xyz() * (-1.0) * self.shadow_dist;
                    let mut up = glm::vec3(0.0, 1.0, 0.0);
                    if (up.dot(&dir.normalize()) - 1.0).abs() <= 0.0000001 {
                        up = glm::vec3(0.0, 0.0, 1.0);
                    }
                    let pos = c.borrow().position();
                    let light_view = glm::look_at(&(pos + dir), &pos, &up);
                    l.light_proj = light.light_proj * light_view;
                }

                if let Some(sp) = &mut self.shadow_program {
                    let shadow_map_dt = sp.get_depth_stencil_view();
                    unsafe {
                        (*ctx).ClearDepthStencilView(
                            shadow_map_dt,
                            dx11::D3D11_CLEAR_DEPTH | dx11::D3D11_CLEAR_STENCIL,
                            1.0f32,
                            0,
                        );
                    }
                    sp.set_light_space(l.light_proj.clone(), true)
                        .expect("Error setting light space mat for shadowmapping");
                    self.backend.pix_begin_event("Shadow Mapping");
                    self.backend.cull_front();
                    let viewport = sp.get_shadow_map_viewport();
                    unsafe { (*ctx).RSSetViewports(1, viewport) };
                    unsafe {
                        let null_sampler: [*mut dx11::ID3D11SamplerState; 1] =
                            [std::ptr::null_mut()];
                        let null_srv: [*mut dx11::ID3D11ShaderResourceView; 1] =
                            [std::ptr::null_mut()];
                        (*ctx).PSSetSamplers(5, 1, null_sampler.as_ptr());
                        (*ctx).PSSetShaderResources(5, 1, null_srv.as_ptr());
                    };
                    let depth_stencil = sp.get_depth_stencil_view();
                    unsafe { (*ctx).OMSetRenderTargets(0, std::ptr::null(), depth_stencil) };
                    self.shadow_program.as_mut().unwrap().prepare_draw(ctx);

                    self.scene.draw(ObjType::Any);
                    self.backend.cull_back();
                    self.backend.pix_end_event();
                }
            }

            let viewport = self.backend.get_viewport();
            unsafe { (*ctx).RSSetViewports(1, viewport) };
            if let Some(deferred_light) = &mut self.deferred_program_light {
                let render_target = deferred_light.get_render_target_view();
                unsafe { (*ctx).OMSetRenderTargets(1, &render_target, std::ptr::null_mut()) };
                self.backend.pix_begin_event("Deferred Light Pass");
                deferred_light
                    .set_light(&l, true)
                    .expect("Error updating light");
                deferred_light.prepare_draw(ctx);

                if let Some(sp) = &self.shadow_program {
                    let tex = sp.get_shadow_map();
                    unsafe {
                        (*ctx).PSSetSamplers(5, 1, &tex.get_sampler() as *const *mut _);
                        (*ctx).PSSetShaderResources(5, 1, &tex.get_texture_view() as *const *mut _);
                    }
                }

                if let Some(ssao) = &mut self.ssao_program {
                    let tex = ssao.get_render_target();
                    unsafe {
                        (*ctx).PSSetShaderResources(4, 1, &tex.get_texture_view() as *const *mut _);
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
                let render_target = fwd.get_render_target_view();
                self.backend.enable_alpha_blend();
                self.backend.pix_begin_event("Main Pass");
                fwd.set_light(&l, true).expect("Error updating light");

                unsafe { (*ctx).OMSetRenderTargets(1, &render_target, depth_stencil) };
                fwd.prepare_draw(ctx);
                if self.deferred_program_light.is_none() {
                    if let Some(sp) = &self.shadow_program {
                        let tex = sp.get_shadow_map();
                        unsafe {
                            (*ctx).PSSetSamplers(5, 1, &tex.get_sampler() as *const *mut _);
                            (*ctx).PSSetShaderResources(
                                5,
                                1,
                                &tex.get_texture_view() as *const *mut _,
                            );
                        }
                    }
                }
                self.scene.draw(ObjType::Transparent);
                self.backend.pix_end_event();
            };
        }

        let render_target = self.backend.get_render_target_view();
        unsafe { (*ctx).OMSetRenderTargets(1, &render_target, std::ptr::null_mut()) };
        if let Some(op) = &mut self.output_program {
            self.backend.pix_begin_event("Forward+");
            op.prepare_draw();
            let mut rts = Vec::<*mut dx11::ID3D11ShaderResourceView>::new();
            if let Some(def) = &self.deferred_program_light {
                rts.push(def.get_render_target().get_texture_view());
            }
            if let Some(fwd) = &self.forward_program {
                rts.push(fwd.get_render_target().get_texture_view());
            }
            unsafe {
                (*ctx).PSSetShaderResources(0, rts.len() as u32, rts.as_ptr());
            }
            self.screen_quad.draw(ctx);
            self.backend.pix_end_event();
        }

        unsafe { (*ctx).OMSetRenderTargets(1, &render_target, depth_stencil) };

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
            self.window.borrow().get_resolution(),
            self.backend.get_device(),
            self.backend.get_context(),
        )?);
        self.deferred_program_pre = Some(draw_programs::DeferredPassPre::create(
            self.window.borrow().get_resolution(),
            self.backend.get_device(),
            self.backend.get_context(),
        )?);
        self.deferred_program_light = Some(draw_programs::DeferredPassLight::create(
            self.window.borrow().get_resolution(),
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
        self.ssao_program = Some(gameworks::SSAO::new(
            self.window.borrow().get_resolution(),
            self.backend.get_depth_stencil_shader_view(),
            self.camera.as_ref().unwrap().borrow().projection_mat(),
            self.backend.get_device(),
        )?);
        self.output_program = Some(draw_programs::OutputPass::create(
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
