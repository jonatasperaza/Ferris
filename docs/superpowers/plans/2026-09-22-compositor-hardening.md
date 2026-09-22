# Ferris Compositor Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `ferris-compositor` from a bin-only crate into a lib+bin crate with the GPU/window code properly located in `renderer/`, a logical-pixel coordinate contract with HiDPI scaling applied only at the render boundary, and a render loop that defaults to vsync instead of an uncapped spin loop — closing the 4 findings from sub-project 1's final review that actually block sub-project 2 (the HTML/CSS/DOM engine).

**Architecture:** `src/lib.rs` exposes `scene`, `perf`, and `renderer` as public modules. `GpuState` (currently in `main.rs`) becomes `renderer::Renderer`, dropping its redundant `Arc<Window>` field. `scene::RectCommand`/`TextCommand` gain a `.scaled(factor)` method; `Renderer` stores the window's `scale_factor` and applies it once, at the render boundary, via that method — `scene.rs` itself never touches DPI. `renderer::choose_present_mode` gains an `uncapped: bool` parameter that flips its preference order between vsync-first (default) and immediate-first (opt-in via `FERRIS_UNCAPPED=1`).

**Tech Stack:** Rust 2021, `winit` 0.30, `wgpu` 22, `glyphon` 0.6 (unchanged from sub-project 1 — no new dependencies).

**Spec:** `docs/superpowers/specs/2026-09-22-compositor-hardening-design.md`

## Global Constraints

- Scope is exactly the 4 items the spec calls blocking: lib.rs export, wgpu setup relocated to `renderer/`, logical-pixel HiDPI contract, vsync-by-default frame pacing. The 13 parked findings (device-lost recovery, frame-time histogram, adapter/refresh-rate logging, reentrancy guards, per-frame allocation reduction, uniform buffer padding, SDF radius clamp, extra tests, repo cleanup, CI) are explicitly out of scope — do not fix them in this plan even if a task happens to touch nearby code.
- `RectCommand`/`TextCommand` coordinates are logical pixels (CSS-like); only `Renderer` (the render boundary) ever multiplies by `scale_factor`. `scene.rs` must never read a scale factor or a `Window`.
- No behavior regression: the existing 17 tests from sub-project 1 must keep passing throughout (adjusted for renamed/relocated code, never deleted).
- No new `.unwrap()`/`.expect()` on GPU adapter/device/surface acquisition — this guarantee from sub-project 1 must survive the `GpuState` → `Renderer` move unchanged.

---

### Task 1: `src/lib.rs` and wire `main.rs` to consume it

**Files:**
- Create: `src/lib.rs`
- Modify: `src/main.rs` (top-of-file module declarations only)

**Interfaces:**
- Consumes: nothing new — `perf`, `scene`, `renderer` modules already exist from sub-project 1, unchanged in this task.
- Produces: the crate now has both a lib target (`ferris_compositor`) and a bin target (`ferris-compositor`), so `cargo test --lib` becomes valid (it errored on the bin-only crate before this task) and later tasks' pure-logic tests can be run that way if desired.

- [ ] **Step 1: Create `src/lib.rs`**

```rust
pub mod perf;
pub mod renderer;
pub mod scene;
```

- [ ] **Step 2: Update `src/main.rs`'s module declarations**

Replace the top of `src/main.rs`:

```rust
mod perf;
mod scene;
mod renderer;

use std::sync::Arc;
```

with:

```rust
use ferris_compositor::{perf, renderer, scene};

use std::sync::Arc;
```

Leave every other line in `src/main.rs` untouched for this task (the `use renderer::choose_present_mode;` line a few lines below keeps working unchanged, since `renderer` now resolves through the `use ferris_compositor::{...}` import instead of a local `mod` declaration).

- [ ] **Step 3: Build and verify no regressions**

Run: `cargo build`
Expected: compiles clean. Cargo auto-detects both `src/lib.rs` and `src/main.rs` and builds a lib target (`ferris_compositor`) plus a bin target (`ferris-compositor`) from the same package with no `Cargo.toml` changes needed — if `cargo build` reports an ambiguity about two targets, check `Cargo.toml` for a `[lib]`/`[[bin]]` section conflict, but this should not be necessary.

