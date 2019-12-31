
Texture2D<float4> deferred : register(t0);
Texture2D<float4> forward : register(t1);

float4 main(float4 pos : SV_Position) : SV_Target {
    int3 coord = int3(pos.xy, 0);
    float4 def = deferred.Load(coord);
    float4 fwd = forward.Load(coord);

    float3 col = fwd.rgb * fwd.a + def.rgb * (1.0 - fwd.a);
    return float4(col, 1.0);
}