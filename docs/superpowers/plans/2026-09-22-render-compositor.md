# Ferris Render/Compositor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust window application that renders an animated scene of colored rounded rectangles and text via GPU at a sustained 120fps (frame time ≤ 8.3ms), with a live on-screen performance overlay, proving the viability of Ferris's own compositor before the HTML/CSS/DOM engine is built.

**Architecture:** `winit` owns the window and event loop. `wgpu` owns the GPU device/surface and a custom instanced-quad pipeline (with an inline WGSL SDF shader for rounded corners). `glyphon` renders all text (both the demo scene's label and the FPS overlay) on top of the quads in the same render pass. Every frame is rebuilt from scratch as a flat `Frame` of `DrawCommand`s (no retained state) — this keeps this milestone simple; a retained scene graph is a future sub-project.

**Tech Stack:** Rust 2021, `winit` 0.30 (`ApplicationHandler`), `wgpu` 22, `glyphon` 0.6, `bytemuck` 1.16, `pollster` 0.3, `log`/`env_logger`.

**Spec:** `docs/superpowers/specs/2026-09-22-render-compositor-design.md`

## Global Constraints

- Crate name: `ferris-compositor` (single binary crate, not a workspace).
- Target frame budget: **≤ 8.3ms average frame time (120fps)**, measured over a rolling window and validated over a continuous 60-second run (spec's success criterion — exact value, do not round to 60fps's 16.6ms).
- No DOM/HTML/CSS parsing in this sub-project. Scene content is code-generated (`build_test_scene`), never loaded from a file or network.
- No automated pixel-diff/screenshot testing in this sub-project (per spec, out of scope). Automated tests are for pure CPU-side logic only (scene math, perf math, instance batching, present-mode selection). GPU wiring tasks are verified manually by running the app.
- GPU/window failures must never panic/crash the process: adapter-not-found and surface-lost/outdated must be handled gracefully (log + fallback), per spec's error-handling section.

---

### Task 1: Project scaffold, present-mode selection, and a clearing window

**Files:**
- Create: `Cargo.toml`
- Create: `src/renderer/mod.rs`
- Create: `src/main.rs`
- Test: `src/renderer/mod.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Consumes: nothing (first task).
- Produces: `renderer::choose_present_mode(supported: &[wgpu::PresentMode]) -> wgpu::PresentMode`, used by Task 4/6 when reconfiguring the surface. `main.rs`'s `GpuState` struct (fields: `surface`, `device`, `queue`, `config`, `window`) and `App` (`ApplicationHandler`), extended in Tasks 4-7.

- [ ] **Step 1: Create the crate**

```bash
cd "C:\Users\User\Documents\Ferris"
cargo init --name ferris-compositor .
```

- [ ] **Step 2: Write `Cargo.toml` dependencies**

```toml
[package]
name = "ferris-compositor"
version = "0.1.0"
edition = "2021"

[dependencies]
winit = "0.30"
wgpu = "22"
glyphon = "0.6"
bytemuck = { version = "1.16", features = ["derive"] }
pollster = "0.3"
log = "0.4"
env_logger = "0.11"
```

- [ ] **Step 3: Write the failing test for present-mode selection**

Create `src/renderer/mod.rs`:

```rust
pub mod quad;
pub mod text;

pub fn choose_present_mode(supported: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_immediate_when_available() {
        let supported = [
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Immediate,
        ];
        assert_eq!(choose_present_mode(&supported), wgpu::PresentMode::Immediate);
    }

    #[test]
    fn falls_back_to_mailbox_when_no_immediate() {
        let supported = [wgpu::PresentMode::Fifo, wgpu::PresentMode::Mailbox];
        assert_eq!(choose_present_mode(&supported), wgpu::PresentMode::Mailbox);
    }

    #[test]
    fn falls_back_to_fifo_when_nothing_else_supported() {
        let supported = [wgpu::PresentMode::Fifo];
        assert_eq!(choose_present_mode(&supported), wgpu::PresentMode::Fifo);
    }
}
```

Note: `quad` and `text` modules don't exist yet — comment out those two `pub mod` lines for now, they're added in Tasks 4 and 5:

```rust
// pub mod quad;
// pub mod text;
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test --lib renderer::tests`
Expected: FAIL (panics with `not yet implemented` on the `Immediate` case, since `todo!()` always panics).

- [ ] **Step 5: Implement `choose_present_mode`**

Replace `todo!()`:

```rust
pub fn choose_present_mode(supported: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    const PREFERENCE: [wgpu::PresentMode; 3] = [
        wgpu::PresentMode::Immediate,
        wgpu::PresentMode::Mailbox,
        wgpu::PresentMode::Fifo,
    ];
    for mode in PREFERENCE {
        if supported.contains(&mode) {
            return mode;
        }
    }
    wgpu::PresentMode::Fifo
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib renderer::tests`
Expected: 3 passed.

- [ ] **Step 7: Write `src/main.rs`** — window + wgpu init + clear-color loop

```rust
mod renderer;

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

        Some(Self { surface, device, queue, config, window })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render_clear(&self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("clear_encoder"),
        });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
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
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
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
                match gpu.render_clear() {
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
```

- [ ] **Step 8: Build and manually verify**

Run: `cargo build`
Expected: compiles clean. If `wgpu`/`winit`/`glyphon` versions resolved by Cargo have API differences from the code above, fix call sites to match `cargo doc --open -p wgpu -p winit` for the installed versions — the shapes (struct/function names) will be very close, this is normal dependency-version drift, not a design change.

Run: `cargo run`
Expected: a window titled "Ferris" opens, clears to a dark blue-gray background, stays open, resizing the window doesn't crash, closing the window exits cleanly.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/renderer/mod.rs
git commit -m "feat: scaffold ferris-compositor with wgpu window and present-mode selection"
```

---

### Task 2: Frame timer (perf tracking)

**Files:**
- Create: `src/perf.rs`
- Modify: `src/main.rs` (add `mod perf;`)
- Test: `src/perf.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Consumes: nothing new.
- Produces: `perf::FrameTimer` with `new(max_samples: usize)`, `record(&mut self, frame_time: Duration)`, `average_frame_time(&self) -> Duration`, `fps(&self) -> f32`, `meets_budget(&self, budget: Duration) -> bool`; and `perf::BUDGET_120FPS: Duration`. Used by Task 6 to drive the on-screen overlay and by Task 8 for the final validation run.

- [ ] **Step 1: Write the failing tests**

Create `src/perf.rs`:

```rust
use std::collections::VecDeque;
use std::time::Duration;

pub const BUDGET_120FPS: Duration = Duration::from_micros(8333);

pub struct FrameTimer {
    samples: VecDeque<Duration>,
    max_samples: usize,
}

impl FrameTimer {
    pub fn new(max_samples: usize) -> Self {
        todo!()
    }

    pub fn record(&mut self, frame_time: Duration) {
        todo!()
    }

    pub fn average_frame_time(&self) -> Duration {
        todo!()
    }

    pub fn fps(&self) -> f32 {
        todo!()
    }

    pub fn meets_budget(&self, budget: Duration) -> bool {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_of_no_samples_is_zero() {
        let timer = FrameTimer::new(10);
        assert_eq!(timer.average_frame_time(), Duration::ZERO);
    }

    #[test]
    fn average_of_uniform_samples() {
        let mut timer = FrameTimer::new(10);
        for _ in 0..5 {
            timer.record(Duration::from_millis(8));
        }
        assert_eq!(timer.average_frame_time(), Duration::from_millis(8));
    }

    #[test]
    fn drops_oldest_sample_beyond_capacity() {
        let mut timer = FrameTimer::new(2);
        timer.record(Duration::from_millis(100)); // will be evicted
        timer.record(Duration::from_millis(8));
        timer.record(Duration::from_millis(8));
        assert_eq!(timer.average_frame_time(), Duration::from_millis(8));
    }

    #[test]
    fn fps_matches_average_frame_time() {
        let mut timer = FrameTimer::new(10);
        timer.record(Duration::from_millis(10)); // 100fps
        assert!((timer.fps() - 100.0).abs() < 0.5);
    }

    #[test]
    fn meets_budget_true_when_under() {
        let mut timer = FrameTimer::new(10);
        timer.record(Duration::from_micros(8000));
        assert!(timer.meets_budget(BUDGET_120FPS));
    }

    #[test]
    fn meets_budget_false_when_over() {
        let mut timer = FrameTimer::new(10);
        timer.record(Duration::from_millis(16));
        assert!(!timer.meets_budget(BUDGET_120FPS));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib perf::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement `FrameTimer`**

Replace the `todo!()` bodies:

```rust
impl FrameTimer {
    pub fn new(max_samples: usize) -> Self {
        Self { samples: VecDeque::with_capacity(max_samples), max_samples }
    }

    pub fn record(&mut self, frame_time: Duration) {
        if self.samples.len() == self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(frame_time);
    }

    pub fn average_frame_time(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.samples.iter().sum();
        total / self.samples.len() as u32
    }

    pub fn fps(&self) -> f32 {
        let avg = self.average_frame_time();
        if avg.as_secs_f32() <= 0.0 {
            0.0
        } else {
            1.0 / avg.as_secs_f32()
        }
    }

    pub fn meets_budget(&self, budget: Duration) -> bool {
        self.average_frame_time() <= budget
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib perf::tests`
Expected: 6 passed.

- [ ] **Step 5: Register the module**

In `src/main.rs`, add near the top:

```rust
mod perf;
```

- [ ] **Step 6: Commit**

```bash
git add src/perf.rs src/main.rs
git commit -m "feat: add FrameTimer for frame-time/FPS tracking"
```

---

### Task 3: Scene types and the animated test scene

**Files:**
- Create: `src/scene.rs`
- Modify: `src/main.rs` (add `mod scene;`)
- Test: `src/scene.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Consumes: nothing new.
- Produces: `scene::DrawCommand` (enum: `Rect(RectCommand)`, `Text(TextCommand)`), `scene::RectCommand { x, y, width, height, color: [f32;4], corner_radius: f32 }`, `scene::TextCommand { x, y, content: String, size: f32, color: [f32;4] }`, `scene::Frame { commands: Vec<DrawCommand> }` with `new()`/`push()`, `scene::bounce_position(elapsed_secs, speed, min, max, phase_offset) -> f32`, `scene::build_test_scene(elapsed_secs: f32, viewport_width: f32, viewport_height: f32) -> Frame`. Used by Task 4 (`build_quad_instances` reads `Frame.commands`), Task 5 (text extraction), Task 6 (main loop calls `build_test_scene` every frame).

- [ ] **Step 1: Write the failing tests**

Create `src/scene.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectCommand {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub corner_radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextCommand {
    pub x: f32,
    pub y: f32,
    pub content: String,
    pub size: f32,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect(RectCommand),
    Text(TextCommand),
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub commands: Vec<DrawCommand>,
}

impl Frame {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn push(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }
}

pub fn bounce_position(elapsed_secs: f32, speed: f32, min: f32, max: f32, phase_offset: f32) -> f32 {
    todo!()
}

pub fn build_test_scene(elapsed_secs: f32, viewport_width: f32, viewport_height: f32) -> Frame {
    todo!()
}

fn hsv_to_rgba(hue: f32) -> [f32; 4] {
    let h = hue * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as i32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [r, g, b, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_starts_at_min_with_zero_phase() {
        let pos = bounce_position(0.0, 100.0, 0.0, 200.0, 0.0);
        assert_eq!(pos, 0.0);
    }

    #[test]
    fn bounce_moves_toward_max_before_half_period() {
        let pos = bounce_position(0.5, 100.0, 0.0, 200.0, 0.0);
        assert_eq!(pos, 50.0);
    }

    #[test]
    fn bounce_reflects_after_reaching_max() {
        // half_period = range / speed = 200 / 100 = 2.0s
        let at_max = bounce_position(2.0, 100.0, 0.0, 200.0, 0.0);
        assert!((at_max - 200.0).abs() < 0.01);
        let past_max = bounce_position(2.5, 100.0, 0.0, 200.0, 0.0);
        assert!((past_max - 150.0).abs() < 0.01);
    }

    #[test]
    fn bounce_stays_within_bounds_over_time() {
        for i in 0..1000 {
            let t = i as f32 * 0.037;
            let pos = bounce_position(t, 137.0, 10.0, 90.0, 1.7);
            assert!(pos >= 10.0 - 0.01 && pos <= 90.0 + 0.01, "pos {pos} out of bounds at t={t}");
        }
    }

    #[test]
    fn test_scene_has_twenty_rects_and_one_label() {
        let frame = build_test_scene(0.0, 1280.0, 720.0);
        let rect_count = frame.commands.iter().filter(|c| matches!(c, DrawCommand::Rect(_))).count();
        let text_count = frame.commands.iter().filter(|c| matches!(c, DrawCommand::Text(_))).count();
        assert_eq!(rect_count, 20);
        assert_eq!(text_count, 1);
    }

    #[test]
    fn test_scene_rects_stay_within_viewport() {
        let frame = build_test_scene(3.7, 1280.0, 720.0);
        for cmd in &frame.commands {
            if let DrawCommand::Rect(r) = cmd {
                assert!(r.x >= -0.01 && r.x + r.width <= 1280.0 + 0.01);
                assert!(r.y >= 0.0 && r.y + r.height <= 720.0);
            }
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib scene::tests`
Expected: FAIL (panics on `todo!()` in `bounce_position`/`build_test_scene`).

- [ ] **Step 3: Implement `bounce_position` and `build_test_scene`**

```rust
pub fn bounce_position(elapsed_secs: f32, speed: f32, min: f32, max: f32, phase_offset: f32) -> f32 {
    let range = max - min;
    if range <= 0.0 || speed <= 0.0 {
        return min;
    }
    let half_period = range / speed;
    let period = 2.0 * half_period;
    let t = (elapsed_secs + phase_offset).rem_euclid(period);
    if t < half_period {
        min + t * speed
    } else {
        max - (t - half_period) * speed
    }
}

pub fn build_test_scene(elapsed_secs: f32, viewport_width: f32, viewport_height: f32) -> Frame {
    let mut frame = Frame::new();
    let rect_size = 40.0;
    let count = 20;
    for i in 0..count {
        let phase_offset = i as f32 * 0.35;
        let speed = 120.0 + (i as f32 * 8.0);
        let x = bounce_position(elapsed_secs, speed, 0.0, viewport_width - rect_size, phase_offset);
        let y = 80.0 + (i as f32 * (viewport_height - 160.0) / count as f32);
        let hue = i as f32 / count as f32;
        frame.push(DrawCommand::Rect(RectCommand {
            x,
            y,
            width: rect_size,
            height: rect_size,
            color: hsv_to_rgba(hue),
            corner_radius: 8.0,
        }));
    }
    frame.push(DrawCommand::Text(TextCommand {
        x: 20.0,
        y: 20.0,
        content: "Ferris compositor".to_string(),
        size: 24.0,
        color: [1.0, 1.0, 1.0, 1.0],
    }));
    frame
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib scene::tests`
Expected: 6 passed.

- [ ] **Step 5: Register the module**

In `src/main.rs`, add near the top:

```rust
mod scene;
```

- [ ] **Step 6: Commit**

```bash
git add src/scene.rs src/main.rs
git commit -m "feat: add scene types and animated test-scene generator"
```

---

### Task 4: Quad render pipeline (rounded rectangles)

**Files:**
- Create: `src/renderer/quad.rs`
- Modify: `src/renderer/mod.rs` (uncomment `pub mod quad;`)
- Modify: `src/main.rs` (`GpuState` gains `quad_pipeline`, render draws a static 3-rect frame)
- Test: `src/renderer/quad.rs` (inline `#[cfg(test)]` module, CPU-side batching only)

**Interfaces:**
- Consumes: `scene::{Frame, DrawCommand, RectCommand}` from Task 3.
- Produces: `renderer::quad::QuadInstance` (`#[repr(C)]`, `Pod`, `Zeroable`: `position: [f32;2]`, `size: [f32;2]`, `color: [f32;4]`, `corner_radius: f32`), `renderer::quad::build_quad_instances(frame: &scene::Frame) -> Vec<QuadInstance>`, `renderer::quad::QuadPipeline` with `new(device, format) -> Self`, `prepare(&mut self, device, queue, instances: &[QuadInstance], viewport_width: f32, viewport_height: f32)`, `render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>, instance_count: u32)`. Used by Task 6's main render loop and Task 8's validation run.

- [ ] **Step 1: Write the failing test for the pure batching function**

Create `src/renderer/quad.rs`:

```rust
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::scene::{DrawCommand, Frame};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct QuadInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub corner_radius: f32,
}

pub fn build_quad_instances(frame: &Frame) -> Vec<QuadInstance> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{RectCommand, TextCommand};

    #[test]
    fn extracts_only_rect_commands_in_order() {
        let mut frame = Frame::new();
        frame.push(DrawCommand::Rect(RectCommand {
            x: 1.0, y: 2.0, width: 10.0, height: 20.0, color: [1.0, 0.0, 0.0, 1.0], corner_radius: 4.0,
        }));
        frame.push(DrawCommand::Text(TextCommand {
            x: 0.0, y: 0.0, content: "ignored".into(), size: 12.0, color: [1.0, 1.0, 1.0, 1.0],
        }));
        frame.push(DrawCommand::Rect(RectCommand {
            x: 5.0, y: 6.0, width: 30.0, height: 40.0, color: [0.0, 1.0, 0.0, 1.0], corner_radius: 0.0,
        }));

        let instances = build_quad_instances(&frame);
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].position, [1.0, 2.0]);
        assert_eq!(instances[0].size, [10.0, 20.0]);
        assert_eq!(instances[0].corner_radius, 4.0);
        assert_eq!(instances[1].position, [5.0, 6.0]);
    }

    #[test]
    fn empty_frame_yields_no_instances() {
        let frame = Frame::new();
        assert!(build_quad_instances(&frame).is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib renderer::quad::tests`
Expected: FAIL (`todo!()` panic).

- [ ] **Step 3: Implement `build_quad_instances`**

```rust
pub fn build_quad_instances(frame: &Frame) -> Vec<QuadInstance> {
    frame
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::Rect(r) => Some(QuadInstance {
                position: [r.x, r.y],
                size: [r.width, r.height],
                color: r.color,
                corner_radius: r.corner_radius,
            }),
            DrawCommand::Text(_) => None,
        })
        .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib renderer::quad::tests`
Expected: 2 passed.

- [ ] **Step 5: Add the GPU pipeline (shader, buffers, `QuadPipeline`)**

Append to `src/renderer/quad.rs`, above the `#[cfg(test)]` block:

```rust
const QUAD_VERTICES: &[[f32; 2]] = &[
    [0.0, 0.0], [1.0, 0.0], [0.0, 1.0],
    [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
];

const QUAD_SHADER: &str = r#"
struct Globals {
    viewport_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct VertexInput {
    @location(0) corner: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) corner_radius: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) corner_radius: f32,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let pixel_pos = instance.position + vertex.corner * instance.size;
    let ndc_x = (pixel_pos.x / globals.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / globals.viewport_size.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = instance.color;
    out.half_size = instance.size * 0.5;
    out.local_pos = (vertex.corner - vec2<f32>(0.5, 0.5)) * instance.size;
    out.corner_radius = instance.corner_radius;
    return out;
}

fn rounded_rect_sdf(local_pos: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(local_pos) - half_size + vec2<f32>(radius, radius);
    return length(max(q, vec2<f32>(0.0, 0.0))) - radius;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = rounded_rect_sdf(in.local_pos, in.half_size, in.corner_radius);
    let alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

pub struct QuadPipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
}

impl QuadPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad_shader"),
            source: wgpu::ShaderSource::Wgsl(QUAD_SHADER.into()),
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_vertex_buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let instance_capacity = 256usize;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad_instance_buffer"),
            size: (instance_capacity * std::mem::size_of::<QuadInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad_globals_buffer"),
            size: std::mem::size_of::<[f32; 2]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad_globals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad_globals_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        };

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 16, shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 32, shader_location: 4, format: wgpu::VertexFormat::Float32 },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_layout, instance_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self { pipeline, vertex_buffer, instance_buffer, instance_capacity, globals_buffer, globals_bind_group }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[QuadInstance],
        viewport_width: f32,
        viewport_height: f32,
    ) {
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[viewport_width, viewport_height]));

        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("quad_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<QuadInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>, instance_count: u32) {
        if instance_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..instance_count);
    }
}
```

- [ ] **Step 6: Uncomment the module declaration**

In `src/renderer/mod.rs`, uncomment:

```rust
pub mod quad;
```

- [ ] **Step 7: Wire `QuadPipeline` into `GpuState` and draw a static test frame**

In `src/main.rs`:
- Add `mod scene;` near the top (if not already present from Task 3).
- Add field `quad_pipeline: renderer::quad::QuadPipeline` to `GpuState`.
- In `GpuState::new`, after `surface.configure(...)`, add: `let quad_pipeline = renderer::quad::QuadPipeline::new(&device, format);` and include it in the returned `Self { ..., quad_pipeline }`.
- Replace `render_clear` with `render_frame`:

```rust
fn render_frame(&mut self, frame: &scene::Frame) -> Result<(), wgpu::SurfaceError> {
    let instances = renderer::quad::build_quad_instances(frame);
    self.quad_pipeline.prepare(&self.device, &self.queue, &instances, self.config.width as f32, self.config.height as f32);

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
    }
    self.queue.submit(std::iter::once(encoder.finish()));
    output.present();
    Ok(())
}
```

- In `App::window_event`'s `WindowEvent::RedrawRequested` arm, replace the `gpu.render_clear()` call with a static 3-rect test frame for now (animation comes in Task 6):

```rust
WindowEvent::RedrawRequested => {
    let mut frame = scene::Frame::new();
    frame.push(scene::DrawCommand::Rect(scene::RectCommand {
        x: 100.0, y: 100.0, width: 120.0, height: 80.0,
        color: [0.9, 0.2, 0.2, 1.0], corner_radius: 12.0,
    }));
    frame.push(scene::DrawCommand::Rect(scene::RectCommand {
        x: 260.0, y: 160.0, width: 90.0, height: 90.0,
        color: [0.2, 0.7, 0.3, 1.0], corner_radius: 45.0,
    }));
    frame.push(scene::DrawCommand::Rect(scene::RectCommand {
        x: 400.0, y: 100.0, width: 150.0, height: 60.0,
        color: [0.2, 0.4, 0.9, 1.0], corner_radius: 4.0,
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
```

- [ ] **Step 8: Build and manually verify**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo run`
Expected: window shows 3 rectangles — a red one with visibly rounded corners, a green circle (radius = half of width/height), a blue rectangle with slightly rounded corners — on the dark background, static (not moving yet).

- [ ] **Step 9: Commit**

```bash
git add src/renderer/quad.rs src/renderer/mod.rs src/main.rs
git commit -m "feat: add instanced quad pipeline with rounded-rect SDF shader"
```

---

### Task 5: Text rendering via glyphon

**Files:**
- Create: `src/renderer/text.rs`
- Modify: `src/renderer/mod.rs` (uncomment `pub mod text;`)
- Modify: `src/main.rs` (`GpuState` gains `text_layer`, renders the test frame's `TextCommand`)

**Interfaces:**
- Consumes: `scene::TextCommand` from Task 3.
- Produces: `renderer::text::TextLayer` with `new(device, queue, format) -> Self`, `prepare(&mut self, device, queue, width: u32, height: u32, texts: &[TextCommand])`, `render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>)`. Used by Task 6's main loop for both the demo label and the FPS overlay.

- [ ] **Step 1: Write `src/renderer/text.rs`**

No pure-logic unit tests here — glyphon's `FontSystem`/`TextRenderer` require a live `wgpu::Device`, so this is verified manually per the spec (no automated pixel tests in this sub-project).

```rust
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

use crate::scene::TextCommand;

pub struct TextLayer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    buffers: Vec<GlyphonBuffer>,
}

impl TextLayer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        Self { font_system, swash_cache, viewport, atlas, text_renderer, buffers: Vec::new() }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        texts: &[TextCommand],
    ) {
        self.buffers.clear();
        for cmd in texts {
            let mut buffer = GlyphonBuffer::new(&mut self.font_system, Metrics::new(cmd.size, cmd.size * 1.2));
            buffer.set_text(&mut self.font_system, &cmd.content, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
            buffer.set_size(&mut self.font_system, Some(width as f32), Some(height as f32));
            self.buffers.push(buffer);
        }

        self.viewport.update(queue, Resolution { width, height });

        let text_areas: Vec<TextArea> = self
            .buffers
            .iter()
            .zip(texts.iter())
            .map(|(buffer, cmd)| TextArea {
                buffer,
                left: cmd.x,
                top: cmd.y,
                scale: 1.0,
                bounds: TextBounds { left: 0, top: 0, right: width as i32, bottom: height as i32 },
                default_color: glyphon::Color::rgba(
                    (cmd.color[0] * 255.0) as u8,
                    (cmd.color[1] * 255.0) as u8,
                    (cmd.color[2] * 255.0) as u8,
                    (cmd.color[3] * 255.0) as u8,
                ),
                custom_glyphs: &[],
            })
            .collect();

        self.text_renderer
            .prepare(device, queue, &mut self.font_system, &mut self.atlas, &self.viewport, text_areas, &mut self.swash_cache)
            .expect("glyphon prepare failed");
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>) {
        self.text_renderer.render(&self.atlas, &self.viewport, render_pass).expect("glyphon render failed");
    }

    pub fn trim_atlas(&mut self) {
        self.atlas.trim();
    }
}
```

- [ ] **Step 2: Uncomment the module declaration**

In `src/renderer/mod.rs`, uncomment:

```rust
pub mod text;
```

- [ ] **Step 3: Wire `TextLayer` into `GpuState` and render the frame's text commands**

In `src/main.rs`:
- Add field `text_layer: renderer::text::TextLayer` to `GpuState`.
- In `GpuState::new`, after creating `quad_pipeline`, add: `let text_layer = renderer::text::TextLayer::new(&device, &queue, format);` and include it in `Self { ..., text_layer }`.
- In `render_frame`, before building the render pass, extract texts and prepare the text layer:

```rust
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
```

- In the `WindowEvent::RedrawRequested` handler in `App`, add a `scene::DrawCommand::Text` to the static test frame:

```rust
frame.push(scene::DrawCommand::Text(scene::TextCommand {
    x: 20.0, y: 20.0, content: "Ferris compositor".to_string(), size: 24.0, color: [1.0, 1.0, 1.0, 1.0],
}));
```

- [ ] **Step 4: Build and manually verify**

Run: `cargo build`
Expected: compiles clean. If the installed `glyphon` version's API differs (struct/field names for `TextArea`, `Cache`, `Viewport`, etc.), consult `cargo doc --open -p glyphon` for the resolved version and adjust call sites — the overall flow (FontSystem → Buffer → TextRenderer::prepare → render) is stable across recent glyphon releases.

Run: `cargo run`
Expected: the same 3 rectangles from Task 4, plus the text "Ferris compositor" rendered in white in the top-left corner, legible and correctly positioned.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/text.rs src/renderer/mod.rs src/main.rs
git commit -m "feat: render text via glyphon"
```

---

### Task 6: Wire the animated test scene, clock, and FPS overlay

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `perf::FrameTimer`/`BUDGET_120FPS` (Task 2), `scene::build_test_scene` (Task 3), `GpuState::render_frame` (Tasks 4-5).
- Produces: the complete running app — no further public interfaces consumed by later tasks except the overall binary behavior (Task 8 validates it as-is).

- [ ] **Step 1: Add a start `Instant` and `FrameTimer` to `App`**

In `src/main.rs`, add `mod perf;` near the top if not already present (from Task 2), and update the `App` struct:

```rust
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
```

- [ ] **Step 2: Replace the static test frame with the animated scene plus an FPS overlay**

Replace the body of the `WindowEvent::RedrawRequested` arm in `App::window_event`:

```rust
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
```

Note: `gpu` is already borrowed via `let Some(gpu) = self.gpu.as_mut() else { return };` at the top of `window_event` — `self.frame_timer`/`self.start`/`self.last_frame_start` are separate fields so this doesn't conflict with the existing borrow.

- [ ] **Step 3: Build and manually verify**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo run`
Expected: 20 colored rounded rectangles bouncing horizontally at different speeds/phases, the "Ferris compositor" label, and below it a live-updating line like `118.3 fps | 8.45 ms/frame | budget MISSED` (or `OK`) that changes every frame.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: drive animated test scene and live FPS overlay from FrameTimer"
```

---

### Task 7: Graceful failure handling under stress

**Files:**
- Modify: `src/main.rs` (already partially handled in Task 1/6 — this task verifies and hardens it)

**Interfaces:**
- Consumes: existing `GpuState`/`App` from Tasks 1-6.
- Produces: no new public interface — hardens existing error paths per spec.

- [ ] **Step 1: Confirm adapter/device failures already degrade gracefully**

Review `GpuState::new` (Task 1): it must return `Option<Self>`, never call `.expect()`/`.unwrap()` on `request_adapter`/`request_device`/`create_surface`. If any `.expect()`/`.unwrap()` crept in during Tasks 4-6 (e.g. added near `quad_pipeline`/`text_layer` construction), remove it — those constructors (`QuadPipeline::new`, `TextLayer::new`) don't return `Result` and are expected to succeed once the device exists, so no change needed there; the guard is specifically around adapter/device acquisition in `GpuState::new`.

- [ ] **Step 2: Confirm `resumed()` exits cleanly instead of panicking on GPU init failure**

Review `App::resumed` (Task 1): on `GpuState::new(...) == None`, it must `log::error!` and `event_loop.exit()`, not panic. This is already written this way in Task 1 — verify it's still intact.

- [ ] **Step 3: Stress-test the resize/surface-lost path manually**

Run: `cargo run`
While running: rapidly resize the window by dragging its edge for 10-15 seconds, then minimize and restore it 5+ times, then resize to 1x1 pixel (drag corner to the smallest possible size) and back to a normal size.
Expected: no crash, no panic in the terminal, no frozen window. The animated scene and FPS overlay resume correctly after each resize/minimize/restore cycle. `wgpu::SurfaceError::Lost`/`Outdated` cases (already handled in the `RedrawRequested` arm since Task 1/4) are what make this safe — if a panic occurs here, add the missing `Err(...)` arm that reconfigures the surface via `gpu.resize(gpu.config.width, gpu.config.height)`.

- [ ] **Step 4: Commit (only if Step 1 or 3 required code changes)**

```bash
git add src/main.rs
git commit -m "fix: harden GPU init and surface-lost handling against crashes"
```

If no changes were needed (everything already held up), skip the commit — there's nothing to commit.

---

### Task 8: 60-second frame-budget validation (spec success criterion)

**Files:**
- Create: `docs/superpowers/plans/2026-09-22-render-compositor-results.md`

**Interfaces:**
- Consumes: the complete app from Tasks 1-7.
- Produces: a recorded validation result closing out this sub-project's spec success criterion.

- [ ] **Step 1: Build the release binary**

Run: `cargo build --release`
Expected: compiles clean (release profile catches any debug-only assumptions).

- [ ] **Step 2: Run the 60-second validation**

Run: `cargo run --release`
Let it run undisturbed for 60 continuous seconds with the window focused and unobstructed. Watch the FPS overlay throughout. Note the overlay's `ms/frame` and `fps` values at 10-second intervals (roughly 6 readings), and whether it reads `OK` or `MISSED` against the 8.3ms budget.

- [ ] **Step 3: Record the result**

Create `docs/superpowers/plans/2026-09-22-render-compositor-results.md`:

```markdown
# Render/Compositor Validation Result

Date: <fill in actual date run>
Hardware: <fill in actual GPU/CPU used for the test>

## 60-second run

| t (s) | fps | ms/frame | budget |
|-------|-----|----------|--------|
| 10    |     |          |        |
| 20    |     |          |        |
| 30    |     |          |        |
| 40    |     |          |        |
| 50    |     |          |        |
| 60    |     |          |        |

## Verdict

<PASS if average ms/frame across the run was <= 8.3ms (spec's success
criterion), otherwise FAIL with a note on what the bottleneck looked
like (CPU-bound scene build, GPU-bound fill rate, vsync cap, etc.) and
what sub-project 1 should revisit before sub-project 2 begins.>
```

Fill in the table and verdict with the actual numbers observed in Step 2 — do not leave placeholder rows.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/plans/2026-09-22-render-compositor-results.md
git commit -m "docs: record 120fps validation result for render/compositor sub-project"
```
