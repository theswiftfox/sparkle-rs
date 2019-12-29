#include "light.hlsli"

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
float3 BRDF(
    float3 V, 
    float3 N, 
    float3 position, 
    float3 albedo, 
    float3 F0, 
    Light light, 
    float metallic, 
    float roughness)
{
	// Precalculate vectors and dot products
	float3 L = light.type == 0 ? normalize(-light.position) : light.position.xyz - position;
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
