import light;
import pbr;
import shadow;

struct PS_OUT {
	float4 color : SV_Target;
};

[[vk::binding(6, 0)]]
Sampler2D<uint4> gbuffer[] : register(t0);

[[vk::binding(1, 0)]] cbuffer ubo : register(b2) {
	float3 cameraPos;
	uint ssao;
}

[[vk::push_constant]] cbuffer PushConstants {
	float4x4 model;
	uint tex0;
	uint tex1;
	uint tex2;
	uint tex3;
	uint tex4;
};

[shader("pixel")]
PS_OUT main(float4 screenPos : SV_Position) {
    PS_OUT output;

	int3 texPos = int3((int2)screenPos.xy, 0);

    // unpack
	uint4 pos_pack = gbuffer[tex0].Load(texPos);
    uint4 alb_pack = gbuffer[tex1].Load(texPos);

	float4 pos = float4(f16tof32(pos_pack.r >> 16), f16tof32(pos_pack.r), f16tof32(pos_pack.g >> 16), f16tof32(pos_pack.g));
	float3 normal = float3(f16tof32(pos_pack.b >> 16), f16tof32(pos_pack.b), f16tof32(pos_pack.a >> 16)) * 2.0 - 1.0;

    float4 albedo = float4(f16tof32(alb_pack.r >> 16), f16tof32(alb_pack.r), f16tof32(alb_pack.g >> 16), f16tof32(alb_pack.g));
	float2 mr = float2(f16tof32(alb_pack.b >> 16), f16tof32(alb_pack.b));

	if (length(pos.rgb) == 0.0) {
		output.color = 0.0;
		return output;
	}

	float3 color = 0.0;
	if (light0.type != AMBIENT) {
		float3 F0 = lerp(float3(0.04, 0.04, 0.04), albedo.rgb, mr.r);
		float shadowed = shadow(pos, normal, tex3);
		color += BRDF(
			cameraPos,
			normal,
			pos.xyz,
			albedo.rgb,
			F0,
			mr.r,
			mr.g
		) * shadowed;
	} else {
		float ambientOcclusion = 1.0;
		float3 ambient = 0.15 * albedo.rgb * ambientOcclusion;
		color = ambient;
	}
	color = (color / (color + 1.0));
	color = pow(color, 1/2.2);
	output.color = float4(color, 1.0);
    return output;
}
