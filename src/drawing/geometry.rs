pub struct Vertex {
    pub position: glm::Vec4,
    pub color: glm::Vec4,
}

pub struct Light {
    pub direction: glm::Vec4,
    pub color: glm::Vec4,
}

impl Default for Vertex {
    fn default() -> Vertex {
        Vertex {
            position: glm::zero(),
            color: glm::zero(),
        }
    }
}

impl Vertex {
    pub fn new(position: &glm::Vec4, color: &glm::Vec4) -> Vertex {
        Vertex {
            position: *position,
            color: *color,
        }
    }
    pub fn new_from_f32(x: f32, y: f32, z: f32, w: f32, r: f32, g: f32, b: f32, a: f32) -> Vertex {
        Vertex::new(&glm::vec4(x, y, z, w), &glm::vec4(r, g, b, a))
    }
}
