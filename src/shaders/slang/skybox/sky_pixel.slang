struct PS_IN {
    float4 pos: SV_POSITION;
    float3 texCoord: TEXCOORD;
};

[[vk::binding(7, 0)]]
SamplerCube<float4> skyboxCubemaps[] : register(t0);

[[vk::push_constant]] cbuffer PushConstants {
	float4x4 model;
	uint tex0;
	uint tex1;
	uint tex2;
	uint tex3;
};

[shader("pixel")]
float4 main(PS_IN input) : SV_TARGET {
    return float4(skyboxCubemaps[tex0].Sample(input.texCoord).rgb, 1.0);
}
