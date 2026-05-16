// SSAO blur pass: 4x4 box blur to smooth the noisy SSAO output.
//
// Group 3: raw SSAO texture (bound as render target texture)

@group(3) @binding(0) var ssao_tex: texture_2d<f32>;
@group(3) @binding(1) var ssao_sampler: sampler;

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
    let dims = vec2<f32>(textureDimensions(ssao_tex));
    let texel_size = 1.0 / dims;
    let uv = in.position.xy / dims;

    var result = 0.0;
    for (var x = -2i; x < 2i; x = x + 1i) {
        for (var y = -2i; y < 2i; y = y + 1i) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
            result += textureSample(ssao_tex, ssao_sampler, uv + offset).r;
        }
    }
    result = result / 16.0;

    return vec4<f32>(result, result, result, 1.0);
}
