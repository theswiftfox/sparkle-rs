
struct VS_IN {
	float4 pos		: SV_Position;
	float4 color	: COLOR0;
};

struct VS_OUT {
	float4 pos		: SV_Position;
	float4 color	: COLOR0;
};

VS_OUT main(VS_IN input) {
	return input;
}