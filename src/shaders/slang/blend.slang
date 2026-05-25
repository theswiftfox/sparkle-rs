
[[vk::binding(0, 4)]]
Sampler2D<float4> blendTex[] : register(t0);

[shader("pixel")]
float4 main(float4 pos : SV_Position) : SV_Target {
    int3 coord = int3((int2)pos.xy, 0);
    float4 def = blendTex[7].Load(coord);
    float4 fwd = blendTex[8].Load(coord);

    float3 col = fwd.rgb * fwd.a + def.rgb * (1.0 - fwd.a);
    return float4(col, 1.0);
}