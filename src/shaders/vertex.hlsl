
struct VS_IN {
	float3 pos			: SV_Position;
	float3 normal		: NORMAL;
	float3 tangent		: TANGENT0;
	float3 bitangent	: BITANGENT0;
	float2 txCoord 		: TEXCOORD0;
	float2 txCoordNM	: TEXCOORD1;
};

struct VS_OUT {
	float4 pos			: SV_Position;
	float3 worldPos 	: POSITION_WORLD;
	float3 normal		: NORMAL;
	float2 txCoord 		: TEXCOORD0;
	float2 txCoordNM	: TEXCOORD1;
	float3x3 TBN		: TBN_MATRIX;
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
	float4 worldPos = mul(model, float4(input.pos, 1.0));
	output.worldPos = worldPos.xyz;
	output.pos = mul(proj, mul(view , worldPos));
	output.txCoord = input.txCoord;
	output.txCoordNM = input.txCoordNM;

	//Make sure tangent is completely orthogonal to normal
	float3 tangent = normalize(input.tangent - dot(input.tangent, input.normal)*input.normal);
	float3 T = normalize(mul(model, float4(tangent, 0.0)).xyz);
	float3 B = normalize(mul(model, float4(input.bitangent, 0.0)).xyz);
	float3 N = normalize(mul(model, float4(input.normal, 0.0)).xyz);

	output.TBN = float3x3(T,B,N);
	output.normal = input.normal;

	return output;
}