#include "pbr.hlsli"
#include "shadow.hlsli"

struct PS_IN {
	float4 pos : SV_Position;
	float4 worldPos : POSITION_WORLD;
	float3 normal : NORMAL;
	float2 txCoord : TEXCOORD0;
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
}

PS_OUT main(PS_IN input) {
	PS_OUT output;
	float4 alb = txDiffuse.Sample(samplerLinear, input.txCoord);
	if (alb.a < 0.01) {
		discard; // discard fully transparent fragments
	}
	float2 mr = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
	float3 normal = txNormal.Sample(samplerNormal, input.txCoord).xyz;

	// transform to range [-1,1]
	normal = normalize((normal * 2.0) - 1.0);
	//normal.y = -normal.y;
	// move into world space
	float3 N = normalize(mul(normal, input.TBN));
	//N.y = -1.0 * N.y;
	// float3 N = input.normal;

	float ambientOcclusion = 1.0; //ssao ? ssaoTex.Sample(samplerSSAO, input.txCoord) : 1.0;

	//float metallic = 16.0;//mr_tex.r;

	float3 color = 0.0;
	if (light.type != AMBIENT) {
		float3 F0 = lerp(float3(0.04, 0.04, 0.04), alb.rgb, mr.r);
		float shadowed = shadow(input.worldPos, N);//normalize(-lightsBuffer[0].position.xyz));
		color += BRDF(
			cameraPos,
			N,
			input.worldPos.xyz,
			alb.rgb,
			F0,
			mr.r,
			mr.g
		) * shadowed;
	} else {
		float3 ambient = 0.15 * alb.rgb * ambientOcclusion;
		color = ambient;
	}
	color = color / (color + 1.0);
	color = pow(color, 1/2.2);
	output.color = float4(color, alb.a);

	return output;
}