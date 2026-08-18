// file: my_engine/src/app.rs
use winit::event_loop::ActiveEventLoop;
use winit::application::ApplicationHandler;
use winit::window::{Window, WindowId};
use winit::event::WindowEvent;

use crate::renderer::Renderer;
use crate::camera::Camera;
use crate::scene::{Scene, SceneObject, Selection, ray_sphere_hit};
use crate::ui::UiState;
use crate::gizmo::{self, GizmoAxis, GizmoPart, GizmoState};
use crate::gltf_loader::load_gltf_mesh;
use crate::vertex::Vertex;

pub struct App {
    pub window: Option<std::sync::Arc<Window>>,
    pub renderer: Option<Renderer>,
    pub ui: Option<UiState>,
    pub camera: Camera,
    pub scene: Scene,
    pub is_dragging: bool,
    pub last_mouse_pos: Option<(f64, f64)>,
    pub mouse_down_pos: Option<(f64, f64)>,
    pub occluded: bool,
    pub game_view_tex_id: Option<egui::TextureId>,
    pub space_held: bool,
    pub gizmo: GizmoState,
    pub shift_held: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            ui: None,
            camera: Camera::new(),
            scene: Scene::new(),
            is_dragging: false,
            last_mouse_pos: None,
            mouse_down_pos: None,
            occluded: false,
            game_view_tex_id: None,
            space_held: false,
            gizmo: GizmoState::default(),
            shift_held: false,
        }
    }

    fn mouse_ray(&self, x: f64, y: f64) -> Option<(glam::Vec3, glam::Vec3)> {
        let (Some(window), Some(renderer)) = (&self.window, &self.renderer) else { return None };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 { return None; }
        let ndc_x = (x as f32 / size.width as f32) * 2.0 - 1.0;
        let ndc_y = 1.0 - (y as f32 / size.height as f32) * 2.0;
        Some(self.camera.screen_ray(ndc_x, ndc_y, renderer.aspect()))
    }

    fn update_gizmo_hover(&mut self, cursor: (f64, f64)) {
        self.gizmo.hovered = None;
        let Selection::Object(i) = self.scene.selected else { return; };
        let Some(obj) = self.scene.objects.get(i) else { return; };
        let Some((origin, dir)) = self.mouse_ray(cursor.0, cursor.1) else { return; };
        let object_size = obj.pick_radius.max(0.5);
        let scale = gizmo::gizmo_scale(obj.transform.position, self.camera.eye(), object_size);
        self.gizmo.hovered = gizmo::hit_test(origin, dir, obj.transform.position, scale);
    }

    fn begin_gizmo_drag(&mut self, cursor: (f64, f64)) -> bool {
        let Selection::Object(i) = self.scene.selected else { return false; };
        let Some(obj) = self.scene.objects.get(i) else { return false; };
        let Some((origin, dir)) = self.mouse_ray(cursor.0, cursor.1) else { return false; };
        let object_size = obj.pick_radius.max(0.5);
        let scale = gizmo::gizmo_scale(obj.transform.position, self.camera.eye(), object_size);
        let Some(part) = gizmo::hit_test(origin, dir, obj.transform.position, scale) else { return false; };

        let start_param = match part {
            GizmoPart::Move(axis) => gizmo::ray_line_closest(origin, dir, obj.transform.position, axis.dir()).0,
            GizmoPart::Rotate(axis) => {
                gizmo::ray_plane_hit(origin, dir, obj.transform.position, axis.dir())
                    .map(|hit| gizmo::ring_angle(axis.dir(), obj.transform.position, hit))
                    .unwrap_or(0.0)
            }
        };

        self.gizmo.dragging = Some(gizmo::GizmoDrag {
            part,
            object_start_pos: obj.transform.position,
            object_start_rot: obj.transform.rotation,
            start_param,
        });
        true
    }

    fn update_gizmo_drag(&mut self, cursor: (f64, f64)) {
        let Some(drag_part) = self.gizmo.dragging.as_ref().map(|d| d.part) else { return; };
        let (start_pos, start_rot, start_param) = {
            let d = self.gizmo.dragging.as_ref().unwrap();
            (d.object_start_pos, d.object_start_rot, d.start_param)
        };
        let Selection::Object(i) = self.scene.selected else { return; };
        let Some((origin, dir)) = self.mouse_ray(cursor.0, cursor.1) else { return; };
        let Some(obj) = self.scene.objects.get_mut(i) else { return; };

        match drag_part {
            GizmoPart::Move(axis) => {
                let (t, _) = gizmo::ray_line_closest(origin, dir, start_pos, axis.dir());
                let delta = t - start_param;
                let mut new_pos = start_pos + axis.dir() * delta;
                if self.shift_held {
                    let v = match axis {
                        GizmoAxis::X => &mut new_pos.x,
                        GizmoAxis::Y => &mut new_pos.y,
                        GizmoAxis::Z => &mut new_pos.z,
                    };
                    *v = v.round();
                }
                obj.transform.position = new_pos;
            }
            GizmoPart::Rotate(axis) => {
                if let Some(hit) = gizmo::ray_plane_hit(origin, dir, start_pos, axis.dir()) {
                    let angle = gizmo::ring_angle(axis.dir(), start_pos, hit);
                    let mut delta = angle - start_param;
                    if delta > std::f32::consts::PI { delta -= std::f32::consts::TAU; }
                    if delta < -std::f32::consts::PI { delta += std::f32::consts::TAU; }

                    let mut new_rot = start_rot;
                    let comp = match axis {
                        GizmoAxis::X => &mut new_rot.x,
                        GizmoAxis::Y => &mut new_rot.y,
                        GizmoAxis::Z => &mut new_rot.z,
                    };
                    *comp += delta;
                    if self.shift_held {
                        let snap = 15f32.to_radians();
                        *comp = (*comp / snap).round() * snap;
                    }
                    obj.transform.rotation = new_rot;
                }
            }
        }
        if let Some(window) = &self.window { window.request_redraw(); }
    }

    fn pick_at(&mut self, x: f64, y: f64) {
        let (Some(window), Some(renderer)) = (&self.window, &self.renderer) else { return };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 { return; }
        let ndc_x = (x as f32 / size.width as f32) * 2.0 - 1.0;
        let ndc_y = 1.0 - (y as f32 / size.height as f32) * 2.0;
        let (origin, dir) = self.camera.screen_ray(ndc_x, ndc_y, renderer.aspect());

        let mut best: Option<(f32, Selection)> = None;

        for (i, obj) in self.scene.objects.iter().enumerate() {
            if let Some(t) = ray_sphere_hit(origin, dir, obj.transform.position, obj.pick_radius) {
                if best.map_or(true, |(bt, _)| t < bt) {
                    best = Some((t, Selection::Object(i)));
                }
            }
        }
        if let Some(cam) = &self.scene.scene_camera {
            if let Some(t) = ray_sphere_hit(origin, dir, cam.transform.position, 0.3) {
                if best.map_or(true, |(bt, _)| t < bt) {
                    best = Some((t, Selection::Camera));
                }
            }
        }

        self.scene.selected = best.map(|(_, s)| s).unwrap_or(Selection::None);
    }

    fn frame_selected(&mut self) {
        let (Some(renderer), (pos, radius)) = (&self.renderer, match self.scene.selected {
            Selection::Object(i) => {
                let Some(obj) = self.scene.objects.get(i) else { return };
                (obj.transform.position, obj.pick_radius.max(0.3))
            }
            Selection::Camera => {
                let Some(cam) = &self.scene.scene_camera else { return };
                (cam.transform.position, 0.3)
            }
            Selection::None => return,
        }) else { return };

        self.camera.target = pos;

        let half_fov_y = (self.camera.fov_y_deg.to_radians() * 0.5).max(0.01);
        let half_fov_x = (2.0 * ((half_fov_y).tan() * renderer.aspect()).atan()).max(0.01);
        let tightest_half_fov = half_fov_y.min(half_fov_x * 0.5);

        // Padding factor so the object doesn't touch the frame edge —
        // 1.6x gives comfortable breathing room, like Unity's "F" framing.
        let padding = 1.6;
        let dist = (radius * padding) / tightest_half_fov.sin();

        self.camera.distance = dist.clamp(radius * 1.2, 10000.0);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes().with_title("My Engine");
        let window = event_loop.create_window(window_attributes).unwrap();
        let window = std::sync::Arc::new(window);

        let renderer = Renderer::new(window.clone());

        let mut ui = UiState::new(&window, &renderer.device, renderer.surface_config.format);
        self.game_view_tex_id = Some(ui.register_game_view(&renderer.device, &renderer.game_view_view));
        self.ui = Some(ui);

        // --- load glTF (graceful fallback) ---
        // --- load glTF ---
        match crate::gltf_loader::load_gltf_mesh("assets/scene.gltf", "assets/scene.bin") {
            Ok((positions, normals, indices)) => {
                let vertices: Vec<Vertex> = positions
                    .into_iter()
                    .zip(normals)
                    .map(|(p, n)| Vertex { position: p, normal: n })
                    .collect();

                let model = SceneObject::from_mesh(&renderer.device, "model", vertices, indices);
                self.scene.objects.push(model);
                println!("Loaded glTF model successfully.");
            }
            Err(e) => {
                eprintln!("Failed to load glTF: {}. Using fallback cube.", e);
                let (cv, ci) = crate::scene::cube_mesh(glam::Vec3::ZERO, 0.5);
                let cube = SceneObject::from_mesh(&renderer.device, "Cube", cv, ci);
                self.scene.objects.push(cube);
            }
        }

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let (Some(ui), Some(window)) = (&mut self.ui, &self.window) {
            if ui.handle_event(window, &event) {
                if matches!(event, WindowEvent::RedrawRequested) {
                    // still need to redraw even if egui consumed this event
                } else {
                    return;
                }
            }
        }

        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    if state == winit::event::ElementState::Pressed {
                        let cur = self.last_mouse_pos.unwrap_or((0.0, 0.0));
                        if self.begin_gizmo_drag(cur) {
                            // dragging a gizmo handle — skip orbit
                        } else {
                            self.is_dragging = true;
                            self.mouse_down_pos = self.last_mouse_pos;
                        }
                    } else {
                        if self.gizmo.dragging.is_some() {
                            self.gizmo.dragging = None;
                        } else {
                            self.is_dragging = false;
                            if let (Some(down), Some(cur)) = (self.mouse_down_pos, self.last_mouse_pos) {
                                let moved = ((cur.0 - down.0).powi(2) + (cur.1 - down.1).powi(2)).sqrt();
                                if moved < 4.0 {
                                    self.pick_at(cur.0, cur.1);
                                }
                            }
                            self.mouse_down_pos = None;
                        }
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_y = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 1.0,
                    // macOS trackpads send PixelDelta with small values (often 1–10px
                    // per gesture tick) — scale up so it actually moves the camera.
                    winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.05,
                };
                if scroll_y != 0.0 {
                    self.camera.distance = (self.camera.distance * (1.0 - scroll_y * 0.1)).max(0.05);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                let cur = (position.x, position.y);
                if self.gizmo.dragging.is_some() {
                    self.update_gizmo_drag(cur);
                } else if self.is_dragging {
                    if let Some((last_x, last_y)) = self.last_mouse_pos {
                        let dx = (position.x - last_x) as f32;
                        let dy = (position.y - last_y) as f32;
                        if self.space_held {
                            // Pan: move the orbit target along the camera's own
                            // right/up vectors so panning always feels screen-relative.
                            let pan_speed = self.camera.distance * 0.0015;
                            let yaw = self.camera.rotation_y;
                            let right = glam::Vec3::new(yaw.cos(), 0.0, -yaw.sin());
                            self.camera.target -= right * dx * pan_speed;
                            self.camera.target += glam::Vec3::Y * dy * pan_speed;
                        } else {
                            self.camera.rotation_y += dx * 0.01;
                            self.camera.rotation_x += dy * 0.01;
                        }
                    }
                } else {
                    self.update_gizmo_hover(cur);
                }
                self.last_mouse_pos = Some(cur);
            }

            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_held = modifiers.state().shift_key();
            }

            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size.width, new_size.height);
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Occluded(is_occluded) => {
                self.occluded = is_occluded;
                if !is_occluded {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space) = &event.logical_key {
                    self.space_held = event.state == winit::event::ElementState::Pressed;
                }
                if event.state == winit::event::ElementState::Pressed && !event.repeat {
                    if let winit::keyboard::Key::Character(s) = &event.logical_key {
                        if s.as_str().eq_ignore_ascii_case("f") {
                            self.frame_selected();
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if !self.occluded {
                    self.window.as_ref().unwrap().request_redraw();
                }
                if self.occluded {
                    return;
                }

                if self.renderer.is_some() && self.window.is_some() {
                    let renderer = self.renderer.as_mut().unwrap();
                    let window = self.window.as_ref().unwrap();

                    if let Some((output, view)) = renderer.acquire_frame() {
                        let mut encoder = renderer
                            .device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

                        let camera_icon = self.scene.scene_camera.as_ref().map(|cam| {
                            let (cv, ci) = crate::scene::cube_mesh(cam.transform.position, 0.15);
                            SceneObject::from_mesh(&renderer.device, "scene_camera_icon", cv, ci)
                        });

                        let camera_frustum = if self.scene.selected == Selection::Camera {
                            self.scene.scene_camera.as_ref().map(|cam| {
                                let (v, i) = crate::scene::camera_frustum_mesh(cam, renderer.game_view_aspect(), 0.01);
                                SceneObject::from_mesh(&renderer.device, "camera_frustum_gizmo", v, i)
                            })
                        } else {
                            None
                        };

                        let gizmo_objects = match self.scene.selected {
                            Selection::Camera => {
                                if let Some(cam) = &self.scene.scene_camera {
                                    let pos = cam.transform.position;
                                    let axes = [(glam::Vec3::X, 1.0f32), (glam::Vec3::Y, 1.0f32), (glam::Vec3::Z, 1.0f32)];
                                    Some(axes.iter().map(|(dir, len)| {
                                        let (v, idx) = crate::scene::axis_box_mesh(pos, pos + *dir * *len, 0.01);
                                        SceneObject::from_mesh(&renderer.device, "gizmo_axis", v, idx)
                                    }).collect::<Vec<_>>())
                                } else { None }
                            }
                            _ => None,
                        };

                                                let transform_gizmo: Option<(Vec<SceneObject>, Vec<[f32; 4]>)> =
                            if let Selection::Object(i) = self.scene.selected {
                                self.scene.objects.get(i).map(|obj| {
                                    // Unity-style: gizmo stays constant size on screen
                                    let scale = gizmo::gizmo_scale(obj.transform.position, self.camera.eye(), obj.pick_radius);
                                    let pos = obj.transform.position;
                                    let mut objs = Vec::new();
                                    let mut colors = Vec::new();

                                    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
                                        // Moderate thickness for visibility
                                        let (v, idx) = gizmo::arrow_mesh(pos, axis.dir(), scale * 1.2, scale * 0.08);
                                        objs.push(SceneObject::from_mesh(&renderer.device, "gizmo_arrow", v, idx));
                                        let active = self.gizmo.dragging.as_ref().map_or(false, |d| d.part == GizmoPart::Move(axis))
                                            || self.gizmo.hovered == Some(GizmoPart::Move(axis));
                                        colors.push(if active { axis.hover_color() } else { axis.base_color() });
                                    }
                                    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
                                        // Moderate thickness for visibility
                                        let (v, idx) = gizmo::ring_mesh(pos, axis.dir(), scale * 1.6, scale * 0.06, 48);
                                        objs.push(SceneObject::from_mesh(&renderer.device, "gizmo_ring", v, idx));
                                        let active = self.gizmo.dragging.as_ref().map_or(false, |d| d.part == GizmoPart::Rotate(axis))
                                            || self.gizmo.hovered == Some(GizmoPart::Rotate(axis));
                                        colors.push(if active { axis.hover_color() } else { axis.base_color() });
                                    }
                                    (objs, colors)
                                })
                            } else { None };

                        renderer.draw_scene_view(
                            &mut encoder, &view, &self.camera, &self.scene,
                            camera_icon.as_ref(), camera_frustum.as_ref(),
                            gizmo_objects.as_deref(),
                            transform_gizmo.as_ref().map(|(o, c)| (o.as_slice(), c.as_slice())),
                        );
                        renderer.draw_game_view(&mut encoder, &self.scene);

                        if let Some(ui) = &mut self.ui {
                            let full_output = ui.run(window, &mut self.camera, &mut self.scene, self.game_view_tex_id, renderer.game_view_size);
                            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [renderer.surface_config.width, renderer.surface_config.height],
                                pixels_per_point: window.scale_factor() as f32,
                            };
                            ui.paint(&renderer.device, &renderer.queue, &mut encoder, &view, screen_descriptor, full_output);
                        }

                        renderer.queue.submit(std::iter::once(encoder.finish()));
                        output.present();
                    }
                }
            }
            _ => {}
        }
    }
}