Run: `cargo test`
Expected: all 17 pre-existing tests still pass (same as sub-project 1's final count), no new test failures.

Run: `cargo test --lib`
Expected: now succeeds (this specific command errored with "no library targets found in package" before this task, per sub-project 1's final review finding — this run is your proof the fix landed).

Run: `cargo run`
Expected: window opens, renders the same as before (unchanged behavior) — this task is a pure reorganization, verify visually that nothing broke.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs src/main.rs
git commit -m "feat: add lib target, expose perf/scene/renderer as public modules"
```

---

### Task 2: Move `GpuState` into `renderer::Renderer`, drop the duplicate `Arc<Window>`

**Files:**
- Modify: `src/renderer/mod.rs` (gains the `Renderer` struct and its `impl`)
- Modify: `src/main.rs` (drops `GpuState`, uses `renderer::Renderer` instead)

**Interfaces:**
- Consumes: `renderer::choose_present_mode(supported: &[wgpu::PresentMode]) -> wgpu::PresentMode` (unchanged signature in this task — Task 4 adds a parameter to it later), `renderer::quad::{QuadPipeline, build_quad_instances}`, `renderer::text::TextLayer` (all unchanged from sub-project 1).
- Produces: `renderer::Renderer` with `new(window: Arc<Window>) -> Option<Self>`, `resize(&mut self, width: u32, height: u32)`, `render_frame(&mut self, frame: &scene::Frame) -> Result<(), wgpu::SurfaceError>`, `width(&self) -> u32`, `height(&self) -> u32`. Task 3 adds a `scale_factor` parameter to `new` and a `set_scale_factor` method; Task 4 adds an `uncapped` parameter to `new`.

- [ ] **Step 1: Replace `src/renderer/mod.rs`'s top (before the `#[cfg(test)]` block) with the `Renderer` struct**

Replace everything in `src/renderer/mod.rs` from the top down to (but not including) the `#[cfg(test)]` line with:

```rust
pub mod quad;
pub mod text;

use std::sync::Arc;

use winit::window::Window;

use crate::scene;

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

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    quad_pipeline: quad::QuadPipeline,
    text_layer: text::TextLayer,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Option<Self> {
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

        let quad_pipeline = quad::QuadPipeline::new(&device, format);
        let text_layer = text::TextLayer::new(&device, &queue, format);

        Some(Self { surface, device, queue, config, quad_pipeline, text_layer })
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

    pub fn render_frame(&mut self, frame: &scene::Frame) -> Result<(), wgpu::SurfaceError> {
        let instances = quad::build_quad_instances(frame);
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
```

Note: this is the same logic as sub-project 1's `GpuState`, renamed to `Renderer`, with the `window: Arc<Window>` field removed (the `Arc<Window>` passed into `new` is still consumed by `instance.create_surface(window.clone())` — wgpu's `Surface` keeps its own internal reference to keep the window alive for as long as the surface exists, so dropping our own extra field does not invalidate the surface) and `width()`/`height()` accessor methods added in place of the previous direct `gpu.config.width`/`gpu.config.height` field access from `main.rs`.

- [ ] **Step 2: Replace `src/main.rs` to use `renderer::Renderer` instead of the local `GpuState`**

Replace the entire contents of `src/main.rs` with:

```rust
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
        match Renderer::new(window.clone()) {
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
                if let Some(window) = &self.window {
                    window.request_redraw();
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
```

Changes from sub-project 1's `main.rs`: `GpuState` is gone (replaced by `renderer::Renderer`), the `window` field is dropped from what `gpu` exposes (`gpu.window.request_redraw()` becomes `self.window`'s own `request_redraw()` call, moved to right after the `render_frame` match instead of being read off `gpu`), and `gpu.config.width`/`gpu.config.height` become `gpu.width()`/`gpu.height()`.

