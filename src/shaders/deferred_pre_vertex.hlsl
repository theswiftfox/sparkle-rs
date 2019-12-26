
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
	float4 worldPos 	: POSITION_WORLD;
	float4 viewPos		: POSITION_VIEW;
	float3 normal		: NORMAL;
	float3 normalVS		: NORMAL_VIEW;
	float2 txCoord 		: TEXCOORD0;
	// float2 txCoordNM	: TEXCOORD1;
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
	output.worldPos = worldPos;//.xyz;
	output.viewPos = mul(view, worldPos);
	output.pos = mul(proj, output.viewPos);
	output.txCoord = input.txCoord;
	// output.txCoordNM = input.txCoordNM;

	float3x3 normalMat = transpose((float3x3)model);

	output.normal = normalize(mul(normalMat, input.normal));
	float3x3 normalMatVS = (float3x3)mul(view, model);
	output.normalVS = normalize(mul(normalMatVS, input.normal));

	//Make sure tangent is completely orthogonal to normal
	float3 tangent = normalize(input.tangent - dot(input.tangent, input.normal)*input.normal);
	float3 T = normalize(mul(normalMat, tangent));
	float3 B = normalize(mul(normalMat, input.bitangent));
	float3 N = normalize(mul(normalMat, input.normal));

	output.TBN = float3x3(T,B,N);
	return output;
}