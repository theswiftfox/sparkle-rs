// Shadow miss shader: shadow ray missed all geometry, so the surface is lit
struct ShadowPayload {
    bool shadowed;
};

[shader("miss")]
void main(inout ShadowPayload payload) {
    payload.shadowed = false;
}
