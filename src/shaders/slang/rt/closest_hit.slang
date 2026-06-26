// RT closest-hit shader: fires shadow sub-rays and accumulates visibility
import light;
import shadow_utils;

struct HitPayload {
    float visibility;
};

struct ShadowPayload {
    bool shadowed;
};

[[vk::push_constant]] cbuffer RtPushConstants {
    uint frameIndex;
    uint width;
    uint height;
    uint numberOfLights;
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

[vk::constant_id(0)] uint shadowSamples = 8; // overridable specialization constant to change shadow_samples via settings.

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

    float fi = float(frameIndex);
    float3 frameSeed = hitPos + float3(fi * 0.1, fi * 0.2, fi * 0.3);

    // One random rotation angle per pixel per frame — rotates the whole disk uniformly
    float rotAngle = random(frameSeed, 0) * 6.28318;
    float sinA = sin(rotAngle);
    float cosA = cos(rotAngle);

    for (uint i = 0; i < numberOfLights; i++) {
        Light light = lights[i];
        if (light.type == AMBIENT) continue;

        float3 L;
        if (light.type == DIRECTIONAL) {
            L = normalize(-light.position);

            // Build a tangent frame around L for cone jitter
            float3 right = (abs(L.y) < 0.999) ? float3(1, 0, 0) : float3(0, 1, 0);
            float3 up2   = normalize(cross(L, right));
            float3 right2 = cross(up2, L);

            float spread = light.radius * 0.01;
            float lightVisibility = 0.0;

            for (int s = 0; s < shadowSamples; s++) {
                float2 disk = poissonDisk[(s * (64 / shadowSamples)) % 64];
                float2 rotated = float2(
                    cosA * disk.x - sinA * disk.y,
                    sinA * disk.x + cosA * disk.y
                );
                float3 jitteredL = normalize(L + (rotated.x * right2 + rotated.y * up2) * spread);

                // Trace shadow ray toward this jittered sample
                RayDesc shadowRay;
                shadowRay.Origin = biasedOrigin;
                shadowRay.Direction = jitteredL;
                shadowRay.TMin = 0.001;
                shadowRay.TMax = 10000.0;

                ShadowPayload sp;
                sp.shadowed = true;

                TraceRay(
                    topLevelAS,
                    RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_SKIP_CLOSEST_HIT_SHADER,
                    0xFF,
                    0, // hit group (unused due to SKIP_CLOSEST_HIT_SHADER)
                    0,
                    1, // miss index 1 -> shadow miss
                    shadowRay,
                    sp,
                );
                lightVisibility += sp.shadowed ? 0.0 : 1.0;
            }
            totalVisibility += lightVisibility / float(shadowSamples);
            litSamples++;
        } else if (light.type == AREA) {
            // base dir towards light
            float3 toLight = light.position - hitPos;
            float dist = length(toLight); // actual metric distance
            L = toLight / dist;           // manual normalize

            // Build a local frame around L to spread samples on a disk
            float3 up = abs(L.y) < 0.999 ? float3(0,1,0) : float3(1,0,0);
            float3 tangent   = normalize(cross(up, L));
            float3 bitangent = cross(L, tangent);

            float softRadius = light.radius / max(dist, 0.001);

            float lightVisibility = 0.0;
            for (int s = 0; s < shadowSamples; s++) {
                float2 disk = poissonDisk[(s * (64 / shadowSamples)) % 64];
                float2 rotated = float2(
                    cosA * disk.x - sinA * disk.y,
                    sinA * disk.x + cosA * disk.y
                );

                float3 sampleOffset = (rotated.x * tangent + rotated.y * bitangent) * softRadius;
                float3 samplePos = light.position + sampleOffset;
                float3 sampleL   = normalize(samplePos - hitPos);

                // Trace shadow ray toward this jittered sample
                RayDesc shadowRay;
                shadowRay.Origin = biasedOrigin;
                shadowRay.Direction = sampleL;
                shadowRay.TMin = 0.001;
                shadowRay.TMax = light.radius;

                ShadowPayload sp;
                sp.shadowed = true;

                TraceRay(
                    topLevelAS,
                    RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_SKIP_CLOSEST_HIT_SHADER,
                    0xFF,
                    0, // hit group (unused due to SKIP_CLOSEST_HIT_SHADER)
                    0,
                    1, // miss index 1 -> shadow miss
                    shadowRay,
                    sp,
                );
                lightVisibility += sp.shadowed ? 0.0 : 1.0;
            }
            totalVisibility += lightVisibility / float(shadowSamples);
            litSamples++;
        }
    }

    payload.visibility = (litSamples > 0u) ? (totalVisibility / float(litSamples)) : 1.0;
}
