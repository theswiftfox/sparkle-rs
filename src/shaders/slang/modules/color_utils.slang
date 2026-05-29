module color_utils;

static const float LOW_CONST = 12.92;

public float3 srgbToLinear(float3 color, float gamma) {
    float3 low = color / LOW_CONST;
    float3 high = pow((color + 0.055) / 1.055, gamma);
    float3 isHigh = step(0.0404482362771082, color);
    return lerp(low, high, isHigh);
}

public float3 linearToSrgb(float3 linear, float gamma) {
    bool3 cutoff = linear <= float3(0.0031308);
    float3 high = 1.055 * pow(linear, float3(1.0 / gamma)) - 0.055;
    float3 low = linear *LOW_CONST;
    return lerp(high, low, float3(cutoff));
}

// aces tonemappig
public float3 tonemappingAces(float3 color) {
    return clamp((color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14), 0.0, 1.0);
}

public float3 liearToHdr10(float3 color, float white_point) {
    // Convert Rec.709 to Rec.2020 color space to broaden the palette
    // source: https://panoskarabelas.com/blog/posts/hdr_in_under_10_minutes/
    static const float3x3 from709to2020 =
    {
        { 0.6274040f, 0.3292820f, 0.0433136f },
        { 0.0690970f, 0.9195400f, 0.0113612f },
        { 0.0163916f, 0.0880132f, 0.8955950f }
    };

    float3 mapped = mul(from709to2020, color);

    // normalize ([0..>1] to [0..1]
    const float st2084_max = 10000.0f;
    mapped = mapped * white_point / st2084_max;

    // ST.2084 (PQ curve) for HDR10
    static const float m1 = 2610.0f / 4096.0 / 4.0;
    static const float m2 = 2523.0f / 4096.0 * 128.0;
    static const float c1 = 3424.0f / 4096.0;
    static const float c2 = 2413.0f / 4096.0 * 32.0;
    static const float c3 = 2392.0f / 4096.0 * 32.0;

    float3 cp = pow(abs(mapped), m1);
    mapped = pow((c1 + c2 * cp) / (1 + c3 * cp), m2);

    return mapped;
}