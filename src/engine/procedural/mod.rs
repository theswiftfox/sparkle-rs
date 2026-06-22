use crate::{engine::{
    backend::{ComputePipelineDesc, GpuBackend, GpuError},
    scenegraph::Scenegraph,
}, import};

struct Asset {
    path: String,
}

pub struct ProceduralConfig {
    assets: Vec<Asset>,
    input_seed: String,
}

pub fn create_pipeline<B: GpuBackend>(backend: &B) -> Result<B::Pipeline, GpuError> {
    let shaders = backend.load_proc_gen_shaders();
    backend.create_compute_pipeline(&ComputePipelineDesc {
        label: "Scatter Procedural Assets",
        shader_source: &shaders.scattering,
    })
}

pub fn load_procedural_world<B: GpuBackend>(
    backend: &B,
    config: &ProceduralConfig,
) -> Result<Scenegraph<B>, GpuError> {
    let loaded_assets = config.assets.iter().map(|asset| {
        let node = import::load_gltf(&asset.path, backend)?;
    })
    todo!()
}
