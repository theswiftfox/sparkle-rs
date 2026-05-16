// SSAO pass: screen-space ambient occlusion.
// Reads G-buffer positions and normals, outputs single-channel AO.
//
// Group 2: noise texture (4x4, tiled via repeat addressing)
// Group 3: SSAO uniforms + G-buffer textures (position, normal+roughness)

struct SsaoUniforms {
    projection: mat4x4<f32>,
    view: mat4x4<f32>,
    resolution: vec2<f32>,
    radius: f32,
    bias: f32,
    kernel: array<vec4<f32>, 32>,
};

@group(3) @binding(0) var<uniform> ssao: SsaoUniforms;
@group(3) @binding(1) var position_tex: texture_2d<f32>;
@group(3) @binding(2) var normal_roughness_tex: texture_2d<f32>;

@group(2) @binding(0) var noise_tex: texture_2d<f32>;
@group(2) @binding(1) var noise_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
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
    out.position = vec4<f32>(pos.x * 2.0 - 1.0, -(pos.y * 2.0 - 1.0), 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(in.position.xy);
    let uv = in.position.xy / ssao.resolution;

    // Read world-space position from G-buffer (plain float, no unpacking)
    let pos_data = textureLoad(position_tex, coord, 0);
    let world_pos = pos_data.xyz;

    // Skip empty pixels (no geometry)
    if (pos_data.w == 0.0) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    }

    // Read world-space normal from G-buffer
    let nr_data = textureLoad(normal_roughness_tex, coord, 0);
    let world_normal = normalize(nr_data.xyz);

    // Transform to view space
    let view_pos = (ssao.view * vec4<f32>(world_pos, 1.0)).xyz;
    let view_normal = normalize((ssao.view * vec4<f32>(world_normal, 0.0)).xyz);

    // Sample noise texture (tiled across screen via repeat addressing)
    let noise_scale = ssao.resolution / 4.0;
    let noise = textureSample(noise_tex, noise_sampler, uv * noise_scale).xyz * 2.0 - 1.0;

    // Build TBN matrix (Gram-Schmidt process)
    let tangent = normalize(noise - view_normal * dot(noise, view_normal));
    let bitangent = cross(view_normal, tangent);
    let tbn = mat3x3<f32>(tangent, bitangent, view_normal);

    // Accumulate occlusion
    var occlusion = 0.0;
    let sample_count = 32u;

    for (var i = 0u; i < sample_count; i = i + 1u) {
        // Get sample position in view space
        let sample_dir = tbn * ssao.kernel[i].xyz;
        let sample_pos = view_pos + sample_dir * ssao.radius;

        // Project sample to screen space
        let clip = ssao.projection * vec4<f32>(sample_pos, 1.0);
        var sample_uv = clip.xy / clip.w;
        sample_uv = sample_uv * vec2<f32>(0.5, -0.5) + 0.5;

        // Bounds check
        if (sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0) {
            continue;
        }

        // Read G-buffer at sample position
        let sample_coord = vec2<i32>(sample_uv * ssao.resolution);
        let sample_pack = textureLoad(position_tex, sample_coord, 0);
        let sample_world = sample_pack.xyz;

        // Transform sampled position to view space
        let sample_depth = (ssao.view * vec4<f32>(sample_world, 1.0)).z;

        // Range check: avoid AO from distant geometry
        let range_check = smoothstep(0.0, 1.0, ssao.radius / abs(view_pos.z - sample_depth));

        // Occlusion test: sample is occluded if geometry is closer to camera
        // (In right-handed view space, closer = less negative = greater z)
        if (sample_depth >= sample_pos.z + ssao.bias) {
            occlusion = occlusion + range_check;
        }
    }

    let ao = 1.0 - (occlusion / f32(sample_count));
    return vec4<f32>(ao, ao, ao, 1.0);
}
