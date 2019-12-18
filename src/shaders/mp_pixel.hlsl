#include "shared_pixel.hlsli"

struct PS_IN {
	float4 pos : SV_Position;
	float4 posLS : POSITION_LIGHT_SPACE;
	float3 worldPos : POSITION_WORLD;
	float3 normal : NORMAL;
	float2 txCoord : TEXCOORD0;
	float2 txCoordNM : TEXCOORD1;
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
	float4 cameraPos;
	Light directionalLight;
}

PS_OUT main(PS_IN input) {
	PS_OUT output;
	float4 alb = txDiffuse.Sample(samplerLinear, input.txCoord);
	if (alb.a < 0.01) {
		discard; // discard fully transparent fragments
	}
	float2 mr_tex = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
	float3 normal = txNormal.Sample(samplerNormal, input.txCoordNM).xyz;

	// transform to range [-1,1]
	//normal = normalize((normal * 2.0) - 1.0);
	// move into world space
	//float3 N = normalize(mul(normal, input.TBN));

	float metallic = 32.0;//mr_tex.r;
	float shadowed = shadow(input.posLS, input.normal, normalize(-directionalLight.direction.xyz));
	float3 color = blinn_phong(directionalLight, cameraPos.xyz, input.worldPos, input.normal, alb.rgb, metallic, shadowed);
	color = pow(color, 1/2.2);
	output.color = float4(color, alb.a);

	return output;
}