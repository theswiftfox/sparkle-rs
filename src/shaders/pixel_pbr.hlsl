
struct PX_IN {
	float4 pos : SV_Position;
	float3 worldPos : POSITION_WORLD;
	float3 normal : NORMAL0;
	float2 txCoord : TEXCOORD0;
};

struct PX_OUT {
	float4 color : SV_Target;
};

struct Light {
	float4 position;
	float4 color;
	float radius;
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

static const float PI = 3.14159265359;
static const float Epsilon = 0.001;
static const float MinRoughness = 0.04;

// Normal Distribution function --------------------------------------
float NDF(float dotNH, float roughness)
{
	float alpha = roughness * roughness;
	float alpha2 = alpha * alpha;
	float denom = dotNH * dotNH * (alpha2 - 1.0) + 1.0;
	return (alpha2) / (PI * denom * denom);
}

// Geometric Distribution function --------------------------------------
float SchlickSmithGGX(float dotNL, float dotNV, float roughness)
{
	float r = (roughness + 1.0);
	float k = (r * r) / 8.0;
	float GL = dotNL / (dotNL * (1.0 - k) + k);
	float GV = dotNV / (dotNV * (1.0 - k) + k);
	return GL * GV;
}

// Fresnel function ----------------------------------------------------
float3 FresnelSchlick(float cosTheta, float3 F0)
{
	float3 F = F0 + (1.0 - F0) * pow(1.0 - cosTheta, 5.0);
	return F;
}

// Specular BRDF composition --------------------------------------------
float3 BRDF(float3 V, float3 N, float3 position, float3 albedo, float3 F0, Light light, float metallic, float roughness)
{
	// Precalculate vectors and dot products
	float3 L = light.position.xyz - position;
	float distance = length(L);
	float attenuation = 1.0 / (distance * distance);
	float3 radiance = light.color.rgb * attenuation;

	L = normalize(L);
	float3 H = normalize(L + V);

	float dotNV = clamp(abs(dot(N, V)), 0.001, 1.0);
	float dotNL = clamp(dot(N, L), 0.001, 1.0);
	float dotNH = clamp(dot(N, H), 0.0, 1.0);
	float dotHV = clamp(dot(L, H), 0.0, 1.0);

	radiance *= dotNL;

	// D = Normal distribution
	float D = NDF(dotNH, roughness);
	// G = Geometric shadowing term (Microfacets shadowing)
	float G = SchlickSmithGGX(dotNL, dotNV, roughness);
	// F = Fresnel factor
	float3 F = FresnelSchlick(dotHV, F0);

	float3 kD = 1.0 - F;
	float3 diffuse = kD * (albedo / PI);
	float3 specular = (F * G * D) / (4.0 * dotNL * dotNV);

	float3 color = (diffuse + specular) * radiance;

	return color;
}

// ----------------------------------------------------------------------------

PX_OUT main(PX_IN input) {
	PX_OUT output;
	float4 alb = txDiffuse.Sample(samplerLinear, input.txCoord);
	float2 mr_tex = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
	//float3 normal = txNormal.Sample(samplerNormal, input.txCoord).xyz;
	Light l = {
		float4(0.0, 10.0, 5.0, 1.0),
		float4(0.6, 0.4, 0.0, 1.0),
		3.0f
	};

	float3 V = normalize(cameraPos.rgb - input.pos.rgb);
	float3 N = normalize(input.normal);

	float metallic = mr_tex.r;
	float roughness = clamp(mr_tex.g, MinRoughness, 1.0);

	float3 F0 = lerp(float3(0.04, 0.04, 0.04), alb.rgb, metallic);
	float3 color = BRDF(V, N, input.worldPos, alb.rgb, F0, l, metallic, roughness);
	output.color = float4(color, alb.a);

	return output;
}