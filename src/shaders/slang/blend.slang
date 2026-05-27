
[[vk::binding(6, 0)]]
Sampler2D<float4> blendTex[] : register(t0);

[[vk::push_constant]] cbuffer PushConstants {
	float4x4 model;
	uint tex0;
	uint tex1;
	uint tex2;
	uint tex3;
};

[shader("pixel")]
float4 main(float4 pos : SV_Position) : SV_Target {
    int3 coord = int3((int2)pos.xy, 0);
    float4 def = blendTex[tex0].Load(coord);
    float4 fwd = blendTex[tex1].Load(coord);

    float3 col = fwd.rgb * fwd.a + def.rgb * (1.0 - fwd.a);
    return float4(col, 1.0);
}
