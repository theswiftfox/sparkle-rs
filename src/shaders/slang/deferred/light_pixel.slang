import light;
import pbr;
import shadow;

struct PS_OUT {
	float4 color : SV_Target;
};

[[vk::binding(5, 0)]]
Sampler2D<float4> gbuffer[] : register(t0);

[[vk::binding(2, 0)]] cbuffer ubo : register(b2) {
	float3 cameraPos;
	uint ssao;
}

static const uint POS_WORLD = 0;
static const uint POS_NORMALS_ROUGHNESS = 1;
static const uint POS_ALBEDO_METALLIC = 2;

[shader("pixel")]
PS_OUT main(float4 screenPos : SV_Position) {
    PS_OUT output;

	int3 texPos = int3((int2)screenPos.xy, 0);

	float4 pos = gbuffer[POS_WORLD].Load(texPos);
	float4 normal_roughness = gbuffer[POS_NORMALS_ROUGHNESS].Load(texPos);
	float4 albedo_metallic = gbuffer[POS_ALBEDO_METALLIC].Load(texPos);

	float3 normal = normal_roughness.xyz;
	float3 albedo = albedo_metallic.rgb;
	float2 mr = float2(albedo_metallic.w, normal_roughness.w);

	if (length(pos.rgb) == 0.0) {
		output.color = 0.0;
		return output;
	}

	float3 color = 0.0;
	if (light0.type != AMBIENT) {
		float3 F0 = lerp(float3(0.04, 0.04, 0.04), albedo.rgb, mr.r);
		float shadowed = shadow(pos, normal);
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
		bool ssao_enabled = ssao != 0;
		float ambientOcclusion = ssao_enabled ? gbuffer[6].Load(texPos).r : 1.0;
		float3 ambient = 0.15 * albedo.rgb * ambientOcclusion;
		color = ambient;
	}
	color = (color / (color + 1.0));
	color = pow(color, 1/2.2);
	output.color = float4(color, 1.0);
    return output;
}