// Primary miss shader: ray escaped the scene (sky), scene is fully lit
struct HitPayload {
    float visibility;
};

[shader("miss")]
void main(inout HitPayload payload) {
    payload.visibility = 1.0;
}
