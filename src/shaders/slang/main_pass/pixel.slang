import light;
import pbr;
import shadow;

import color_utils;

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

[[vk::binding(6, 0)]]
Sampler2D<float4> textures[] : register(t0);

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

[vk::constant_id(0)] uint isHdr = false;
[vk::constant_id(1)] float gamma = 2.2;

[shader("pixel")]
PS_OUT main(PS_IN input) {
	PS_OUT output;
	float4 alb = textures[tex0].Sample(input.txCoord);
	if (alb.a < 0.01) {
		discard;
	}
	alb.rgb = srgbToLinear(alb.rgb, gamma);

	float2 mr = textures[tex1].Sample(input.txCoord).gb;
	float3 normal = textures[tex2].Sample(input.txCoord).xyz;

	normal = normalize((normal * 2.0) - 1.0);
	float3 N = normalize(mul(normal, input.TBN));

	float ambientOcclusion = 1.0;

	float3 color = 0.0;
	if (light0.type != AMBIENT) {
		float3 F0 = lerp(float3(0.04, 0.04, 0.04), alb.rgb, mr.r);
		float shadowed = shadow(input.worldPos, N, tex3);
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
	// hdr
	// color = color / (color + 1.0);
	// color = pow(color, 1/2.2);
	output.color = float4(color, alb.a);

	return output;
}
