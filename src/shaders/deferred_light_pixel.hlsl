#include "shared_pixel.hlsli"

struct PS_IN {
	float2 uv : UV;
};

struct PS_OUT {
	float4 color : SV_Target;
};

Texture2D positionTex : register(t0);
SamplerState posSampler : register(s0);
// Texture2D positionLSTex : register(t1);
// SamplerState posLSSampler : register(s1);
Texture2D normalsTex : register(t1);
SamplerState normSampler : register(s1);
Texture2D albedoTex : register(t2);
SamplerState albSampler : register(s2);
Texture2D metallicRoughnessTex : register(t3);
SamplerState mrSampler : register(s3);

cbuffer ubo : register(b0) {
	float4 cameraPos;
	Light directionalLight;
    float4x4 lightSpace;
}

PS_OUT main(PS_IN input) {
    PS_OUT output;

    // unpack
	float4 pos = positionTex.Sample(posSampler, input.uv);

	if (length(pos.rgb) == 0.0) {
		output.color = 0.0;
		return output;
	}
    float4 posLS = 	posLS = mul(lightSpace, float4(pos.xyz, 1.0));
	float3 normal = normalsTex.Sample(normSampler, input.uv).rgb * 2.0 - 1.0;

	float4 albedo = albedoTex.Sample(albSampler, input.uv);
	float4 metallicRoughness = metallicRoughnessTex.Sample(mrSampler, input.uv);

    float metallic = 32.0;//mr_tex.r;
	float shadowed = shadow(posLS, normal, normalize(-directionalLight.direction.xyz));
	float3 color = blinn_phong(directionalLight, cameraPos.xyz, pos.xyz, normal, albedo.rgb, metallic, shadowed);
	color = pow(color, 1/2.2);
	output.color = float4(color, 1.0);
    return output;
}