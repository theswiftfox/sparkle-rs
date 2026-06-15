import light;
import pbr;
import shadow;
import parallax;

import color_utils;
import push_constants;


struct PS_IN {
	float4 pos : SV_Position;
	float4 worldPos : POSITION_WORLD;
	float3 worldView	: VIEW_WORLD;
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

[vk::constant_id(1)] float gamma = 2.2;

[shader("pixel")]
PS_OUT main(PS_IN input) {
	PS_OUT output;

	float2 dx = ddx(input.txCoord);
	float2 dy = ddy(input.txCoord);

	float3 tView = normalize(mul(input.TBN, input.worldView));
    // flip vector to point INTO the floor surface, and match glTF UV orientation
 	float3 tangentView;
    tangentView.x = tView.x;   // Keep X native
    tangentView.y = tView.y;  // Flip Y to cancel forward/backward drift
    tangentView.z = abs(tView.z); // Force Z positive so ray-marching always steps downward into depth layers

	float2 offsetTxCoords = input.txCoord;
	if (has_parallax > 0) {
		offsetTxCoords = parallaxMapping(textures[tex2], input.txCoord, tangentView, dx, dy);
	}

	float2 sharpDx = dx * 0.4;
	float2 sharpDy = dy * 0.4;

	float4 alb = textures[tex0].SampleGrad(offsetTxCoords, sharpDx, sharpDy);
	if (alb.a < 0.01) {
		discard;
	}
	alb.rgb = srgbToLinear(alb.rgb, gamma);

	float2 mr = textures[tex1].SampleGrad(offsetTxCoords, sharpDx, sharpDy).gb;
	float3 normal = textures[tex2].SampleGrad(offsetTxCoords, sharpDx, sharpDy).xyz;

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
	output.color = float4(color, alb.a);

	return output;
}
