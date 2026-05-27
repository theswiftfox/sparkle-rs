struct VS_IN {
	float3 pos			: POSITION;
};

struct VS_OUT {
	float4 pos			: SV_Position;
};

[[vk::binding(3, 0)]] cbuffer FrameConsts : register(b0) {
	float4x4 lightSpaceMatrix;
};

[[vk::push_constant]] cbuffer PushConstants {
	float4x4 model;
	uint tex0;
	uint tex1;
	uint tex2;
	uint tex3;
};

[shader("vertex")]
VS_OUT main(VS_IN input) {
	VS_OUT output;
	float4 worldPos = mul(model, float4(input.pos, 1.0));
	output.pos = mul(lightSpaceMatrix, worldPos);

	return output;
}
