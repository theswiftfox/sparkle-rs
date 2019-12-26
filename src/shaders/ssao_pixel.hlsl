struct PS_IN {
	float2 uv : UV;
};

struct PS_OUT {
	float4 color : SV_Target;
};

// Texture2D<uint4> positionTex : register(t0);
Texture2D<uint4> viewSpace : register(t0);
// SamplerState samplerVS : register(s1);
Texture2D<float4> ssaoNoise : register(t2);
SamplerState samplerNoise: register(s2);

cbuffer ubo : register(b0) {
	float4 kernel[16];
    float4x4 proj;
    float2 screen_resolution;
};
#define kernelSize 16
#define radius 1.0
#define bias 0.025
#define scale 2.0
#define intensity 2.0

float3 getPosition(in float2 txcoords) {
    float2 coords = txcoords;
    coords.x = coords.x < 0.0 ? 0.0 : coords.x > screen_resolution.x ? screen_resolution.x - 1 : coords.x;
    coords.y = coords.y < 0.0 ? 0.0 : coords.y > screen_resolution.y ? screen_resolution.y - 1 : coords.y;

    uint4 pos_pack = viewSpace.Load(int3(coords, 0));
    return float3(f16tof32(pos_pack.r >> 16), f16tof32(pos_pack.r), f16tof32(pos_pack.g >> 16));
}

float doAmbientOcclusion(in float2 tcoord, in float2 uv, in float3 p, in float3 cnorm)
{
	//float3 sampled = getPosition(tcoord + uv);
	const float3 diff = getPosition(tcoord + uv) - p;
	const float3 v = normalize(diff);
	const float d = length(diff) * scale;
	//float rangeCheck = smoothstep(0.0, 1.0, radius / abs(p.z - sampled.z));
	//return (sampled.z >= p.z + bias ? 1.0 : 0.0) * rangeCheck;
	return max(0.0, dot(cnorm, v) - bias) * (1.0 / (1.0 + d)) * intensity;
}

PS_OUT main(PS_IN input, float4 screenPos : SV_Position) {
    PS_OUT output = (PS_OUT)0;

    // unpack
	uint4 pos_pack = viewSpace.Load(int3(screenPos.xy, 0));
    float4 pos = float4(f16tof32(pos_pack.r >> 16), f16tof32(pos_pack.r), f16tof32(pos_pack.g >> 16), f16tof32(pos_pack.g));
	float3 normal = float3(f16tof32(pos_pack.b >> 16), f16tof32(pos_pack.b), f16tof32(pos_pack.a)) * 2.0 - 1.0;
    normal = normalize(normal);
    // output.color = float4(normal, 1.0);
    // return output;
	if (length(pos.rgb) == 0.0) {
		output.color = 0.0;
		return output;
	}

   	float2 noise_scale = 1.0 / 8.0;
    float3 noise = normalize(ssaoNoise.Sample(samplerNoise, screenPos.xy * noise_scale).xyz);

	const float2 vec[4] = { float2(1,0),float2(-1,0), float2(0,1),float2(0,-1) };
	float ao = 0.0f;
	float rad = radius;// / pos.z;

	//**SSAO Calculation**// int iterations = 4; 
	[unroll]
	for (int j = 0; j < 4; ++j) 
	{
		float2 coord1 = reflect(vec[j], noise.xy) * rad;
		float2 coord2 = float2(coord1.x * 0.707 - coord1.y * 0.707, coord1.x * 0.707 + coord1.y * 0.707);

		ao += doAmbientOcclusion(screenPos.xy, coord1 * 0.25, pos.xyz, normal);
		ao += doAmbientOcclusion(screenPos.xy, coord2 * 0.5, pos.xyz, normal);
		ao += doAmbientOcclusion(screenPos.xy, coord1 * 0.75, pos.xyz, normal);
		ao += doAmbientOcclusion(screenPos.xy, coord2, pos.xyz, normal);
	}

	ao /= (float)4 * 4.0;
	float occlusion = 1.0 - ao;
    output.color = saturate(pow(occlusion, 4.0f));
    return output;
}