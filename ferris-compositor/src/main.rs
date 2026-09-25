use ferris_compositor::{perf, renderer, scene};

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use renderer::Renderer;

use ferris_css::parser::Parser as CssParser;
use ferris_css::tokenizer::Tokenizer as CssTokenizer;
use ferris_layout::layout;
use ferris_paint::paint::paint;
use ferris_style::resolve_styles;

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Renderer>,
    frame_timer: perf::FrameTimer,
    last_frame_start: Option<std::time::Instant>,
    occluded: bool,
    page_frame: Option<scene::Frame>,
    source: Option<ferris_loader::Source>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            gpu: None,
            frame_timer: perf::FrameTimer::new(120),
            last_frame_start: None,
            occluded: false,
            page_frame: None,
            source: None,
        }
    }
}

fn build_page_frame(root: &ferris_dom::dom::Element, stylesheet: &ferris_css::stylesheet::Stylesheet, viewport_width: f32, viewport_height: f32) -> scene::Frame {
    let styled = resolve_styles(root, stylesheet);
    match layout::layout(&styled, viewport_width, viewport_height) {
        Some(layout_box) => paint(&layout_box),
        None => {
            log::warn!("page root has no visible layout (display:none); showing an empty frame");
            scene::Frame::default()
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
                let loaded = match &self.source {
                    Some(source) => match ferris_loader::load_page(source) {
                        Ok(page) => page,
                        Err(ferris_loader::LoadError::Fetch(msg)) => {
                            log::error!("failed to load page: {msg}");
                            std::process::exit(1);
                        }
                    },
                    None => {
                        let html = include_str!("../assets/fixture.html");
                        let css = include_str!("../assets/fixture.css");
                        let root = ferris_dom::parser::parse_document(html);
                        let css_tokens = CssTokenizer::tokenize(css);
                        let stylesheet = CssParser::parse(&css_tokens);
                        (root, stylesheet)
                    }
                };
                let (root, stylesheet) = loaded;

                self.page_frame = Some(build_page_frame(&root, &stylesheet, gpu.logical_width(), gpu.logical_height()));
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
                if !occluded {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let minimized = self.window.as_ref().and_then(|w| w.is_minimized()).unwrap_or(false);
                if self.occluded || minimized {
                    self.last_frame_start = None;
                    return;
                }

                let now = std::time::Instant::now();
                if let Some(last) = self.last_frame_start {
                    self.frame_timer.record(now - last);
                }
                self.last_frame_start = Some(now);

                let mut frame = self.page_frame.clone().unwrap_or_default();

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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let minimized = self.window.as_ref().and_then(|w| w.is_minimized()).unwrap_or(false);
        if self.occluded || minimized {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        event_loop.set_control_flow(ControlFlow::Poll);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();
    let source = std::env::args().nth(1).map(|arg| ferris_loader::parse_source(&arg));
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { source, ..App::default() };
    event_loop.run_app(&mut app).expect("event loop error");
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Review Focus: display:none page root must not panic ---
    #[test]
    fn build_page_frame_on_display_none_root_returns_empty_frame_without_panicking() {
        let root = ferris_dom::parser::parse_document("<html><body>oi</body></html>");
        let css_tokens = CssTokenizer::tokenize("html { display: none; }");
        let stylesheet = CssParser::parse(&css_tokens);

        let frame = build_page_frame(&root, &stylesheet, 800.0, 600.0);

        assert!(frame.commands.is_empty());
    }
}
