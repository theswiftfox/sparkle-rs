import light;
import shadow_utils;

[[vk::binding(8, 0)]] Texture2D txShadowMaps[] : register(t6);
[[vk::binding(9, 0)]] SamplerComparisonState samplerShadowMaps[] : register(s7);

public float shadow(float4 pos, float3 normal, uint shadowIdx) {
	float4 fragmentLS = mul(light0.lightSpace, pos);
	float3 lightDir = -light0.position; // direction from surface toward light (negate stored ray direction)
	int sampleCount = 16;
	float2 shadowTexCoords;
	shadowTexCoords.x = 0.5f + (fragmentLS.x / fragmentLS.w * 0.5f);
	shadowTexCoords.y = 0.5f - (fragmentLS.y / fragmentLS.w * 0.5f);
	float pixelDepth = fragmentLS.z / fragmentLS.w;

	float visibility = 1.0;
	if ((saturate(shadowTexCoords.x) == shadowTexCoords.x) &&
    	(saturate(shadowTexCoords.y) == shadowTexCoords.y) &&
    	(pixelDepth > 0) && (pixelDepth <= 1.0)) {
			visibility = 0.0;
			float bias = max(0.005 * (1.0 - dot(normal, lightDir)), 0.0005);
			for (int i = 0; i < sampleCount; i++) {
				int index = i;
				visibility += clamp(txShadowMaps[shadowIdx].SampleCmpLevelZero(samplerShadowMaps[shadowIdx], shadowTexCoords + poissonDisk[index]/1000.0, pixelDepth - bias), 0.0, 1.0) / float(sampleCount);
			}
	}
	return visibility;
}