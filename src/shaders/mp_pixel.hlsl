#include "shared_pixel.hlsli"

struct PS_IN {
	float4 pos : SV_Position;
	float4 posLS : POSITION_LIGHT_SPACE;
	float3 worldPos : POSITION_WORLD;
	float3 normal : NORMAL;
	float2 txCoord : TEXCOORD0;
	// float2 txCoordNM : TEXCOORD1;
	float3x3 TBN : TBN_MATRIX;
};

struct PS_OUT {
	float4 color : SV_Target;
};

Texture2D txDiffuse : register(t0);
SamplerState samplerLinear : register(s0);
Texture2D txMetallicRoughness : register(t1);
SamplerState samplerMR: register(s1);
Texture2D txNormal : register(t2);
SamplerState samplerNormal: register(s2);

cbuffer ubo : register(b0) {
	float3 cameraPos;
	bool ssao;
	Light directionalLight;
}

PS_OUT main(PS_IN input) {
	PS_OUT output;
	float4 alb = txDiffuse.Sample(samplerLinear, input.txCoord);
	if (alb.a < 0.01) {
		discard; // discard fully transparent fragments
	}
	float2 mr_tex = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
	float3 normal = txNormal.Sample(samplerNormal, input.txCoord).xyz;

	// transform to range [-1,1]
	normal = normalize((normal * 2.0) - 1.0);
	//normal.y = -normal.y;
	// move into world space
	float3 N = normalize(mul(normal, input.TBN));
	//N.y = -1.0 * N.y;
	// float3 N = input.normal;

	float ambientOcclusion = 1.0; //ssao ? ssaoTex.Sample(samplerSSAO, input.txCoord) : 1.0;

	float metallic = 16.0;//mr_tex.r;
	float shadowed = shadow(input.posLS, N, normalize(-directionalLight.direction.xyz));
	float3 color = blinn_phong(
		directionalLight, 
		cameraPos, 
		input.worldPos, 
		N, 
		alb.rgb, 
		metallic, 
		shadowed, 
		ambientOcclusion
	);
	color = pow(color, 1/2.2);
	output.color = float4(color, alb.a);

	return output;
}