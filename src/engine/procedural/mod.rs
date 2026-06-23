use std::rc::Rc;

use rand::{SeedableRng, RngExt, rngs::SmallRng};

use crate::{
    engine::{
        backend::{
            AddressMode, BufferDesc, BufferUsage, ComputePipelineDesc, Drawable, FilterMode,
            GpuBackend, GpuError, GpuErrorKind, IndirectDrawable, ObjType, SamplerDesc,
            TextureDesc, TextureFormat, as_bytes,
        },
        geometry::Vertex,
        scenegraph::{
            Scenegraph,
            node::{Node, collect_drawables},
        },
    },
    import,
};

pub struct AssetConfig {
    pub path: String,
    pub max_count: u32,
    /// Normalized height range [0, 1] for valid spawn locations.
    pub spawn_height_min: f32,
    pub spawn_height_max: f32,
    /// Maximum slope allowed: 0 = flat only, 1 = any slope.
    pub slope_max: f32,
    /// Uniform scale range applied to each instance.
    pub scale_min: f32,
    pub scale_max: f32,
    /// How much the object tilts to align with the terrain normal: 0 = upright, 1 = fully aligned.
    pub tilt_factor: f32,
}

pub struct ProceduralConfig {
    pub assets: Vec<AssetConfig>,
    pub input_seed: String,
    pub terrain_dir: String,
    pub world_dimension: f32,
    pub terrain_segments: u32,
    pub max_height: f32,
    /// How many times the terrain texture tiles across the mesh in each axis.
    /// Higher values = smaller, more frequent repetition. Default: 16.0
    pub texture_tile_factor: f32,
}

struct TerrainTextures<B: GpuBackend> {
    albedo: B::Texture,
    normal: B::Texture,
    roughness: B::Texture,
}

struct TerrainMesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

#[repr(C)]
struct DrawIndirectCommand {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    vertex_offset: i32,
    first_instance: u32,
}

pub fn create_pipeline<B: GpuBackend>(
    backend: &B,
    world_dimension: f32,
) -> Result<B::Pipeline, GpuError> {
    let shaders = backend.load_proc_gen_shaders();
    backend.create_compute_pipeline(&ComputePipelineDesc {
        label: "Scatter Procedural Assets",
        shader_source: &shaders.scattering,
        world_dimension: Some(world_dimension),
    })
}

fn load_terrain_textures<B: GpuBackend>(
    backend: &B,
    terrain_dir: &str,
) -> Result<TerrainTextures<B>, GpuError> {
    // Albedo
    let albedo_img = image::open(format!(
        "{terrain_dir}/textures/forest_ground_04_diff_4k.jpg"
    ))
    .map_err(|e| GpuError::new(e.to_string(), GpuErrorKind::Other))?
    .to_rgba8();
    let albedo = backend.create_texture(
        &TextureDesc {
            format: TextureFormat::Rgba8UnormSrgb,
            width: albedo_img.width(),
            height: albedo_img.height(),
            sampler: SamplerDesc {
                address_u: AddressMode::Repeat,
                address_v: AddressMode::Repeat,
                filter: FilterMode::Anisotropic,
                compare: None,
            },
            generate_mipmaps: false,
        },
        albedo_img.as_raw(),
    )?;

    // Normal
    let normal_img = image::open(format!(
        "{terrain_dir}/textures/forest_ground_04_nor_gl_4k.jpg"
    ))
    .map_err(|e| GpuError::new(e.to_string(), GpuErrorKind::Other))?
    .to_rgba8();
    let normal = backend.create_texture(
        &TextureDesc {
            format: TextureFormat::Rgba8Unorm,
            width: normal_img.width(),
            height: normal_img.height(),
            sampler: SamplerDesc {
                address_u: AddressMode::Repeat,
                address_v: AddressMode::Repeat,
                filter: FilterMode::Anisotropic,
                compare: None,
            },
            generate_mipmaps: false,
        },
        normal_img.as_raw(),
    )?;

    // Metallic-roughness: remap roughness to G channel, metallic=0
    let rough_src = image::open(format!(
        "{terrain_dir}/textures/forest_ground_04_rough_4k.jpg"
    ))
    .map_err(|e| GpuError::new(e.to_string(), GpuErrorKind::Other))?
    .to_rgba8();
    let mr_pixels: Vec<u8> = rough_src
        .pixels()
        .flat_map(|p| {
            [255u8, p[1], 0u8, 255u8] // R=1, G=roughness, B=metallic=0, A=1
        })
        .collect();
    let mr = backend.create_texture(
        &TextureDesc {
            format: TextureFormat::Rgba8Unorm,
            width: rough_src.width(),
            height: rough_src.height(),
            sampler: SamplerDesc {
                address_u: AddressMode::Repeat,
                address_v: AddressMode::Repeat,
                filter: FilterMode::Anisotropic,
                compare: None,
            },
            generate_mipmaps: false,
        },
        &mr_pixels,
    )?;

    Ok(TerrainTextures {
        albedo,
        normal,
        roughness: mr,
    })
}

