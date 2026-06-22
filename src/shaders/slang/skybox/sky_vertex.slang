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

[[vk::push_constant]] cbuffer PushConstants {
	float4x4 model;
	uint tex0;
	uint tex1;
	uint tex2;
	uint tex3;
};

[shader("vertex")]
VS_OUT main(VS_IN input) {
    VS_OUT output = (VS_OUT)0;

	float4x4 view = float4x4(1.0);
    output.pos = mul(proj, mul(view, mul(model, float4(input.pos, 1.0)))).xyww;
    output.texCoord = input.pos;

    return output;
}
