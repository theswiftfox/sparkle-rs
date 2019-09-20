
struct VS_IN {
	float4 pos		: SV_Position;
	float2 txCoord 	: TEXCOORD0;
};

struct VS_OUT {
	float4 pos		: SV_Position;
	float2 txCoord 	: TEXCOORD0;
};

cbuffer FrameConsts : register(b0) {
	float4x4 view;
	float4x4 proj;
};

cbuffer PerInstance : register(b1) {
	float4x4 model;
};

VS_OUT main(VS_IN input) {
	VS_OUT output;
	output.pos = mul(proj, mul(view , mul(model, input.pos)));
	output.txCoord = input.txCoord;
	return output;
}