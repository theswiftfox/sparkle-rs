struct PS_IN {
    float4 pos: SV_POSITION;
    float3 texCoord: TEXCOORD;
};

TextureCube skybox : register(t0);
SamplerState skyboxSampler: register(s0);

float4 main(PS_IN input) : SV_TARGET {
    return float4(skybox.Sample(skyboxSampler, input.texCoord).rgb, 1.0);
}