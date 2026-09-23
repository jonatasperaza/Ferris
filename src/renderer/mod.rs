pub mod quad;
pub mod text;

use std::sync::Arc;

use winit::window::Window;

use crate::scene;

pub fn choose_present_mode(supported: &[wgpu::PresentMode], uncapped: bool) -> wgpu::PresentMode {
    let preference: [wgpu::PresentMode; 3] = if uncapped {
        [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo]
    } else {
        [wgpu::PresentMode::Fifo, wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate]
    };
    for mode in preference {
        if supported.contains(&mode) {
            return mode;
        }
    }
    wgpu::PresentMode::Fifo
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    quad_pipeline: quad::QuadPipeline,
    text_layer: text::TextLayer,
    scale_factor: f32,
}

impl Renderer {
    pub fn new(window: Arc<Window>, scale_factor: f32, uncapped: bool) -> Option<Self> {
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
        let present_mode = choose_present_mode(&caps.present_modes, uncapped);

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

        let quad_pipeline = quad::QuadPipeline::new(&device, format);
        let text_layer = text::TextLayer::new(&device, &queue, format);

        Some(Self { surface, device, queue, config, quad_pipeline, text_layer, scale_factor })
    }

    pub fn width(&self) -> u32 {
        self.config.width
    }

    pub fn height(&self) -> u32 {
        self.config.height
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn set_scale_factor(&mut self, factor: f32) {
        self.scale_factor = factor;
    }

    pub fn render_frame(&mut self, frame: &scene::Frame) -> Result<(), wgpu::SurfaceError> {
        let instances = quad::build_quad_instances(frame, self.scale_factor);
        self.quad_pipeline.prepare(&self.device, &self.queue, &instances, self.config.width as f32, self.config.height as f32);

        let texts: Vec<scene::TextCommand> = frame.commands.iter().filter_map(|c| match c {
            scene::DrawCommand::Text(t) => Some(t.scaled(self.scale_factor)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_fifo_by_default_when_available() {
        let supported = [
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Fifo,
        ];
        assert_eq!(choose_present_mode(&supported, false), wgpu::PresentMode::Fifo);
    }

    #[test]
    fn falls_back_to_mailbox_when_no_fifo_and_capped() {
        let supported = [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox];
        assert_eq!(choose_present_mode(&supported, false), wgpu::PresentMode::Mailbox);
    }

    #[test]
    fn falls_back_to_immediate_when_only_immediate_supported_and_capped() {
        let supported = [wgpu::PresentMode::Immediate];
        assert_eq!(choose_present_mode(&supported, false), wgpu::PresentMode::Immediate);
    }

    #[test]
    fn prefers_immediate_when_uncapped() {
        let supported = [
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Immediate,
        ];
        assert_eq!(choose_present_mode(&supported, true), wgpu::PresentMode::Immediate);
    }
}
