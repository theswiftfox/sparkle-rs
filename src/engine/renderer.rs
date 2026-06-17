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
use super::scene_data::{self, LightData, NodeTransform, SceneData};
use super::scene_info::NodeInfo;
use super::scenegraph::Scenegraph;
use super::settings::Settings;
use super::skybox::Skybox;

use crate::import;
use crate::input::Camera;

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
    /// Path to the currently loaded glTF scene file.
    scene_file: Option<String>,
    backend: B,
    clock: Instant,
    // Shared UBO buffers bound permanently to descriptor set bindings 0-5
    ubo_view_proj: B::Buffer,          // binding 0, ViewProjUniforms (128B)
    ubo_camera_pixel: B::Buffer,       // binding 1, CameraUniforms (16B)
    ubo_light_data: B::Buffer,         // binding 2, GpuLight (96B)
    ubo_shadow_light_space: B::Buffer, // binding 3, LightSpaceUniforms (64B)
    ubo_skybox_view_proj: B::Buffer,   // binding 4, ViewProjUniforms (128B)
    ubo_near_far: B::Buffer,           // binding 5, NearFarUniforms (16B)
    // CPU-side copies for partial updates
    view_proj_cpu: ViewProjUniforms,
    camera_pixel_cpu: CameraUniforms,
    skybox_view_proj_cpu: ViewProjUniforms,
    near_far_cpu: NearFarUniforms,
}

