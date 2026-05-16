//! The main engine renderer, generic over [`GpuBackend`].
//!
//! Orchestrates the full deferred+forward rendering pipeline:
//! deferred pre-pass -> SSAO -> per-light (shadow -> deferred light -> forward) -> output -> skybox.
//!
//! The Renderer owns the backend, scenegraph, and all render pass programs.
//! It does NOT own the window or event loop — those are managed externally
//! by the winit-based window module.

use super::backend::*;
use super::draw_programs::*;
use super::geometry::{Light, LightType};
use super::scenegraph::Scenegraph;
use super::settings::Settings;
use super::skybox::Skybox;

use crate::import;
use crate::input::input_handler::InputHandler;
use crate::input::Camera;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

pub struct Renderer<B: GpuBackend> {
    settings: Settings,
    scene: Scenegraph<B>,
    shadow_dist: f32,
    screen_quad: ScreenQuad<B>,
    forward_program: Option<ForwardPass<B>>,
    deferred_program_pre: Option<DeferredPassPre<B>>,
    deferred_program_light: Option<DeferredPassLight<B>>,
    shadow_program: Option<ShadowPass<B>>,
    skybox_program: Option<SkyBoxPass<B>>,
    skybox: Option<Skybox<B>>,
    ssao_program: Option<SsaoPass<B>>,
    output_program: Option<OutputPass<B>>,
    input_handler: Option<Rc<RefCell<dyn InputHandler>>>,
    camera: Option<Rc<RefCell<dyn Camera>>>,
    backend: B,
    clock: Instant,
}

impl<B: GpuBackend> Renderer<B> {
    /// Create a new renderer with the given backend and settings.
    ///
    /// Draw programs are initially None — they require compiled shaders
    /// which are provided later via `init_draw_programs()` (Phase 3).
    /// Without draw programs, the renderer falls back to clearing the screen.
    pub fn create(backend: B, settings: Settings) -> Result<Self, GpuError> {
        let screen_quad = ScreenQuad::create(&backend)?;

        Ok(Renderer {
            settings,
            scene: Scenegraph::empty(),
            shadow_dist: 50.0,
            screen_quad,
            forward_program: None,
            deferred_program_pre: None,
            deferred_program_light: None,
            shadow_program: None,
            skybox_program: None,
            skybox: None,
            ssao_program: None,
            output_program: None,
            input_handler: None,
            camera: None,
            backend,
            clock: Instant::now(),
        })
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn set_input_handler(&mut self, handler: Rc<RefCell<dyn InputHandler>>) {
        self.input_handler = Some(handler);
    }

    /// Set the camera and propagate its projection matrix to all passes.
    pub fn set_camera(&mut self, cam: Rc<RefCell<dyn Camera>>) {
        let proj = cam.borrow().projection_mat();
        let (near, far) = cam.borrow().near_far();

        if let Some(ref mut fwd) = self.forward_program {
            fwd.set_proj(proj);
        }
        if let Some(ref mut dp) = self.deferred_program_pre {
            dp.set_proj(proj);
            dp.set_camera_planes(near, far);
        }
        if let Some(ref mut sky) = self.skybox_program {
            sky.set_proj(proj);
        }
        if let Some(ref mut ssao) = self.ssao_program {
            ssao.set_proj(proj);
        }

        self.camera = Some(cam);
    }

    /// Resize the backend and (future) render targets.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.backend.resize(width, height);
        // TODO: recreate draw program render targets at new resolution
    }

