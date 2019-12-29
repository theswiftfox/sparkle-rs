pub struct Vertex {
    pub position: glm::Vec3,
    pub normal: glm::Vec3,
    pub tangent: glm::Vec3,
    pub bitangent: glm::Vec3,
    pub tex_coord: glm::Vec2,
   // pub tex_coord_normalmap: glm::Vec2,
}

#[derive(Clone)]
pub struct Light {
    pub position: glm::Vec3,
    pub t: u32,
    pub color: glm::Vec3,
    pub radius: f32,
}

impl Default for Vertex {
    fn default() -> Vertex {
        Vertex {
            position: glm::zero(),
            normal: glm::zero(),
            tangent: glm::zero(),
            bitangent: glm::zero(),
            tex_coord: glm::zero(),
      //      tex_coord_normalmap: glm::zero(),
        }
    }
}
