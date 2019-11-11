use crate::drawing::geometry::Vertex;

pub fn cube() -> (Vec<Vertex>, Vec<u32>) {
    let mut verts: Vec<Vertex> = Vec::new();

    // face z-neg
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, -1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, -1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, 1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 1.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, 1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 1.0f32)
    });

    // face z-pos
        verts.push(Vertex {
        position: glm::vec3(1.0f32, -1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, -1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, 1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 1.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, 1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 1.0f32)
    });

    // face x-neg
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, -1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, 1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, 1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 1.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, -1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 0.0f32)
    });

    // face x-pos
    verts.push(Vertex {
        position: glm::vec3(1.0f32, 1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 1.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, -1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, -1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, 1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 1.0f32)
    });

    // face y-neg
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, -1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 1.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, -1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 1.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, 01.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, 01.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 0.0f32)
    });

    // face y-pos
    verts.push(Vertex {
        position: glm::vec3(1.0f32, 1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, 1.0f32, 1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 0.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(-1.0f32, 1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(0.0f32, 1.0f32)
    });
    verts.push(Vertex {
        position: glm::vec3(1.0f32, 1.0f32, -1.0f32),
        normal: glm::zero(),
        tex_coord: glm::vec2(1.0f32, 1.0f32)
    });

    let indices: [u32; 36] = [
        0, 2, 1, 0, 3, 2, 4, 6, 5, 4, 7, 6, 8, 10, 9, 8, 11, 10, 12, 14, 13, 12, 15, 14, 16, 18,
        17, 16, 19, 18, 20, 22, 21, 20, 23, 22,
    ];
    (verts, indices.to_vec())
}
