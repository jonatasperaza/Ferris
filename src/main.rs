use ferris_compositor::{perf, renderer, scene};

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use renderer::Renderer;

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Renderer>,
    start: std::time::Instant,
    frame_timer: perf::FrameTimer,
    last_frame_start: Option<std::time::Instant>,
    occluded: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            gpu: None,
            start: std::time::Instant::now(),
            frame_timer: perf::FrameTimer::new(120),
            last_frame_start: None,
            occluded: false,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = match event_loop.create_window(Window::default_attributes().with_title("Ferris")) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("failed to create window: {e:?}");
                event_loop.exit();
                return;
            }
        };
        let scale_factor = window.scale_factor() as f32;
        let uncapped = std::env::var("FERRIS_UNCAPPED").map(|v| v == "1").unwrap_or(false);
        match Renderer::new(window.clone(), scale_factor, uncapped) {
            Some(gpu) => {
                self.gpu = Some(gpu);
                self.window = Some(window);
            }
            None => {
                log::error!("GPU initialization failed, exiting");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gpu.resize(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                gpu.set_scale_factor(scale_factor as f32);
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
            }
            WindowEvent::RedrawRequested => {
                let minimized = self.window.as_ref().and_then(|w| w.is_minimized()).unwrap_or(false);
                if self.occluded || minimized {
                    return;
                }

                let now = std::time::Instant::now();
                if let Some(last) = self.last_frame_start {
                    self.frame_timer.record(now - last);
                }
                self.last_frame_start = Some(now);

                let elapsed = self.start.elapsed().as_secs_f32();
                let (w, h) = (gpu.width() as f32, gpu.height() as f32);
                let mut frame = scene::build_test_scene(elapsed, w, h);

                let overlay = format!(
                    "{:.1} fps | {:.2} ms/frame | budget {}",
                    self.frame_timer.fps(),
                    self.frame_timer.average_frame_time().as_secs_f32() * 1000.0,
                    if self.frame_timer.meets_budget(perf::BUDGET_120FPS) { "OK" } else { "MISSED" },
                );
                frame.push(scene::DrawCommand::Text(scene::TextCommand {
                    x: 20.0, y: 50.0, content: overlay, size: 18.0, color: [1.0, 0.9, 0.3, 1.0],
                }));

                match gpu.render_frame(&frame) {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let (w, h) = (gpu.width(), gpu.height());
                        gpu.resize(w, h);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPU out of memory, exiting");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("surface error: {e:?}"),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("event loop error");
}
