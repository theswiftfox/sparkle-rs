#include "shared_pixel.hlsli"

struct PS_IN {
	float4 pos : SV_Position;
	float4 worldPos : POSITION_WORLD;
	float3 normal : NORMAL;
	float2 txCoord : TEXCOORD0;
	float3x3 TBN : TBN_MATRIX;
};

struct PS_OUT {
	uint4 position : SV_Target0;
	uint4 albedo : SV_Target1;
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
    PS_OUT output = (PS_OUT)0;

	float4 albedo = txDiffuse.Sample(samplerLinear, input.txCoord);
    float4 pos = input.worldPos;//float4(input.worldPos, calcLinearDepth(input.pos.z));

	// float3 normal_out = input.normal;
	float3 normal = txNormal.Sample(samplerNormal, input.txCoord).xyz;
	// transform to range [-1,1]
	normal = normalize((normal * 2.0) - 1.0);
	// normal.y = normal.y * -1.0;
	// move into world space
	float3 normal_out = normalize(mul(normal, input.TBN));
	// normal_out.y = -normal_out.y;
	normal_out = normal_out * 0.5 + 0.5;

	uint4 phalf = f32tof16(pos);
	output.position.r = (phalf.r << 16) | phalf.g;
	output.position.g = (phalf.b << 16) | phalf.a;
	uint3 nhalf = f32tof16(normal_out);
	// uint3 nhalf_surf = f32tof16(input.normal * 0.5 + 0.5);
	output.position.b = (nhalf.r << 16) | nhalf.g;
	output.position.a = (nhalf.b << 16) | 0; //nhalf_surf.r;
	// output.albedo.b = (nhalf_surf.g << 16) | nhalf_surf.b;
	
	uint4 chalf = f32tof16(albedo);
	output.albedo.r = (chalf.r << 16) | chalf.g;
	output.albedo.g = (chalf.b << 16) | chalf.a;


	// float2 mr = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
	
	// float roughness = mr.g;
	// float metallic = mr.r; //txMetallicRoughness.Sample(textureSampler, input.uv).r;
	// uint rhalf = f32tof16(roughness);
	// uint mhalf = f32tof16(metallic);

	// output.albedo.b = (mhalf << 16) | rhalf;
	// output.albedo.a = 0;

	return output;
}