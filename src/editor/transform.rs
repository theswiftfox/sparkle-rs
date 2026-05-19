//! Transform decomposition and recomposition utilities.
//!
//! Decomposes a 4x4 affine matrix into Position, Rotation (Euler XYZ degrees),
//! and Scale components for display in the inspector. Recomposes back to a
//! matrix after editing.

/// Decomposed transform: position, rotation (Euler degrees), scale.
#[derive(Clone, Debug)]
pub struct DecomposedTransform {
    pub position: [f32; 3],
    /// Euler angles in degrees (X, Y, Z).
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

impl DecomposedTransform {
    /// Decompose a 4x4 affine matrix into position, Euler rotation, and scale.
    ///
    /// Assumes the matrix is a valid affine transform (no shear/projection).
    /// Extracts scale from column magnitudes, then normalizes to get the
    /// rotation matrix, and converts to Euler XYZ angles.
    pub fn from_mat4(m: &glm::Mat4) -> Self {
        // Extract translation from column 3
        let position = [m[(0, 3)], m[(1, 3)], m[(2, 3)]];

        // Extract scale from column magnitudes
        let sx = glm::vec3(m[(0, 0)], m[(1, 0)], m[(2, 0)]).magnitude();
        let sy = glm::vec3(m[(0, 1)], m[(1, 1)], m[(2, 1)]).magnitude();
        let sz = glm::vec3(m[(0, 2)], m[(1, 2)], m[(2, 2)]).magnitude();

        let scale = [sx, sy, sz];

        // Normalize columns to get pure rotation matrix
        let r00 = if sx > 1e-6 { m[(0, 0)] / sx } else { 1.0 };
        let r10 = if sx > 1e-6 { m[(1, 0)] / sx } else { 0.0 };
        let r20 = if sx > 1e-6 { m[(2, 0)] / sx } else { 0.0 };
        let _r01 = if sy > 1e-6 { m[(0, 1)] / sy } else { 0.0 };
        let r11 = if sy > 1e-6 { m[(1, 1)] / sy } else { 1.0 };
        let r21 = if sy > 1e-6 { m[(2, 1)] / sy } else { 0.0 };
        let _r02 = if sz > 1e-6 { m[(0, 2)] / sz } else { 0.0 };
        let r12 = if sz > 1e-6 { m[(1, 2)] / sz } else { 0.0 };
        let r22 = if sz > 1e-6 { m[(2, 2)] / sz } else { 1.0 };

        // Extract Euler XYZ angles (intrinsic rotation order)
        // Using the standard decomposition from a rotation matrix
        let (rx, ry, rz);
        if r20.abs() < 0.9999 {
            ry = (-r20 as f64).asin() as f32;
            let cy = ry.cos();
            rx = (r21 / cy).atan2(r22 / cy);
            rz = (r10 / cy).atan2(r00 / cy);
        } else {
            // Gimbal lock
            ry = if r20 < 0.0 {
                std::f32::consts::FRAC_PI_2
            } else {
                -std::f32::consts::FRAC_PI_2
            };
            rx = r12.atan2(r11);
            rz = 0.0;
        }

        let rotation = [rx.to_degrees(), ry.to_degrees(), rz.to_degrees()];

        DecomposedTransform {
            position,
            rotation,
            scale,
        }
    }

    /// Recompose back into a 4x4 affine matrix.
    ///
    /// Order: Scale -> Rotate (Euler XYZ intrinsic) -> Translate.
    pub fn to_mat4(&self) -> glm::Mat4 {
        let t = glm::translation(&glm::vec3(
            self.position[0],
            self.position[1],
            self.position[2],
        ));

        let rx = self.rotation[0].to_radians();
        let ry = self.rotation[1].to_radians();
        let rz = self.rotation[2].to_radians();

        let rot_x = glm::rotation(rx, &glm::vec3(1.0, 0.0, 0.0));
        let rot_y = glm::rotation(ry, &glm::vec3(0.0, 1.0, 0.0));
        let rot_z = glm::rotation(rz, &glm::vec3(0.0, 0.0, 1.0));
        // Intrinsic XYZ = extrinsic ZYX: M = Rz * Ry * Rx
        let r = rot_z * rot_y * rot_x;

        let s = glm::scaling(&glm::vec3(self.scale[0], self.scale[1], self.scale[2]));

        t * r * s
    }
}
