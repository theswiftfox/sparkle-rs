struct VS_IN {
	float3 pos		: POSITION;
	float3 normal		: NORMAL;
	float3 tangent		: TANGENT0;
	float3 bitangent	: BITANGENT0;
	float2 txCoord		: TEXCOORD0;
};

struct VS_OUT {
	float4 pos		: SV_Position;
	float2 uv		: TEXCOORD0;
};

[shader("vertex")]
VS_OUT main(VS_IN input) {
	VS_OUT output;
	output.pos = float4(input.pos.xy, 0.0, 1.0);
	output.uv = input.txCoord;
	return output;
}
