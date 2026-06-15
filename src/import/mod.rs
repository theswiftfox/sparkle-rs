// gltf importer for sparkle-rs
//
// Loads glTF scenes into the engine's scenegraph, creating backend-agnostic
// GPU resources (textures, vertex/index buffers, drawables) via the GpuBackend trait.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::engine::backend::*;
use crate::engine::geometry::Vertex;
use crate::engine::scenegraph::node::Node;

#[derive(Debug, Clone)]
pub struct ImportError {
    cause: String,
    description: String,
}

impl ImportError {
    pub fn from(c: &str, d: &str) -> ImportError {
        ImportError {
            cause: c.to_string(),
            description: d.to_string(),
        }
    }
    pub fn new() -> ImportError {
        ImportError {
            cause: "Sparkle: Import Failure".to_string(),
            description: "Unknown error occurred during scene import...".to_string(),
        }
    }
}

struct GltfImporter<'a, B: GpuBackend> {
    buffers: Vec<gltf::buffer::Data>,
    images: Vec<gltf::image::Data>,
    backend: &'a B,
    texture_buffer: HashMap<usize, (Rc<B::Texture>, bool)>,
    missing_tex: Rc<B::Texture>,
    flat_normal_tex: Rc<B::Texture>,
}

pub fn load_gltf<B: GpuBackend>(
    path: &str,
    backend: &B,
) -> Result<Rc<RefCell<Node<B>>>, ImportError> {
    let (gltf, buffers, images) = match gltf::import(path) {
        Ok(g) => g,
        Err(e) => return Err(ImportError::from("GLTF Import Error", &format!("{}", e))),
    };

    // Load fallback "missing texture" placeholder
    let img = image::open("images/textures/missing_tex.png")
        .map_err(|e| ImportError::from("Image Load", &format!("{}", e)))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let pixels = rgba.into_raw();
    let missing_tex = Rc::new(
        backend
            .create_texture(
                &TextureDesc {
                    width: w,
                    height: h,
                    format: TextureFormat::Rgba8Unorm,
                    sampler: SamplerDesc {
                        address_u: AddressMode::Clamp,
                        address_v: AddressMode::Clamp,
                        filter: FilterMode::Nearest,
                        compare: None,
                    },
                    generate_mipmaps: false,
                },
                &pixels,
            )
            .map_err(|e| ImportError::from("Texture Creation", &e.message))?,
    );

    // Create a 1x1 flat normal map (encodes normal (0,0,1) as RGB=(128,128,255))
    let flat_normal_tex = Rc::new(
        backend
            .create_texture(
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
            )
            .map_err(|e| ImportError::from("Texture Creation", &e.message))?,
    );

    let mut importer = GltfImporter {
        buffers,
        images,
        backend,
        texture_buffer: HashMap::new(),
        missing_tex,
        flat_normal_tex,
    };

    let root = Node::create(None, glm::identity(), None);
    for scene in gltf.scenes() {
        for node in scene.nodes() {
            importer.process_node(node, &root)?;
        }
    }
    Ok(root)
}

/// Pad a 3-channel image (RGB or BGR) to 4 channels by inserting alpha=255.
fn convert_3ch_to_4ch_img(image: &gltf::image::Data) -> Vec<u8> {
    let len = (image.width * image.height * 4) as usize;
    let mut image_data = vec![255; len];
    let mut src_index = 0;
    let mut ch = 0;
    for i in 0..len {
        if ch == 3 {
            ch = 0;
        } else {
            image_data[i] = image.pixels[src_index];
            src_index += 1;
            ch += 1;
        }
    }
    image_data
}

/// Convert glTF wrap mode to backend address mode.
fn gltf_address_mode(mode: gltf::texture::WrappingMode) -> AddressMode {
    match mode {
        gltf::texture::WrappingMode::ClampToEdge => AddressMode::Clamp,
        gltf::texture::WrappingMode::Repeat => AddressMode::Repeat,
        gltf::texture::WrappingMode::MirroredRepeat => AddressMode::Mirror,
    }
}

