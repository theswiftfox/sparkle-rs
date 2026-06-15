use crate::input::{
    input_handler::{Action, ApplicationRequest, Button, InputHandler, Key, ScrollAxis},
    AppSettings, Camera,
};

use std::collections::HashMap;

type ActionCallback = fn(&mut FPSController, Action) -> ApplicationRequest;

const MOUSE_SPEED: f32 = 0.05f32;

pub struct FPSController {
    world_up: glm::Vec3,
    pos: glm::Vec3,
    front: glm::Vec3,
    up: glm::Vec3,
    right: glm::Vec3,
    h_angle_deg: f32,
    v_angle_deg: f32,
    projection_mat: glm::Mat4,
    near_plane: f32,
    far_plane: f32,
    settings: AppSettings,

    keybinds: HashMap<Key, ActionCallback>,
    mousebinds: HashMap<Button, ActionCallback>,

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
        self.front = glm::vec3(
            pitch_rad.cos() * yaw_rad.cos(),
            pitch_rad.sin(),
            pitch_rad.cos() * yaw_rad.sin(),
        )
        .normalize();
        self.right = self.front.cross(&self.world_up).normalize();
        self.up = self.right.cross(&self.front).normalize();
    }
    fn view_mat(&self) -> glm::Mat4 {
        let center = self.pos + self.front;
        glm::look_at_rh(&self.pos, &center, &glm::vec3(0.0f32, 1.0f32, 0.0f32))
    }
    fn projection_mat(&self) -> glm::Mat4 {
        self.projection_mat
    }
    fn position(&self) -> glm::Vec3 {
        self.pos
    }
    fn near_far(&self) -> (f32, f32) {
        (self.near_plane, self.far_plane)
    }
}

impl InputHandler for FPSController {
    fn update(&mut self, delta_t: f32, settings: &mut crate::engine::settings::Settings) {
        if self.move_f {
            self.pos = self.pos + self.move_speed * delta_t * self.get_front();
        }
        if self.move_b {
            self.pos = self.pos + -self.move_speed * delta_t * self.get_front();
        }
        if self.move_r {
            self.pos = self.pos + self.move_speed * delta_t * self.get_right();
        }
        if self.move_l {
            self.pos = self.pos + -self.move_speed * delta_t * self.get_right();
        }

        settings.ssao = self.settings.ssao;
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
            self.h_angle_deg += (x as f32) * MOUSE_SPEED;
            self.v_angle_deg -= (y as f32) * MOUSE_SPEED;
            self.v_angle_deg = (-89.9f32).max(self.v_angle_deg).min(89.9f32);
        }
    }
    fn settings(&self) -> &AppSettings {
        &self.settings
    }
}

impl FPSController {
    pub fn is_aiming(&self) -> bool {
        self.aiming
    }
    pub fn create(aspect: f32, fov: f32, near: f32, far: f32) -> FPSController {
        let fov_rad = glm::radians(&glm::vec1(fov)).x;
        let proj = glm::perspective_zo(aspect, fov_rad, near, far);
        FPSController {
            world_up: glm::vec3(0.0f32, 1.0f32, 0.0f32),
            pos: glm::vec3(0.0f32, 1.5f32, 0.0f32),
            front: glm::zero(),
            up: glm::zero(),
            right: glm::zero(),
            h_angle_deg: -90.0f32,
            v_angle_deg: 0.0f32,
            projection_mat: proj,
            near_plane: near,
            far_plane: far,
            keybinds: FPSController::default_keybinds(),
            mousebinds: FPSController::default_mousebinds(),
            aiming: false,
            move_speed: 1.0f32,
            move_b: false,
            move_f: false,
            move_l: false,
            move_r: false,
            first_mouse: true,
            settings: AppSettings::default(),
        }
    }
    fn get_front(&self) -> glm::Vec3 {
        self.front.clone()
    }
    fn get_up(&self) -> glm::Vec3 {
        self.up.clone()
    }
    fn get_right(&self) -> glm::Vec3 {
        self.right.clone()
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

    fn toggle_ssao(&mut self, action: Action) -> ApplicationRequest {
        match action {
            Action::Up => ApplicationRequest::Nothing,
            Action::Down => {
                self.settings.ssao = !self.settings.ssao;
                ApplicationRequest::SettingsChange
            }
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

    fn default_keybinds() -> HashMap<Key, ActionCallback> {
        let mut keybinds: HashMap<Key, ActionCallback> = HashMap::new();
        keybinds.insert(Key::W, FPSController::movement_front);
        keybinds.insert(Key::S, FPSController::movement_back);
        keybinds.insert(Key::A, FPSController::movement_left);
        keybinds.insert(Key::D, FPSController::movement_right);
        keybinds.insert(Key::Esc, FPSController::request_quit);
        keybinds.insert(Key::F3, FPSController::toggle_ssao);

        return keybinds;
    }
    fn default_mousebinds() -> HashMap<Button, ActionCallback> {
        let mut mousebinds: HashMap<Button, ActionCallback> = HashMap::new();
        mousebinds.insert(Button::Left, FPSController::toggle_aim);
        return mousebinds;
    }
}
