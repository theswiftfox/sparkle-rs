struct VS_OUT {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
    float4 col : COLOR;
};

[[vk::binding(0, 0)]] Sampler2D<float4> fontTex : register(t0);

[shader("pixel")]
float4 main(VS_OUT input) : SV_Target {
    return fontTex.Sample(input.uv) * input.col;
}
