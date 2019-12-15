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
	float4 position : SV_Target0;
	float4 normal : SV_Target1;
	float4 albedo : SV_Target2;
	float4 pbrSpecular : SV_Target3;
};

cbuffer ubo : register(b0) {
	float near_plane;
    float far_plane;
};

float calcLinearDepth(float zval)
{
	float z = zval * 2.0 - 1.0;
	return (2.0 * near_plane * far_plane) / (far_plane + near_plane - z * (far_plane - near_plane));
}

Texture2D txDiffuse : register(t0);
SamplerState samplerLinear : register(s0);
Texture2D txMetallicRoughness : register(t1);
SamplerState samplerMR: register(s1);
Texture2D txNormal : register(t2);
SamplerState samplerNormal: register(s2);

PS_OUT main(PS_IN input) {
    PS_OUT output;

    output.position = float4(input.worldPos, calcLinearDepth(input.pos.z));

	float4 albedo = txDiffuse.Sample(samplerLinear, input.txCoord);
	//float4 normal;
	//if ((materialFeatures & SPARKLE_MAT_NORMAL_MAP) == SPARKLE_MAT_NORMAL_MAP) {
	float4 normal = 2.0 * txNormal.Sample(samplerNormal, input.txCoordNM) - 1.0;
	//} else {
	//	normal = float4(input.normal, 0.0);
	//}

	float3 n = mul(normalize(normal.xyz), input.TBN);
	output.normal = float4(n * 0.5 + 0.5, 0.0);


	// packing - not working, idk. i'm stupid? maybe use clear value of uint32_t?
	//uint4 chalf = f32tof16(albedo);
	//output.albedoMR.r = (chalf.r << 8) | chalf.g;
	//output.albedoMR.g = (chalf.b << 8) | chalf.a;
	//if (materialFeatures & SPARKLE_MAT_PBR == SPARKLE_MAT_PBR) {
	//	float roughness = roughnessTexture.Sample(textureSampler, input.uv).r;
	//	float metallic = metallicTexture.Sample(textureSampler, input.uv).r;
	//	uint rhalf = f32tof16(roughness);
	//	uint mhalf = f32tof16(metallic);

	//	output.albedoMR.b = (mhalf << 8) | rhalf;
	//	output.albedoMR.a = 0;
	//} else {
	//	float4 specular = specularTexture.Sample(textureSampler, input.uv);
	//	uint4 shalf = f32tof16(specular);
	//	output.albedoMR.b = (shalf.r << 8) | shalf.g;
	//	output.albedoMR.a = (shalf.b << 8) | shalf.a;
	//}

	//return output;

	output.albedo = albedo;
	//if ((materialFeatures & SPARKLE_MAT_PBR) == SPARKLE_MAT_PBR) {
		float2 mr_tex = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
		output.pbrSpecular = float4(mr_tex.r, mr_tex.g, 0.0, 0.0);
	// } else {
	// 	output.pbrSpecular = specularTexture.Sample(specSampler, input.uv);
	// 	output.pbrSpecular.a = 1.0;
	// }

	return output;
}