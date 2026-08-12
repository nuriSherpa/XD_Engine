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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GridMode {
    TwoD,
    ThreeD,
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
    pub grid_mode: GridMode,
    pub grid_resolution: f32, // spacing between grid lines, world units
}

impl Scene {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            selected: Selection::None,
            scene_camera: Some(SceneCamera::new()),
            grid_mode: GridMode::ThreeD,
            grid_resolution: 1.0,
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

/// Builds one merged grid mesh centered on `center`, snapped to `step` so it
/// stays visually stable as it's rebuilt while the camera moves — this is
/// what gives the "infinite" feel without an actual infinite shader plane.
/// 3D mode: ground grid on XZ. 2D mode: flat grid on XY (matches the 2D
/// camera lock, which only pans on X/Y).
pub fn grid_mesh(mode: GridMode, center: glam::Vec3, half_extent: f32, step: f32, thickness: f32) -> (Vec<Vertex>, Vec<u32>) {
    let step = step.max(0.05);
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let mut add_line = |a: glam::Vec3, b: glam::Vec3, vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>| {
        let (v, i) = axis_box_mesh(a, b, thickness);
        let base = vertices.len() as u32;
        vertices.extend(v);
        indices.extend(i.into_iter().map(|idx| idx + base));
    };

    let steps = (half_extent / step).ceil() as i32;

    match mode {
        GridMode::ThreeD => {
            let cx = (center.x / step).round() * step;
            let cz = (center.z / step).round() * step;
            for i in -steps..=steps {
                let x = cx + i as f32 * step;
                let z = cz + i as f32 * step;
                add_line(glam::Vec3::new(x, 0.0, cz - half_extent), glam::Vec3::new(x, 0.0, cz + half_extent), &mut vertices, &mut indices);
                add_line(glam::Vec3::new(cx - half_extent, 0.0, z), glam::Vec3::new(cx + half_extent, 0.0, z), &mut vertices, &mut indices);
            }
        }
        GridMode::TwoD => {
            let cx = (center.x / step).round() * step;
            let cy = (center.y / step).round() * step;
            for i in -steps..=steps {
                let x = cx + i as f32 * step;
                let y = cy + i as f32 * step;
                add_line(glam::Vec3::new(x, cy - half_extent, 0.0), glam::Vec3::new(x, cy + half_extent, 0.0), &mut vertices, &mut indices);
                add_line(glam::Vec3::new(cx - half_extent, y, 0.0), glam::Vec3::new(cx + half_extent, y, 0.0), &mut vertices, &mut indices);
            }
        }
    }
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