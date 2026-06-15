struct VS_OUT {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
    float4 col : COLOR;
};

struct VS_IN {
    [[vk::location(0)]] float2 pos;
    [[vk::location(1)]] float2 uv;
    [[vk::location(2)]] float4 col;
};

[[vk::push_constant]] cbuffer PushConstants {
    float2 scale;
    float2 translate;
};

[shader("vertex")]
VS_OUT main(VS_IN input) {
    VS_OUT output;
    output.pos = float4(input.pos * scale + translate, 0.0, 1.0);
    output.uv  = input.uv;
    output.col = input.col;
    return output;
}
