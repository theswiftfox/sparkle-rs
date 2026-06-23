import color_utils;
import push_constants;
import parallax;

struct PS_IN
{
    float4 pos : SV_Position;
    float4 worldPos : POSITION_WORLD;
    float3 worldView : VIEW_WORLD;
    float3 normal : NORMAL;
    float2 txCoord : TEXCOORD0;
    float3x3 TBN : TBN_MATRIX;
};

struct PS_OUT
{
    float4 position : SV_Target0;
    float4 normal_roughness : SV_Target1;
    float4 albedo_metallic : SV_Target2;
};

[[vk::binding(5, 0)]]
cbuffer ubo : register(b2)
{
    float near_plane;
    float far_plane;
};

float calcLinearDepth(float zval)
{
    float z = zval * 2.0 - 1.0;
    return (2.0 * near_plane * far_plane) / (far_plane + near_plane - z * (far_plane - near_plane));
}

[[vk::binding(6, 0)]]
Sampler2D<float4> textures[] : register(t0);

[vk::constant_id(1)]
float gamma = 2.2;

[shader("pixel")]
PS_OUT main(PS_IN input)
{
    PS_OUT output = (PS_OUT)0;

    float2 dx = ddx(input.txCoord);
    float2 dy = ddy(input.txCoord);

    float3 tView = normalize(mul(input.TBN, input.worldView));
    // flip vector to point INTO the floor surface, and match glTF UV orientation
    float3 tangentView;
    tangentView.x = tView.x;      // Keep X native
    tangentView.y = tView.y;      // Flip Y to cancel forward/backward drift
    tangentView.z = abs(tView.z); // Force Z positive so ray-marching always steps downward into depth layers

    float2 offsetTxCoords = input.txCoord;
    if (has_parallax > 0)
    {
        offsetTxCoords = parallaxMapping(textures[tex2], input.txCoord, tangentView, dx, dy);
    }

    float2 sharpDx = dx * 0.4;
    float2 sharpDy = dy * 0.4;

    float4 albedo = textures[tex0].SampleGrad(offsetTxCoords, sharpDx, sharpDy);
    albedo.rgb = srgbToLinear(albedo.rgb, gamma);
    float4 pos = input.worldPos;

    float3 normal = textures[tex2].SampleGrad(offsetTxCoords, sharpDx, sharpDy).xyz;
    // normal.xy = normal.xy * 1.5; // increase depth illusion
    normal = normalize((normal * 2.0) - 1.0);

    float3 normal_out = normalize(mul(normal, input.TBN));

    float2 mr = textures[tex1].SampleGrad(offsetTxCoords, sharpDx, sharpDy).gb;

    float roughness = mr.g;
    float metallic = mr.r;

    output.position = input.worldPos;
    output.normal_roughness = float4(normal_out, roughness);
    output.albedo_metallic = float4(albedo.rgb, metallic);

    return output;
}