The `let Some(gpu) = self.gpu.as_mut() else { return };` line borrows only the `self.gpu` field (not all of `self`) — this is the same disjoint-field-borrow pattern sub-project 1's Task 6 already relied on (`self.frame_timer`/`self.start`/`self.last_frame_start` were read alongside an active `gpu` borrow there too), so reading `self.window` later in the same match arm compiles the same way. If `cargo build` disagrees, this is the one spot to look at first.

- [ ] **Step 3: Build and verify no regressions**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo test`
Expected: all 17 pre-existing tests still pass — this task touches no test code, only moves/renames production code.

Run: `cargo run`
Expected: window opens, renders the same animated scene + overlay as before (unchanged behavior). Confirm resize still works (drag the window edge) and close still works (click the X) — this task rewrote the render/resize/close plumbing, so a quick manual check here is worth it even though the logic is unchanged line-for-line.

- [ ] **Step 4: Commit**

```bash
git add src/renderer/mod.rs src/main.rs
git commit -m "refactor: move GpuState into renderer::Renderer, drop duplicate Arc<Window>"
```

---

### Task 3: HiDPI coordinate contract — logical pixels in `scene.rs`, scaling only in the renderer

**Files:**
- Modify: `src/scene.rs` (add `RectCommand::scaled`/`TextCommand::scaled`)
- Modify: `src/renderer/quad.rs` (`build_quad_instances` gains a `scale_factor` parameter)
- Modify: `src/renderer/mod.rs` (`Renderer` gains a `scale_factor` field, `new` gains a parameter, add `set_scale_factor`, `render_frame` applies scaling)
- Modify: `src/main.rs` (read `window.scale_factor()`, pass it to `Renderer::new`, handle `WindowEvent::ScaleFactorChanged`)
- Test: `src/scene.rs`, `src/renderer/quad.rs` (inline `#[cfg(test)]` modules)

**Interfaces:**
- Consumes: `scene::{RectCommand, TextCommand}` (from sub-project 1), `renderer::Renderer` (from Task 2 of this plan).
- Produces: `scene::RectCommand::scaled(&self, factor: f32) -> RectCommand`, `scene::TextCommand::scaled(&self, factor: f32) -> TextCommand`, `renderer::quad::build_quad_instances(frame: &Frame, scale_factor: f32) -> Vec<QuadInstance>` (signature changed — added `scale_factor`), `renderer::Renderer::new(window: Arc<Window>, scale_factor: f32) -> Option<Self>` (signature changed — added `scale_factor`), `renderer::Renderer::set_scale_factor(&mut self, factor: f32)`. Task 4 adds one more parameter to `Renderer::new` (`uncapped: bool`) on top of this task's `scale_factor` parameter.

- [ ] **Step 1: Write the failing tests for `RectCommand::scaled`/`TextCommand::scaled`**

In `src/scene.rs`, add (just after the `TextCommand` struct definition, before the `DrawCommand` enum):

```rust
impl RectCommand {
    pub fn scaled(&self, factor: f32) -> RectCommand {
        todo!()
    }
}

impl TextCommand {
    pub fn scaled(&self, factor: f32) -> TextCommand {
        todo!()
    }
}
```