    /// Initialize all draw programs from compiled WGSL shaders.
    ///
    /// This creates the full rendering pipeline:
    /// deferred pre → SSAO → shadow → deferred light → forward → output → skybox.
    /// After this call, the renderer will use the full pipeline instead of
    /// the fallback clear-to-screen path.
    pub fn init_draw_programs(&mut self) -> Result<(), GpuError> {
        let resolution = self.backend.resolution();
        let backbuffer_format = self.backend.backbuffer().format();

        // Load WGSL shaders
        let deferred_pre_wgsl = include_bytes!("../shaders/wgsl/deferred_pre.wgsl");
        let ssao_wgsl = include_bytes!("../shaders/wgsl/ssao.wgsl");
        let ssao_blur_wgsl = include_bytes!("../shaders/wgsl/ssao_blur.wgsl");
        let shadow_wgsl = include_bytes!("../shaders/wgsl/shadow.wgsl");
        let deferred_light_wgsl = include_bytes!("../shaders/wgsl/deferred_light.wgsl");
        let forward_wgsl = include_bytes!("../shaders/wgsl/forward.wgsl");
        let output_wgsl = include_bytes!("../shaders/wgsl/output.wgsl");
        let skybox_wgsl = include_bytes!("../shaders/wgsl/skybox.wgsl");

        println!("Initializing draw programs...");

        // Deferred pre-pass (G-buffer fill)
        self.deferred_program_pre = Some(DeferredPassPre::create(
            &self.backend,
            resolution,
            deferred_pre_wgsl,
        )?);
        println!("  deferred_pre: OK");

        // SSAO pass (ambient occlusion)
        self.ssao_program = Some(SsaoPass::create(
            &self.backend,
            resolution,
            ssao_wgsl,
            ssao_blur_wgsl,
        )?);
        println!("  ssao: OK");

        // Shadow mapping pass
        self.shadow_program = Some(ShadowPass::create(&self.backend, shadow_wgsl)?);
        println!("  shadow: OK");

        // Deferred lighting pass
        self.deferred_program_light = Some(DeferredPassLight::create(
            &self.backend,
            resolution,
            deferred_light_wgsl,
        )?);
        println!("  deferred_light: OK");

        // Forward pass (transparent objects)
        self.forward_program = Some(ForwardPass::create(
            &self.backend,
            resolution,
            forward_wgsl,
        )?);
        println!("  forward: OK");

        // Output composite pass
        self.output_program = Some(OutputPass::create(
            &self.backend,
            output_wgsl,
            backbuffer_format,
        )?);
        println!("  output: OK");

        // Skybox pass
        self.skybox_program = Some(SkyBoxPass::create(
            &self.backend,
            skybox_wgsl,
            backbuffer_format,
        )?);
        println!("  skybox: OK");

        println!("All draw programs initialized.");
        Ok(())
    }

