
struct PX_IN {
	float4 pos : SV_Position;
	float3 worldPos : POSITION_WORLD;
	float3 normal : NORMAL;
	float2 txCoord : TEXCOORD0;
	float2 txCoordNM : TEXCOORD1;
	float3x3 TBN : TBN_MATRIX;
};

struct PX_OUT {
	float4 color : SV_Target;
};

struct Light {
	float3 direction;
	float3 color;
};

Texture2D txDiffuse : register(t0);
SamplerState samplerLinear : register(s0);
Texture2D txMetallicRoughness : register(t1);
SamplerState samplerMR: register(s1);
Texture2D txNormal : register(t2);
SamplerState samplerNormal: register(s2);

cbuffer ubo : register(b0) {
	float4 cameraPos;
}

float3 blinn_phong(Light light, float3 view_pos, float3 world_pos, float3 normal, float3 albedo, float metallic) {
	float3 ld = normalize(light.direction);
	float3 vd = normalize(view_pos - world_pos);
	float3 hwd = normalize(ld + vd);

	float spec = pow(max(dot(normal, hwd), 0.0), metallic);
	float3 specular = light.color * spec;

	float diff = max(dot(ld, normal), 0.0);
	float3 diffuse = diff * albedo;

	float3 ambient = 0.05 * albedo;
	return ambient + diffuse + specular;
}

PX_OUT main(PX_IN input) {
	PX_OUT output;
	float4 alb = txDiffuse.Sample(samplerLinear, input.txCoord);
	if (alb.a < 0.01) {
		discard; // discard fully transparent fragments
	}
	float2 mr_tex = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
	float3 normal = txNormal.Sample(samplerNormal, input.txCoordNM).xyz;
	Light l = {
		float3(0.0, 1.0, 0.5),
		float3(0.6, 0.4, 0.0),
	};

	// transform to range [-1,1]
	normal = normalize((normal * 2.0) - 1.0);
	// move into world space
	float3 N = normalize(mul(normal, input.TBN));

	float metallic = 16.0;//mr_tex.r;

	float3 color = blinn_phong(l, cameraPos, input.worldPos, N, alb.rgb, metallic);
	output.color = float4(color, alb.a);

	return output;
}