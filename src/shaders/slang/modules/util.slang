module util;

/// extract cameraPos from viewMatrix
public float3 getCameraPos(float4x4 viewMat) {
    // 1. translation vector
	float3 translation = viewMat[3].xyz;

    // 2. Extract upper 3x3 rotation elements
    float3x3 rotation = float3x3(
        viewMat[0].xyz,
        viewMat[1].xyz,
        viewMat[2].xyz
    );

    // 3. Mathematical Inversion: -Transpose(Rot) * Trans
    return -mul(transpose(rotation), translation);
}