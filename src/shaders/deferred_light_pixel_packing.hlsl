#include "pbr.hlsli"
#include "shadow.hlsli"

struct PS_IN {
	float2 uv : UV;
};

struct PS_OUT {
	float4 color : SV_Target;
};

Texture2D<uint4> positionTex : register(t0);
Texture2D<uint4> albedoTex : register(t1);

cbuffer ubo : register(b0) {
	float3 cameraPos;
	bool ssao;
}

StructuredBuffer<Light> lightsBuffer : register(t3);

cbuffer lsubo : register(b1) {
	float4x4 lightSpace;
}

PS_OUT main(PS_IN input, float4 screenPos : SV_Position) {
    PS_OUT output;

    // unpack
	uint4 pos_pack = positionTex.Load(int3(screenPos.xy, 0));
    uint4 alb_pack = albedoTex.Load(int3(screenPos.xy, 0));

	float4 pos = float4(f16tof32(pos_pack.r >> 16), f16tof32(pos_pack.r), f16tof32(pos_pack.g >> 16), f16tof32(pos_pack.g));
	float3 normal = float3(f16tof32(pos_pack.b >> 16), f16tof32(pos_pack.b), f16tof32(pos_pack.a >> 16)) * 2.0 - 1.0;
	// float3 surface_normal = float3(f16tof32(pos_pack.a), f16tof32(alb_pack.b >> 16), f16tof32(alb_pack.b)) * 2.0 - 1.0;
	float4 albedo = float4(f16tof32(alb_pack.r >> 16), f16tof32(alb_pack.r), f16tof32(alb_pack.g >> 16), f16tof32(alb_pack.g));
	float2 mr = float2(f16tof32(alb_pack.b >> 16), f16tof32(alb_pack.b));
	
	if (length(pos.rgb) == 0.0) {
		output.color = 0.0;
		return output;
	}
    float4 posLS = mul(lightSpace, pos);//float4(pos.xyz, 1.0));

	float4 ao = ssaoTex.Load(int3(screenPos.xy, 0));
	float ambientOcclusion = ssao ? ao.r : 1.0;

    float metallic = 16.0;//mr_tex.r;
	float shadowed = shadow(posLS, normal, normalize(-lightsBuffer[0].position.xyz));

	float3 F0 = lerp(float3(0.04, 0.04, 0.04), albedo.rgb, mr.r);
	uint numLights;
	uint stride;
	lightsBuffer.GetDimensions(numLights, stride);
	float3 color = 0.0;
	for (uint i = 0; i < numLights; i++) {
		color += BRDF(
			cameraPos,
			normal,
			pos.xyz,
			albedo.rgb,
			F0,
			lightsBuffer[i],
			mr.r,
			mr.g
		);
	}
	float3 ambient = 0.15 * albedo.rgb * ambientOcclusion;
	color = ambient + shadowed * (color / (color + float3(1.0, 1.0, 1.0)));
	color = pow(color, 1/2.2);
	output.color = float4(color, 1.0);
    return output;
}