// Deferred pre-pass: fills G-buffer with position, normal, albedo, and material data.
// Three MRT outputs (float textures): position, normal+roughness, albedo+metallic.

// Group 0: per-frame vertex uniforms
struct ViewProjUniforms {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> frame: ViewProjUniforms;

// Group 1: per-object uniforms
struct ModelUniforms {
    model: mat4x4<f32>,
};
@group(1) @binding(0) var<uniform> object: ModelUniforms;

// Group 2: material textures
@group(2) @binding(0) var diffuse_texture: texture_2d<f32>;
@group(2) @binding(1) var diffuse_sampler: sampler;
@group(2) @binding(2) var mr_texture: texture_2d<f32>;
@group(2) @binding(3) var mr_sampler: sampler;
@group(2) @binding(4) var normal_texture: texture_2d<f32>;
@group(2) @binding(5) var normal_sampler: sampler;

// Group 3: fragment uniforms (near/far planes)
struct NearFarUniforms {
    near_plane: f32,
    far_plane: f32,
    _pad0: f32,
    _pad1: f32,
};
@group(3) @binding(0) var<uniform> planes: NearFarUniforms;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_coord: vec2<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
};

struct FragmentOutput {
    @location(0) position: vec4<f32>,
    @location(1) normal_roughness: vec4<f32>,
    @location(2) albedo_metallic: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec3<f32>,
    @location(3) bitangent: vec3<f32>,
    @location(4) tex_coord: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = object.model * vec4<f32>(pos, 1.0);
    out.world_pos = world_pos;
    out.clip_pos = frame.proj * frame.view * world_pos;
    out.tex_coord = tex_coord;

    // Normal matrix: upper-left 3x3 of model (valid for uniform scale)
    let normal_mat = mat3x3<f32>(
        object.model[0].xyz,
        object.model[1].xyz,
        object.model[2].xyz,
    );
    out.normal = normalize(normal_mat * normal);

    // Gram-Schmidt re-orthogonalize tangent
    let t = normalize(tangent - dot(tangent, normal) * normal);
    out.tangent = normalize(normal_mat * t);
    out.bitangent = normalize(normal_mat * bitangent);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    let albedo = textureSample(diffuse_texture, diffuse_sampler, in.tex_coord);

    // Sample and transform normal map
    var normal_sample = textureSample(normal_texture, normal_sampler, in.tex_coord).xyz;
    normal_sample = normalize(normal_sample * 2.0 - 1.0);
    // TBN matrix
    let tbn = mat3x3<f32>(in.tangent, in.bitangent, in.normal);
    let normal_out = normalize(tbn * normal_sample);

    // Metallic-roughness (glTF: G=roughness, B=metallic)
    let mr = textureSample(mr_texture, mr_sampler, in.tex_coord);
    let roughness = mr.g;
    let metallic = mr.b;

    // RT0: world position (Rgba32Float)
    out.position = in.world_pos;

    // RT1: world normal (xyz) + roughness (w) — stored in [-1,1] range directly
    out.normal_roughness = vec4<f32>(normal_out, roughness);

    // RT2: albedo (rgb) + metallic (a)
    out.albedo_metallic = vec4<f32>(albedo.rgb, metallic);

    return out;
}
