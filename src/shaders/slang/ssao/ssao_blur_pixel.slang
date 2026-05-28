struct VS_OUT {
	float4 pos		: SV_Position;
	float2 uv		: TEXCOORD0;
};

[[vk::binding(5, 0)]]
Sampler2D<float4> ssaoTexture : register(t5);

[shader("fragment")]
float4 main(VS_OUT input) : SV_Target {
	float2 uv = input.uv;
	float4 ssao = ssaoTexture.Sample(uv);
	return ssao;
}
