use wgpu::util::DeviceExt;
use crate::vertex::Vertex;
use crate::transform::Transform;

pub struct SceneObject {
    pub name: String,
    pub transform: Transform,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub pick_radius: f32, // bounding-sphere approximation for click picking
}

impl SceneObject {
    pub fn from_mesh(device: &wgpu::Device, name: &str, vertices: Vec<Vertex>, indices: Vec<u32>) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SceneObject Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SceneObject Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let num_indices = indices.len() as u32;
        Self {
            name: name.to_string(),
            transform: Transform::identity(),
            vertex_buffer,
            index_buffer,
            num_indices,
            pick_radius: 0.6, // rough default; replace with real bbox radius once meshes carry bounds
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProjectionMode {
    Perspective,
    Orthographic,
}

/// The scene's own camera — a real, deletable/addable object with a transform,
/// separate from the free-flying editor camera. This is what the Game view
/// renders from.
pub struct SceneCamera {
    pub transform: Transform,
    pub projection: ProjectionMode,
    pub fov_y_deg: f32,
    pub orthographic_size: f32, // half-height of the view volume in ortho mode
    pub near: f32,
    pub far: f32,
    pub aspect_override: Option<f32>,
    pub backface_culling: bool,
}

impl SceneCamera {
    pub fn new() -> Self {
        let mut transform = Transform::identity();
        transform.position = glam::Vec3::new(0.0, 1.5, 5.0);
        Self {
            transform,
            projection: ProjectionMode::Perspective,
            fov_y_deg: 60.0,
            orthographic_size: 5.0,
            near: 0.1,
            far: 100.0,
            aspect_override: None,
            backface_culling: true,
        }
    }

    pub fn view_proj(&self, viewport_aspect: f32) -> glam::Mat4 {
        let aspect = self.aspect_override.unwrap_or(viewport_aspect);

        let rot = glam::Mat4::from_euler(
            glam::EulerRot::YXZ,
            self.transform.rotation.y,
            self.transform.rotation.x,
            self.transform.rotation.z,
        );
        let forward = rot.transform_vector3(glam::Vec3::NEG_Z);
        let up = rot.transform_vector3(glam::Vec3::Y);
        let eye = self.transform.position;
        let view = glam::Mat4::look_at_rh(eye, eye + forward, up);

        let proj = match self.projection {
            ProjectionMode::Perspective => {
                glam::Mat4::perspective_rh(self.fov_y_deg.to_radians(), aspect, self.near, self.far)
            }
            ProjectionMode::Orthographic => {
                let h = self.orthographic_size;
                let w = h * aspect;
                glam::Mat4::orthographic_rh(-w, w, -h, h, self.near, self.far)
            }
        };
        proj * view
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Selection {
    None,
    Object(usize),
    Camera,
}

pub struct Scene {
    pub objects: Vec<SceneObject>,
    pub selected: Selection,
    pub scene_camera: Option<SceneCamera>, // None = deleted; user can re-add
}

impl Scene {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            selected: Selection::None,
            scene_camera: Some(SceneCamera::new()),
        }
    }

    pub fn add_default_camera(&mut self) {
        if self.scene_camera.is_none() {
            self.scene_camera = Some(SceneCamera::new());
        }
    }

    pub fn remove_camera(&mut self) {
        self.scene_camera = None;
        if self.selected == Selection::Camera {
            self.selected = Selection::None;
        }
    }

    pub fn delete_object(&mut self, index: usize) {
        if index < self.objects.len() {
            self.objects.remove(index);
            self.selected = Selection::None; // indices shift; simplest safe reset
        }
    }
}

fn build_box_mesh(corners: [glam::Vec3; 8]) -> (Vec<Vertex>, Vec<u32>) {
    let n = glam::Vec3::Y;
    let vertices: Vec<Vertex> = corners.iter().map(|&p| Vertex {
        position: p.into(),
        normal: n.into(),
    }).collect();
    let indices: Vec<u32> = vec![
        0,1,2, 0,2,3,
        4,6,5, 4,7,6,
        0,4,5, 0,5,1,
        3,2,6, 3,6,7,
        1,5,6, 1,6,2,
        0,3,7, 0,7,4,
    ];
    (vertices, indices)
}

pub fn axis_box_mesh(from: glam::Vec3, to: glam::Vec3, thickness: f32) -> (Vec<Vertex>, Vec<u32>) {
    let dir = (to - from).normalize();
    let len = (to - from).length();
    let helper = if dir.dot(glam::Vec3::Y).abs() > 0.99 { glam::Vec3::X } else { glam::Vec3::Y };
    let right = dir.cross(helper).normalize() * thickness;
    let up = dir.cross(right).normalize() * thickness;
    let center = from + dir * (len * 0.5);
    let half_len = dir * (len * 0.5);
    let corners = [
        center - half_len - right - up, center + half_len - right - up,
        center + half_len + right - up, center - half_len + right - up,
        center - half_len - right + up, center + half_len - right + up,
        center + half_len + right + up, center - half_len + right + up,
    ];
    build_box_mesh(corners)
}

pub fn cube_mesh(center: glam::Vec3, half_size: f32) -> (Vec<Vertex>, Vec<u32>) {
    let h = half_size;
    let corners = [
        center + glam::Vec3::new(-h,-h,-h), center + glam::Vec3::new(h,-h,-h),
        center + glam::Vec3::new(h,h,-h), center + glam::Vec3::new(-h,h,-h),
        center + glam::Vec3::new(-h,-h,h), center + glam::Vec3::new(h,-h,h),
        center + glam::Vec3::new(h,h,h), center + glam::Vec3::new(-h,h,h),
    ];
    build_box_mesh(corners)
}

/// Wireframe frustum for the scene camera, shown like Unity's camera gizmo.
/// Built by unprojecting the NDC cube corners through the camera's own view_proj.
pub fn camera_frustum_mesh(cam: &SceneCamera, viewport_aspect: f32, thickness: f32) -> (Vec<Vertex>, Vec<u32>) {
    let aspect = cam.aspect_override.unwrap_or(viewport_aspect);

    // Cap the far plane just for the gizmo so it doesn't stretch to `far` (100+ units)
    // and dominate the scene view — Unity does the same visual clamp.
    let display_far = cam.far.min(cam.near + 15.0);
    let mut clipped = SceneCamera {
        transform: cam.transform.clone(),
        projection: cam.projection,
        fov_y_deg: cam.fov_y_deg,
        orthographic_size: cam.orthographic_size,
        near: cam.near,
        far: display_far,
        aspect_override: cam.aspect_override,
        backface_culling: cam.backface_culling,
    };
    let inv = clipped.view_proj(aspect).inverse();
    let unproject = |x: f32, y: f32, z: f32| inv.project_point3(glam::Vec3::new(x, y, z));

    let near_z = 0.0; // wgpu clip-space z range is 0..1
    let far_z = 1.0;
    let n = [
        unproject(-1.0, -1.0, near_z), unproject(1.0, -1.0, near_z),
        unproject(1.0, 1.0, near_z),  unproject(-1.0, 1.0, near_z),
    ];
    let f = [
        unproject(-1.0, -1.0, far_z), unproject(1.0, -1.0, far_z),
        unproject(1.0, 1.0, far_z),  unproject(-1.0, 1.0, far_z),
    ];

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut add_edge = |a: glam::Vec3, b: glam::Vec3, vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>| {
        let (v, i) = axis_box_mesh(a, b, thickness);
        let base = vertices.len() as u32;
        vertices.extend(v);
        indices.extend(i.into_iter().map(|idx| idx + base));
    };

    for i in 0..4 {
        add_edge(n[i], n[(i + 1) % 4], &mut vertices, &mut indices); // near rectangle
        add_edge(f[i], f[(i + 1) % 4], &mut vertices, &mut indices); // far rectangle
        add_edge(n[i], f[i], &mut vertices, &mut indices);           // connecting edges
    }
    let _ = &mut clipped; // silence unused-mut if you don't touch it further
    (vertices, indices)
}

/// Ray-sphere intersection for click picking. Returns hit distance if any.
pub fn ray_sphere_hit(origin: glam::Vec3, dir: glam::Vec3, center: glam::Vec3, radius: f32) -> Option<f32> {
    let oc = origin - center;
    let b = oc.dot(dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 { return None; }
    let t = -b - disc.sqrt();
    if t >= 0.0 { Some(t) } else { None }
}