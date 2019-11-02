
struct PX_IN {
	float4 pos : SV_Position;
	float3 normal : NORMAL0;
	float2 txCoord : TEXCOORD0;
};

struct PX_OUT {
	float4 color : SV_Target;
};

Texture2D txDiffuse : register(t0);
SamplerState samplerLinear : register(s0);
Texture2D txMetallicRoughness : register(t1);
SamplerState samplerMR: register(s1);
Texture2D txNormal : register(t2);
SamplerState samplerNormal: register(s2);

PX_OUT main(PX_IN input) {
	PX_OUT output;
	output.color = txDiffuse.Sample(samplerLinear, input.txCoord);
	float2 mr_tex = txMetallicRoughness.Sample(samplerMR, input.txCoord).gb;
	float3 normal = txNormal.Sample(samplerNormal, input.txCoord).xyz;
	return output;
}