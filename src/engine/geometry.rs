pub struct Vertex {
    pub position: glm::Vec3,
    pub normal: glm::Vec3,
    pub tangent: glm::Vec3,
    pub bitangent: glm::Vec3,
    pub tex_coord: glm::Vec2,
    // pub tex_coord_normalmap: glm::Vec2,
}

#[derive(Clone, PartialEq, Eq)]
pub enum LightType {
    Ambient,
    Directional,
    Area,
}

#[derive(Clone)]
pub struct Light {
    pub position: glm::Vec3,
    pub t: LightType,
    pub color: glm::Vec3,
    pub radius: f32,
    pub light_proj: glm::Mat4,
}

impl Default for Light {
    fn default() -> Light {
        Light {
            position: glm::zero(),
            t: LightType::Ambient,
            color: glm::zero(),
            radius: 0.0,
            light_proj: glm::identity(),
        }
    }
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
