module parallax;

public float2 parallaxMapping(Sampler2D texture, float2 texCoord, float3 viewDir, float2 dx, float2 dy) {
	// parameters
	const float heightScale = 0.006;
	const float minLayers = 8.0;
	const float maxLayers = 32.0;

	float numLayers = lerp(maxLayers, minLayers, abs(dot(float3(0.0, 0.0, 1.0), viewDir)));

	float layerDepth = 1.0 / numLayers;
	float currentLayerDepth = 0.0;

	float2 P = viewDir.xy / max(viewDir.z, 0.0001) * heightScale;
	float2 deltaCoords = P / numLayers;

	float2 currentTxCoords = texCoord;
	float currentDepthMapValue = texture.SampleLevel(currentTxCoords, 0).w;


	// ray marching
	[loop]
	while(currentLayerDepth < currentDepthMapValue) {
		currentTxCoords -= deltaCoords;
		float currentDepthMapValue = texture.SampleLevel(currentTxCoords, 0).w;
		currentLayerDepth += layerDepth;
	}

	float2 prev = currentTxCoords + deltaCoords;
	float afterDepth = currentDepthMapValue - currentLayerDepth;

	float prevDepthMapValue = texture.SampleLevel(prev, 0).w;
	float beforeDepth = prevDepthMapValue - currentLayerDepth + layerDepth;

	float weight = afterDepth / max(afterDepth - beforeDepth, 0.0001);
	if (isnan(weight) || isinf(weight)) {
		return texCoord;
	}
	return prev * weight + currentTxCoords * (1.0 - weight);
}