import push_constants;

struct VS_IN {
	float3 pos			: POSITION;
};

struct VS_OUT {
	float4 pos			: SV_Position;
};

[[vk::binding(3, 0)]] cbuffer FrameConsts : register(b0) {
	float4x4 lightSpaceMatrix;
};

[[vk::binding(10, 0)]] StructuredBuffer<float4x4> instanceTransforms;

[shader("vertex")]
VS_OUT main(VS_IN input, uint instanceID : SV_InstanceID) {
	VS_OUT output;
	float4x4 M = is_instanced != 0u ? instanceTransforms[instanceID] : model;
	float4 worldPos = mul(M, float4(input.pos, 1.0));
	output.pos = mul(lightSpaceMatrix, worldPos);

	return output;
}
