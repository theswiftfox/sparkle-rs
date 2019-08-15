
struct PX_IN {
	float4 pos : SV_Position;
	float4 color : COLOR0;
};

struct PX_OUT {
	float4 color : SV_Target;
};

PX_OUT main(PX_IN input) {
	PX_OUT output;
	output.color = input.color;
	return output;
}