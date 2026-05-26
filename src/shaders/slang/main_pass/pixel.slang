import light;
import pbr;
import shadow;

struct PS_IN {
	float4 pos : SV_Position;
	float4 worldPos : POSITION_WORLD;
	float3 normal : NORMAL;
	float2 txCoord : TEXCOORD0;
	float3x3 TBN : TBN_MATRIX;
};

struct PS_OUT {
	float4 color : SV_Target;
};

[[vk::binding(4, 0)]]
Sampler2D<float4> textures[] : register(t0);

static const uint POS_ALBEDO = 0;
static const uint POS_METALLIC_ROUGHNESS = 1;
static const uint POS_NORMALS = 2;

[[vk::binding(2, 0)]] cbuffer ubo : register(b2) {
	float3 cameraPos;
	uint ssao;
}

[shader("pixel")]
PS_OUT main(PS_IN input) {
	PS_OUT output;
	float4 alb = textures[POS_ALBEDO].Sample(input.txCoord);
	if (alb.a < 0.01) {
		discard; // discard fully transparent fragments
	}
	float2 mr = textures[POS_METALLIC_ROUGHNESS].Sample(input.txCoord).gb;
	float3 normal = textures[POS_NORMALS].Sample(input.txCoord).xyz;

	// transform to range [-1,1]
	normal = normalize((normal * 2.0) - 1.0);
	// move into world space
	float3 N = normalize(mul(normal, input.TBN));


	float ambientOcclusion = 1.0; //ssao ? ssaoTex.Sample(samplerSSAO, input.txCoord) : 1.0;

	float3 color = 0.0;
	if (light0.type != AMBIENT) {
		float3 F0 = lerp(float3(0.04, 0.04, 0.04), alb.rgb, mr.r);
		float shadowed = shadow(input.worldPos, N);
		color += BRDF(
			cameraPos,
			N,
			input.worldPos.xyz,
			alb.rgb,
			F0,
			mr.r,
			mr.g
		) * shadowed;
	} else {
		float3 ambient = 0.15 * alb.rgb * ambientOcclusion;
		color = ambient;
	}
	color = color / (color + 1.0);
	color = pow(color, 1/2.2);
	output.color = float4(color, alb.a);

	return output;
}