impl<B: GpuBackend> Renderer<B> {
    /// Create a new renderer with the given backend and settings.
    ///
    /// Draw programs are initially None — they require compiled shaders
    /// which are provided later via `init_draw_programs()` (Phase 3).
    /// Without draw programs, the renderer falls back to clearing the screen.
    pub fn create(backend: B, settings: Settings) -> Result<Self, GpuError> {
        let screen_quad = ScreenQuad::create(&backend)?;

        let ubo_desc = |label: &str, size| BufferDesc {
            label: label.to_string(),
            usage: BufferUsage::Uniform,
            size,
        };

        let ubo_view_proj = backend.create_buffer(
            &ubo_desc("shared_view_proj", std::mem::size_of::<ViewProjUniforms>()),
            None,
        )?;
        let ubo_camera_pixel = backend.create_buffer(
            &ubo_desc("shared_camera_pixel", std::mem::size_of::<CameraUniforms>()),
            None,
        )?;
        let ubo_light_data = backend.create_buffer(
            &ubo_desc("shared_light_data", std::mem::size_of::<GpuLight>()),
            None,
        )?;
        let ubo_shadow_light_space = backend.create_buffer(
            &ubo_desc(
                "shared_shadow_light_space",
                std::mem::size_of::<LightSpaceUniforms>(),
            ),
            None,
        )?;
        let ubo_skybox_view_proj = backend.create_buffer(
            &ubo_desc(
                "shared_skybox_view_proj",
                std::mem::size_of::<ViewProjUniforms>(),
            ),
            None,
        )?;
        let ubo_near_far = backend.create_buffer(
            &ubo_desc("shared_near_far", std::mem::size_of::<NearFarUniforms>()),
            None,
        )?;

        backend.bind_ubo_to_descriptor(0, &ubo_view_proj);
        backend.bind_ubo_to_descriptor(1, &ubo_camera_pixel);
        backend.bind_ubo_to_descriptor(2, &ubo_light_data);
        backend.bind_ubo_to_descriptor(3, &ubo_shadow_light_space);
        backend.bind_ubo_to_descriptor(4, &ubo_skybox_view_proj);
        backend.bind_ubo_to_descriptor(5, &ubo_near_far);

        let identity = glm::Mat4::identity();
        let view_proj_cpu = ViewProjUniforms {
            view: identity,
            proj: identity,
        };
        let camera_pixel_cpu = CameraUniforms {
            camera_pos: glm::Vec3::zeros(),
            ssao: 0,
        };
        let skybox_view_proj_cpu = ViewProjUniforms {
            view: identity,
            proj: identity,
        };
        let near_far_cpu = NearFarUniforms {
            near_plane: 0.1,
            far_plane: 100.0,
            _pad: 0.0,
            _pad2: 0.0,
        };

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
            scene_file: None,
            backend,
            clock: Instant::now(),
            ubo_view_proj,
            ubo_camera_pixel,
            ubo_light_data,
            ubo_shadow_light_space,
            ubo_skybox_view_proj,
            ubo_near_far,
            view_proj_cpu,
            camera_pixel_cpu,
            skybox_view_proj_cpu,
            near_far_cpu,
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

    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    // ---- Scene data accessors for editor ----

    /// Extract a lightweight snapshot of the entire scenegraph tree.
    ///
    /// Returns `None` if no scene is loaded (no root node).
    pub fn scene_tree(&self) -> Option<NodeInfo> {
        self.scene
            .root()
            .as_ref()
            .map(|root| NodeInfo::from_node(root))
    }

    /// Get a reference to the scene's lights.
    pub fn lights(&self) -> &Vec<Light> {
        self.scene.get_lights()
    }

    /// Update a light at the given index.
    pub fn update_light(&mut self, index: usize, light: Light) {
        let _ = self.scene.update_light(light, index);
    }

    /// Add a new light to the scene. Returns its index.
    pub fn add_light(&mut self, light: Light) -> usize {
        self.scene.add_light(light);
        self.scene.get_lights().len() - 1
    }

    /// Remove a light by index.
    pub fn remove_light(&mut self, index: usize) {
        let _ = self.scene.remove_light(index);
    }

    /// Set the local transform of a node identified by name, then rebuild
    /// the world matrices for the entire scene.
    pub fn set_node_transform(&mut self, name: &str, transform: glm::Mat4) {
        if let Ok(node) = self.scene.get_node_named(name) {
            node.borrow_mut().set_local_transform(transform);
            self.scene.build_matrices(&self.backend);
        }
    }

    /// Returns the currently loaded scene file path (if any).
    pub fn scene_file(&self) -> Option<&str> {
        self.scene_file.as_deref()
    }

    /// Extract the current scene state as a serializable `SceneData`.
    ///
    /// Captures all node transforms and lights. Returns `None` if no scene
    /// is loaded.
    pub fn extract_scene_data(&self) -> Option<SceneData> {
        let scene_file = self.scene_file.as_ref()?.clone();

        // Collect node transforms by traversing the scenegraph
        let mut node_transforms = Vec::new();
        if let Some(root) = self.scene.root() {
            let nodes = root.borrow().traverse();
            for node_rc in &nodes {
                let node = node_rc.borrow();
                if let Some(ref name) = node.name {
                    node_transforms.push(NodeTransform {
                        name: name.clone(),
                        transform: scene_data::mat4_to_array(&node.local_transform()),
                    });
                }
            }
        }

        // Collect lights
        let lights: Vec<LightData> = self
            .scene
            .get_lights()
            .iter()
            .map(LightData::from)
            .collect();

        Some(SceneData {
            scene_file,
            node_transforms,
            lights,
        })
    }

    /// Apply a loaded `SceneData` overlay to the current scene.
    ///
    /// Sets node transforms by name and replaces all lights.
    /// The base glTF scene must already be loaded.
    pub fn apply_scene_data(&mut self, data: &SceneData) {
        // Apply node transform overrides
        for nt in &data.node_transforms {
            let mat = scene_data::array_to_mat4(&nt.transform);
            if let Ok(node) = self.scene.get_node_named(&nt.name) {
                node.borrow_mut().set_local_transform(mat);
            }
        }

        // Replace lights
        self.scene.clear_lights();
        for ld in &data.lights {
            self.scene.add_light(ld.to_light());
        }

        // Rebuild world matrices
        self.scene.build_matrices(&self.backend);
    }

    /// Propagate the camera's projection matrix to all passes.
    /// Call once after creating the renderer or when switching cameras.
    pub fn set_camera_projection(&mut self, camera: &dyn Camera) {
        let proj = camera.projection_mat();
        let (near, far) = camera.near_far();

        // Update shared UBOs
        self.view_proj_cpu.proj = proj;
        self.backend.update_buffer(
            &self.ubo_view_proj,
            as_bytes(std::slice::from_ref(&self.view_proj_cpu)),
        );
        self.skybox_view_proj_cpu.proj = proj;
        self.backend.update_buffer(
            &self.ubo_skybox_view_proj,
            as_bytes(std::slice::from_ref(&self.skybox_view_proj_cpu)),
        );
        self.near_far_cpu.near_plane = near;
        self.near_far_cpu.far_plane = far;
        self.backend.update_buffer(
            &self.ubo_near_far,
            as_bytes(std::slice::from_ref(&self.near_far_cpu)),
        );

        if let Some(ref mut sky) = self.skybox_program {
            sky.set_proj(proj);
        }
        if let Some(ref mut ssao) = self.ssao_program {
            ssao.set_proj(proj);
        }
    }

    /// Resize the backend and all resolution-dependent render targets.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.backend.resize(width, height);

        let resolution = (width, height);
        if let Some(ref mut dp) = self.deferred_program_pre {
            if let Err(e) = dp.resize(&self.backend, resolution) {
                eprintln!("Failed to resize deferred pre targets: {}", e);
            }
        }
        if let Some(ref mut ssao) = self.ssao_program {
            if let Err(e) = ssao.resize(&self.backend, resolution) {
                eprintln!("Failed to resize SSAO targets: {}", e);
            }
        }
        if let Some(ref mut dl) = self.deferred_program_light {
            if let Err(e) = dl.resize(&self.backend, resolution) {
                eprintln!("Failed to resize deferred light target: {}", e);
            }
        }
        if let Some(ref mut fwd) = self.forward_program {
            if let Err(e) = fwd.resize(&self.backend, resolution) {
                eprintln!("Failed to resize forward target: {}", e);
            }
        }
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

        let Shaders {
            deferred_pre,
            ssao,
            ssao_blur,
            shadow: shadow_wgsl,
            deferred_light,
            forward,
            output,
            skybox,
        } = self.backend.load_shaders();

        println!("Initializing draw programs...");

        // Deferred pre-pass (G-buffer fill)
        self.deferred_program_pre = Some(DeferredPassPre::create(
            &self.backend,
            resolution,
            &deferred_pre,
        )?);
        println!("  deferred_pre: OK");

        // SSAO pass (ambient occlusion)
        if self.settings.ssao {
            self.ssao_program = Some(SsaoPass::create(
                &self.backend,
                resolution,
                &ssao,
                &ssao_blur,
            )?);
            println!("  ssao: OK");
        }

        // Shadow mapping pass
        self.shadow_program = Some(ShadowPass::create(&self.backend, &shadow_wgsl)?);
        println!("  shadow: OK");

        // Deferred lighting pass
        self.deferred_program_light = Some(DeferredPassLight::create(
            &self.backend,
            resolution,
            &deferred_light,
        )?);
        println!("  deferred_light: OK");

        // Forward pass (transparent objects)
        self.forward_program = Some(ForwardPass::create(&self.backend, resolution, &forward)?);
        println!("  forward: OK");

        // Output composite pass
        self.output_program = Some(OutputPass::create(
            &self.backend,
            &output,
            backbuffer_format,
        )?);
        println!("  output: OK");

        // Skybox pass
        self.skybox_program = Some(SkyBoxPass::create(
            &self.backend,
            &skybox,
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
        self.scene_file = Some(scene_file.to_string());

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
    ///   1. `update_state(dt, camera)` — update camera matrices and uniforms
    ///   2. `render_scene(camera)` — execute all render passes (no present)
    ///   3. `finish_frame()` — submit commands and present
    /// Between steps 2 and 3, the editor can render its overlay (egui).
    pub fn update(&mut self, dt: f32, camera: &mut dyn Camera) -> Result<(), GpuError> {
        self.backend.begin_frame()?;
        self.update_state(dt, camera);
        self.render(camera)?;
        self.finish_frame()
    }

    /// Step 1: Update camera state and propagate to GPU uniform buffers.
    /// Call before render_scene().
    pub fn update_state(&mut self, dt: f32, camera: &mut dyn Camera) {
        camera.update(dt);
        self.update_camera_uniforms(camera);
    }

    /// Step 2: Execute the full rendering pipeline (all passes), but do NOT
    /// present. Call after update_state() and before finish_frame().
    /// Note: ensure begin_frame was called by update_state or manually
    pub fn render_scene(&mut self, camera: &dyn Camera) -> Result<(), GpuError> {
        self.render(camera)
    }

    /// Step 3: Submit GPU commands and present the frame.
    /// Call after render_scene() and any overlay rendering (e.g., egui).
    pub fn finish_frame(&mut self) -> Result<(), GpuError> {
        self.backend.end_frame()
    }

    pub fn present(&mut self) -> Result<(), GpuError> {
        self.backend.present()
    }

    /// Upload current camera state to all pass uniform buffers.
    fn update_camera_uniforms(&mut self, camera: &dyn Camera) {
        let view = camera.view_mat();
        let pos = camera.position();
        let ssao_enabled = self.settings.ssao;

        // Update shared UBOs (bindings 0, 1, 4)
        self.view_proj_cpu.view = view;
        self.backend.update_buffer(
            &self.ubo_view_proj,
            as_bytes(std::slice::from_ref(&self.view_proj_cpu)),
        );

        self.camera_pixel_cpu.camera_pos = pos;
        self.camera_pixel_cpu.ssao = ssao_enabled as u32;
        self.backend.update_buffer(
            &self.ubo_camera_pixel,
            as_bytes(std::slice::from_ref(&self.camera_pixel_cpu)),
        );

        self.skybox_view_proj_cpu.view = glm::mat3_to_mat4(&glm::mat4_to_mat3(&view));
        self.backend.update_buffer(
            &self.ubo_skybox_view_proj,
            as_bytes(std::slice::from_ref(&self.skybox_view_proj_cpu)), // This is fine, outside the loop
        );

        // Keep legacy draw program updates (still needed by wgpu path)
        if let Some(ref mut fwd) = self.forward_program {
            fwd.set_view(view);
            fwd.set_camera_pos(pos);
            fwd.set_ssao(ssao_enabled);
        }

        if let Some(ref mut dl) = self.deferred_program_light {
            dl.set_camera_pos(pos);
            dl.set_ssao(ssao_enabled);
        }
        if let Some(ref mut sky) = self.skybox_program {
            sky.set_view(glm::mat3_to_mat4(&glm::mat4_to_mat3(&view)));
            sky.update(&self.backend);
        }
    }

    /// Execute the full rendering pipeline for one frame.
    fn render(&mut self, camera: &dyn Camera) -> Result<(), GpuError> {
        let depth = self.backend.main_depth_target().clone();
        let viewport = self.backend.default_viewport();

        // Deferred pre-pass (opaque objects -> G-buffer)
        if let Some(ref deferred_pre) = self.deferred_program_pre {
            let positions = deferred_pre.positions().clone();
            let normal_roughness = deferred_pre.normal_roughness().clone();
            let albedo_metallic = deferred_pre.albedo_metallic().clone();

            self.backend.begin_event("Deferred Pre Pass");
            self.backend
                .bind_uniform(ShaderStage::Vertex, 0, &self.ubo_view_proj);
            self.backend
                .bind_uniform(ShaderStage::Fragment, 0, &self.ubo_near_far);
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
                    if last_ds != Some(ds) {
                        deferred_pre.set_pipeline_for(&mut self.backend, ds);
                        last_ds = Some(ds);
                    }
                    drawable.draw(&mut self.backend, true);
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

                // Bind G-buffer inputs (slot 0 = position, slot 1 = normal+roughness)
                if let Some(ref dp) = self.deferred_program_pre {
                    self.backend
                        .bind_render_target_as_texture(0, dp.positions());
                    self.backend
                        .bind_render_target_as_texture(1, dp.normal_roughness());
                }

                self.backend
                    .bind_uniform(ShaderStage::Fragment, 0, &ssao.uniforms_buf);
                self.backend.bind_texture(0, &ssao.noise_texture);

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
                self.backend.set_viewport(&viewport);
                ssao.prepare_draw_ssao(&mut self.backend);
                self.screen_quad.draw(&mut self.backend);
                self.backend.end_render_pass();
                self.backend.end_event();

                self.backend.bind_render_target_as_texture(0, &ssao_rt);

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
                self.screen_quad.draw(&mut self.backend);
                self.backend.end_render_pass();
                self.backend.end_event();
            }
        }

        // Process lights individually to correctly synchronize shadow mapping and accumulation
        let lights = self.scene.get_lights().clone();
        let mut first_light = true;

        for mut light in lights {
            if light.t != LightType::Ambient {
                let dir = light.position * (-1.0) * self.shadow_dist;
                let mut up = glm::vec3(0.0, 1.0, 0.0);
                if (up.dot(&dir.normalize()) - 1.0).abs() <= 0.0000001 {
                    up = glm::vec3(0.0, 0.0, 1.0);
                }
                let pos = camera.position();
                let light_view = glm::look_at(&(pos + dir), &pos, &up);
                light.light_proj = light.light_proj * light_view;
            }

            self.backend.cmd_update_buffer(
                &self.ubo_light_data,
                as_bytes(std::slice::from_ref(&GpuLight::from_light(&light))),
            );

            // 1. Shadow Mapping for this light
            if let Some(ref mut shadow) = self.shadow_program {
                if light.t != LightType::Ambient {
                    self.backend.cmd_update_buffer(
                        &self.ubo_shadow_light_space,
                        as_bytes(std::slice::from_ref(&LightSpaceUniforms {
                            light_space_matrix: light.light_proj,
                        })),
                    );

                    self.backend
                        .bind_uniform(ShaderStage::Vertex, 0, &self.ubo_shadow_light_space);
                    self.backend.begin_event("Shadow Mapping");
                    self.backend.begin_render_pass(&RenderPassDesc {
                        label: "shadow",
                        color_targets: vec![],
                        depth_target: Some(DepthAttachment {
                            target: shadow.shadow_map(),
                            load_op: LoadOp::Clear, // Always clear for the specific light's map
                            clear_depth: 1.0,
                            write_enabled: true,
                        }),
                    });
                    self.backend.set_viewport(shadow.viewport());
                    shadow.prepare_draw(&mut self.backend);
                    if let Ok(drawables) = self.scene.traverse() {
                        let mut last_ds: Option<bool> = None;
                        for drawable in drawables {
                            let drawable = drawable.borrow();
                            let ds = drawable.is_double_sided();
                            if last_ds != Some(ds) {
                                shadow.set_pipeline_for(&mut self.backend, ds);
                                last_ds = Some(ds);
                            }
                            drawable.draw(&mut self.backend, false);
                        }
                    }
                    self.backend.end_render_pass();
                    self.backend.end_event();
                }
            }

            // Bind shared inputs for lighting and transparency
            if let Some(ref sp) = self.shadow_program {
                self.backend
                    .bind_render_target_as_texture(3, sp.shadow_map());
            }
            if let Some(ref ssao) = self.ssao_program {
                self.backend
                    .bind_render_target_as_texture(4, ssao.blur_target());
            }
            if let Some(ref dp) = self.deferred_program_pre {
                self.backend
                    .bind_render_target_as_texture(0, dp.positions());
                self.backend
                    .bind_render_target_as_texture(1, dp.normal_roughness());
                self.backend
                    .bind_render_target_as_texture(2, dp.albedo_metallic());
            }

            // 2. Deferred Lighting accumulation
            if let Some(ref mut def_light) = self.deferred_program_light {
                let def_light_rt = def_light.render_target().clone();

                // Bind shared inputs (UBOs)
                self.backend
                    .bind_uniform(ShaderStage::Fragment, 0, &self.ubo_camera_pixel);
                self.backend
                    .bind_uniform(ShaderStage::Fragment, 1, &self.ubo_light_data);

                self.backend.begin_event("Deferred Light Pass");
                self.backend.begin_render_pass(&RenderPassDesc {
                    label: "deferred_light",
                    color_targets: vec![ColorAttachment {
                        target: &def_light_rt,
                        load_op: if first_light {
                            LoadOp::Clear
                        } else {
                            LoadOp::Load
                        },
                        clear_color: [0.0, 0.0, 0.0, 0.0],
                    }],
                    depth_target: None,
                });
                self.backend.set_viewport(&viewport);
                def_light.prepare_draw(&mut self.backend);
                self.screen_quad.draw(&mut self.backend);
                self.backend.end_render_pass();
                self.backend.end_event();
            }

            // 3. Forward accumulation
            if let Some(ref mut fwd) = self.forward_program {
                let fwd_rt = fwd.render_target().clone();

                self.backend
                    .bind_uniform(ShaderStage::Vertex, 0, &self.ubo_view_proj);
                self.backend
                    .bind_uniform(ShaderStage::Fragment, 0, &self.ubo_camera_pixel);
                self.backend
                    .bind_uniform(ShaderStage::Fragment, 1, &self.ubo_light_data);

                self.backend.begin_event("Forward Pass");
                self.backend.begin_render_pass(&RenderPassDesc {
                    label: "forward",
                    color_targets: vec![ColorAttachment {
                        target: &fwd_rt,
                        load_op: if first_light {
                            LoadOp::Clear
                        } else {
                            LoadOp::Load
                        },
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

                if let Ok(drawables) = self.scene.traverse() {
                    let mut last_ds: Option<bool> = None;
                    for drawable in drawables {
                        let drawable = drawable.borrow();
                        if drawable.object_type() != ObjType::Transparent {
                            continue;
                        }
                        let ds = drawable.is_double_sided();
                        if last_ds != Some(ds) {
                            fwd.set_pipeline_for(&mut self.backend, ds);
                            last_ds = Some(ds);
                        }
                        drawable.draw(&mut self.backend, true);
                    }
                }
                self.backend.end_render_pass();
                self.backend.end_event();
            }

            first_light = false;
        }

        let backbuffer = self.backend.backbuffer().clone();

        if let Some(ref dl) = self.deferred_program_light {
            self.backend
                .bind_render_target_as_texture(0, dl.render_target());
        }
        if let Some(ref fwd) = self.forward_program {
            self.backend
                .bind_render_target_as_texture(1, fwd.render_target());
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
            self.backend.set_viewport(&viewport);
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
                self.backend
                    .bind_uniform(ShaderStage::Vertex, 0, &skybox_prog.vertex_uniforms_buf);
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
