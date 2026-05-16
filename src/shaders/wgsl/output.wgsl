// Output composite pass: blends deferred lighting result with forward rendering result.
// Uses ScreenQuad vertex buffer; maps [0,1] positions to clip space [-1,1].

// Group 3: render target textures
@group(3) @binding(0) var deferred_texture: texture_2d<f32>;
@group(3) @binding(1) var deferred_sampler: sampler;
@group(3) @binding(2) var forward_texture: texture_2d<f32>;
@group(3) @binding(3) var forward_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
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
    // Map ScreenQuad [0,1] to clip space [-1,1]
    out.position = vec4<f32>(pos.x * 2.0 - 1.0, -(pos.y * 2.0 - 1.0), 0.0, 1.0);
    out.uv = vec2<f32>(pos.x, pos.y);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(vec2<f32>(in.position.xy));
    let def = textureLoad(deferred_texture, coord, 0);
    let fwd = textureLoad(forward_texture, coord, 0);

    // Alpha-blend forward over deferred (both in linear HDR space)
    var col = fwd.rgb * fwd.a + def.rgb * (1.0 - fwd.a);

    // Tone mapping (Reinhard) — output linear LDR; the sRGB backbuffer
    // format handles gamma encoding automatically via hardware.
    col = col / (col + 1.0);

    return vec4<f32>(col, 1.0);
}
