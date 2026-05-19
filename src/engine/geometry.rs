#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: glm::Vec3,
    pub normal: glm::Vec3,
    pub tangent: glm::Vec3,
    pub bitangent: glm::Vec3,
    pub tex_coord: glm::Vec2,
}

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug)]
pub struct AABB {
    pub min: glm::Vec3,
    pub max: glm::Vec3,
}

impl AABB {
    /// An empty (inverted) AABB that will expand to fit any point.
    pub fn empty() -> Self {
        AABB {
            min: glm::vec3(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            max: glm::vec3(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }

    /// Expand this AABB to include the given point.
    pub fn expand_point(&mut self, p: &glm::Vec3) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.min.z = self.min.z.min(p.z);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
        self.max.z = self.max.z.max(p.z);
    }

    /// Merge another AABB into this one (union).
    pub fn merge(&mut self, other: &AABB) {
        self.expand_point(&other.min);
        self.expand_point(&other.max);
    }

    /// Compute from a slice of vertices.
    pub fn from_vertices(vertices: &[Vertex]) -> Self {
        let mut aabb = AABB::empty();
        for v in vertices {
            aabb.expand_point(&v.position);
        }
        aabb
    }

    /// Returns true if this AABB was never expanded (still inverted/empty).
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }

    /// Transform this AABB by a 4x4 matrix, returning a new world-space AABB.
    ///
    /// Transforms all 8 corners and computes a new axis-aligned box around them.
    pub fn transformed(&self, mat: &glm::Mat4) -> AABB {
        if self.is_empty() {
            return *self;
        }
        let corners = [
            glm::vec3(self.min.x, self.min.y, self.min.z),
            glm::vec3(self.max.x, self.min.y, self.min.z),
            glm::vec3(self.min.x, self.max.y, self.min.z),
            glm::vec3(self.max.x, self.max.y, self.min.z),
            glm::vec3(self.min.x, self.min.y, self.max.z),
            glm::vec3(self.max.x, self.min.y, self.max.z),
            glm::vec3(self.min.x, self.max.y, self.max.z),
            glm::vec3(self.max.x, self.max.y, self.max.z),
        ];
        let mut result = AABB::empty();
        for c in &corners {
            let p4 = mat * glm::vec4(c.x, c.y, c.z, 1.0);
            result.expand_point(&glm::vec3(p4.x, p4.y, p4.z));
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LightType {
    Ambient,
    Directional,
    Area,
}

#[derive(Clone, Debug)]
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
        }
    }
}
