// Forward rendering pass: renders transparent objects with PBR lighting + shadow mapping.

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

// Group 3: fragment uniforms + shadow map
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

@group(3) @binding(2) var shadow_tex: texture_depth_2d;
@group(3) @binding(3) var shadow_sampler: sampler_comparison;

// Constants
const PI: f32 = 3.14159265359;
const AMBIENT: u32 = 0u;
const DIRECTIONAL: u32 = 1u;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tex_coord: vec2<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
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

    let normal_mat = mat3x3<f32>(
        object.model[0].xyz,
        object.model[1].xyz,
        object.model[2].xyz,
    );

    // Gram-Schmidt re-orthogonalize tangent
    let t = normalize(tangent - dot(tangent, normal) * normal);
    out.tangent = normalize(normal_mat * t);
    out.bitangent = normalize(normal_mat * bitangent);
    out.normal = normalize(normal_mat * normal);

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

fn brdf(n: vec3<f32>, position: vec3<f32>, albedo: vec3<f32>, f0: vec3<f32>, metallic: f32, roughness: f32) -> vec3<f32> {
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
    let v = normalize(camera.camera_pos - position);
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

    if (shadow_tex_coords.x < 0.0 || shadow_tex_coords.x > 1.0 ||
        shadow_tex_coords.y < 0.0 || shadow_tex_coords.y > 1.0 ||
        pixel_depth <= 0.0) {
        return 1.0;
    }

    // PCF 3x3
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

// ---- Main fragment ----

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alb = textureSample(diffuse_texture, diffuse_sampler, in.tex_coord);

    // Alpha test
    if (alb.a < 0.01) {
        discard;
    }

    let mr_sample = textureSample(mr_texture, mr_sampler, in.tex_coord);
    let roughness = mr_sample.g;
    let metallic = mr_sample.b;

    // Normal mapping
    var normal_sample = textureSample(normal_texture, normal_sampler, in.tex_coord).xyz;
    normal_sample = normalize(normal_sample * 2.0 - 1.0);
    let tbn = mat3x3<f32>(in.tangent, in.bitangent, in.normal);
    let n = normalize(tbn * normal_sample);

    var color = vec3<f32>(0.0, 0.0, 0.0);
    if (light.light_type != AMBIENT) {
        let f0 = mix(vec3<f32>(0.04), alb.rgb, metallic);
        let shadowed = shadow(in.world_pos, n);
        color = brdf(n, in.world_pos.xyz, alb.rgb, f0, metallic, roughness) * shadowed;
    } else {
        let ambient = light.color * alb.rgb;
        color = ambient;
    }

    return vec4<f32>(color, alb.a);
}
