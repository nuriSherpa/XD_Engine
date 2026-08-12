use crate::scene::{GridMode, Scene, Selection, ProjectionMode};
use crate::camera::Camera;

pub struct UiState {
    pub egui_ctx: egui::Context,
    pub egui_winit: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,
    pub last_frame_time: std::time::Instant,
    pub fps: f32,
    pub frame_time_ms: f32,
}

impl UiState {
    pub fn new(window: &winit::window::Window, device: &wgpu::Device, output_format: wgpu::TextureFormat) -> Self {
        let egui_ctx = egui::Context::default();
        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            None,
            None,
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
        device,
        output_format,
        egui_wgpu::RendererOptions::default(),
    );

        Self {
            egui_ctx,
            egui_winit,
            egui_renderer,
            last_frame_time: std::time::Instant::now(),
            fps: 0.0,
            frame_time_ms: 0.0,
        }
    }

    pub fn register_game_view(&mut self, device: &wgpu::Device, view: &wgpu::TextureView) -> egui::TextureId {
        self.egui_renderer.register_native_texture(device, view, wgpu::FilterMode::Linear)
    }

    pub fn handle_event(&mut self, window: &winit::window::Window, event: &winit::event::WindowEvent) -> bool {
        self.egui_winit.on_window_event(window, event).consumed
    }

    pub fn run(
        &mut self,
        window: &winit::window::Window,
        camera: &mut Camera,
        scene: &mut Scene,
        game_view_tex_id: Option<egui::TextureId>,
        game_view_size: (u32, u32),
    ) -> egui::FullOutput {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
            self.frame_time_ms = dt * 1000.0;
        }

        let raw_input = self.egui_winit.take_egui_input(window);

        let full_output = self.egui_ctx.run_ui(raw_input, |ctx| {
            egui::Window::new("Editor Camera").show(ctx, |ui| {
                ui.add(egui::Slider::new(&mut camera.distance, 0.5..=30.0).text("Distance"));
                if scene.grid_mode == GridMode::ThreeD {
                    ui.add(egui::Slider::new(&mut camera.rotation_y, -std::f32::consts::PI..=std::f32::consts::PI).text("Yaw"));
                    ui.add(egui::Slider::new(&mut camera.rotation_x, -1.5..=1.5).text("Pitch"));
                } else {
                    ui.label("(2D mode: drag to pan, rotation locked)");
                }
                ui.separator();
                ui.add(egui::Slider::new(&mut camera.fov_y_deg, 10.0..=120.0).text("Zoom (FOV°)"));
            });

            egui::Window::new("Hierarchy").show(ctx, |ui| {
                if scene.scene_camera.is_some() {
                    let selected = scene.selected == Selection::Camera;
                    ui.horizontal(|ui| {
                        if ui.selectable_label(selected, "📷 Scene Camera").clicked() {
                            scene.selected = Selection::Camera;
                        }
                        if ui.small_button("🗑").clicked() {
                            scene.remove_camera();
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label("(no scene camera)");
                        if ui.button("+ Add Camera").clicked() {
                            scene.add_default_camera();
                        }
                    });
                }

                ui.separator();
                if scene.objects.is_empty() {
                    ui.label("(no objects in scene)");
                }
                let mut delete_index: Option<usize> = None;
                for i in 0..scene.objects.len() {
                    let name = scene.objects[i].name.clone();
                    let selected = scene.selected == Selection::Object(i);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(selected, name).clicked() {
                            scene.selected = Selection::Object(i);
                        }
                        if ui.small_button("🗑").clicked() {
                            delete_index = Some(i);
                        }
                    });
                }
                if let Some(i) = delete_index {
                    scene.delete_object(i);
                }
            });

            if let Selection::Object(i) = scene.selected {
                if let Some(obj) = scene.objects.get_mut(i) {
                    egui::Window::new("Transform").show(ctx, |ui| {
                        ui.label(&obj.name);
                        ui.separator();
                        ui.label("Position");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut obj.transform.position.x).prefix("X: ").speed(0.01));
                            ui.add(egui::DragValue::new(&mut obj.transform.position.y).prefix("Y: ").speed(0.01));
                            ui.add(egui::DragValue::new(&mut obj.transform.position.z).prefix("Z: ").speed(0.01));
                        });
                        ui.separator();
                        ui.label("Rotation (rad)");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut obj.transform.rotation.x).prefix("X: ").speed(0.01));
                            ui.add(egui::DragValue::new(&mut obj.transform.rotation.y).prefix("Y: ").speed(0.01));
                            ui.add(egui::DragValue::new(&mut obj.transform.rotation.z).prefix("Z: ").speed(0.01));
                        });
                        ui.separator();
                        ui.label("Scale");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut obj.transform.scale.x).prefix("X: ").speed(0.01));
                            ui.add(egui::DragValue::new(&mut obj.transform.scale.y).prefix("Y: ").speed(0.01));
                            ui.add(egui::DragValue::new(&mut obj.transform.scale.z).prefix("Z: ").speed(0.01));
                        });
                    });
                }
            }

            if scene.selected == Selection::Camera {
                if let Some(cam) = &mut scene.scene_camera {
                    egui::Window::new("Camera Properties").show(ctx, |ui| {
                        ui.label("Position");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut cam.transform.position.x).prefix("X: ").speed(0.05));
                            ui.add(egui::DragValue::new(&mut cam.transform.position.y).prefix("Y: ").speed(0.05));
                            ui.add(egui::DragValue::new(&mut cam.transform.position.z).prefix("Z: ").speed(0.05));
                        });
                        ui.label("Rotation (rad)");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut cam.transform.rotation.x).prefix("X: ").speed(0.01));
                            ui.add(egui::DragValue::new(&mut cam.transform.rotation.y).prefix("Y: ").speed(0.01));
                            ui.add(egui::DragValue::new(&mut cam.transform.rotation.z).prefix("Z: ").speed(0.01));
                        });
                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label("Projection:");
                            if ui.selectable_label(cam.projection == ProjectionMode::Perspective, "Perspective").clicked() {
                                cam.projection = ProjectionMode::Perspective;
                            }
                            if ui.selectable_label(cam.projection == ProjectionMode::Orthographic, "Orthographic").clicked() {
                                cam.projection = ProjectionMode::Orthographic;
                            }
                        });
                        match cam.projection {
                            ProjectionMode::Perspective => {
                                ui.add(egui::Slider::new(&mut cam.fov_y_deg, 10.0..=120.0).text("FOV"));
                            }
                            ProjectionMode::Orthographic => {
                                ui.add(egui::Slider::new(&mut cam.orthographic_size, 0.5..=50.0).text("Ortho Size"));
                            }
                        }
                        ui.add(egui::Slider::new(&mut cam.near, 0.01..=5.0).text("Near"));
                        ui.add(egui::Slider::new(&mut cam.far, 5.0..=1000.0).text("Far"));

                        ui.separator();
                        let mut use_override = cam.aspect_override.is_some();
                        ui.checkbox(&mut use_override, "Override Aspect Ratio");
                        if use_override {
                            let mut val = cam.aspect_override.unwrap_or(16.0 / 9.0);
                            ui.add(egui::DragValue::new(&mut val).prefix("Aspect: ").speed(0.01).range(0.1..=5.0));
                            cam.aspect_override = Some(val);
                        } else {
                            cam.aspect_override = None;
                        }

                        ui.separator();
                        ui.checkbox(&mut cam.backface_culling, "Backface Culling");
                    });
                }
            }

            egui::Window::new("Scene Settings").show(ctx, |ui| {
                ui.label("Grid");
                ui.horizontal(|ui| {
                    if ui.selectable_label(scene.grid_mode == GridMode::TwoD, "2D").clicked() {
                        scene.grid_mode = GridMode::TwoD;
                    }
                    if ui.selectable_label(scene.grid_mode == GridMode::ThreeD, "3D").clicked() {
                        scene.grid_mode = GridMode::ThreeD;
                    }
                });
                ui.add(egui::Slider::new(&mut scene.grid_resolution, 0.1..=5.0).text("Grid Resolution"));
            });

            if let Some(tex_id) = game_view_tex_id {
                egui::Window::new("Game").show(ctx, |ui| {
                    let (w, h) = game_view_size;
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex_id, egui::vec2(w as f32, h as f32))));
                });
            }

            egui::Window::new("Debug").show(ctx, |ui| {
                ui.label(format!("FPS: {:.1}", self.fps));
                ui.label(format!("Frame time: {:.2} ms", self.frame_time_ms));
            });
        });

        self.egui_winit.handle_platform_output(window, full_output.platform_output.clone());
        full_output
    }

    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        screen_descriptor: egui_wgpu::ScreenDescriptor,
        full_output: egui::FullOutput,
    ) {
        let clipped_primitives = self.egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer.update_texture(device, queue, *id, image_delta);
        }
        self.egui_renderer.update_buffers(device, queue, encoder, &clipped_primitives, &screen_descriptor);

        let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        let mut render_pass = render_pass.forget_lifetime();
        self.egui_renderer.render(&mut render_pass, &clipped_primitives, &screen_descriptor);
        drop(render_pass);

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }
}