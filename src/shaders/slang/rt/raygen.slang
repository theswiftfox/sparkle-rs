// RT raygen shader — fires a primary camera ray per pixel (tutorial Phase 5.1 approach).
// Ray reconstructed from pixel UV + inv_view/inv_proj (no G-buffer reads).
// Closest-hit fires shadow sub-rays and writes visibility to outImage.

#ifndef INFINITE
static const float INFINITE = 1e32F;
#endif

struct HitPayload {
    float visibility;
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
[[vk::binding(1, 1)]] RWTexture2D<float4> outImage;

[shader("raygeneration")]
void main() {
    uint2 launchIndex = DispatchRaysIndex().xy;
    uint2 launchDim = DispatchRaysDimensions().xy;

     // Convert pixel coordinates to normalized device coordinates (NDC)
    // Range: [-1,1] for both x and y
    float2 uv = float2(launchIndex) / float2(launchDim);
    float2 ndc = float2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);

    float3 origin = mul(inv_view, float4(0.0, 0.0, 0.0, 1.0)).xyz;
    float4 viewCoords = mul(inv_proj, float4(ndc, 1.0, 1.0));
    float3 viewDir = viewCoords.xyz / viewCoords.w;
    float3 direction = normalize(mul(inv_view, float4(viewDir, 0.0)).xyz);

    // Limit printf to exactly ONE pixel thread at the center of the screen
    // if (launchIndex.x == launchDim.x / 2 && launchIndex.y == launchDim.y / 2) {
    //     printf("NDC: %f, %f | Ray Origin: %f, %f, %f | Ray Dir: %f, %f, %f\n", 
    //            ndc.x, ndc.y, 
    //            origin.x, origin.y, origin.z, 
    //            direction.x, direction.y, direction.z);
    // }

    RayDesc ray;
    ray.Origin = origin;
    ray.Direction = direction;
    ray.TMin = 0.001;
    ray.TMax = INFINITE;

    HitPayload payload;
    payload.visibility = 1.0;

    TraceRay(
        topLevelAS,
        RAY_FLAG_NONE,
        0xFF,  // instance mask
        0,     // hit group offset
        0,     // sbt record stride
        0,     // miss index (primary miss)
        ray,
        payload
    );

    outImage[launchIndex] = float4(payload.visibility, 0.0, 0.0, 0.0);
    // Temporary Raygen test right after TraceRay:
    // if (payload.visibility == 1.0) {
    //     outImage[launchIndex] = float4(0.0, 0.0, 1.0, 1.0); // Blue = Missed everything!
    // } else {
    //     outImage[launchIndex] = float4(1.0, 0.0, 0.0, 1.0); // Red = Hit geometry!
    // }
}
