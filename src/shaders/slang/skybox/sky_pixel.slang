struct PS_IN {
    float4 pos: SV_POSITION;
    float3 texCoord: TEXCOORD;
};

[[vk::binding(0, 7)]]
SamplerCube<float4> skybox : register(t0);

[shader("pixel")]
float4 main(PS_IN input) : SV_TARGET {
    return float4(skybox.Sample(input.texCoord).rgb, 1.0);
}