#include "shared_pixel.hlsli"

struct PS_IN {
	float2 uv : UV;
};

struct PS_OUT {
	float4 color : SV_Target;
};

Texture2D<uint4> positionTex : register(t0);
Texture2D<uint4> albedoTex : register(t1);

cbuffer ubo : register(b0) {
	float4 cameraPos;
	Light directionalLight;
}

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
	// output.color = float4(normal, 1.0);
	// return output;
	if (length(pos.rgb) == 0.0) {
		output.color = 0.0;
		return output;
	}
    float4 posLS = 	posLS = mul(lightSpace, pos);//float4(pos.xyz, 1.0));

    float metallic = 16.0;//mr_tex.r;
	float shadowed = shadow(posLS, normal, normalize(-directionalLight.direction.xyz));
	float3 color = blinn_phong(directionalLight, cameraPos.xyz, pos.xyz, normal, albedo.rgb, metallic, shadowed);
	color = pow(color, 1/2.2);
	output.color = float4(color, 1.0);
    return output;
}