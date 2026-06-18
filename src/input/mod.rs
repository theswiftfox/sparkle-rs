pub mod first_person;
pub mod input_handler;
pub mod orbit;

pub trait Camera {
    fn update(&mut self, delta_t: f32);
    fn view_mat(&self) -> glm::Mat4;
    fn projection_mat(&self) -> glm::Mat4;
    fn position(&self) -> glm::Vec3;
    fn near_far(&self) -> (f32, f32);
}

/// Read-only camera snapshot sent from main thread to render thread.
/// Implements Camera trait so it can be used wherever &dyn Camera is expected.
#[derive(Clone)]
pub struct CameraSnapshot {
    pub view_matrix: glm::Mat4,
    pub projection_matrix: glm::Mat4,
    pub pos: glm::Vec3,
    pub near: f32,
    pub far: f32,
}

impl Camera for CameraSnapshot {
    fn update(&mut self, _delta_t: f32) {}
    fn view_mat(&self) -> glm::Mat4 {
        self.view_matrix
    }
    fn projection_mat(&self) -> glm::Mat4 {
        self.projection_matrix
    }
    fn position(&self) -> glm::Vec3 {
        self.pos
    }
    fn near_far(&self) -> (f32, f32) {
        (self.near, self.far)
    }
}

pub struct AppSettings {
    pub ssao: bool,
}

impl Default for AppSettings {
    fn default() -> AppSettings {
        AppSettings { ssao: true }
    }
}
