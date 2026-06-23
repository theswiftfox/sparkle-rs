/// Push constant layout for the scatter compute shader.
///
/// Shared between the Vulkan pipeline layout declaration (`create_compute_pipeline_layout`)
/// and the dispatch call site (`execute_compute_one_shot`).
///
/// **Must stay in sync with `ComputePushConstants` in `scattering_comp.slang`.**
/// Field order, types, and padding must match exactly — the struct is cast to raw
/// bytes and written directly to `vkCmdPushConstants`.
#[repr(C)]
pub struct ComputePushConstants {
    pub max_instances: u32,
    pub asset_offset: u32,
    pub max_height: f32,
    pub spawn_height_min: f32,
    pub spawn_height_max: f32,
    pub slope_max: f32,
    pub scale_min: f32,
    pub scale_max: f32,
    pub tilt_factor: f32,
    pub terrain_segments_f: f32,
}
