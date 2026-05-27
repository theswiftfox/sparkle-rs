struct VS_IN {
	float3 pos			: POSITION;
	// float3 normal		: NORMAL;
	// float3 tangent		: TANGENT0;
	// float3 bitangent	: BITANGENT0;
	// float2 txCoord 		: TEXCOORD0;
	// float2 txCoordNM	: TEXCOORD1;
};

struct VS_OUT {
	float4 pos			: SV_Position;
};

[[vk::binding(4, 0)]] cbuffer FrameConsts : register(b0) {
	float4x4 lightSpaceMatrix;
};

[[vk::binding(1, 0)]] cbuffer PerInstance : register(b1) {
	float4x4 model;
};

[shader("vertex")]
VS_OUT main(VS_IN input) {
	VS_OUT output;
	float4 worldPos = mul(model, float4(input.pos, 1.0));
	output.pos = mul(lightSpaceMatrix, worldPos);

	return output;
}