#[derive(Clone, Copy)]
pub struct Transform {
    pub position: glam::Vec3,
    pub rotation: glam::Vec3, // euler angles, radians (x, y, z)
    pub scale: glam::Vec3,
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            position: glam::Vec3::ZERO,
            rotation: glam::Vec3::ZERO,
            scale: glam::Vec3::ONE,
        }
    }

    pub fn to_matrix(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(
            self.scale,
            glam::Quat::from_euler(
                glam::EulerRot::XYZ,
                self.rotation.x,
                self.rotation.y,
                self.rotation.z,
            ),
            self.position,
        )
    }
}