use winit::event_loop::ActiveEventLoop;
use winit::application::ApplicationHandler;
use winit::window::{Window, WindowId};
use winit::event::WindowEvent;

use crate::renderer::Renderer;
use crate::camera::Camera;
use crate::scene::{Scene, SceneObject, GridMode, Selection, ray_sphere_hit};
use crate::ui::UiState;

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
    pub grid_object: Option<SceneObject>,
    pub last_grid_mode: Option<GridMode>,
    pub last_grid_res: f32,
    pub last_grid_center: glam::Vec3,
    pub game_view_tex_id: Option<egui::TextureId>,
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
            grid_object: None,
            last_grid_mode: None,
            last_grid_res: 1.0,
            last_grid_center: glam::Vec3::ZERO,
            game_view_tex_id: None,
        }
    }

    fn rebuild_grid_if_needed(&mut self, device: &wgpu::Device) {
        let center = self.camera.target;
        let mode_changed = self.last_grid_mode != Some(self.scene.grid_mode);
        let res_changed = (self.last_grid_res - self.scene.grid_resolution).abs() > 0.001;
        let moved_enough = (center - self.last_grid_center).length() > self.scene.grid_resolution;

        if self.grid_object.is_none() || mode_changed || res_changed || moved_enough {
            let (gv, gi) = crate::scene::grid_mesh(self.scene.grid_mode, center, 20.0, self.scene.grid_resolution, 0.01);
            self.grid_object = Some(SceneObject::from_mesh(device, "grid", gv, gi));
            self.last_grid_mode = Some(self.scene.grid_mode);
            self.last_grid_res = self.scene.grid_resolution;
            self.last_grid_center = center;
        }
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
                        self.is_dragging = true;
                        self.mouse_down_pos = self.last_mouse_pos;
                    } else {
                        self.is_dragging = false;
                        // Click (not drag) => try to pick something in the scene.
                        if let (Some(down), Some(cur)) = (self.mouse_down_pos, self.last_mouse_pos) {
                            let moved = ((cur.0 - down.0).powi(2) + (cur.1 - down.1).powi(2)).sqrt();
                            if moved < 4.0 {
                                self.pick_at(cur.0, cur.1);
                            }
                        }
                        self.mouse_down_pos = None;
                        self.last_mouse_pos = None;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.is_dragging {
                    if let Some((last_x, last_y)) = self.last_mouse_pos {
                        let dx = (position.x - last_x) as f32;
                        let dy = (position.y - last_y) as f32;

                        if self.scene.grid_mode == GridMode::TwoD {
                            // 2D lock: pan on X/Y only, no rotation.
                            let pan_speed = self.camera.distance * 0.0025;
                            self.camera.target.x -= dx * pan_speed;
                            self.camera.target.y += dy * pan_speed;
                        } else {
                            self.camera.rotation_y += dx * 0.01;
                            self.camera.rotation_x += dy * 0.01;
                        }
                    }
                    self.last_mouse_pos = Some((position.x, position.y));
                } else {
                    self.last_mouse_pos = Some((position.x, position.y));
                }
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
            WindowEvent::RedrawRequested => {
                if !self.occluded {
                    self.window.as_ref().unwrap().request_redraw();
                }
                if self.occluded {
                    return;
                }

                if self.renderer.is_some() && self.window.is_some() {
                    let device = self.renderer.as_ref().unwrap().device.clone();
                    self.rebuild_grid_if_needed(&device);

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

                        let gizmo_objects = match self.scene.selected {
                            Selection::Object(i) => {
                                let pos = self.scene.objects[i].transform.position;
                                let axes = [(glam::Vec3::X, 1.0f32), (glam::Vec3::Y, 1.0f32), (glam::Vec3::Z, 1.0f32)];
                                Some(axes.iter().map(|(dir, len)| {
                                    let (v, idx) = crate::scene::axis_box_mesh(pos, pos + *dir * *len, 0.01);
                                    SceneObject::from_mesh(&renderer.device, "gizmo_axis", v, idx)
                                }).collect::<Vec<_>>())
                            }
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
                            Selection::None => None,
                        };

                        renderer.draw_scene_view(
                            &mut encoder, &view, &self.camera, &self.scene,
                            self.grid_object.as_ref(), camera_icon.as_ref(), gizmo_objects.as_deref(),
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