struct PS_IN {
	float4 pos : SV_Position;
	float4 worldPos : POSITION_WORLD;
	float3 normal : NORMAL;
	float2 txCoord : TEXCOORD0;
	float3x3 TBN : TBN_MATRIX;
};

struct PS_OUT {
	float4 position 		: SV_Target0;
	float4 normal_roughness	: SV_Target1;
	float4 albedo_metallic	: SV_Target2;
};

[[vk::binding(2, 0)]] cbuffer ubo : register(b2) {
	float near_plane;
    float far_plane;
};

float calcLinearDepth(float zval)
{
	float z = zval * 2.0 - 1.0;
	return (2.0 * near_plane * far_plane) / (far_plane + near_plane - z * (far_plane - near_plane));
}

[[vk::binding(5, 0)]]
Sampler2D<float4> textures[] : register(t0);

static const uint POS_ALBEDO = 0;
static const uint POS_METALLIC_ROUGHNESS = 1;
static const uint POS_NORMALS = 2;

[shader("pixel")]
PS_OUT main(PS_IN input) {
    PS_OUT output = (PS_OUT)0;

	float4 albedo = textures[POS_ALBEDO].Sample(input.txCoord);
    float4 pos = input.worldPos;

	// float3 normal_out = input.normal;
	float3 normal = textures[POS_NORMALS].Sample(input.txCoord).xyz;
	// transform to range [-1,1]
	normal = normalize((normal * 2.0) - 1.0);

    // move into world space
	float3 normal_out = normalize(mul(normal, input.TBN));
    // normal_out = normal_out * 0.5 + 0.5;

	float2 mr = textures[POS_METALLIC_ROUGHNESS].Sample(input.txCoord).gb;

	float roughness = mr.g;
	float metallic = mr.r;

	output.position = input.worldPos;
	output.normal_roughness = float4(normal_out, roughness);
	output.albedo_metallic = float4(albedo.rgb, metallic);

	return output;
}