Add these tests inside the existing `#[cfg(test)] mod tests { ... }` block at the bottom of `src/scene.rs` (alongside the existing 6 tests, don't remove any of them):

```rust
#[test]
fn rect_scaled_multiplies_position_size_and_radius() {
    let rect = RectCommand { x: 10.0, y: 20.0, width: 30.0, height: 40.0, color: [1.0, 0.0, 0.0, 1.0], corner_radius: 5.0 };
    let scaled = rect.scaled(2.0);
    assert_eq!(scaled.x, 20.0);
    assert_eq!(scaled.y, 40.0);
    assert_eq!(scaled.width, 60.0);
    assert_eq!(scaled.height, 80.0);
    assert_eq!(scaled.corner_radius, 10.0);
    assert_eq!(scaled.color, [1.0, 0.0, 0.0, 1.0], "color must not be scaled");
}

#[test]
fn rect_scaled_by_one_is_identity() {
    let rect = RectCommand { x: 10.0, y: 20.0, width: 30.0, height: 40.0, color: [0.5, 0.5, 0.5, 1.0], corner_radius: 5.0 };
    assert_eq!(rect.scaled(1.0), rect);
}

#[test]
fn text_scaled_multiplies_position_and_size_not_content_or_color() {
    let text = TextCommand { x: 10.0, y: 20.0, content: "hi".to_string(), size: 16.0, color: [1.0, 1.0, 1.0, 1.0] };
    let scaled = text.scaled(1.5);
    assert_eq!(scaled.x, 15.0);
    assert_eq!(scaled.y, 30.0);
    assert_eq!(scaled.size, 24.0);
    assert_eq!(scaled.content, "hi");
    assert_eq!(scaled.color, [1.0, 1.0, 1.0, 1.0]);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib scene::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement `scaled`**

```rust
impl RectCommand {
    pub fn scaled(&self, factor: f32) -> RectCommand {
        RectCommand {
            x: self.x * factor,
            y: self.y * factor,
            width: self.width * factor,
            height: self.height * factor,
            color: self.color,
            corner_radius: self.corner_radius * factor,
        }
    }
}

impl TextCommand {
    pub fn scaled(&self, factor: f32) -> TextCommand {
        TextCommand {
            x: self.x * factor,
            y: self.y * factor,
            content: self.content.clone(),
            size: self.size * factor,
            color: self.color,
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib scene::tests`
Expected: 9 passed (6 pre-existing + 3 new).

- [ ] **Step 5: Write the failing test for `build_quad_instances`'s new `scale_factor` parameter**

In `src/renderer/quad.rs`, change the function signature (the existing implementation body needs updating too — do both in this step since the old signature won't compile once the test calls the new one):

```rust
pub fn build_quad_instances(frame: &Frame, scale_factor: f32) -> Vec<QuadInstance> {
    todo!()
}
```

Update the two existing tests in `src/renderer/quad.rs`'s `#[cfg(test)]` block to pass `1.0` (preserving their original assertions unchanged, since scaling by 1.0 is a no-op):

```rust
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

    let instances = build_quad_instances(&frame, 1.0);
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].position, [1.0, 2.0]);
    assert_eq!(instances[0].size, [10.0, 20.0]);
    assert_eq!(instances[0].corner_radius, 4.0);
    assert_eq!(instances[1].position, [5.0, 6.0]);
}

#[test]
fn empty_frame_yields_no_instances() {
    let frame = Frame::new();
    assert!(build_quad_instances(&frame, 1.0).is_empty());
}

#[test]
fn applies_scale_factor_to_position_size_and_radius() {
    let mut frame = Frame::new();
    frame.push(DrawCommand::Rect(RectCommand {
        x: 10.0, y: 20.0, width: 30.0, height: 40.0, color: [1.0, 0.0, 0.0, 1.0], corner_radius: 5.0,
    }));

    let instances = build_quad_instances(&frame, 2.0);
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].position, [20.0, 40.0]);
    assert_eq!(instances[0].size, [60.0, 80.0]);
    assert_eq!(instances[0].corner_radius, 10.0);
}
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test --lib renderer::quad::tests`
Expected: FAIL (panics on `todo!()` in `build_quad_instances`).

- [ ] **Step 7: Implement the scaled `build_quad_instances`**

```rust
pub fn build_quad_instances(frame: &Frame, scale_factor: f32) -> Vec<QuadInstance> {
    frame
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::Rect(r) => {
                let scaled = r.scaled(scale_factor);
                Some(QuadInstance {
                    position: [scaled.x, scaled.y],
                    size: [scaled.width, scaled.height],
                    color: scaled.color,
                    corner_radius: scaled.corner_radius,
                })
            }
            DrawCommand::Text(_) => None,
        })
        .collect()
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --lib renderer::quad::tests`
Expected: 3 passed (2 pre-existing + 1 new).

- [ ] **Step 9: Wire `scale_factor` into `Renderer`**

In `src/renderer/mod.rs`:
- Add a `scale_factor: f32` field to the `Renderer` struct (alongside `surface`, `device`, etc.).
- Change `new`'s signature to `pub fn new(window: Arc<Window>, scale_factor: f32) -> Option<Self>`, and add `scale_factor` to the final `Some(Self { surface, device, queue, config, quad_pipeline, text_layer, scale_factor })`.
- Add this method (near `resize`):

```rust
pub fn set_scale_factor(&mut self, factor: f32) {
    self.scale_factor = factor;
}
```

- In `render_frame`, change the two lines that build quad instances and texts:

```rust
let instances = quad::build_quad_instances(frame, self.scale_factor);
self.quad_pipeline.prepare(&self.device, &self.queue, &instances, self.config.width as f32, self.config.height as f32);

let texts: Vec<scene::TextCommand> = frame.commands.iter().filter_map(|c| match c {
    scene::DrawCommand::Text(t) => Some(t.scaled(self.scale_factor)),
    scene::DrawCommand::Rect(_) => None,
}).collect();
self.text_layer.prepare(&self.device, &self.queue, self.config.width, self.config.height, &texts);
```

(Only those two lines change — everything else in `render_frame` from Task 2 stays as-is.)

- [ ] **Step 10: Wire `scale_factor` into `main.rs`**

In `src/main.rs`:
- Change the `Renderer::new(window.clone())` call in `resumed` to:

```rust
let scale_factor = window.scale_factor() as f32;
match Renderer::new(window.clone(), scale_factor) {
```

- Add a new arm to the `match event` block in `window_event`, alongside `CloseRequested`/`Resized`/`RedrawRequested`:

```rust
WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
    gpu.set_scale_factor(scale_factor as f32);
}
```

Note: winit already sends a separate `WindowEvent::Resized` alongside `ScaleFactorChanged` whenever the physical pixel size actually changes (e.g. moving the window to a monitor with a different DPI) — the existing `WindowEvent::Resized(size) => gpu.resize(size.width, size.height)` arm already reconfigures the surface for that case, so `ScaleFactorChanged` only needs to update the stored scale factor here, not touch the surface config itself.

- [ ] **Step 11: Build and verify**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo test`
Expected: all 21 tests pass (17 from sub-project 1 + 3 new `scaled` tests in `scene.rs` from Step 1 + 1 new `applies_scale_factor` test in `quad.rs` from Step 5).

Run: `cargo run`
Expected: scene renders the same as before on a 100%-scale display (scale_factor 1.0 is a no-op, so this should look identical to Task 2's output). If you have access to a display or Windows display-scaling setting at 125%/150%, set it, restart the app, and confirm the rectangles/text are NOT shrunk to a quarter/half size relative to the window (i.e. they still look like sensible on-screen sizes, not tiny) — this is the actual HiDPI contract working. If no scaled display is available in your environment, note that in your report as an untestable case rather than skipping verification silently.

- [ ] **Step 12: Commit**

```bash
git add src/scene.rs src/renderer/quad.rs src/renderer/mod.rs src/main.rs
git commit -m "feat: logical-pixel coordinate contract with HiDPI scaling at the render boundary"
```

---

### Task 4: Frame pacing — vsync by default, `FERRIS_UNCAPPED` opt-out, skip rendering while occluded/minimized

**Files:**
- Modify: `src/renderer/mod.rs` (`choose_present_mode` gains an `uncapped` parameter; `Renderer::new` gains an `uncapped` parameter)
- Modify: `src/main.rs` (read `FERRIS_UNCAPPED` env var, track occlusion, skip redraw work while occluded/minimized, drop the redundant `request_redraw()` call)
- Test: `src/renderer/mod.rs` (inline `#[cfg(test)]` module — rewrite existing tests, add one)

**Interfaces:**
- Consumes: `renderer::Renderer::new` (from Task 3 of this plan, currently `(window, scale_factor)`).
- Produces: `renderer::choose_present_mode(supported: &[wgpu::PresentMode], uncapped: bool) -> wgpu::PresentMode` (signature changed — added `uncapped`), `renderer::Renderer::new(window: Arc<Window>, scale_factor: f32, uncapped: bool) -> Option<Self>` (signature changed — added `uncapped`). This is the last task in this plan; nothing downstream in this plan depends on further changes.

- [ ] **Step 1: Write the failing tests for `choose_present_mode`'s new `uncapped` parameter**

Replace the entire `#[cfg(test)] mod tests { ... }` block at the bottom of `src/renderer/mod.rs` with:

```rust
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
```

This won't compile yet since `choose_present_mode` still only takes one parameter — that's expected for this RED step.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib renderer::tests`
Expected: FAIL to compile (wrong number of arguments to `choose_present_mode`) — this is the expected RED state for a signature change.

- [ ] **Step 3: Implement the `uncapped` parameter**

Replace `choose_present_mode` in `src/renderer/mod.rs`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib renderer::tests`
Expected: 4 passed.

- [ ] **Step 5: Thread `uncapped` through `Renderer::new`**

In `src/renderer/mod.rs`:
- Change `new`'s signature to `pub fn new(window: Arc<Window>, scale_factor: f32, uncapped: bool) -> Option<Self>`.
- Change the line `let present_mode = choose_present_mode(&caps.present_modes);` to `let present_mode = choose_present_mode(&caps.present_modes, uncapped);`.

- [ ] **Step 6: Update `src/main.rs`: read `FERRIS_UNCAPPED`, pass it through**

Change the `resumed` function's `Renderer::new` call:

```rust
let scale_factor = window.scale_factor() as f32;
let uncapped = std::env::var("FERRIS_UNCAPPED").map(|v| v == "1").unwrap_or(false);
match Renderer::new(window.clone(), scale_factor, uncapped) {
```

- [ ] **Step 7: Skip rendering while occluded or minimized, drop the redundant inner `request_redraw`**

Add an `occluded: bool` field to the `App` struct and its `Default` impl:

```rust
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
```

Add a new arm to `window_event`'s `match event` block:

```rust
WindowEvent::Occluded(occluded) => {
    self.occluded = occluded;
}
```

Replace the `WindowEvent::RedrawRequested` arm's body with (this adds the occlusion/minimize guard at the top and removes the redundant `window.request_redraw()` call at the end — `about_to_wait` already requests the next redraw unconditionally, so `RedrawRequested` doesn't need to ask again):

```rust
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
```

- [ ] **Step 8: Build and verify**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo test`
Expected: all 22 tests pass (21 from Task 3's end state + 1 net new — `choose_present_mode`'s tests were rewritten in place in this task's Step 1 from 3 tests to 4, the other 3 modules unchanged).

Run: `cargo run`
Expected: watch the FPS overlay — it should now read a number close to your monitor's refresh rate (e.g. ~60 or ~144, not the ~1700-3800fps seen in sub-project 1's uncapped runs), confirming vsync is capping the loop by default.

Run: `FERRIS_UNCAPPED=1 cargo run` (PowerShell: `$env:FERRIS_UNCAPPED=1; cargo run` — remember to `Remove-Item Env:\FERRIS_UNCAPPED` afterward so it doesn't leak into later runs)
Expected: FPS overlay reads a high uncapped number again (similar to sub-project 1's ~1700-3800fps range on this machine), confirming the escape hatch still works.

Manually test occlusion: run the app, then fully cover its window with another window for a few seconds, then uncover it — confirm (via a log line if you add `RUST_LOG=info` temporarily, or just by observing the overlay's fps/ms numbers before and after) that the app doesn't crash and resumes updating the overlay once uncovered. Also try minimizing and restoring the window.

- [ ] **Step 9: Commit**

```bash
git add src/renderer/mod.rs src/main.rs
git commit -m "feat: vsync-by-default frame pacing with FERRIS_UNCAPPED escape hatch, skip render while occluded/minimized"
```