/// Hash a string seed to u64 via FNV-1a.
fn seed_to_u64(s: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

/// Smooth interpolation (smoothstep / fade).
#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// Sample value noise at (nx, nz) using the permutation table.
fn value_noise(perm: &[u8; 256], nx: f32, nz: f32) -> f32 {
    let xi = nx.floor() as i32;
    let zi = nz.floor() as i32;
    let xf = nx - nx.floor();
    let zf = nz - nz.floor();

    let u = fade(xf);
    let v = fade(zf);

    // Corner indices (wrapping into 0..255)
    let aa = perm[(xi     & 0xFF) as usize] as usize;
    let ab = perm[(xi     & 0xFF) as usize + 1 & 0xFF] as usize;
    let ba = perm[(xi + 1 & 0xFF) as usize] as usize;
    let bb = perm[(xi + 1 & 0xFF) as usize + 1 & 0xFF] as usize;

    // Corner values derived from z index
    let v00 = (perm[(aa + (zi     & 0xFF) as usize) & 0xFF] as f32) / 255.0;
    let v10 = (perm[(ba + (zi     & 0xFF) as usize) & 0xFF] as f32) / 255.0;
    let v01 = (perm[(ab + (zi + 1 & 0xFF) as usize) & 0xFF] as f32) / 255.0;
    let v11 = (perm[(bb + (zi + 1 & 0xFF) as usize) & 0xFF] as f32) / 255.0;

    lerp(lerp(v00, v10, u), lerp(v01, v11, u), v)
}

/// Generate a normalized [0,1] heightmap via FBM over value noise.
/// Output: (segments+1)^2 floats, row-major [z*(segments+1)+x].
fn generate_heightmap(segments: u32, seed: &str) -> Vec<f32> {
    let n = (segments + 1) as usize;
    let mut rng = SmallRng::seed_from_u64(seed_to_u64(seed));

    // Build permutation table
    let mut perm = [0u8; 256];
    for (i, v) in perm.iter_mut().enumerate() {
        *v = i as u8;
    }
    // Fisher-Yates shuffle
    for i in (1..256usize).rev() {
        let j = rng.random_range(0..=(i as u8)) as usize;
        perm.swap(i, j);
    }

    let octaves = 6;
    let lacunarity = 2.0f32;
    let gain = 0.5f32;
    let base_frequency = 2.5f32; // controls overall feature size relative to grid

    let mut map = vec![0.0f32; n * n];
    let mut min_v = f32::MAX;
    let mut max_v = f32::MIN;

    for z in 0..n {
        for x in 0..n {
            let nx = x as f32 / segments as f32 * base_frequency;
            let nz = z as f32 / segments as f32 * base_frequency;

            let mut value = 0.0f32;
            let mut amplitude = 1.0f32;
            let mut frequency = 1.0f32;
            let mut total_amplitude = 0.0f32;

            for _ in 0..octaves {
                value += amplitude * value_noise(&perm, nx * frequency, nz * frequency);
                total_amplitude += amplitude;
                amplitude *= gain;
                frequency *= lacunarity;
            }

            let v = value / total_amplitude;
            map[z * n + x] = v;
            if v < min_v { min_v = v; }
            if v > max_v { max_v = v; }
        }
    }

    // Normalize to [0, 1]
    let range = (max_v - min_v).max(1e-6);
    for v in &mut map {
        *v = (*v - min_v) / range;
    }

    map
}

/// Build heightmap GPU texture (Rgba32Float, R channel = normalized height [0,1]).
fn build_heightmap_texture<B: GpuBackend>(
    backend: &B,
    heightmap: &[f32],
    segments: u32,
) -> Result<Rc<B::Texture>, GpuError> {
    let dim = segments + 1;
    // Pack as Rgba32Float: [h, 0.0, 0.0, 1.0] per pixel
    let mut pixel_data: Vec<u8> = Vec::with_capacity(heightmap.len() * 4 * 4);
    for &h in heightmap {
        pixel_data.extend_from_slice(&h.to_ne_bytes());
        pixel_data.extend_from_slice(&0.0f32.to_ne_bytes());
        pixel_data.extend_from_slice(&0.0f32.to_ne_bytes());
        pixel_data.extend_from_slice(&1.0f32.to_ne_bytes());
    }

    let tex = backend.create_texture(
        &TextureDesc {
            format: TextureFormat::Rgba32Float,
            width: dim,
            height: dim,
            sampler: SamplerDesc {
                address_u: AddressMode::Clamp,
                address_v: AddressMode::Clamp,
                // Nearest matches the point-sampled heights used by the terrain mesh,
                // preventing scatter placements from floating on steep slopes.
                filter: FilterMode::Nearest,
                compare: None,
            },
            generate_mipmaps: false,
        },
        &pixel_data,
    )?;

    Ok(Rc::new(tex))
}

/// Generate terrain mesh with height from heightmap. Normals recalculated via finite differences.
fn generate_terrain_mesh(
    segments: u32,
    world_size: f32,
    heightmap: &[f32],
    max_height: f32,
    texture_tile_factor: f32,
) -> TerrainMesh {
    let n = (segments + 1) as usize;
    let mut positions = Vec::with_capacity(n * n);
    let mut indices = Vec::with_capacity((segments * segments * 6) as usize);

    // Build positions first (needed for normal calculation)
    for z in 0..=segments {
        for x in 0..=segments {
            let fx = x as f32 / segments as f32;
            let fz = z as f32 / segments as f32;
            let h = heightmap[z as usize * n + x as usize] * max_height;
            positions.push(glm::vec3(
                fx * world_size - world_size / 2.0,
                h,
                fz * world_size - world_size / 2.0,
            ));
        }
    }

    // Helper: sample position, clamping at edges
    let pos = |x: i32, z: i32| -> glm::Vec3 {
        let xc = x.clamp(0, segments as i32) as usize;
        let zc = z.clamp(0, segments as i32) as usize;
        positions[zc * n + xc]
    };

    // Build vertices with finite-difference normals
    let mut vertices = Vec::with_capacity(n * n);
    for z in 0..=segments as i32 {
        for x in 0..=segments as i32 {
            let fx = x as f32 / segments as f32;
            let fz = z as f32 / segments as f32;

            // Finite differences: central where possible, one-sided at edges
            let dpdx = pos(x + 1, z) - pos(x - 1, z);
            let dpdz = pos(x, z + 1) - pos(x, z - 1);

            // cross(dpdz, dpdx) gives upward-facing normal for CCW terrain
            let normal = glm::normalize(&glm::cross(&dpdz, &dpdx));

            // Tangent along X axis projected onto the surface
            let world_up = glm::vec3(0.0f32, 1.0, 0.0);
            let tangent = if glm::dot(&normal, &glm::vec3(1.0f32, 0.0, 0.0)).abs() < 0.999 {
                glm::normalize(&glm::cross(&world_up, &normal))
            } else {
                glm::normalize(&glm::cross(&glm::vec3(0.0f32, 0.0, 1.0), &normal))
            };
            let bitangent = glm::cross(&normal, &tangent);

            vertices.push(Vertex {
                position: positions[z as usize * n + x as usize],
                normal,
                tangent,
                bitangent,
                tex_coord: glm::vec2(fx * texture_tile_factor, fz * texture_tile_factor),
            });
        }
    }

    // Build indices (CCW winding from above)
    for z in 0..segments {
        for x in 0..segments {
            let bl = z * (segments + 1) + x;
            let br = bl + 1;
            let tl = (z + 1) * (segments + 1) + x;
            let tr = tl + 1;
            indices.extend_from_slice(&[bl, tl, br, br, tl, tr]);
        }
    }

    TerrainMesh { vertices, indices }
}

pub fn load_procedural_world<B: GpuBackend>(
    backend: &B,
    config: &ProceduralConfig,
    pipeline: &B::Pipeline,
) -> Result<Scenegraph<B>, GpuError> {
    // --- Heightmap (normalized [0,1], used for both mesh and texture) ---
    let heightmap = generate_heightmap(config.terrain_segments, &config.input_seed);
    let heightmap_tex = build_heightmap_texture(backend, &heightmap, config.terrain_segments)?;

    // --- Terrain ---
    let terrain_textures = load_terrain_textures(backend, &config.terrain_dir)?;
    let terrain_mesh = generate_terrain_mesh(
        config.terrain_segments,
        config.world_dimension,
        &heightmap,
        config.max_height,
        config.texture_tile_factor,
    );

    let mut terrain = Drawable::from_verts(
        backend,
        &terrain_mesh.vertices,
        &terrain_mesh.indices,
        ObjType::Opaque,
    )?;
    terrain.add_texture(0, Rc::new(terrain_textures.albedo));
    terrain.add_texture(1, Rc::new(terrain_textures.roughness));
    terrain.add_texture(2, Rc::new(terrain_textures.normal));

    // --- Instanced assets ---
    let mut instanced_assets = Vec::new();
    let mut asset_offset: u32 = 0;
    for asset in &config.assets {
        let node = import::load_gltf(&asset.path, backend)
            .map_err(|e| GpuError::new(e.to_string(), GpuErrorKind::Other))?;
        let mut drawables = Vec::new();
        collect_drawables(&node, &mut drawables);

        for drawable in drawables {
            let count = drawable.index_count;

            let instance_buf = backend.create_buffer(
                &BufferDesc {
                    label: format!("procedural_instance_{count}"),
                    usage: BufferUsage::Storage,
                    size: asset.max_count as usize * std::mem::size_of::<glm::Mat4>(),
                },
                None,
            )?;

            let init_cmd = DrawIndirectCommand {
                index_count: count,
                instance_count: 0,
                first_index: 0,
                vertex_offset: 0,
                first_instance: 0,
            };
            let cmd_buf = backend.create_buffer(
                &BufferDesc {
                    label: format!("procedural_cmd_{count}"),
                    usage: BufferUsage::Indirect,
                    size: std::mem::size_of::<DrawIndirectCommand>(),
                },
                Some(as_bytes(std::slice::from_ref(&init_cmd))),
            )?;

            // Scatter compute: populate instance transforms and instance count
            let dispatch_x = (asset.max_count + 63) / 64;
            backend.execute_compute_one_shot(
                pipeline,
                &[(10, &instance_buf), (11, &cmd_buf)],
                &[(0, heightmap_tex.as_ref())],
                (dispatch_x, 1, 1),
                asset.max_count,
                asset_offset,
                config.max_height,
                asset.spawn_height_min,
                asset.spawn_height_max,
                asset.slope_max,
                asset.scale_min,
                asset.scale_max,
                asset.tilt_factor,
                config.terrain_segments as f32,
            )?;
            asset_offset += dispatch_x * 64; // advance past all thread IDs used by this asset

            instanced_assets.push(IndirectDrawable::from_drawable(
                drawable,
                instance_buf,
                cmd_buf,
            ));
        }
    }

    // --- Assemble scenegraph ---
    let world_node = Node::create_procedural_world(
        Some("procedural_world"),
        terrain,
        instanced_assets,
        heightmap_tex,
    );
    let mut sg = Scenegraph::empty();
    sg.set_root(world_node);
    Ok(sg)
}
