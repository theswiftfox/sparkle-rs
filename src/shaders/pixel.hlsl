
struct PX_IN {
	float4 pos : SV_Position;
	float2 txCoord : TEXCOORD0;
};

struct PX_OUT {
	float4 color : SV_Target;
};

Texture2D txDiffuse : register(t0);
SamplerState samplerLinear : register(s0);

PX_OUT main(PX_IN input) {
	PX_OUT output;
	output.color = txDiffuse.Sample(samplerLinear, input.txCoord);
	return output;
}