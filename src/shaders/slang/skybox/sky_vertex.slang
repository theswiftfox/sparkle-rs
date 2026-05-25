struct VS_IN {
	float3 pos			: POSITION;
	float3 normal		: NORMAL;
	float3 tangent		: TANGENT0;
	float3 bitangent	: BITANGENT0;
	float2 txCoord 		: TEXCOORD0;
};

struct VS_OUT {
    float4 pos: SV_POSITION;
    float3 texCoord: TEXCOORD;
};

[[vk::binding(0, 0)]] cbuffer FrameConsts : register(b0) {
	float4x4 view;
	float4x4 proj;
};

[[vk::binding(0, 1)]] cbuffer PerInstance : register(b1) {
	float4x4 model;
};

[shader("vertex")]
VS_OUT main(VS_IN input) {
    VS_OUT output = (VS_OUT)0;

    output.pos = mul(proj, mul(view, mul(model, float4(input.pos, 1.0)))).xyww;
    output.texCoord = input.pos;

    return output;
}