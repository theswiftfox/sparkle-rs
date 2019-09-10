use crate::input::input_handler::{
    Action, ApplicationRequest, Button, InputHandler, Key, ScrollAxis,
};
use crate::input::Camera;

use std::collections::HashMap;

type KeyCallback = fn(&mut FPSController, Action) -> ApplicationRequest;
type MouseButtonCallback = fn(&mut FPSController, Action) -> ApplicationRequest;

const MOUSE_SPEED: f32 = 0.05f32; // π/180 (convert deg to rad) * 0.05 (sensitivity) //0.00390625f32;

pub struct FPSController {
    pos: glm::Vec3,
    h_angle_deg: f32,
    v_angle_deg: f32,
    view_mat: glm::Mat4,
    projection_mat: glm::Mat4,

    keybinds: HashMap<Key, KeyCallback>,
    mousebinds: HashMap<Button, MouseButtonCallback>,

    move_speed: f32,
    move_f: bool,
    move_b: bool,
    move_l: bool,
    move_r: bool,
    aiming: bool,
    first_mouse: bool,
}

impl Camera for FPSController {
    fn update(&mut self, _delta_t: f32) {
        let pitch_rad = glm::radians(&glm::vec1(self.v_angle_deg)).x;
        let yaw_rad = glm::radians(&glm::vec1(self.h_angle_deg)).x;
        let dir = glm::vec3(
            pitch_rad.cos() * yaw_rad.cos(),
            pitch_rad.sin(),
            pitch_rad.cos() * yaw_rad.sin(),
        )
        .normalize();
        let center = self.pos + dir;
        self.view_mat = glm::look_at(&self.pos, &center, &glm::vec3(0.0f32, 1.0f32, 0.0f32));
    }
    fn view_mat(&self) -> glm::Mat4 {
        self.view_mat
    }
    fn projection_mat(&self) -> glm::Mat4 {
        self.projection_mat
    }
}

impl InputHandler for FPSController {
    fn update(&mut self, delta_t: f32) {
        if self.move_f {
            self.pos = self.pos + -self.move_speed * delta_t * self.get_front();
        }
        if self.move_b {
            self.pos = self.pos + self.move_speed * delta_t * self.get_front();
        }
        if self.move_r {
            self.pos = self.pos + self.move_speed * delta_t * self.get_right();
        }
        if self.move_l {
            self.pos = self.pos + -self.move_speed * delta_t * self.get_right();
        }
    }
    fn handle_key(&mut self, key: Key, action: Action) -> ApplicationRequest {
        match self.keybinds.get(&key) {
            Some(func) => func(self, action),
            None => ApplicationRequest::Nothing,
        }
    }
    fn handle_mouse(&mut self, button: Button, action: Action) -> ApplicationRequest {
        match self.mousebinds.get(&button) {
            Some(func) => func(self, action),
            None => ApplicationRequest::Nothing,
        }
    }
    fn handle_wheel(&mut self, _axis: ScrollAxis, _value: f32) {}
    fn handle_mouse_move(&mut self, x: i32, y: i32) {
        if self.aiming {
            if self.first_mouse {
                self.first_mouse = false;
                return;
            }
            // println!("Mouse Event: x({}), y({})", x, y);
            self.h_angle_deg += (x as f32) * MOUSE_SPEED;
            self.v_angle_deg += (y as f32) * MOUSE_SPEED;
            self.v_angle_deg = (-89.9f32).max(self.v_angle_deg).min(89.9f32);
        }
    }
}

