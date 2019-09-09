use crate::input::input_handler::{Action, ApplicationRequest, Button, InputHandler, Key, ScrollAxis};
use crate::input::Camera;
use cgmath::EuclideanSpace;
use cgmath::{num_traits::One, vec3, Matrix4, Point3, Vector3};
use std::collections::HashMap;

type KeyCallback = fn(&mut FPSController, Action) -> Option<ApplicationRequest>;
type MouseButtonCallback = fn(&mut FPSController, Action) -> ();

pub struct FPSController {
    pos: Vector3<f32>,
    h_angle: f32,
    v_angle: f32,
    view_mat: Matrix4<f32>,
    projection_mat: Matrix4<f32>,

    keybinds: HashMap<Key, KeyCallback>,
    mousebinds: HashMap<Button, MouseButtonCallback>,

    move_f: bool,
    move_b: bool,
    move_l: bool,
    move_r: bool,
    aiming: bool,
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

impl InputHandler for FPSController {
    fn update(&mut self, delta_t: f32) {}
    fn handle_key(&mut self, key: Key, action: Action) -> ApplicationRequest {
        match self.keybinds.get(&key) {
            Some(func) => match func(self, action) {
                Some(r) => r,
                None => ApplicationRequest::Nothing,
            },
            None => ApplicationRequest::Nothing,
        }
        
    }
    fn handle_mouse(&mut self, button: Button, action: Action) {}
    fn handle_wheel(&mut self, axis: ScrollAxis, value: f32) {}
    fn handle_mouse_move(&mut self, x: i32, y: i32) {}
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
            keybinds: FPSController::default_keybinds(),
            mousebinds: FPSController::default_mousebinds(),
            aiming: true,
            move_b: false,
            move_f: false,
            move_l: false,
            move_r: false,
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
    fn movement_front(&mut self, action: Action) -> Option<ApplicationRequest> {
        self.move_f = match action {
            Action::Up => false,
            Action::Down => true,
        };
        None
    }
    fn movement_back(&mut self, action: Action) -> Option<ApplicationRequest> {
        self.move_b = match action {
            Action::Up => false,
            Action::Down => true,
        };
        None
    }
    fn movement_left(&mut self, action: Action) -> Option<ApplicationRequest> {
        self.move_l = match action {
            Action::Up => false,
            Action::Down => true,
        };
        None
    }
    fn movement_right(&mut self, action: Action) -> Option<ApplicationRequest> {
        self.move_r = match action {
            Action::Up => false,
            Action::Down => true,
        };
        None
    }
    fn request_quit(&mut self, action: Action) -> Option<ApplicationRequest> {
        match action {
            Action::Up => Some(ApplicationRequest::Quit),
            Action::Down => None,
        }
    }

    fn default_keybinds() -> HashMap<Key, KeyCallback> {
        let mut keybinds : HashMap<Key, KeyCallback> = HashMap::new();
        keybinds.insert(Key::W, FPSController::movement_front);
        keybinds.insert(Key::S, FPSController::movement_back);
        keybinds.insert(Key::A, FPSController::movement_left);
        keybinds.insert(Key::D, FPSController::movement_right);
        keybinds.insert(Key::Esc, FPSController::request_quit);

        return keybinds;
    }
    fn default_mousebinds() -> HashMap<Button, MouseButtonCallback> {
        HashMap::new()
    }
}
