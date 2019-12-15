#include "shared_pixel.hlsli"

struct PS_IN {
	float2 uv : UV;
};

struct PS_OUT {
	float4 color : SV_Target;
};

PS_OUT main(PS_IN input) {
    PS_OUT output;
    output.color = float4(0.0,0.0,0.0,1.0);
    return output;
}