impl FPSController {
    fn proj_lh(aspect: f32, fov: f32, near: f32, far: f32) -> glm::Mat4 {
        let mut mat: glm::Mat4 = glm::zero();
        let y_scale = 1.0f32 / glm::tan(&(glm::radians(&glm::vec1(fov)) / 2.0f32)).x;
        let x_scale = y_scale * aspect;
        mat.column_mut(0)
            .copy_from(&glm::vec4(x_scale, 0.0f32, 0.0f32, 0.0f32));
        mat.column_mut(1)
            .copy_from(&glm::vec4(0.0f32, y_scale, 0.0f32, 0.0f32));
        mat.column_mut(2).copy_from(&glm::vec4(
            0.0f32,
            0.0f32,
            far / (near - far),
            (near * far) / (near - far),
        ));
        mat.column_mut(3)
            .copy_from(&glm::vec4(0.0f32, 0.0f32, -1.0f32, 0.0f32));
        return mat;
    }
    pub fn create(aspect: f32, fov: f32, near: f32, far: f32) -> FPSController {
        let mut proj = glm::perspective_zo(aspect, fov, near, far);
        proj[(1, 1)] *= -1.0f32;
        //cgmath::perspective(cgmath::Rad::from(Deg(fov)), aspect, near, far);
        FPSController {
            pos: glm::vec3(0.0f32, 0.0f32, 3.0f32),
            h_angle_deg: -90.0f32,
            v_angle_deg: 0.0f32,
            view_mat: glm::identity(),
            projection_mat: proj,
            keybinds: FPSController::default_keybinds(),
            mousebinds: FPSController::default_mousebinds(),
            aiming: false,
            move_speed: 1.0f32,
            move_b: false,
            move_f: false,
            move_l: false,
            move_r: false,
            first_mouse: true,
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
    fn get_front(&self) -> glm::Vec3 {
        self.view_mat.column(2).xyz()
    }
    fn get_up(&self) -> glm::Vec3 {
        self.view_mat.column(1).xyz()
    }
    fn get_right(&self) -> glm::Vec3 {
        self.view_mat.column(0).xyz()
    }

    fn movement_front(&mut self, action: Action) -> ApplicationRequest {
        self.move_f = match action {
            Action::Up => false,
            Action::Down => true,
        };
        ApplicationRequest::Nothing
    }
    fn movement_back(&mut self, action: Action) -> ApplicationRequest {
        self.move_b = match action {
            Action::Up => false,
            Action::Down => true,
        };
        ApplicationRequest::Nothing
    }
    fn movement_left(&mut self, action: Action) -> ApplicationRequest {
        self.move_l = match action {
            Action::Up => false,
            Action::Down => true,
        };
        ApplicationRequest::Nothing
    }
    fn movement_right(&mut self, action: Action) -> ApplicationRequest {
        self.move_r = match action {
            Action::Up => false,
            Action::Down => true,
        };
        ApplicationRequest::Nothing
    }
    fn request_quit(&mut self, action: Action) -> ApplicationRequest {
        match action {
            Action::Up => ApplicationRequest::Quit,
            Action::Down => ApplicationRequest::Nothing,
        }
    }

    fn toggle_aim(&mut self, action: Action) -> ApplicationRequest {
        match action {
            Action::Down => match self.aiming {
                true => {
                    self.aiming = false;
                    ApplicationRequest::UnsnapMouse
                }
                false => {
                    self.aiming = true;
                    self.first_mouse = true;
                    ApplicationRequest::SnapMouse
                }
            },
            Action::Up => ApplicationRequest::Nothing,
        }
    }

    fn default_keybinds() -> HashMap<Key, KeyCallback> {
        let mut keybinds: HashMap<Key, KeyCallback> = HashMap::new();
        keybinds.insert(Key::W, FPSController::movement_front);
        keybinds.insert(Key::S, FPSController::movement_back);
        keybinds.insert(Key::A, FPSController::movement_left);
        keybinds.insert(Key::D, FPSController::movement_right);
        keybinds.insert(Key::Esc, FPSController::request_quit);

        return keybinds;
    }
    fn default_mousebinds() -> HashMap<Button, MouseButtonCallback> {
        let mut mousebinds: HashMap<Button, MouseButtonCallback> = HashMap::new();
        mousebinds.insert(Button::Left, FPSController::toggle_aim);
        return mousebinds;
    }
}
