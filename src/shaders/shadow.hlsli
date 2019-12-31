Texture2D txShadowMap : register(t5);
SamplerComparisonState samplerShadowMap : register(s5);

// from https://www.geeks3d.com/20100628/3d-programming-ready-to-use-64-sample-poisson-disc/
static float2 poissonDisk[64] = { 
	float2(-0.613392, 0.617481), float2( 0.170019,-0.040254), float2(-0.299417, 0.791925), float2( 0.645680, 0.493210),
    float2(-0.651784, 0.717887), float2( 0.421003, 0.027070), float2(-0.817194,-0.271096), float2(-0.705374,-0.668203),
    float2( 0.977050,-0.108615), float2( 0.063326, 0.142369), float2( 0.203528, 0.214331), float2(-0.667531, 0.326090),
    float2(-0.098422,-0.295755), float2(-0.885922, 0.215369), float2( 0.566637, 0.605213), float2( 0.039766,-0.396100),
    float2( 0.751946, 0.453352), float2( 0.078707,-0.715323), float2(-0.075838,-0.529344), float2( 0.724479,-0.580798),
    float2( 0.222999,-0.215125), float2(-0.467574,-0.405438), float2(-0.248268,-0.814753), float2( 0.354411,-0.887570),
    float2( 0.175817, 0.382366), float2( 0.487472,-0.063082), float2(-0.084078, 0.898312), float2( 0.488876,-0.783441),
    float2( 0.470016, 0.217933), float2(-0.696890,-0.549791), float2(-0.149693, 0.605762), float2( 0.034211, 0.979980),
    float2( 0.503098,-0.308878), float2(-0.016205,-0.872921), float2( 0.385784,-0.393902), float2(-0.146886,-0.859249),
    float2( 0.643361, 0.164098), float2( 0.634388,-0.049471), float2(-0.688894, 0.007843), float2( 0.464034,-0.188818),
    float2(-0.440840, 0.137486), float2( 0.364483, 0.511704), float2( 0.034028, 0.325968), float2( 0.099094,-0.308023),
    float2( 0.693960,-0.366253), float2( 0.678884,-0.204688), float2( 0.001801, 0.780328), float2( 0.145177,-0.898984),
    float2( 0.062655,-0.611866), float2( 0.315226,-0.604297), float2(-0.780145, 0.486251), float2(-0.371868, 0.882138),
    float2( 0.200476, 0.494430), float2(-0.494552,-0.711051), float2( 0.612476, 0.705252), float2(-0.578845,-0.768792),
    float2(-0.772454,-0.090976), float2( 0.504440, 0.372295), float2( 0.155736, 0.065157), float2( 0.391522, 0.849605),
    float2(-0.620106,-0.328104), float2( 0.789239,-0.419965), float2(-0.545396, 0.538133), float2(-0.178564,-0.596057)
};

float random(float3 seed3, int i) {
	float4 seed4 = float4(seed3, float(i));
	float dot_product = dot(seed4, float4(12.9898,78.233,45.164,94.673));
    return frac(sin(dot_product) * 43758.5453);
}

float shadow(float4 pos, float3 normal) {
	float4 fragmentLS = mul(light.lightSpace, pos);
	float3 lightDir = light.type == 0 ?
		-light.position : light.position;
	int sampleCount = 16;
	float2 shadowTexCoords;
	shadowTexCoords.x = 0.5f + (fragmentLS.x / fragmentLS.w * 0.5f);
	shadowTexCoords.y = 0.5f - (fragmentLS.y / fragmentLS.w * 0.5f);
	float pixelDepth = fragmentLS.z / fragmentLS.w;

	float visibility = 1.0;
	if ((saturate(shadowTexCoords.x) == shadowTexCoords.x) &&
    	(saturate(shadowTexCoords.y) == shadowTexCoords.y) &&
    	(pixelDepth > 0)) {
			visibility = 0.0;
			float bias = max(0.005 * (1.0 - dot(normal, lightDir)), 0.0005);
			for (int i = 0; i < sampleCount; i++) {
				int index = i;
				visibility += clamp(txShadowMap.SampleCmpLevelZero(samplerShadowMap, shadowTexCoords + poissonDisk[index]/1000.0, pixelDepth /*- bias*/), 0.0, 1.0) / float(sampleCount);
			}
	}
	return visibility;
}