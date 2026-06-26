// RT any-hit shader: alpha-cutout transparency for foliage and other masked surfaces.
//
// Fired for every potential intersection with a non-OPAQUE BLAS (transparent geometry).
// Reads the albedo alpha of the hit texel via barycentric UV interpolation and calls
// IgnoreHit() if the alpha is below the cutout threshold.
//
// Per-instance material data (InstanceMaterial, 24 bytes = 6 × uint32):
//   uint  albedo_tex_idx     - bindless index into the global texture array (u32::MAX = none)
//   uint  _pad
//   uint  vertex_addr_lo     - low  32 bits of vertex buffer device address
//   uint  vertex_addr_hi     - high 32 bits of vertex buffer device address
//   uint  index_addr_lo      - low  32 bits of index buffer device address
//   uint  index_addr_hi      - high 32 bits of index buffer device address
//
// Vertex layout (matches geometry.rs Vertex, stride = 56 bytes):
//   float3 position  @  0 bytes
//   float3 normal    @ 12 bytes
//   float3 tangent   @ 24 bytes
//   float3 bitangent @ 36 bytes
//   float2 tex_coord @ 48 bytes

static const uint  VERTEX_STRIDE_BYTES = 56u;
static const uint  UV_BYTE_OFFSET      = 48u;
static const float ALPHA_THRESHOLD     = 0.5;

struct ShadowPayload {
    bool shadowed;
};

struct InstanceMaterial {
    uint albedo_tex_idx;
    uint _pad;
    uint vertex_addr_lo;
    uint vertex_addr_hi;
    uint index_addr_lo;
    uint index_addr_hi;
};

// Set 0, binding 6: global bindless sampled-texture array (same as raster passes).
[[vk::binding(6, 0)]] Sampler2D bindlessTextures[];

// Set 1, binding 3: per-instance material data (one entry per TLAS instance).
[[vk::binding(3, 1)]] StructuredBuffer<InstanceMaterial> instanceMaterials;

// Load a uint from a raw device address using Slang Ptr<T>.
uint loadUint(uint64_t addr)
{
    Ptr<uint> p = reinterpret<Ptr<uint>>(addr);
    return p[0];
}

// Load a float from a raw device address using Slang Ptr<T>.
float loadFloat(uint64_t addr)
{
    Ptr<float> p = reinterpret<Ptr<float>>(addr);
    return p[0];
}

float2 loadUV(uint64_t vertexBase, uint vertexIndex)
{
    uint64_t uvAddr = vertexBase + uint64_t(vertexIndex) * VERTEX_STRIDE_BYTES + UV_BYTE_OFFSET;
    float u = loadFloat(uvAddr);
    float v = loadFloat(uvAddr + 4);
    return float2(u, v);
}

[shader("anyhit")]
void main(inout ShadowPayload payload, in BuiltInTriangleIntersectionAttributes attr)
{
    uint instanceIdx = InstanceIndex();
    InstanceMaterial mat = instanceMaterials[instanceIdx];

    // Sentinel: no albedo texture — treat as fully opaque.
    if (mat.albedo_tex_idx == ~0u) {
        return;
    }

    // Sentinel: device addresses not available — treat as opaque.
    uint64_t vertexAddr = (uint64_t(mat.vertex_addr_hi) << 32) | uint64_t(mat.vertex_addr_lo);
    uint64_t indexAddr  = (uint64_t(mat.index_addr_hi)  << 32) | uint64_t(mat.index_addr_lo);
    if (vertexAddr == 0 || indexAddr == 0) {
        return;
    }

    // Read the three vertex indices of the hit triangle.
    uint primitiveIdx = PrimitiveIndex();
    uint baseByteOffset = primitiveIdx * 3u * 4u; // 3 indices × 4 bytes each

    uint i0 = loadUint(indexAddr + baseByteOffset);
    uint i1 = loadUint(indexAddr + baseByteOffset + 4);
    uint i2 = loadUint(indexAddr + baseByteOffset + 8);

    // Fetch UVs from vertex buffer.
    float2 uv0 = loadUV(vertexAddr, i0);
    float2 uv1 = loadUV(vertexAddr, i1);
    float2 uv2 = loadUV(vertexAddr, i2);

    // Barycentric interpolation: barycentrics.xy = (b1, b2); b0 = 1 - b1 - b2.
    float2 bary = attr.barycentrics;
    float2 uv = uv0 * (1.0 - bary.x - bary.y) + uv1 * bary.x + uv2 * bary.y;

    // Sample albedo alpha. LOD 0 — no ray differentials available in any-hit.
    float alpha = bindlessTextures[mat.albedo_tex_idx].SampleLevel(uv, 0).a;

    if (alpha < ALPHA_THRESHOLD) {
        // Texel is masked out: let the ray pass through.
        IgnoreHit();
    }
    // Otherwise: fall through — intersection accepted, shadow ray is occluded.
}
