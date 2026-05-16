// Deferred lighting pass: fullscreen quad that reads G-buffer, applies PBR lighting.
// Accumulative (additive blend) — called once per light.

// Group 3: fragment uniforms + textures
struct CameraUniforms {
    camera_pos: vec3<f32>,
    ssao: u32,
};
@group(3) @binding(0) var<uniform> camera: CameraUniforms;

struct GpuLight {
    position: vec3<f32>,
    light_type: u32,
    color: vec3<f32>,
    radius: f32,
    light_space: mat4x4<f32>,
};
@group(3) @binding(1) var<uniform> light: GpuLight;

@group(3) @binding(2) var position_tex: texture_2d<f32>;
@group(3) @binding(3) var normal_roughness_tex: texture_2d<f32>;
@group(3) @binding(4) var albedo_metallic_tex: texture_2d<f32>;
@group(3) @binding(5) var shadow_tex: texture_depth_2d;
@group(3) @binding(6) var shadow_sampler: sampler_comparison;
@group(3) @binding(7) var ssao_tex: texture_2d<f32>;
@group(3) @binding(8) var ssao_sampler: sampler;

// Constants
const PI: f32 = 3.14159265359;
const AMBIENT: u32 = 0u;
const DIRECTIONAL: u32 = 1u;

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

// ---- PBR functions ----

fn NDF(dot_nh: f32, roughness: f32) -> f32 {
    let alpha = roughness * roughness;
    let alpha2 = alpha * alpha;
    let denom = dot_nh * dot_nh * (alpha2 - 1.0) + 1.0;
    return alpha2 / (PI * denom * denom);
}

fn schlick_smith_ggx(dot_nl: f32, dot_nv: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    let gl = dot_nl / (dot_nl * (1.0 - k) + k);
    let gv = dot_nv / (dot_nv * (1.0 - k) + k);
    return gl * gv;
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

fn brdf(camera_pos: vec3<f32>, n: vec3<f32>, position: vec3<f32>, albedo: vec3<f32>, f0: vec3<f32>, metallic: f32, roughness: f32) -> vec3<f32> {
    var l: vec3<f32>;
    if (light.light_type == DIRECTIONAL) {
        l = normalize(-light.position);
    } else {
        l = light.position - position;
    }
    let distance = length(l);
    let attenuation = light.radius / (distance * distance);
    var radiance = light.color * attenuation;

    l = normalize(l);
    let v = normalize(camera_pos - position);
    let h = normalize(l + v);

    let dot_nv = clamp(abs(dot(n, v)), 0.001, 1.0);
    let dot_nl = clamp(dot(n, l), 0.001, 1.0);
    let dot_nh = clamp(dot(n, h), 0.0, 1.0);
    let dot_hv = clamp(dot(l, h), 0.0, 1.0);

    radiance = radiance * dot_nl;

    let d = NDF(dot_nh, roughness);
    let g = schlick_smith_ggx(dot_nl, dot_nv, roughness);
    let f = fresnel_schlick(dot_hv, f0);

    let k_d = (1.0 - f) * (1.0 - metallic);
    let diffuse = k_d * (albedo / PI);
    let specular = (f * g * d) / (4.0 * dot_nl * dot_nv);

    return (diffuse + specular) * radiance;
}

// ---- Shadow sampling ----

fn shadow(pos: vec4<f32>, normal: vec3<f32>) -> f32 {
    let fragment_ls = light.light_space * pos;
    let shadow_tex_coords = vec2<f32>(
        0.5 + (fragment_ls.x / fragment_ls.w * 0.5),
        0.5 - (fragment_ls.y / fragment_ls.w * 0.5),
    );
    let pixel_depth = fragment_ls.z / fragment_ls.w;

    // Check if in shadow map bounds
    if (shadow_tex_coords.x < 0.0 || shadow_tex_coords.x > 1.0 ||
        shadow_tex_coords.y < 0.0 || shadow_tex_coords.y > 1.0 ||
        pixel_depth <= 0.0) {
        return 1.0;
    }

    // Simple PCF with 9 samples
    let texel_size = 1.0 / 4096.0;
    var visibility = 0.0;
    for (var x = -1i; x <= 1i; x = x + 1i) {
        for (var y = -1i; y <= 1i; y = y + 1i) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
            visibility += textureSampleCompare(shadow_tex, shadow_sampler, shadow_tex_coords + offset, pixel_depth);
        }
    }
    return visibility / 9.0;
}

// ---- Main ----

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(vec2<f32>(in.position.xy));

    // Read G-buffer (plain float textures, no unpacking needed)
    let pos = textureLoad(position_tex, coord, 0);
    let nr = textureLoad(normal_roughness_tex, coord, 0);
    let am = textureLoad(albedo_metallic_tex, coord, 0);

    let normal = nr.xyz;
    let roughness = nr.w;
    let albedo = am.rgb;
    let metallic = am.a;

    // Skip empty pixels (no geometry rendered)
    if (pos.w == 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    var color = vec3<f32>(0.0, 0.0, 0.0);
    if (light.light_type != AMBIENT) {
        let f0 = mix(vec3<f32>(0.04), albedo, metallic);
        let shadowed = shadow(pos, normal);
        color = brdf(camera.camera_pos, normal, pos.xyz, albedo, f0, metallic, roughness) * shadowed;
    } else {
        // Sample SSAO (or use 1.0 if disabled)
        var ambient_occlusion = 1.0;
        if (camera.ssao != 0u) {
            let dims = vec2<f32>(textureDimensions(ssao_tex));
            let uv = in.position.xy / dims;
            ambient_occlusion = textureSample(ssao_tex, ssao_sampler, uv).r;
        }
        let ambient = light.color * albedo * ambient_occlusion;
        color = ambient;
    }

    return vec4<f32>(color, 1.0);
}
