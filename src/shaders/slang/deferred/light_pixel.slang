import light;
import pbr;
import shadow;
import push_constants;

struct PS_OUT {
	float4 color : SV_Target;
};

[[vk::binding(6, 0)]]
Sampler2D<float4> gbuffer[] : register(t0);

[[vk::binding(1, 0)]] cbuffer ubo : register(b2) {
	float3 cameraPos;
	uint ssao;
}

[shader("pixel")]
PS_OUT main(float4 screenPos : SV_Position) {
    PS_OUT output;

	int3 texPos = int3((int2)screenPos.xy, 0);

	float4 pos = gbuffer[tex0].Load(texPos);
	float4 normal_roughness = gbuffer[tex1].Load(texPos);
	float4 albedo_metallic = gbuffer[tex2].Load(texPos);

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
		float shadowed = (tex4 != ~0u)
			? gbuffer[tex4].Load(texPos).r
			: shadow(pos, normal, tex3);
		// float shadowed = gbuffer[tex4].Load(texPos).r;
		// color = BRDF(
		// 	normalize(cameraPos - pos.xyz),
		// 	normal,
		// 	pos.xyz,
		// 	albedo.rgb,
		// 	F0,
		// 	mr.r,
		// 	mr.g,
		// ) * shadowed;
		color = gbuffer[tex4].Load(texPos).rgb;
	} else {
		float ambientOcclusion = 1.0;
		float3 ambient = 0.15 * albedo.rgb * ambientOcclusion;
		color = ambient;
	}
	output.color = float4(color, 1.0);
    return output;
}
