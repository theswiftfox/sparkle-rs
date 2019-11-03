pub struct Vertex {
    pub position: glm::Vec3,
    pub normal: glm::Vec3,
    pub tangent: glm::Vec3,
    pub bitangent: glm::Vec3,
    pub tex_coord: glm::Vec2,
    pub tex_coord_normalmap: glm::Vec2,
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
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
            tex_coord_normalmap: glm::zero(),
        }
    }
}