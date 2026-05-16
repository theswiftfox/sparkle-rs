// Skybox pass: renders a cubemap skybox using cube vertex positions as texture coordinates.
// The .xyww trick forces fragments to max depth (behind all scene geometry).

// Group 0: per-frame vertex uniforms
struct ViewProjUniforms {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> frame: ViewProjUniforms;

// Group 1: per-object uniforms (model/rotation matrix)
struct ModelUniforms {
    model: mat4x4<f32>,
};
@group(1) @binding(0) var<uniform> object: ModelUniforms;

// Group 2: cubemap texture
@group(2) @binding(0) var skybox_texture: texture_cube<f32>;
@group(2) @binding(1) var skybox_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) _normal: vec3<f32>,
    @location(2) _tangent: vec3<f32>,
    @location(3) _bitangent: vec3<f32>,
    @location(4) _tex_coord: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = frame.proj * frame.view * object.model * vec4<f32>(pos, 1.0);
    // .xyww trick: set z = w so depth is always 1.0 (far plane)
    out.position = vec4<f32>(world_pos.xy, world_pos.w, world_pos.w);
    out.tex_coord = pos;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(skybox_texture, skybox_sampler, in.tex_coord).rgb;
    return vec4<f32>(color, 1.0);
}