impl<'a, B: GpuBackend> GltfImporter<'a, B> {
    fn process_node(
        &mut self,
        node: gltf::scene::Node<'_>,
        parent: &Rc<RefCell<Node<B>>>,
    ) -> Result<(), ImportError> {
        let mut parent = parent.clone();
        if !node.camera().is_some() {
            let transform: glm::Mat4 = glm::make_mat4(&(node.transform().matrix().concat()));
            if let Some(mesh) = node.mesh() {
                let mut drawables: Vec<Rc<RefCell<Drawable<B>>>> = Vec::new();
                for primitive in mesh.primitives() {
                    let mat = primitive.material();
                    let pbr = mat.pbr_metallic_roughness();
                    let alb = pbr.base_color_texture();

                    let mut positions: Vec<glm::Vec3> = Vec::new();
                    let mut indices: Vec<u32> = Vec::new();
                    let mut normals: Vec<glm::Vec3> = Vec::new();
                    let mut tex_coords: Vec<glm::Vec2> = Vec::new();
                    let mut tangents_raw: Vec<glm::Vec4> = Vec::new();
                    {
                        let reader = primitive.reader(|buffer| Some(&self.buffers[buffer.index()]));
                        if let Some(it) = reader.read_positions() {
                            for vtx_pos in it {
                                positions.push(glm::vec3(vtx_pos[0], vtx_pos[1], vtx_pos[2]));
                            }
                        }
                        if let Some(it) = reader.read_indices() {
                            for idx in it.into_u32() {
                                indices.push(idx);
                            }
                        }
                        if let Some(it) = reader.read_normals() {
                            for norm in it {
                                normals.push(glm::vec3(norm[0], norm[1], norm[2]));
                            }
                        };
                        tangents_raw = match reader.read_tangents() {
                            Some(it) => {
                                let mut trvec: Vec<glm::Vec4> = Vec::new();
                                for tang in it {
                                    trvec.push(glm::vec4(tang[0], tang[1], tang[2], tang[3]));
                                }
                                trvec
                            }
                            None => Vec::new(),
                        };
                        if let Some(info) = &alb {
                            if let Some(it) = reader.read_tex_coords(info.tex_coord()) {
                                for uv in it.into_f32() {
                                    tex_coords.push(glm::vec2(uv[0], uv[1]));
                                }
                            }
                        }
                    }

                    // 1. Extract MR texture pixels and classify the material BEFORE importing
                    //    so we can decide whether to generate height maps from normal maps.
                    let material_classification = match pbr.metallic_roughness_texture() {
                        Some(info) => {
                            let tx = info.texture();
                            let img = tx.source();
                            let img_raw = &self.images[img.index()];
                            let (width, height) = (img_raw.width, img_raw.height);

                            let rgba_pixels: Vec<u8> = match img_raw.format {
                                gltf::image::Format::R8G8B8 => convert_3ch_to_4ch_img(&img_raw),
                                gltf::image::Format::R8G8B8A8 => img_raw.pixels.to_vec(),
                                _ => Vec::new(),
                            };

                            if !rgba_pixels.is_empty() {
                                Some(classify_gltf_material(width, height, &rgba_pixels))
                            } else {
                                None
                            }
                        }
                        None => None,
                    };

                    let classification =
                        material_classification.unwrap_or(MaterialClassification {
                            parallax_enabled: false,
                            roughness_factor_override: 0.0,
                        });

                    // Load textures: albedo, metallic-roughness, normal map
                    let (tex_color, transparent) = match alb {
                        Some(info) => {
                            let tx = info.texture();
                            self.import_texture(tx, true, false)
                        }
                        None => (self.missing_tex.clone(), false),
                    };
                    let tex_mr = match pbr.metallic_roughness_texture() {
                        Some(info) => {
                            let tx = info.texture();
                            self.import_texture(tx, true, false).0
                        }
                        None => self.missing_tex.clone(),
                    };
                    let tex_norm = match mat.normal_texture() {
                        Some(info) => {
                            let tx = info.texture();
                            self.import_texture(tx, false, classification.parallax_enabled)
                                .0
                        }
                        None => self.flat_normal_tex.clone(),
                    };

                    // Calculate tangents and bitangents
                    let (tangents, bitangents) = if !tangents_raw.is_empty() {
                        let mut bts = Vec::<glm::Vec3>::new();
                        let mut ts = Vec::<glm::Vec3>::new();
                        for i in 0..tangents_raw.len() {
                            let n = normals[i];
                            let t = tangents_raw[i];
                            let bt = n.cross(&t.xyz()) * t.w;
                            bts.push(bt);
                            ts.push(t.xyz());
                        }
                        (ts, bts)
                    } else {
                        // Accumulate per-vertex tangent/bitangent from all adjacent triangles
                        let mut trvec = vec![glm::Vec3::zeros(); positions.len()];
                        let mut btvec = vec![glm::Vec3::zeros(); positions.len()];
                        if tex_coords.len() != positions.len() {
                            panic!("No UV Coordinates provided!");
                        }
                        let mut index = 0;
                        for _ in 0..(indices.len() / 3) {
                            let i0 = indices[index] as usize;
                            let i1 = indices[index + 1] as usize;
                            let i2 = indices[index + 2] as usize;

                            let v0 = positions[i0];
                            let v1 = positions[i1];
                            let v2 = positions[i2];

                            let w0 = tex_coords[i0];
                            let w1 = tex_coords[i1];
                            let w2 = tex_coords[i2];

                            let e1 = v1 - v0;
                            let e2 = v2 - v0;
                            let x1 = w1.x - w0.x;
                            let x2 = w2.x - w0.x;
                            let y1 = w1.y - w0.y;
                            let y2 = w2.y - w0.y;

                            let denom = x1 * y2 - x2 * y1;
                            if denom.abs() < 1e-8 {
                                index += 3;
                                continue;
                            }
                            let r = 1.0 / denom;
                            let t = (e1 * y2 - e2 * y1) * r;
                            let b = (e2 * x1 - e1 * x2) * r;

                            // Accumulate to each vertex of the triangle
                            trvec[i0] += t;
                            trvec[i1] += t;
                            trvec[i2] += t;
                            btvec[i0] += b;
                            btvec[i1] += b;
                            btvec[i2] += b;

                            index += 3;
                        }
                        // Normalize accumulated tangents/bitangents
                        for i in 0..positions.len() {
                            let t_len = glm::length(&trvec[i]);
                            let b_len = glm::length(&btvec[i]);
                            if t_len > 1e-8 {
                                trvec[i] = trvec[i] / t_len;
                            }
                            if b_len > 1e-8 {
                                btvec[i] = btvec[i] / b_len;
                            }
                        }
                        (trvec, btvec)
                    };

                    // Build vertex buffer
                    let mut vertices: Vec<Vertex> = Vec::new();
                    for i in 0..positions.len() {
                        let p = positions[i];
                        let n = if i < normals.len() {
                            normals[i]
                        } else {
                            glm::zero()
                        };
                        let uv = if i < tex_coords.len() {
                            tex_coords[i]
                        } else {
                            glm::zero()
                        };
                        let t = if i < tangents.len() {
                            tangents[i]
                        } else {
                            glm::zero()
                        };
                        let bt = if i < bitangents.len() {
                            bitangents[i]
                        } else {
                            glm::zero()
                        };
                        vertices.push(Vertex {
                            position: p,
                            normal: n,
                            tangent: t,
                            bitangent: bt,
                            tex_coord: uv,
                        });
                    }

                    // Create drawable with backend-agnostic resources
                    let drawable = Drawable::from_verts(
                        self.backend,
                        &vertices,
                        &indices,
                        if transparent {
                            ObjType::Transparent
                        } else {
                            ObjType::Opaque
                        },
                    )
                    .map_err(|e| ImportError::from("Drawable Creation", &e.message))?;

                    drawable.borrow_mut().add_texture(0, tex_color);
                    drawable.borrow_mut().add_texture(1, tex_mr);
                    drawable.borrow_mut().add_texture(2, tex_norm);
                    drawable
                        .borrow_mut()
                        .set_parallax(classification.parallax_enabled);
                    if mat.double_sided() {
                        drawable.borrow_mut().set_double_sided(true);
                    }
                    drawables.push(drawable);
                }
                let n = Node::create(node.name(), transform, Some(drawables));
                parent
                    .borrow_mut()
                    .add_child(n.clone())
                    .expect("Unable to add child node to parent..");
                parent = n;
            }
            for c in node.children() {
                self.process_node(c, &parent)?
            }
        }
        Ok(())
    }

