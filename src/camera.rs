pub struct Camera {
    pub rotation_x: f32,   // pitch, radians
    pub rotation_y: f32,   // yaw, radians
    pub distance: f32,     // orbit distance from target
    pub target: glam::Vec3,
    pub up: glam::Vec3,
    pub fov_y_deg: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            rotation_x: 0.3,
            rotation_y: 0.5,
            distance: 4.0,
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fov_y_deg: 45.0,
            near: 0.1,
            far: 100.0,
        }
    }

    pub fn eye(&self) -> glam::Vec3 {
        let cx = self.rotation_x.cos();
        let sx = self.rotation_x.sin();
        let cy = self.rotation_y.cos();
        let sy = self.rotation_y.sin();
        // spherical -> cartesian, orbiting around target
        let offset = glam::Vec3::new(
            self.distance * cx * sy,
            self.distance * sx,
            self.distance * cx * cy,
        );
        self.target + offset
    }

    pub fn view_proj(&self, aspect: f32) -> glam::Mat4 {
        let view = glam::Mat4::look_at_rh(self.eye(), self.target, self.up);
        let dynamic_far = self.far.max(self.distance * 6.0 + 100.0);
        // Scale near plane with distance too, keeping far/near ratio bounded —
        // this is what actually prevents depth-buffer precision flicker at
        // large zoom levels (fixed near + growing far is the bad combo).
        let dynamic_near = self.near.max(self.distance * 0.001);
        let proj = glam::Mat4::perspective_rh(
            self.fov_y_deg.to_radians(),
            aspect,
            dynamic_near,
            dynamic_far,
        );
        proj * view
    }

        pub fn screen_ray(&self, ndc_x: f32, ndc_y: f32, aspect: f32) -> (glam::Vec3, glam::Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let near = inv.project_point3(glam::Vec3::new(ndc_x, ndc_y, 0.0));
        let far = inv.project_point3(glam::Vec3::new(ndc_x, ndc_y, 1.0));
        (near, (far - near).normalize())
    }
}