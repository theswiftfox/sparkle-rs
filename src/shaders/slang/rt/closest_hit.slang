// RT closest-hit shader: fires shadow sub-rays and accumulates visibility
import light;

struct HitPayload {
    float visibility;
};

struct ShadowPayload {
    bool shadowed;
};

[[vk::push_constant]] cbuffer RtPushConstants {
    uint frame_index;
    uint width;
    uint height;
    uint number_of_lights;
};

// Set 0: bindless resources (same as raster)
[[vk::binding(0, 0)]] cbuffer FrameConsts {
    float4x4 view;
    float4x4 proj;
    float4x4 inv_view;
    float4x4 inv_proj;
};

// Set 1: RT resources (push descriptor)
[[vk::binding(0, 1)]] RaytracingAccelerationStructure topLevelAS;
[[vk::binding(2, 1)]] StructuredBuffer<Light> lights;

[shader("closesthit")]
void main(inout HitPayload payload, in BuiltInTriangleIntersectionAttributes attr) {
    // World position of the hit point
    float3 hitPos = WorldRayOrigin() + RayTCurrent() * WorldRayDirection();

    // Bias origin along the negative incoming ray direction to prevent self-intersection.
    // This avoids shadow acne / axis-aligned dark lines without needing vertex normals.
    float3 biasedOrigin = hitPos + (-WorldRayDirection()) * 0.01;

    // Loop over all lights, trace shadow rays for non-ambient ones
    float totalVisibility = 0.0;
    uint  litSamples = 0u;

    for (uint i = 0; i < number_of_lights; i++) {
        Light light = lights[i];
        if (light.type == AMBIENT) continue;

        float3 L;
        if (light.type == DIRECTIONAL) {
            L = normalize(-light.position);
        } else {
            L = normalize(light.position - hitPos);
        }

        RayDesc shadowRay;
        shadowRay.Origin = biasedOrigin;
        shadowRay.Direction = L;
        shadowRay.TMin = 0.001;
        shadowRay.TMax = (light.type == DIRECTIONAL) ? 10000.0 : light.radius;

        ShadowPayload shadowPayload;
        shadowPayload.shadowed = true;

        TraceRay(
            topLevelAS,
            RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_SKIP_CLOSEST_HIT_SHADER,
            0xFF,
            0, // hit group (unused due to SKIP_CLOSEST_HIT_SHADER)
            0,
            1, // miss index 1 -> shadow miss
            shadowRay,
            shadowPayload
        );

        totalVisibility += shadowPayload.shadowed ? 0.0 : 1.0;
        litSamples++;
    }

    payload.visibility = (litSamples > 0u) ? (totalVisibility / float(litSamples)) : 1.0;
}
