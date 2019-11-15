
struct PX_IN {
	float4 pos : SV_Position;
	float4 posLS : POSITION_LIGHT_SPACE;
	float3 worldPos : POSITION_WORLD;
	float3 normal : NORMAL;
	float2 txCoord : TEXCOORD0;
	float2 txCoordNM : TEXCOORD1;
	float3x3 TBN : TBN_MATRIX;
};

struct PX_OUT {
	float4 color : SV_Target;
};

struct Light {
	float4 direction;
	float4 color;
};

Texture2D txDiffuse : register(t0);
SamplerState samplerLinear : register(s0);
Texture2D txMetallicRoughness : register(t1);
SamplerState samplerMR: register(s1);
Texture2D txNormal : register(t2);
SamplerState samplerNormal: register(s2);

Texture2D txShadowMap : register(t3);
SamplerState samplerShadowMap : register(s3);

cbuffer ubo : register(b0) {
	float4 cameraPos;
	Light directionalLight;
}

float3 blinn_phong(Light light, float3 view_pos, float3 world_pos, float3 normal, float3 albedo, float metallic, float shadowed) {
	float3 ld = normalize(-light.direction.xyz);
	float3 vd = normalize(view_pos - world_pos);
	float3 hwd = normalize(ld + vd);

	float spec = pow(max(dot(normal, hwd), 0.0), metallic);
	float3 specular = light.color.rgb * spec;

	float diff = max(dot(ld, normal), 0.0);
	float3 diffuse = diff * albedo;

	float3 ambient = 0.25 * albedo;
	return ambient + min((1.0 - shadowed), 1.0) * (diffuse + specular);
}

float shadow(float4 fragment, float3 normal, float3 lightDir) {
	float2 shadowTexCoords;
	shadowTexCoords.x = 0.5f + (fragment.x / fragment.w * 0.5f);
	shadowTexCoords.y = 0.5f - (fragment.y / fragment.w * 0.5f);
	float pixelDepth = fragment.z / fragment.w;

	if ((saturate(shadowTexCoords.x) == shadowTexCoords.x) &&
    	(saturate(shadowTexCoords.y) == shadowTexCoords.y) &&
    	(pixelDepth > 0)) {
			float closest = txShadowMap.Sample(samplerShadowMap, shadowTexCoords).r;
			
			float bias = max(0.05 * (1.0 - dot(normal, lightDir)), 0.005);  
			return (pixelDepth - bias) > closest ? 1.0 : 0.0;
	}
	return 0.0;
}

PX_OUT main(PX_IN input) {
	PX_OUT output;
	float4 alb = txDiffuse.Sample(samplerLinear, input.txCoord);
	if (alb.a < 0.01) {
		discard; // discard fully transparent fragments
	}
	float2 mr_tex = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
	float3 normal = txNormal.Sample(samplerNormal, input.txCoordNM).xyz;

	// transform to range [-1,1]
	normal = normalize((normal * 2.0) - 1.0);
	// move into world space
	float3 N = normalize(mul(normal, input.TBN));

	float metallic = 32.0;//mr_tex.r;
	float shadowed = shadow(input.posLS, input.normal, normalize(-directionalLight.direction.xyz));
	float3 color = blinn_phong(directionalLight, cameraPos.xyz, input.worldPos, input.normal, alb.rgb, metallic, shadowed);
	output.color = float4(color, alb.a);

	return output;
}