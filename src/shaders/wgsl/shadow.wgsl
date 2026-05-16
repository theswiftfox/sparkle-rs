// Shadow map pass: renders scene depth from the light's perspective.
// Depth-only (no fragment stage needed, but wgpu requires one for proper depth writing).

// Group 0: per-frame vertex uniforms (light-space matrix)
struct LightSpaceUniforms {
    light_space_matrix: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> frame: LightSpaceUniforms;

// Group 1: per-object uniforms (model matrix)
struct ModelUniforms {
    model: mat4x4<f32>,
};
@group(1) @binding(0) var<uniform> object: ModelUniforms;

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) _normal: vec3<f32>,
    @location(2) _tangent: vec3<f32>,
    @location(3) _bitangent: vec3<f32>,
    @location(4) _tex_coord: vec2<f32>,
) -> @builtin(position) vec4<f32> {
    let world_pos = object.model * vec4<f32>(pos, 1.0);
    return frame.light_space_matrix * world_pos;
}
