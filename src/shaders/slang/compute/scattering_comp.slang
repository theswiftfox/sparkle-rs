const static uint WORLD_DIMENSION = 500;

struct DrawCommand {
    uint indexCount;
    Atomic<uint> instanceCount;
    uint firstIndex;
    int  vertexOffset;
    uint firstInstance;
};

[[vk::binding(10, 0)]] RWStructuredBuffer<float4x4> instanceTransforms;

[[vk::binding(11, 0)]] RWStructuredBuffer<DrawCommand> commands;

[[vk::binding(12, 0)]] Sampler2D heightmap;

[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 threadId : SV_DispatchThreadID) {
    uint id = threadId.x;
    // 1. Establish prospective grid tracking coordinates (e.g., world space)
    float u = hash(id * 3 + 0);
    float v = hash(id * 3 + 1);

    float2 worldXZ = float2(u * WORLD_DIMENSION, v * WORLD_DIMENSION);

    float terrainHeight = heightmap.SampleLevel(float2(u, v), 0).r;

    if (!isValidForSpawn(terrainHeight)) {
        return;
    }

    uint matrixId = commands[id].instanceCount.add(1);
    
    float4x4 model = float4x4(1.0);

    // random rotation
    float rot = hash(id * 3  + 2) * 6.28318; // between 0 and 2pi

    model[3] = float4(worldXZ.x, terrainHeight * 50.0, worldXZ.y, 1.0);

    instanceTransforms[matrixId] = model;
}

// Simple hash function to generate pseudo-random numbers per thread index
float hash(uint x) {
    x = ((x >> 16) ^ x) * 0x45d9f3b;
    x = ((x >> 16) ^ x) * 0x45d9f3b;
    x = (x >> 16) ^ x;
    return float(x) / 4294967295.0;
}

bool isValidForSpawn(float terrainHeight) {
    return terrainHeight > 0.19 || terrainHeight < 0.79;
}