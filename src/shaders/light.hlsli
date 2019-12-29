Texture2D ssaoTex : register(t4);

struct Light {
	float3 position;
    uint type; // 0 directional, 1 point, 
	float3 color;
    float radius;
};

float3 blinn_phong(Light light, float3 view_pos, float3 world_pos, float3 normal, float3 albedo, float metallic, float shadowed, float ambientOcclusion) {
	float3 L = light.type == 0 ? normalize(-light.position) : light.position - view_pos;
    float distance = length(L);
	float attenuation = 1.0 / (distance * distance);
	float3 radiance = light.color.rgb * attenuation;

    float3 ld = normalize(L);
	float3 vd = normalize(view_pos - world_pos);

	float3 specular = float3(0.0, 0.0, 0.0);
	if (dot(normal, ld)) {
		float3 hwd = normalize(ld + vd);
		float spec = pow(max(dot(normal, hwd), 0.0), metallic);
		specular = radiance * 0.01 * spec;
	}

	float diff = max(dot(ld, normal), 0.0);
	float3 diffuse = diff * albedo;

	float3 ambient = 0.15 * albedo * ambientOcclusion;
	return ambient + max(shadowed, 0.0) * (diffuse + specular);
}