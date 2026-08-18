// file: my_engine/src/renderer.rs
use wgpu::util::DeviceExt;
use crate::vertex::Vertex;
use crate::camera::Camera;
use crate::scene::{Scene, SceneObject, Selection};
use crate::vertex::GridVertex;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct UniformData {
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
}

pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub render_pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_stride: u64,
    pub bind_group: wgpu::BindGroup,
    pub max_draws: u64,
    pub depth_view: wgpu::TextureView,

    // Game view: offscreen render target showing what scene.scene_camera sees.
    pub game_view_texture: wgpu::Texture,
    pub game_view_view: wgpu::TextureView,
    pub game_view_depth: wgpu::TextureView,
    pub game_view_size: (u32, u32),
    game_view_slot_offset: u64,

    pub render_pipeline_no_cull: wgpu::RenderPipeline,
    pub outline_pipeline: wgpu::RenderPipeline,


    pub grid_pipeline: wgpu::RenderPipeline,
    pub grid_vertex_buffer: wgpu::Buffer,
    pub grid_uniform_buffer: wgpu::Buffer,
    pub grid_bind_group: wgpu::BindGroup,
}

impl Renderer {
    pub fn new(window: std::sync::Arc<winit::window::Window>) -> Self {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            }
        )).unwrap();

        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default())
        ).unwrap();

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats[0];

        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else {
            surface_caps.present_modes[0]
        };

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Main Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // --- Uniform buffer (per-draw MVP + color, dynamic offset) ---
        // max_draws was 64 — bumped to 128 and split into two zones so the
        // Scene view (0..64) and Game view (64..128) never collide within a frame.
        let align = device.limits().min_uniform_buffer_offset_alignment as u64;
        let uniform_size = std::mem::size_of::<UniformData>() as u64;
        let uniform_stride = ((uniform_size + align - 1) / align) * align;
        let max_draws = 128u64;
        let game_view_slot_offset = 64u64;

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: uniform_stride * max_draws,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(uniform_size),
                }),
            }],
        });

        // --- Depth texture (main window) ---
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d { width: size.width.max(1), height: size.height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });


        let render_pipeline_no_cull = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline (No Cull)"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader, entry_point: Some("vs_main"),
            buffers: &[Vertex::layout()], compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader, entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

        let outline_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Outline Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader_outline.wgsl").into()),
        });

        let outline_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Outline Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &outline_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &outline_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            // Cull FRONT faces so only the expanded back-faces render —
            // that ring is exactly the outline silhouette.
            primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Front), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

    

            // --- Infinite grid ---
        let grid_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader_grid.wgsl").into()),
        });

        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct GridUniform {
            view_proj: [[f32; 4]; 4],
            camera_pos: [f32; 4],
            params: [f32; 4],
        }

        let grid_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Uniform Buffer"),
            size: std::mem::size_of::<GridUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let grid_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Grid Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let grid_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Grid Bind Group"),
            layout: &grid_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: grid_uniform_buffer.as_entire_binding(),
            }],
        });

        let grid_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grid Pipeline Layout"),
            bind_group_layouts: &[Some(&grid_bind_group_layout)],
            immediate_size: 0,
        });

        let grid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Pipeline"),
            layout: Some(&grid_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &grid_shader,
                entry_point: Some("vs_main"),
                buffers: &[GridVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &grid_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let s = 2000.0f32;
        let grid_verts = [
            GridVertex { position: [-s, 0.0, -s] },
            GridVertex { position: [ s, 0.0, -s] },
            GridVertex { position: [ s, 0.0,  s] },
            GridVertex { position: [-s, 0.0, -s] },
            GridVertex { position: [ s, 0.0,  s] },
            GridVertex { position: [-s, 0.0,  s] },
        ];
        let grid_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Grid Vertex Buffer"),
            contents: bytemuck::cast_slice(&grid_verts),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        // --- Game view: offscreen render target the Game window displays ---
        let game_view_size = (320u32, 180u32);
        let game_view_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Game View Texture"),
            size: wgpu::Extent3d { width: game_view_size.0, height: game_view_size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let game_view_view = game_view_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let game_view_depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Game View Depth"),
            size: wgpu::Extent3d { width: game_view_size.0, height: game_view_size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let game_view_depth = game_view_depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            surface, device, queue, surface_config,
            render_pipeline,
            render_pipeline_no_cull,
            outline_pipeline,
            uniform_buffer, uniform_stride, bind_group, max_draws,
            depth_view,
            game_view_texture, game_view_view, game_view_depth, game_view_size,
            game_view_slot_offset,
            grid_pipeline, grid_vertex_buffer, grid_uniform_buffer, grid_bind_group,
        }
    }

    pub fn aspect(&self) -> f32 {
        self.surface_config.width as f32 / self.surface_config.height as f32
    }

    pub fn game_view_aspect(&self) -> f32 {
        self.game_view_size.0 as f32 / self.game_view_size.1 as f32
    }

    pub fn acquire_frame(&self) -> Option<(wgpu::SurfaceTexture, wgpu::TextureView)> {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return None,
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        Some((output, view))
    }

    fn render_objects_pass<'a>(
        &self,
        render_pass: &mut wgpu::RenderPass<'a>,
        view_proj: glam::Mat4,
        objects: impl Iterator<Item = (&'a SceneObject, [f32; 4])>,
        slot: &mut u64,
    ) {
        for (obj, color) in objects {
            assert!(*slot < self.max_draws, "exceeded max_draws in Renderer");
            let mvp = view_proj * obj.transform.to_matrix();
            let data = UniformData { mvp: mvp.to_cols_array_2d(), color };
            let offset = *slot * self.uniform_stride;
            self.queue.write_buffer(&self.uniform_buffer, offset, bytemuck::bytes_of(&data));
            render_pass.set_bind_group(0, &self.bind_group, &[offset as u32]);
            render_pass.set_vertex_buffer(0, obj.vertex_buffer.slice(..));
            render_pass.set_index_buffer(obj.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..obj.num_indices, 0, 0..1);
            *slot += 1;
        }
    }

        pub fn draw_scene_view(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        camera: &Camera,
        scene: &Scene,
        camera_icon: Option<&SceneObject>,
        camera_frustum: Option<&SceneObject>,
        gizmo: Option<&[SceneObject]>,
        transform_gizmo: Option<(&[SceneObject], &[[f32; 4]])>,
    ) {
        let view_proj = camera.view_proj(self.aspect());
        let mut slot = 0u64;

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Scene View Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.05, b: 0.06, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        render_pass.set_pipeline(&self.render_pipeline);

               #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct GridUniform { view_proj: [[f32; 4]; 4], camera_pos: [f32; 4], params: [f32; 4] }

        let eye = camera.eye();
        let dist = camera.distance.max(0.05);

        // Adaptive cell size: pick the nearest power-of-10 step so grid density
        // stays readable whether you're at distance 2 or distance 5000.
        let cell_size = 10f32.powf((dist / 8.0).max(0.01).log10().round());
        let fade_far = (dist * 4.0).max(cell_size * 20.0);
        let fade_near = fade_far * 0.4;

        // Recenter+rescale the quad around the camera target each frame so it
        // always covers the visible area, however far you zoom — snapped to
        // half the cell size to avoid visible popping as it moves.
        let snap = cell_size * 0.5;
        let cx = (camera.target.x / snap).round() * snap;
        let cz = (camera.target.z / snap).round() * snap;
        let half = (dist * 8.0).max(200.0);
        let grid_verts = [
            crate::vertex::GridVertex { position: [cx - half, 0.0, cz - half] },
            crate::vertex::GridVertex { position: [cx + half, 0.0, cz - half] },
            crate::vertex::GridVertex { position: [cx + half, 0.0, cz + half] },
            crate::vertex::GridVertex { position: [cx - half, 0.0, cz - half] },
            crate::vertex::GridVertex { position: [cx + half, 0.0, cz + half] },
            crate::vertex::GridVertex { position: [cx - half, 0.0, cz + half] },
        ];
        self.queue.write_buffer(&self.grid_vertex_buffer, 0, bytemuck::cast_slice(&grid_verts));

        let grid_data = GridUniform {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: [eye.x, eye.y, eye.z, 0.0],
            params: [cell_size, fade_near, fade_far, 0.0],
        };
        self.queue.write_buffer(&self.grid_uniform_buffer, 0, bytemuck::bytes_of(&grid_data));
        render_pass.set_pipeline(&self.grid_pipeline);
        render_pass.set_bind_group(0, &self.grid_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.grid_vertex_buffer.slice(..));
        render_pass.draw(0..6, 0..1);

        render_pass.set_pipeline(&self.render_pipeline);

        // Outline pass: draw the selected object's expanded silhouette first.
        if let Selection::Object(i) = scene.selected {
            if let Some(obj) = scene.objects.get(i) {
                render_pass.set_pipeline(&self.outline_pipeline);
                self.render_objects_pass(
                    &mut render_pass, view_proj,
                    std::iter::once((obj, [1.0, 0.85, 0.2, 1.0])), // golden yellow
                    &mut slot,
                );
                render_pass.set_pipeline(&self.render_pipeline);
            }
        }

        let obj_iter = scene.objects.iter().map(|o| (o, [0.0, 0.0, 0.0, 0.0]));
        self.render_objects_pass(&mut render_pass, view_proj, obj_iter, &mut slot);

        if let Some(cam_obj) = camera_icon {
            self.render_objects_pass(&mut render_pass, view_proj, std::iter::once((cam_obj, [0.2, 0.8, 1.0, 1.0])), &mut slot);
        }

        if let Some(frustum) = camera_frustum {
            self.render_objects_pass(&mut render_pass, view_proj, std::iter::once((frustum, [1.0, 0.85, 0.2, 1.0])), &mut slot);
        }

        if let Some(lines) = gizmo {
            let colored = lines.iter().enumerate().map(|(i, l)| {
                let color = match i { 0 => [1.0, 0.2, 0.2, 1.0], 1 => [0.2, 1.0, 0.2, 1.0], _ => [0.2, 0.4, 1.0, 1.0] };
                (l, color)
            });
            self.render_objects_pass(&mut render_pass, view_proj, colored, &mut slot);
        }

        // ADD THIS BLOCK HERE - right after the camera axis lines block
        if let Some((objs, colors)) = transform_gizmo {
            let iter = objs.iter().zip(colors.iter().copied());
            self.render_objects_pass(&mut render_pass, view_proj, iter, &mut slot);
        }
    }

    pub fn draw_game_view(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene) {
    let Some(cam) = &scene.scene_camera else {
        let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Game View Pass (No Camera)"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.game_view_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        return;
    };

    let view_proj = cam.view_proj(self.game_view_aspect());
    let mut slot = self.game_view_slot_offset;

    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Game View Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &self.game_view_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &self.game_view_depth,
            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    render_pass.set_pipeline(if cam.backface_culling { &self.render_pipeline } else { &self.render_pipeline_no_cull });

    let obj_iter = scene.objects.iter().map(|o| (o, [0.0, 0.0, 0.0, 0.0]));
    self.render_objects_pass(&mut render_pass, view_proj, obj_iter, &mut slot);
}

    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == 0 || new_height == 0 {
            return;
        }
        self.surface_config.width = new_width;
        self.surface_config.height = new_height;
        self.surface.configure(&self.device, &self.surface_config);

        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d { width: new_width, height: new_height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
    }
}