mod app;
mod config;
mod shortcuts;

fn main() {
    let event_loop = winit::event_loop::EventLoop::new().expect("event loop");
    let mut state = app::App::new();
    event_loop.run_app(&mut state).expect("run");
}
