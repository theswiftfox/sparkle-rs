pub struct Vertex {
    pub position: glm::Vec4,
    pub tex_coord: glm::Vec2,
}

pub struct Light {
    pub direction: glm::Vec4,
    pub color: glm::Vec4,
}

impl Default for Vertex {
    fn default() -> Vertex {
        Vertex {
            position: glm::zero(),
            tex_coord: glm::zero(),
        }
    }
}

impl Vertex {
    pub fn new(position: &glm::Vec4, tex_coord: &glm::Vec2) -> Vertex {
        Vertex {
            position: *position,
            tex_coord: *tex_coord,
        }
    }
    pub fn new_from_f32(x: f32, y: f32, z: f32, w: f32, u: f32, v: f32) -> Vertex {
        Vertex::new(&glm::vec4(x, y, z, w), &glm::vec2(u, v))
    }
}
