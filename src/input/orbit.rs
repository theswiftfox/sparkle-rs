//! Orbit camera for the editor.
//!
//! Orbits around a focus point with right-click drag (rotate), middle-click
//! drag (pan), and scroll wheel (zoom). Driven directly by the editor from
//! raw winit events rather than through the [`InputHandler`] trait.

use super::Camera;

const MIN_DISTANCE: f32 = 0.5;
const MAX_DISTANCE: f32 = 500.0;
const ORBIT_SPEED: f32 = 0.25; // degrees per pixel of mouse delta
const PAN_SPEED: f32 = 0.005; // world units per pixel of mouse delta (scaled by distance)
const ZOOM_SPEED: f32 = 0.1; // fraction of distance per scroll tick

pub struct OrbitCamera {
    /// The point the camera orbits around.
    focus: glm::Vec3,
    /// Distance from the focus point.
    distance: f32,
    /// Horizontal angle in degrees (0 = looking along +X).
    azimuth: f32,
    /// Vertical angle in degrees (clamped to avoid gimbal lock).
    elevation: f32,
    projection_mat: glm::Mat4,
    near_plane: f32,
    far_plane: f32,
}

impl OrbitCamera {
    pub fn new(aspect: f32, fov: f32, near: f32, far: f32) -> Self {
        let fov_rad = glm::radians(&glm::vec1(fov)).x;
        let proj = glm::perspective_zo(aspect, fov_rad, near, far);
        OrbitCamera {
            focus: glm::vec3(0.0, 1.5, 0.0),
            distance: 5.0,
            azimuth: -90.0,
            elevation: 15.0,
            projection_mat: proj,
            near_plane: near,
            far_plane: far,
        }
    }

    pub fn new_ptr(
        aspect: f32,
        fov: f32,
        near: f32,
        far: f32,
    ) -> std::rc::Rc<std::cell::RefCell<Self>> {
        std::rc::Rc::new(std::cell::RefCell::new(Self::new(aspect, fov, near, far)))
    }

    /// Rotate the camera around the focus point.
    /// `dx` and `dy` are pixel deltas from mouse movement.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.azimuth += dx * ORBIT_SPEED;
        self.elevation += dy * ORBIT_SPEED;
        self.elevation = self.elevation.clamp(-89.9, 89.9);
    }

    /// Pan the focus point in the camera's local XY plane.
    /// `dx` and `dy` are pixel deltas from mouse movement.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let right = self.right_vector();
        let up = self.up_vector();
        let scale = self.distance * PAN_SPEED;
        self.focus = self.focus - right * dx * scale + up * dy * scale;
    }

    /// Zoom in/out by adjusting the distance from focus.
    /// `delta` is the scroll amount (positive = zoom in).
    pub fn zoom(&mut self, delta: f32) {
        self.distance *= 1.0 - delta * ZOOM_SPEED;
        self.distance = self.distance.clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Compute the camera's world-space position from spherical coordinates.
    fn eye_position(&self) -> glm::Vec3 {
        let az = glm::radians(&glm::vec1(self.azimuth)).x;
        let el = glm::radians(&glm::vec1(self.elevation)).x;
        let x = self.distance * el.cos() * az.cos();
        let y = self.distance * el.sin();
        let z = self.distance * el.cos() * az.sin();
        self.focus + glm::vec3(x, y, z)
    }

    fn right_vector(&self) -> glm::Vec3 {
        let az = glm::radians(&glm::vec1(self.azimuth)).x;
        // Right is perpendicular to the view direction in the XZ plane
        glm::vec3(-az.sin(), 0.0, az.cos()).normalize()
    }

    fn up_vector(&self) -> glm::Vec3 {
        // Approximate up in camera space (good enough for panning)
        let forward = (self.focus - self.eye_position()).normalize();
        let right = self.right_vector();
        right.cross(&forward).normalize()
    }
}

impl Camera for OrbitCamera {
    fn update(&mut self, _delta_t: f32) {
        // Orbit camera state is updated immediately by orbit/pan/zoom calls,
        // so there's nothing deferred to do here.
    }

    fn view_mat(&self) -> glm::Mat4 {
        let eye = self.eye_position();
        glm::look_at_rh(&eye, &self.focus, &glm::vec3(0.0, 1.0, 0.0))
    }

    fn projection_mat(&self) -> glm::Mat4 {
        self.projection_mat
    }

    fn position(&self) -> glm::Vec3 {
        self.eye_position()
    }

    fn near_far(&self) -> (f32, f32) {
        (self.near_plane, self.far_plane)
    }
}
