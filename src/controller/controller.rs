use crate::controller::Camera;
use cgmath::EuclideanSpace;
use cgmath::{num_traits::One, vec3, Matrix4, Point3, Vector3};

pub struct FPSController {
    pos: Vector3<f32>,
    h_angle: f32,
    v_angle: f32,
    view_mat: Matrix4<f32>,
    projection_mat: Matrix4<f32>,
}

impl Camera for FPSController {
    fn update(&mut self, _delta_t: f32) {
        let dir = vec3(
            (-self.v_angle).cos() * (-self.h_angle).sin(),
            (-self.v_angle).sin(),
            (-self.v_angle).cos() * (-self.h_angle).cos(),
        );
        let center = self.pos + dir;
        self.view_mat = Matrix4::look_at(
            Point3::from_vec(self.pos),
            Point3::from_vec(center),
            Vector3::unit_y(),
        );
    }
    fn view_mat(&self) -> Matrix4<f32> {
        self.view_mat
    }
    fn projection_mat(&self) -> Matrix4<f32> {
        self.projection_mat
    }
}

impl FPSController {
    pub fn create(aspect: f32, fov: f32, near: f32, far: f32) -> FPSController {
        let proj = cgmath::perspective(cgmath::Rad::from(cgmath::Deg(fov)), aspect, near, far);
        FPSController {
            pos: vec3(0.0f32, 0.0f32, 0.0f32),
            h_angle: std::f32::consts::FRAC_PI_4,
            v_angle: 0.0f32,
            view_mat: Matrix4::one(),
            projection_mat: proj,
        }
    }
    pub fn create_ptr(
        aspect: f32,
        fov: f32,
        near: f32,
        far: f32,
    ) -> std::rc::Rc<std::cell::RefCell<FPSController>> {
        std::rc::Rc::new(std::cell::RefCell::from(FPSController::create(
            aspect, fov, near, far,
        )))
    }
}