    /// Import a glTF texture into a backend texture resource.
    /// Returns `(texture, is_transparent)`.
    /// Caches textures by glTF index to avoid duplicate GPU uploads.
    fn import_texture(
        &mut self,
        gltf_tex: gltf::Texture,
        srgb: bool,
        generate_height: bool,
    ) -> (Rc<B::Texture>, bool) {
        let index = gltf_tex.index();
        if let Some((tex, transparent)) = self.texture_buffer.get(&index) {
            return (tex.clone(), *transparent);
        }

        let img = gltf_tex.source();
        let img_raw = &self.images[img.index()];
        let mut image_data: Vec<u8> = Vec::new();

        let sampler = gltf_tex.sampler();
        let address_u = gltf_address_mode(sampler.wrap_s());
        let address_v = gltf_address_mode(sampler.wrap_t());

        let mut transparent = false;
        let (img_data, format): (&[u8], TextureFormat) = match img_raw.format {
            gltf::image::Format::R8 => (&img_raw.pixels, TextureFormat::R8Unorm),
            gltf::image::Format::R8G8 => (&img_raw.pixels, TextureFormat::Rg8Unorm),
            gltf::image::Format::R8G8B8 => {
                // Pad 3-channel RGB to 4-channel RGBA (alpha = 255)
                image_data = convert_3ch_to_4ch_img(img_raw);
                if generate_height {
                    let height_map =
                        generate_height_map(img_raw.width, img_raw.height, &image_data);
                    for (idx, v) in height_map.into_iter().enumerate() {
                        let i = idx * 4 + 3;
                        image_data[i] = v;
                    }
                }
                let fmt = if srgb {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                };
                (image_data.as_slice(), fmt)
            }
            gltf::image::Format::R8G8B8A8 => {
                // Check for transparency by scanning alpha channel
                for i in (3..(img_raw.width * img_raw.height * 4) as usize).step_by(4) {
                    if img_raw.pixels[i] < 255 {
                        transparent = true;
                        break;
                    }
                }
                if generate_height {
                    let height_map =
                        generate_height_map(img_raw.width, img_raw.height, &image_data);
                    for (idx, v) in height_map.into_iter().enumerate() {
                        let i = idx * 4 + 3;
                        image_data[i] = v;
                    }
                }
                let fmt = if srgb {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                };
                (img_raw.pixels.as_slice(), fmt)
            }
            format => panic!(
                "Unsupported image format {:?} for texture index {}",
                format, index
            ),
        };

        let tex = Rc::new(
            self.backend
                .create_texture(
                    &TextureDesc {
                        width: img_raw.width,
                        height: img_raw.height,
                        format,
                        sampler: SamplerDesc {
                            address_u,
                            address_v,
                            filter: FilterMode::Anisotropic,
                            compare: None,
                        },
                        generate_mipmaps: false,
                    },
                    img_data,
                )
                .unwrap_or_else(|e| {
                    panic!(
                        "Unable to load texture with index {}: {}",
                        gltf_tex.index(),
                        e
                    )
                }),
        );

        self.texture_buffer
            .insert(index, (tex.clone(), transparent));
        (tex, transparent)
    }
}

