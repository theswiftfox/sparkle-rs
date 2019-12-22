struct VS_IN {
	float3 pos			: SV_Position;
	float3 normal		: NORMAL;
	float3 tangent		: TANGENT0;
	float3 bitangent	: BITANGENT0;
	float2 txCoord 		: TEXCOORD0;
	// float2 txCoordNM	: TEXCOORD1;
};

struct VS_OUT {
	float4 pos			: SV_Position;
};

cbuffer FrameConsts : register(b0) {
	float4x4 lightSpaceMatrix;
};

cbuffer PerInstance : register(b1) {
	float4x4 model;
};

VS_OUT main(VS_IN input) {
	VS_OUT output;
	float4 worldPos = mul(model, float4(input.pos, 1.0));
	output.pos = mul(lightSpaceMatrix, worldPos);

	return output;
}