pub mod first_person;
pub mod input_handler;

use cgmath::Matrix4;

trait Camera {
    fn update(&mut self, delta_t: f32);
    fn view_mat(&self) -> Matrix4<f32>;
    fn projection_mat(&self) -> Matrix4<f32>;
}