fn generate_height_map(width: u32, height: u32, img_data: &[u8]) -> Vec<u8> {
    let mut raw_depths = vec![0f32; (width * height) as usize];

    // 1. Calculate raw slope variance
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let nx = (img_data[idx] as f32 / 255.0) * 2.0 - 1.0;
            let ny = (img_data[idx + 1] as f32 / 255.0) * 2.0 - 1.0;

            // Measure steepness
            let slope = (nx.powi(2) + ny.powi(2)).sqrt();
            raw_depths[(y * width + x) as usize] = slope;
        }
    }

    let mut final_map = vec![0u8; (width * height) as usize];

    // 2. Stronger 5x5 Smoothing Filter to wipe out pixel noise completely
    for y in 2..(height - 2) {
        for x in 2..(width - 2) {
            let mut sum = 0.0f32;
            let mut count = 0.0f32;

            for ky in -2..=2 {
                for kx in -2..=2 {
                    let sample_idx = ((y as i32 + ky) * width as i32 + (x as i32 + kx)) as usize;
                    sum += raw_depths[sample_idx];
                    count += 1.0;
                }
            }

            let averaged_slope = sum / count;

            // 3. Add a slight contrast curve so subtle noise stays completely flat (0)
            let mut normalized_depth = averaged_slope.powi(2) * 2.0;
            if normalized_depth < 0.05 {
                normalized_depth = 0.0;
            } // Threshold gate

            final_map[(y * width + x) as usize] = (normalized_depth.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }

    final_map
}

struct MaterialClassification {
    parallax_enabled: bool,
    roughness_factor_override: f32,
}

fn classify_gltf_material(
    width: u32,
    height: u32,
    roughness_rgba8: &[u8],
) -> MaterialClassification {
    let total_pixels = (width * height) as f32;
    let mut sum_roughness = 0.0f32;

    // 1. Calculate the Arithmetic Mean of the Roughness channel
    // Remember: glTF packs Roughness directly into the GREEN channel (idx + 1)
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let r_val = roughness_rgba8[idx + 1] as f32 / 255.0; // [0.0 - 1.0]
            sum_roughness += r_val;
        }
    }
    let mean_roughness = sum_roughness / total_pixels;

    // 2. Calculate the Variance / Standard Deviation of the texture channel
    let mut sum_variance = 0.0f32;
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let r_val = roughness_rgba8[idx + 1] as f32 / 255.0;
            sum_variance += (r_val - mean_roughness).powi(2);
        }
    }
    let standard_deviation = (sum_variance / total_pixels).sqrt();

    // 3. THE CLASSIFIER RULE EXECUTOR:
    // If a texture is uniformly dead matte (High Mean, Extremely Low Standard Deviation),
    // it belongs to a fabric layer (like Sponza curtains) or soft fuzz.
    // If it has micro-detail variances (High Standard Deviation), it belongs to stone/metal tiles.
    let is_fabric = mean_roughness > 0.80 && standard_deviation < 0.08;

    MaterialClassification {
        parallax_enabled: !is_fabric, // Enable parallax only if it's NOT fabric
        roughness_factor_override: mean_roughness,
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[Import Error] - {}: {}", self.cause, self.description)
    }
}

impl std::error::Error for ImportError {
    fn description(&self) -> &str {
        &self.description
    }
}
