module light;

public static const uint AMBIENT = 0;
public static const uint DIRECTIONAL = 1;
public static const uint AREA = 2;

#define DELTA 0.0001

public struct Light {
	public float3 position;
    public uint type;
	public float3 color;
    public float radius;
	public float4x4 lightSpace;
};

[[vk::binding(2, 0)]] public cbuffer lightBuffer : register(b3) {
	public Light light0;
}

public float3 blinn_phong(float3 view_pos, float3 world_pos, float3 normal, float3 albedo, float metallic, float shadowed) {
	float3 L = light0.type == DIRECTIONAL ? normalize(-light0.position) : light0.position - view_pos;
    float distance = length(L);
	float attenuation = 1.0 / (distance * distance);
	float3 radiance = light0.color * attenuation;

    float3 ld = normalize(L);
	float3 vd = normalize(view_pos - world_pos);

	float3 specular = float3(0.0, 0.0, 0.0);
    float dot_res = dot(normal, ld);
	if (dot_res < 1.0 + DELTA && dot_res > 1.0 - DELTA) {
		float3 hwd = normalize(ld + vd);
		float spec = pow(max(dot(normal, hwd), 0.0), metallic);
		specular = radiance * 0.01 * spec;
	}

	float diff = max(dot(ld, normal), 0.0);
	float3 diffuse = diff * albedo;

	return max(shadowed, 0.0) * (diffuse + specular);
}
