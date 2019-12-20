pub mod first_person;
pub mod input_handler;

pub trait Camera {
    fn update(&mut self, delta_t: f32);
    fn view_mat(&self) -> glm::Mat4;
    fn projection_mat(&self) -> glm::Mat4;
    fn position(&self) -> glm::Vec3;
    fn near_far(&self) -> (f32, f32);
}
