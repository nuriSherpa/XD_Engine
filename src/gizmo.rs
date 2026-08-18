use crate::vertex::Vertex;
use crate::scene::axis_box_mesh;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GizmoAxis { X, Y, Z }

impl GizmoAxis {
    pub fn dir(self) -> glam::Vec3 {
        match self {
            GizmoAxis::X => glam::Vec3::X,
            GizmoAxis::Y => glam::Vec3::Y,
            GizmoAxis::Z => glam::Vec3::Z,
        }
    }
    pub fn base_color(self) -> [f32; 4] {
        match self {
            GizmoAxis::X => [0.85, 0.2, 0.2, 1.0],
            GizmoAxis::Y => [0.2, 0.85, 0.2, 1.0],
            GizmoAxis::Z => [0.2, 0.45, 0.95, 1.0],
        }
    }
    pub fn hover_color(self) -> [f32; 4] {
        [1.0, 0.95, 0.2, 1.0] // highlight — Unity-style yellow
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GizmoPart {
    Move(GizmoAxis),
    Rotate(GizmoAxis),
}

pub struct GizmoDrag {
    pub part: GizmoPart,
    pub object_start_pos: glam::Vec3,
    pub object_start_rot: glam::Vec3,
    pub start_param: f32, // t along axis line (move) or angle in radians (rotate)
}

#[derive(Default)]
pub struct GizmoState {
    pub hovered: Option<GizmoPart>,
    pub dragging: Option<GizmoDrag>,
}

/// Scale gizmo like Unity - constant screen-space size regardless of distance,
/// but with a minimum size for small objects
pub fn gizmo_scale(object_pos: glam::Vec3, camera_eye: glam::Vec3, object_size: f32) -> f32 {
    // Unity-style: gizmo stays constant size on screen
    let distance = (object_pos - camera_eye).length();
    let screen_scale = distance * 0.15; // 15% of distance - same as Unity
    
    // Ensure minimum size for very small objects
    screen_scale.max(object_size * 0.8).max(0.1)
}

fn ring_basis(axis: glam::Vec3) -> (glam::Vec3, glam::Vec3) {
    let helper = if axis.dot(glam::Vec3::Y).abs() > 0.99 { glam::Vec3::X } else { glam::Vec3::Y };
    let right = axis.cross(helper).normalize();
    let up = axis.cross(right).normalize();
    (right, up)
}

pub fn cone_mesh(base: glam::Vec3, tip: glam::Vec3, radius: f32, segments: usize) -> (Vec<Vertex>, Vec<u32>) {
    let axis = (tip - base).normalize();
    let (right, up) = ring_basis(axis);

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    vertices.push(Vertex { position: base.into(), normal: (-axis).into() });
    let base_ring_start = vertices.len() as u32;
    for i in 0..segments {
        let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let offset = right * theta.cos() * radius + up * theta.sin() * radius;
        vertices.push(Vertex { position: (base + offset).into(), normal: (-axis).into() });
    }
    for i in 0..segments {
        let a = base_ring_start + i as u32;
        let b = base_ring_start + ((i + 1) % segments) as u32;
        indices.extend([0, b, a]);
    }

    let side_ring_start = vertices.len() as u32;
    for i in 0..segments {
        let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let offset = right * theta.cos() * radius + up * theta.sin() * radius;
        let normal = (offset.normalize() * 0.85 + axis * 0.4).normalize();
        vertices.push(Vertex { position: (base + offset).into(), normal: normal.into() });
    }
    let tip_idx = vertices.len() as u32;
    vertices.push(Vertex { position: tip.into(), normal: axis.into() });
    for i in 0..segments {
        let a = side_ring_start + i as u32;
        let b = side_ring_start + ((i + 1) % segments) as u32;
        indices.extend([a, b, tip_idx]);
    }
    (vertices, indices)
}

/// Arrow = shaft + cone head along `axis`, starting at `center`.
/// Unity-style proportions: visible but not too thick
pub fn arrow_mesh(center: glam::Vec3, axis: glam::Vec3, length: f32, thickness: f32) -> (Vec<Vertex>, Vec<u32>) {
    let shaft_end = center + axis * (length * 0.75);
    let tip = center + axis * length;
    // Moderate thickness for visibility
    let shaft_thickness = thickness * 0.5;
    let (mut vertices, mut indices) = axis_box_mesh(center, shaft_end, shaft_thickness);
    // Cone head - clearly visible
    let (cv, ci) = cone_mesh(shaft_end, tip, shaft_thickness * 2.5, 12);
    let base = vertices.len() as u32;
    vertices.extend(cv);
    indices.extend(ci.into_iter().map(|i| i + base));
    (vertices, indices)
}

/// Ring approximated as a polyline of straight segments in the plane
/// perpendicular to `axis`, centered at `center`.
/// Unity-style: visible ring with moderate thickness
pub fn ring_mesh(center: glam::Vec3, axis: glam::Vec3, radius: f32, thickness: f32, segments: usize) -> (Vec<Vertex>, Vec<u32>) {
    let axis = axis.normalize();
    let (right, up) = ring_basis(axis);

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut prev: Option<glam::Vec3> = None;
    // Moderate thickness for visibility
    let ring_thickness = thickness * 0.6;
    for i in 0..=segments {
        let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let p = center + right * theta.cos() * radius + up * theta.sin() * radius;
        if let Some(prev_p) = prev {
            let (v, idx) = axis_box_mesh(prev_p, p, ring_thickness);
            let base = vertices.len() as u32;
            vertices.extend(v);
            indices.extend(idx.into_iter().map(|x| x + base));
        }
        prev = Some(p);
    }
    (vertices, indices)
}

/// Closest point between a ray and an infinite line. Returns (t_on_line, distance).
pub fn ray_line_closest(ray_origin: glam::Vec3, ray_dir: glam::Vec3, line_point: glam::Vec3, line_dir: glam::Vec3) -> (f32, f32) {
    let ray_dir = ray_dir.normalize();
    let line_dir = line_dir.normalize();
    let w0 = ray_origin - line_point;
    let b = ray_dir.dot(line_dir);
    let d = ray_dir.dot(w0);
    let e = line_dir.dot(w0);
    let denom = 1.0 - b * b;
    let (s, t) = if denom.abs() < 1e-5 {
        (0.0, e)
    } else {
        ((b * e - d) / denom, (e - b * d) / denom)
    };
    let point_on_ray = ray_origin + ray_dir * s;
    let point_on_line = line_point + line_dir * t;
    (t, (point_on_ray - point_on_line).length())
}

/// Ray/plane intersection.
pub fn ray_plane_hit(ray_origin: glam::Vec3, ray_dir: glam::Vec3, plane_point: glam::Vec3, plane_normal: glam::Vec3) -> Option<glam::Vec3> {
    let denom = plane_normal.dot(ray_dir);
    if denom.abs() < 1e-5 { return None; }
    let t = (plane_point - ray_origin).dot(plane_normal) / denom;
    if t < 0.0 { return None; }
    Some(ray_origin + ray_dir * t)
}

/// Angle (radians) of a plane-space point relative to that axis's ring basis —
/// used consistently at drag-start and every drag-move so deltas line up.
pub fn ring_angle(axis: glam::Vec3, center: glam::Vec3, point: glam::Vec3) -> f32 {
    let (right, up) = ring_basis(axis);
    let v = point - center;
    v.dot(up).atan2(v.dot(right))
}

/// Hit-tests all move arrows + rotate rings for one object; returns the closest hit.
pub fn hit_test(ray_origin: glam::Vec3, ray_dir: glam::Vec3, object_pos: glam::Vec3, scale: f32) -> Option<GizmoPart> {
    let arrow_len = scale * 1.2;
    let ring_radius = scale * 1.6;
    let pick_thresh = scale * 0.15;

    let mut best: Option<(f32, GizmoPart)> = None;

    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
        let (t, dist) = ray_line_closest(ray_origin, ray_dir, object_pos, axis.dir());
        if dist < pick_thresh && t > -scale * 0.1 && t < arrow_len * 1.15 {
            if best.map_or(true, |(bd, _)| dist < bd) {
                best = Some((dist, GizmoPart::Move(axis)));
            }
        }
    }
    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
        if let Some(hit) = ray_plane_hit(ray_origin, ray_dir, object_pos, axis.dir()) {
            let d = (hit - object_pos).length();
            let ring_dist = (d - ring_radius).abs();
            if ring_dist < pick_thresh && best.map_or(true, |(bd, _)| ring_dist < bd) {
                best = Some((ring_dist, GizmoPart::Rotate(axis)));
            }
        }
    }
    best.map(|(_, part)| part)
}