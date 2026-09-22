use ferris_compositor::{perf, renderer, scene};

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use renderer::choose_present_mode;

struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    window: Arc<Window>,
    quad_pipeline: renderer::quad::QuadPipeline,
    text_layer: renderer::text::TextLayer,
}

impl GpuState {
    fn new(window: Arc<Window>) -> Option<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("failed to create GPU surface: {e:?}");
                return None;
            }
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }));
        let adapter = match adapter {
            Some(a) => a,
            None => {
                log::error!("no compatible GPU adapter found");
                return None;
            }
        };

        let device_queue = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("ferris_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ));
        let (device, queue) = match device_queue {
            Ok(dq) => dq,
            Err(e) => {
                log::error!("failed to create GPU device: {e:?}");
                return None;
            }
        };

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let present_mode = choose_present_mode(&caps.present_modes);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let quad_pipeline = renderer::quad::QuadPipeline::new(&device, format);
        let text_layer = renderer::text::TextLayer::new(&device, &queue, format);

        Some(Self { surface, device, queue, config, window, quad_pipeline, text_layer })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render_frame(&mut self, frame: &scene::Frame) -> Result<(), wgpu::SurfaceError> {
        let instances = renderer::quad::build_quad_instances(frame);
        self.quad_pipeline.prepare(&self.device, &self.queue, &instances, self.config.width as f32, self.config.height as f32);

        let texts: Vec<scene::TextCommand> = frame.commands.iter().filter_map(|c| match c {
            scene::DrawCommand::Text(t) => Some(t.clone()),
            scene::DrawCommand::Rect(_) => None,
        }).collect();
        self.text_layer.prepare(&self.device, &self.queue, self.config.width, self.config.height, &texts);

        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.05, g: 0.07, b: 0.12, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.quad_pipeline.render(&mut pass, instances.len() as u32);
            self.text_layer.render(&mut pass);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.text_layer.trim_atlas();
        Ok(())
    }
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    start: std::time::Instant,
    frame_timer: perf::FrameTimer,
    last_frame_start: Option<std::time::Instant>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            gpu: None,
            start: std::time::Instant::now(),
            frame_timer: perf::FrameTimer::new(120),
            last_frame_start: None,
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
        match GpuState::new(window.clone()) {
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
            WindowEvent::RedrawRequested => {
                let now = std::time::Instant::now();
                if let Some(last) = self.last_frame_start {
                    self.frame_timer.record(now - last);
                }
                self.last_frame_start = Some(now);

                let elapsed = self.start.elapsed().as_secs_f32();
                let (w, h) = (gpu.config.width as f32, gpu.config.height as f32);
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
                        let (w, h) = (gpu.config.width, gpu.config.height);
                        gpu.resize(w, h);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPU out of memory, exiting");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("surface error: {e:?}"),
                }
                gpu.window.request_redraw();
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
