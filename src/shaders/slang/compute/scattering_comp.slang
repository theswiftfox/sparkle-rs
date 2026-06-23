[vk::constant_id(0)]
float worldDimension = 500.0;

struct DrawCommand
{
    uint indexCount;
    Atomic<uint> instanceCount;
    uint firstIndex;
    int vertexOffset;
    uint firstInstance;
};

[[vk::push_constant]]
cbuffer ComputePushConstants
{
    uint maxInstances;
    uint assetOffset;
    float maxHeight;
    float spawnHeightMin;
    float spawnHeightMax;
    float slopeMax; // 0 = flat only, 1 = any slope (dot with up < 1-slopeMax)
    float scaleMin;
    float scaleMax;
    float tiltFactor;       // 0 = objects stand upright, 1 = fully align to terrain normal
    float terrainSegmentsF; // terrain_segments cast to float, for texel size computation
};

[[vk::binding(10, 0)]]
RWStructuredBuffer<float4x4> instanceTransforms;

[[vk::binding(11, 0)]]
RWStructuredBuffer<DrawCommand> commands;

[[vk::binding(12, 0)]]
Sampler2D heightmap;

// ---------------------------------------------------------------------------
// PCG hash
// Returns [0, 1].
// ---------------------------------------------------------------------------
uint pcg(uint v)
{
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

float hash(uint x)
{
    return float(pcg(x)) / 4294967295.0;
}

// ---------------------------------------------------------------------------
// isValidForSpawn — height band + slope gate.
// normal.y == 1 means perfectly flat; lower values mean steeper slope.
// slopeMax = 0 → only flat surfaces; slopeMax = 1 → all slopes accepted.
// ---------------------------------------------------------------------------
bool isValidForSpawn(float terrainHeight, float normalY)
{
    if (terrainHeight < spawnHeightMin || terrainHeight > spawnHeightMax)
        return false;
    float slope = 1.0 - normalY; // 0=flat, 1=vertical
    return slope <= slopeMax;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 threadId: SV_DispatchThreadID)
{
    uint id = threadId.x + assetOffset;

    // -----------------------------------------------------------------------
    // Two-level clustered placement.
    //    CLUSTER_SIZE threads share a cluster centre; each adds a small
    //    local jitter so objects clump naturally rather than scattering
    //    uniformly across the world.
    // -----------------------------------------------------------------------
    static const uint CLUSTER_SIZE = 8u;
    static const float CLUSTER_RADIUS = 0.08; // UV space (~8 % of world extent)

    uint clusterId = id / CLUSTER_SIZE;
    // Cluster centre — use a separate hash domain so cluster IDs and local
    // IDs don't alias.
    float cu = hash(clusterId * 1031u + 7u);
    float cv = hash(clusterId * 1031u + 13u);

    // Local jitter within the cluster.
    float lu = (hash(id * 7u + 3u) - 0.5) * CLUSTER_RADIUS;
    float lv = (hash(id * 7u + 4u) - 0.5) * CLUSTER_RADIUS;

    float u = saturate(cu + lu);
    float v = saturate(cv + lv);

    // -----------------------------------------------------------------------
    // Sample height and compute surface normal from finite differences.
    //    Uses 4 neighbours (central differences) on the heightmap.
    // -----------------------------------------------------------------------
    float texelSize = 1.0 / terrainSegmentsF;

    float hC = heightmap.SampleLevel(float2(u, v), 0).r;
    float hL = heightmap.SampleLevel(float2(u - texelSize, v), 0).r;
    float hR = heightmap.SampleLevel(float2(u + texelSize, v), 0).r;
    float hD = heightmap.SampleLevel(float2(u, v - texelSize), 0).r;
    float hU = heightmap.SampleLevel(float2(u, v + texelSize), 0).r;

    // Reconstruct normal in world space.  The height gradient is scaled by
    // maxHeight; the horizontal step is (texelSize * worldDimension).
    float invStep = 1.0 / (texelSize * worldDimension);
    float3 surfNormal = normalize(float3(
        (hL - hR) * maxHeight * invStep,
        2.0,
        (hD - hU) * maxHeight * invStep));

    if (!isValidForSpawn(hC, surfNormal.y))
    {
        return;
    }

    // -----------------------------------------------------------------------
    // CAS loop: claim an output slot atomically.
    // -----------------------------------------------------------------------
    uint slot;
    while (true)
    {
        slot = commands[0].instanceCount.load();
        if (slot >= maxInstances)
            return;
        uint original = commands[0].instanceCount.compareExchange(slot, slot + 1);
        if (original == slot)
            break;
    }

    // -----------------------------------------------------------------------
    // Per-instance scale variation.
    // -----------------------------------------------------------------------
    float scale = scaleMin + hash(id * 7u + 5u) * (scaleMax - scaleMin);

    // -----------------------------------------------------------------------
    // Slope-aligned rotation.
    //    Lerp between world-up and the surface normal by tiltFactor, then
    //    build an orthonormal frame (Gram-Schmidt) around a random yaw.
    // -----------------------------------------------------------------------
    float rot = hash(id * 3u + 2u) * 6.28318; // random yaw [0, 2π]
    float sinR = sin(rot);
    float cosR = cos(rot);

    float3 upAxis = normalize(lerp(float3(0.0, 1.0, 0.0), surfNormal, tiltFactor));

    // Pick a yaw-aligned forward direction, then orthogonalise against upAxis.
    float3 fwdRaw = float3(sinR, 0.0, cosR);
    float3 right = normalize(cross(fwdRaw, upAxis));
    float3 forward = normalize(cross(upAxis, right));

    // World position.
    float tx = u * worldDimension - worldDimension * 0.5;
    float ty = hC * maxHeight;
    float tz = v * worldDimension - worldDimension * 0.5;

    // Build TRS matrix (scale then rotate then translate).
    // HLSL float4x4 constructor fills row by row.
    // Column 3 (last column) carries the translation (mul(M, v) convention).
    float4x4 model = float4x4(
        right.x * scale, upAxis.x * scale, forward.x * scale, tx,
        right.y * scale, upAxis.y * scale, forward.y * scale, ty,
        right.z * scale, upAxis.z * scale, forward.z * scale, tz,
        0.0, 0.0, 0.0, 1.0);

    instanceTransforms[slot] = model;
}
