pub struct Vertex {
    pub position: glm::Vec3,
    pub normal: glm::Vec3,
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
            normal: glm::zero(),
            tex_coord: glm::zero(),
        }
    }
}