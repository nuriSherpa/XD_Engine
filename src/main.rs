mod vertex;
mod camera;
mod gltf_loader;
mod renderer;
mod ui;
mod app;
mod scene;
mod transform;


use app::App;

fn main() {
    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}