    /// Load a glTF scene file.
    pub fn load_scene(&mut self, scene_file: &str) -> Result<(), import::ImportError> {
        println!("Reading scene file...");
        let node = import::load_gltf(scene_file, &self.backend)?;
        println!("Processing scene...");
        self.scene.set_root(node);

        // Ambient light — color controls fill intensity in shadowed areas
        self.scene.add_light(Light {
            color: glm::vec3(0.25, 0.25, 0.25),
            ..Light::default()
        });

        // Directional light for shadow mapping
        let shadow_dist = 20.0;
        self.scene.add_light(Light {
            position: glm::vec3(-0.15, -0.5, -0.05).normalize(),
            t: LightType::Directional,
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

        self.scene.build_matrices(&self.backend);

        // Load skybox cubemap
        println!("Loading skybox...");
        match Skybox::load(&self.backend) {
            Ok(sky) => {
                self.skybox = Some(sky);
            }
            Err(e) => {
                println!(
                    "Warning: skybox loading failed: {} (continuing without skybox)",
                    e
                );
            }
        }

        Ok(())
    }

    /// Load a minimal test scene: a textured unit cube at the origin.
    ///
    /// Bypasses glTF import entirely to verify the render pipeline works
    /// with known-good geometry. Creates solid-color textures inline.
    pub fn load_test_scene(&mut self) -> Result<(), GpuError> {
        use super::geometry::Vertex;
        use super::scenegraph::node::Node;

        println!("Loading test scene (unit cube at origin)...");

        // --- Create test textures (1x1 solid colors) ---

        // Albedo: bright red (sRGB)
        let albedo_tex = Rc::new(self.backend.create_texture(
            &TextureDesc {
                width: 1,
                height: 1,
                format: TextureFormat::Rgba8UnormSrgb,
                sampler: SamplerDesc {
                    address_u: AddressMode::Clamp,
                    address_v: AddressMode::Clamp,
                    filter: FilterMode::Nearest,
                    compare: None,
                },
                generate_mipmaps: false,
            },
            &[220, 50, 50, 255],
        )?);

        // Metallic-roughness: roughness=0.5 (G=128), metallic=0 (B=0)
        // Note: loaded with sRGB=false since MR is data, not color
        let mr_tex = Rc::new(self.backend.create_texture(
            &TextureDesc {
                width: 1,
                height: 1,
                format: TextureFormat::Rgba8Unorm,
                sampler: SamplerDesc {
                    address_u: AddressMode::Clamp,
                    address_v: AddressMode::Clamp,
                    filter: FilterMode::Nearest,
                    compare: None,
                },
                generate_mipmaps: false,
            },
            &[0, 128, 0, 255],
        )?);

        // Normal map: flat (0,0,1) encoded as (128,128,255)
        let normal_tex = Rc::new(self.backend.create_texture(
            &TextureDesc {
                width: 1,
                height: 1,
                format: TextureFormat::Rgba8Unorm,
                sampler: SamplerDesc {
                    address_u: AddressMode::Clamp,
                    address_v: AddressMode::Clamp,
                    filter: FilterMode::Nearest,
                    compare: None,
                },
                generate_mipmaps: false,
            },
            &[128, 128, 255, 255],
        )?);

        // --- Build cube geometry ---
        // 24 vertices (4 per face), 36 indices (6 per face)
        // CCW winding when viewed from outside (matches FrontFace::Ccw)

        struct FaceData {
            normal: glm::Vec3,
            tangent: glm::Vec3,
            bitangent: glm::Vec3,
            positions: [[f32; 3]; 4], // bl, br, tr, tl when viewed from outside
        }

        let faces = [
            // +Z face (front)
            FaceData {
                normal: glm::vec3(0.0, 0.0, 1.0),
                tangent: glm::vec3(1.0, 0.0, 0.0),
                bitangent: glm::vec3(0.0, 1.0, 0.0),
                positions: [
                    [-0.5, -0.5, 0.5],
                    [0.5, -0.5, 0.5],
                    [0.5, 0.5, 0.5],
                    [-0.5, 0.5, 0.5],
                ],
            },
            // -Z face (back)
            FaceData {
                normal: glm::vec3(0.0, 0.0, -1.0),
                tangent: glm::vec3(-1.0, 0.0, 0.0),
                bitangent: glm::vec3(0.0, 1.0, 0.0),
                positions: [
                    [0.5, -0.5, -0.5],
                    [-0.5, -0.5, -0.5],
                    [-0.5, 0.5, -0.5],
                    [0.5, 0.5, -0.5],
                ],
            },
            // +X face (right)
            FaceData {
                normal: glm::vec3(1.0, 0.0, 0.0),
                tangent: glm::vec3(0.0, 0.0, -1.0),
                bitangent: glm::vec3(0.0, 1.0, 0.0),
                positions: [
                    [0.5, -0.5, 0.5],
                    [0.5, -0.5, -0.5],
                    [0.5, 0.5, -0.5],
                    [0.5, 0.5, 0.5],
                ],
            },
            // -X face (left)
            FaceData {
                normal: glm::vec3(-1.0, 0.0, 0.0),
                tangent: glm::vec3(0.0, 0.0, 1.0),
                bitangent: glm::vec3(0.0, 1.0, 0.0),
                positions: [
                    [-0.5, -0.5, -0.5],
                    [-0.5, -0.5, 0.5],
                    [-0.5, 0.5, 0.5],
                    [-0.5, 0.5, -0.5],
                ],
            },
            // +Y face (top)
            FaceData {
                normal: glm::vec3(0.0, 1.0, 0.0),
                tangent: glm::vec3(1.0, 0.0, 0.0),
                bitangent: glm::vec3(0.0, 0.0, -1.0),
                positions: [
                    [-0.5, 0.5, 0.5],
                    [0.5, 0.5, 0.5],
                    [0.5, 0.5, -0.5],
                    [-0.5, 0.5, -0.5],
                ],
            },
            // -Y face (bottom)
            FaceData {
                normal: glm::vec3(0.0, -1.0, 0.0),
                tangent: glm::vec3(1.0, 0.0, 0.0),
                bitangent: glm::vec3(0.0, 0.0, 1.0),
                positions: [
                    [-0.5, -0.5, -0.5],
                    [0.5, -0.5, -0.5],
                    [0.5, -0.5, 0.5],
                    [-0.5, -0.5, 0.5],
                ],
            },
        ];

        let face_uvs = [
            glm::vec2(0.0, 1.0), // bl
            glm::vec2(1.0, 1.0), // br
            glm::vec2(1.0, 0.0), // tr
            glm::vec2(0.0, 0.0), // tl
        ];

        let mut vertices: Vec<Vertex> = Vec::with_capacity(24);
        let mut indices: Vec<u32> = Vec::with_capacity(36);

        for (fi, face) in faces.iter().enumerate() {
            let base = (fi * 4) as u32;
            for vi in 0..4 {
                let p = face.positions[vi];
                vertices.push(Vertex {
                    position: glm::vec3(p[0], p[1], p[2]),
                    normal: face.normal,
                    tangent: face.tangent,
                    bitangent: face.bitangent,
                    tex_coord: face_uvs[vi],
                });
            }
            // Two CCW triangles per face: (0,1,2) and (0,2,3)
            indices.push(base);
            indices.push(base + 1);
            indices.push(base + 2);
            indices.push(base);
            indices.push(base + 2);
            indices.push(base + 3);
        }

        // --- Create drawable ---
        let drawable = Drawable::from_verts(&self.backend, &vertices, &indices, ObjType::Opaque)?;
        drawable.borrow_mut().add_texture(0, albedo_tex);
        drawable.borrow_mut().add_texture(1, mr_tex);
        drawable.borrow_mut().add_texture(2, normal_tex);

        // --- Build scenegraph ---
        let node = Node::create(
            Some("test_cube"),
            glm::identity(), // cube at origin
            Some(vec![drawable]),
        );
        let root = Node::create(None, glm::identity(), None);
        root.borrow_mut()
            .add_child(node)
            .expect("Unable to add test cube node");

        self.scene.set_root(root);

        // Ambient light
        self.scene.add_light(Light::default());

        // Directional light
        let shadow_dist = 20.0;
        self.scene.add_light(Light {
            position: glm::vec3(-0.15, -0.5, -0.05).normalize(),
            t: LightType::Directional,
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

        self.scene.build_matrices(&self.backend);

        println!("Test scene loaded: unit cube at origin.");
        Ok(())
    }

    pub fn unload_scene(&mut self) {
        let _ = self.scene.clear();
    }

    /// Main per-frame update. Call once per frame from the event loop.
    ///
    /// Handles input, camera, and rendering. The caller is responsible for
    /// frame timing (dt), FPS tracking, and window title updates.
    ///
    /// For editor integration, use the three-step flow instead:
    ///   1. `update_state(dt)` — process input and camera
    ///   2. `render_scene()` — execute all render passes (no present)
    ///   3. `finish_frame()` — submit commands and present
    /// Between steps 2 and 3, the editor can render its overlay (egui).
    pub fn update(&mut self, dt: f32) -> Result<(), GpuError> {
        self.update_state(dt);
        self.render_scene()?;
        self.finish_frame()
    }

    /// Step 1: Update input and camera state. Call before render_scene().
    pub fn update_state(&mut self, dt: f32) {
        // Update camera direction vectors first (from Euler angles + mouse input)
        // so that movement uses the current frame's direction, not last frame's.
        if let Some(cam) = self.camera.clone() {
            cam.borrow_mut().update(dt);
        }

        // Apply movement (uses the freshly-computed front/right vectors)
        if let Some(ref ih) = self.input_handler {
            ih.borrow_mut().update(dt, &mut self.settings);
        }

        // Propagate final camera state to GPU uniform buffers
        if let Some(cam) = self.camera.clone() {
            self.update_camera_uniforms(&cam);
        }
    }

    /// Step 2: Execute the full rendering pipeline (all passes), but do NOT
    /// present. Call after update_state() and before finish_frame().
    pub fn render_scene(&mut self) -> Result<(), GpuError> {
        self.render()
    }

    /// Step 3: Submit GPU commands and present the frame.
    /// Call after render_scene() and any overlay rendering (e.g., egui).
    pub fn finish_frame(&mut self) -> Result<(), GpuError> {
        self.backend.end_frame()?;
        self.backend.present()
    }

    /// Upload current camera state to all pass uniform buffers.
    fn update_camera_uniforms(&mut self, cam: &Rc<RefCell<dyn Camera>>) {
        let cam = cam.borrow();
        let view = cam.view_mat();
        let pos = cam.position();
        let ssao_enabled = self.settings.ssao;

        if let Some(ref mut fwd) = self.forward_program {
            fwd.set_view(view);
            fwd.set_camera_pos(pos);
            fwd.set_ssao(ssao_enabled);
            fwd.update(&self.backend);
        }
        if let Some(ref mut dp) = self.deferred_program_pre {
            dp.set_view(view);
            dp.update(&self.backend);
        }
        if let Some(ref mut dl) = self.deferred_program_light {
            dl.set_camera_pos(pos);
            dl.set_ssao(ssao_enabled);
            dl.update(&self.backend);
        }
        if let Some(ref mut ssao) = self.ssao_program {
            ssao.set_view(view);
            ssao.update(&self.backend);
        }
        if let Some(ref mut sky) = self.skybox_program {
            // Remove translation from view matrix for skybox
            sky.set_view(glm::mat3_to_mat4(&glm::mat4_to_mat3(&view)));
            sky.update(&self.backend);
        }
    }

    /// Execute the full rendering pipeline for one frame.
    fn render(&mut self) -> Result<(), GpuError> {
        self.backend.begin_frame()?;

        let backbuffer = self.backend.backbuffer().clone();
        let depth = self.backend.main_depth_target().clone();
        let viewport = self.backend.default_viewport();

        // Deferred pre-pass (opaque objects -> G-buffer)
        if let Some(ref deferred_pre) = self.deferred_program_pre {
            let positions = deferred_pre.positions().clone();
            let normal_roughness = deferred_pre.normal_roughness().clone();
            let albedo_metallic = deferred_pre.albedo_metallic().clone();

            self.backend.begin_event("Deferred Pre Pass");
            self.backend.begin_render_pass(&RenderPassDesc {
                label: "deferred_pre",
                color_targets: vec![
                    ColorAttachment {
                        target: &positions,
                        load_op: LoadOp::Clear,
                        clear_color: [0.0, 0.0, 0.0, 0.0],
                    },
                    ColorAttachment {
                        target: &normal_roughness,
                        load_op: LoadOp::Clear,
                        clear_color: [0.0, 0.0, 0.0, 0.0],
                    },
                    ColorAttachment {
                        target: &albedo_metallic,
                        load_op: LoadOp::Clear,
                        clear_color: [0.0, 0.0, 0.0, 0.0],
                    },
                ],
                depth_target: Some(DepthAttachment {
                    target: &depth,
                    load_op: LoadOp::Clear,
                    clear_depth: 1.0,
                    write_enabled: true,
                }),
            });
            self.backend.set_viewport(&viewport);
            deferred_pre.prepare_draw(&mut self.backend);
            // Inline draw loop with per-drawable pipeline switching for double-sided materials
            if let Ok(drawables) = self.scene.traverse() {
                let mut last_ds: Option<bool> = None;
                for i in 0..drawables.len() {
                    let drawable = drawables[i].borrow();
                    if drawable.object_type() != ObjType::Opaque {
                        continue;
                    }
                    let ds = drawable.is_double_sided();
                    let pipeline_switched = last_ds != Some(ds);
                    if pipeline_switched {
                        deferred_pre.set_pipeline_for(&mut self.backend, ds);
                        last_ds = Some(ds);
                    }
                    // Always rebind material after pipeline switch (clears group 2)
                    let rebind_material = pipeline_switched
                        || i == 0
                        || !drawables[i - 1].borrow().material().eq(drawable.material());
                    drawable.draw(&mut self.backend, rebind_material);
                }
            }
            self.backend.end_render_pass();
            self.backend.end_event();
        }

        // SSAO (ambient occlusion)
        if let Some(ref ssao) = self.ssao_program {
            if self.settings.ssao {
                let ssao_rt = ssao.ssao_target().clone();
                let blur_rt = ssao.blur_target().clone();

                // Sub-pass 2a: compute raw SSAO
                self.backend.begin_event("SSAO");
                self.backend.begin_render_pass(&RenderPassDesc {
                    label: "ssao",
                    color_targets: vec![ColorAttachment {
                        target: &ssao_rt,
                        load_op: LoadOp::Clear,
                        clear_color: [1.0, 1.0, 1.0, 1.0],
                    }],
                    depth_target: None,
                });
                ssao.prepare_draw_ssao(&mut self.backend);
                // Bind G-buffer inputs (slot 0 = position, slot 1 = normal+roughness)
                if let Some(ref dp) = self.deferred_program_pre {
                    self.backend
                        .bind_render_target_as_texture(0, dp.positions());
                    self.backend
                        .bind_render_target_as_texture(1, dp.normal_roughness());
                }
                self.screen_quad.draw(&mut self.backend);
                self.backend.end_render_pass();
                self.backend.end_event();

                // Sub-pass 2b: blur SSAO
                self.backend.begin_event("SSAO Blur");
                self.backend.begin_render_pass(&RenderPassDesc {
                    label: "ssao_blur",
                    color_targets: vec![ColorAttachment {
                        target: &blur_rt,
                        load_op: LoadOp::Clear,
                        clear_color: [1.0, 1.0, 1.0, 1.0],
                    }],
                    depth_target: None,
                });
                ssao.prepare_draw_blur(&mut self.backend);
                self.backend.bind_render_target_as_texture(0, &ssao_rt);
                self.screen_quad.draw(&mut self.backend);
                self.backend.end_render_pass();
                self.backend.end_event();
            }
        }

        // Per-light passes (shadow -> deferred light -> forward)
        let lights = self.scene.get_lights().clone();
        let mut first_light = true;

        for mut light in lights {
            if light.t != LightType::Ambient {
                // Calculate light-space matrix for shadow mapping
                if let Some(ref cam) = self.camera {
                    let dir = light.position * (-1.0) * self.shadow_dist;
                    let mut up = glm::vec3(0.0, 1.0, 0.0);
                    if (up.dot(&dir.normalize()) - 1.0).abs() <= 0.0000001 {
                        up = glm::vec3(0.0, 0.0, 1.0);
                    }
                    let pos = cam.borrow().position();
                    let light_view = glm::look_at(&(pos + dir), &pos, &up);
                    light.light_proj = light.light_proj * light_view;
                }

                // Shadow pass
                if let Some(ref mut shadow) = self.shadow_program {
                    let shadow_map = shadow.shadow_map().clone();
                    let shadow_vp = *shadow.viewport();

                    shadow.set_light_space(light.light_proj);
                    shadow.update(&self.backend);

                    self.backend.begin_event("Shadow Mapping");
                    self.backend.begin_render_pass(&RenderPassDesc {
                        label: "shadow",
                        color_targets: vec![],
                        depth_target: Some(DepthAttachment {
                            target: &shadow_map,
                            load_op: LoadOp::Clear,
                            clear_depth: 1.0,
                            write_enabled: true,
                        }),
                    });
                    self.backend.set_viewport(&shadow_vp);
                    shadow.prepare_draw(&mut self.backend);
                    // Inline draw loop with per-drawable pipeline switching for double-sided
                    if let Ok(drawables) = self.scene.traverse() {
                        let mut last_ds: Option<bool> = None;
                        for i in 0..drawables.len() {
                            let drawable = drawables[i].borrow();
                            let ds = drawable.is_double_sided();
                            let pipeline_switched = last_ds != Some(ds);
                            if pipeline_switched {
                                shadow.set_pipeline_for(&mut self.backend, ds);
                                last_ds = Some(ds);
                            }
                            let rebind_material = pipeline_switched
                                || i == 0
                                || !drawables[i - 1].borrow().material().eq(drawable.material());
                            drawable.draw(&mut self.backend, rebind_material);
                        }
                    }
                    self.backend.end_render_pass();
                    self.backend.end_event();
                }
            }

            // Deferred light pass (fullscreen quad, accumulative blending)
            if let Some(ref mut def_light) = self.deferred_program_light {
                let def_light_rt = def_light.render_target().clone();

                def_light.set_light(&light);
                def_light.update(&self.backend);

                let load = if first_light {
                    LoadOp::Clear
                } else {
                    LoadOp::Load
                };

                self.backend.begin_event("Deferred Light Pass");
                self.backend.begin_render_pass(&RenderPassDesc {
                    label: "deferred_light",
                    color_targets: vec![ColorAttachment {
                        target: &def_light_rt,
                        load_op: load,
                        clear_color: [0.0, 0.0, 0.0, 0.0],
                    }],
                    depth_target: None,
                });
                def_light.prepare_draw(&mut self.backend);

                // Bind G-buffer inputs
                if let Some(ref dp) = self.deferred_program_pre {
                    self.backend
                        .bind_render_target_as_texture(0, dp.positions());
                    self.backend
                        .bind_render_target_as_texture(1, dp.normal_roughness());
                    self.backend
                        .bind_render_target_as_texture(2, dp.albedo_metallic());
                }
                // Bind shadow map (sequential slot 3 = 4th texture in group 3)
                if let Some(ref sp) = self.shadow_program {
                    self.backend
                        .bind_render_target_as_texture(3, sp.shadow_map());
                }
                // Bind blurred SSAO texture (sequential slot 4 = 5th texture in group 3)
                if let Some(ref ssao) = self.ssao_program {
                    self.backend
                        .bind_render_target_as_texture(4, ssao.blur_target());
                }

                self.screen_quad.draw(&mut self.backend);
                self.backend.end_render_pass();
                self.backend.end_event();
            }

            // Forward pass (transparent objects)
            if let Some(ref mut fwd) = self.forward_program {
                let fwd_rt = fwd.render_target().clone();

                fwd.set_light(&light);
                fwd.update(&self.backend);

                let load = if first_light {
                    LoadOp::Clear
                } else {
                    LoadOp::Load
                };

                self.backend.begin_event("Forward Pass");
                self.backend.begin_render_pass(&RenderPassDesc {
                    label: "forward",
                    color_targets: vec![ColorAttachment {
                        target: &fwd_rt,
                        load_op: load,
                        clear_color: [0.0, 0.0, 0.0, 0.0],
                    }],
                    depth_target: Some(DepthAttachment {
                        target: &depth,
                        load_op: LoadOp::Load,
                        clear_depth: 1.0,
                        write_enabled: true,
                    }),
                });
                self.backend.set_viewport(&viewport);
                fwd.prepare_draw(&mut self.backend);
                // Bind shadow map (sequential slot 0 = 1st texture in group 3)
                if let Some(ref sp) = self.shadow_program {
                    self.backend
                        .bind_render_target_as_texture(0, sp.shadow_map());
                }
                // Inline draw loop with per-drawable pipeline switching for double-sided
                if let Ok(drawables) = self.scene.traverse() {
                    let mut last_ds: Option<bool> = None;
                    for i in 0..drawables.len() {
                        let drawable = drawables[i].borrow();
                        if drawable.object_type() != ObjType::Transparent {
                            continue;
                        }
                        let ds = drawable.is_double_sided();
                        let pipeline_switched = last_ds != Some(ds);
                        if pipeline_switched {
                            fwd.set_pipeline_for(&mut self.backend, ds);
                            // Rebind shadow map (external to pass, in group 3)
                            if let Some(ref sp) = self.shadow_program {
                                self.backend
                                    .bind_render_target_as_texture(0, sp.shadow_map());
                            }
                            last_ds = Some(ds);
                        }
                        // Always rebind material after pipeline switch (clears group 2)
                        let rebind_material = pipeline_switched
                            || i == 0
                            || !drawables[i - 1].borrow().material().eq(drawable.material());
                        drawable.draw(&mut self.backend, rebind_material);
                    }
                }
                self.backend.end_render_pass();
                self.backend.end_event();
            }

            first_light = false;
        }

        // Output composite (deferred + forward -> backbuffer)
        if let Some(ref output) = self.output_program {
            self.backend.begin_event("Output Composite");
            self.backend.begin_render_pass(&RenderPassDesc {
                label: "output",
                color_targets: vec![ColorAttachment {
                    target: &backbuffer,
                    load_op: LoadOp::Clear,
                    clear_color: [0.05, 0.05, 0.05, 1.0],
                }],
                depth_target: None,
            });
            output.prepare_draw(&mut self.backend);
            if let Some(ref dl) = self.deferred_program_light {
                self.backend
                    .bind_render_target_as_texture(0, dl.render_target());
            }
            if let Some(ref fwd) = self.forward_program {
                self.backend
                    .bind_render_target_as_texture(1, fwd.render_target());
            }
            self.screen_quad.draw(&mut self.backend);
            self.backend.end_render_pass();
            self.backend.end_event();
        } else {
            // Fallback: clear backbuffer only (MVP path, no shaders loaded)
            self.backend.begin_render_pass(&RenderPassDesc {
                label: "clear",
                color_targets: vec![ColorAttachment {
                    target: &backbuffer,
                    load_op: LoadOp::Clear,
                    clear_color: [0.05, 0.05, 0.05, 1.0],
                }],
                depth_target: Some(DepthAttachment {
                    target: &depth,
                    load_op: LoadOp::Clear,
                    clear_depth: 1.0,
                    write_enabled: true,
                }),
            });
            self.backend.end_render_pass();
        }

        // Skybox
        if let Some(ref skybox_prog) = self.skybox_program {
            if let Some(ref skybox) = self.skybox {
                self.backend.begin_event("Skybox");
                self.backend.begin_render_pass(&RenderPassDesc {
                    label: "skybox",
                    color_targets: vec![ColorAttachment {
                        target: &backbuffer,
                        load_op: LoadOp::Load,
                        clear_color: [0.0, 0.0, 0.0, 0.0],
                    }],
                    depth_target: Some(DepthAttachment {
                        target: &depth,
                        load_op: LoadOp::Load,
                        clear_depth: 1.0,
                        write_enabled: false,
                    }),
                });
                skybox_prog.prepare_draw(&mut self.backend);
                skybox.draw(&mut self.backend);
                self.backend.end_render_pass();
                self.backend.end_event();
            }
        }

        // Note: end_frame() and present() are NOT called here.
        // Use finish_frame() after any overlay rendering (e.g., egui).

        Ok(())
    }
}
