use std::rc::Rc;

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
}

pub struct ProceduralConfig {
    pub assets: Vec<AssetConfig>,
    pub input_seed: String,
    pub terrain_dir: String,
    pub world_dimension: f32,
    pub terrain_segments: u32,
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

pub fn create_pipeline<B: GpuBackend>(backend: &B) -> Result<B::Pipeline, GpuError> {
    let shaders = backend.load_proc_gen_shaders();
    backend.create_compute_pipeline(&ComputePipelineDesc {
        label: "Scatter Procedural Assets",
        shader_source: &shaders.scattering,
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

fn generate_terrain_mesh(segments: u32, world_size: f32) -> TerrainMesh {
    let n = (segments + 1) as usize;
    let mut vertices = Vec::with_capacity(n * n);
    let mut indices = Vec::with_capacity((segments * segments * 6) as usize);

    for z in 0..=segments {
        for x in 0..=segments {
            let fx = x as f32 / segments as f32;
            let fz = z as f32 / segments as f32;
            vertices.push(Vertex {
                position: glm::vec3(
                    fx * world_size - world_size / 2.0,
                    0.0,
                    fz * world_size - world_size / 2.0,
                ),
                normal: glm::vec3(0.0, 1.0, 0.0),
                tangent: glm::vec3(1.0, 0.0, 0.0),
                bitangent: glm::vec3(0.0, 0.0, -1.0),
                tex_coord: glm::vec2(fx, fz),
            });
        }
    }

    for z in 0..segments {
        for x in 0..segments {
            let bl = z * (segments + 1) + x;
            let br = bl + 1;
            let tl = (z + 1) * (segments + 1) + x;
            let tr = tl + 1;
            indices.extend_from_slice(&[bl, br, tl, br, tr, tl]);
        }
    }

    TerrainMesh { vertices, indices }
}

pub fn load_procedural_world<B: GpuBackend>(
    backend: &B,
    config: &ProceduralConfig,
) -> Result<Scenegraph<B>, GpuError> {
    let terrain_textures = load_terrain_textures(backend, &config.terrain_dir)?;
    let terrain_mesh = generate_terrain_mesh(config.terrain_segments, config.world_dimension);

    let mut terrain = Drawable::from_verts(
        backend,
        &terrain_mesh.vertices,
        &terrain_mesh.indices,
        ObjType::Opaque,
    )?;
    terrain.add_texture(0, Rc::new(terrain_textures.albedo));
    terrain.add_texture(1, Rc::new(terrain_textures.roughness));
    terrain.add_texture(2, Rc::new(terrain_textures.normal));

    let mut instanced_assets = Vec::new();
    for asset in &config.assets {
        let node = import::load_gltf(&asset.path, backend)
            .map_err(|e| GpuError::new(e.to_string(), GpuErrorKind::Other))?;
        let mut drawables = Vec::new();
        collect_drawables(&node, &mut drawables);

        for drawable in drawables {
            let count = drawable.index_count;

            // Instance matrix SSBO — compute fills this; allocate empty
            let instance_buf = backend.create_buffer(
                &BufferDesc {
                    label: format!("procedural_instance_{count}"),
                    usage: BufferUsage::Storage,
                    size: asset.max_count as usize * std::mem::size_of::<glm::Mat4>(),
                },
                None,
            )?;

            // Indirect command buffer — initialize with indexCount=count, instanceCount=0
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

            instanced_assets.push(IndirectDrawable::from_drawable(
                drawable,
                instance_buf,
                cmd_buf,
            ));
        }
    }

    let world_node =
        Node::create_procedural_world(Some("procedural_world"), terrain, instanced_assets);
    let mut sg = Scenegraph::empty();
    sg.set_root(world_node);
    Ok(sg)
}
