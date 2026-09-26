# Image Support (2.11) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render `<img src="...">` for real: decode PNG/JPEG (file, URL, or `data:` URI) via the `image` crate, size the box using CSS or intrinsic dimensions, and draw the real pixels on screen through a new GPU texture pipeline.

**Architecture:** `ferris-scene` (the dependency-free leaf crate) gains `DecodedImage`/`ImageCommand`. `ferris-loader` gains an `extract_images` step (DFS over the DOM, same pattern as `extract_stylesheets`) that fetches and decodes every `<img src>` into an `ImageMap`. `ferris-layout` gains an additive `layout_with_images` entry point (the existing `layout()` stays untouched — no existing test call site changes) that uses CSS `width`/`height` when specified, or the decoded image's intrinsic size otherwise. `ferris-paint` emits `DrawCommand::Image` for image-bearing boxes. `ferris-compositor` gains a new `renderer/image.rs` GPU pipeline (a textured quad, one draw call per image, texture cached by `Arc` pointer identity so it's only uploaded once per navigation).

**Tech Stack:** Rust, the `image` crate (PNG/JPEG decoding, no new codec written from scratch), the `base64` crate (`data:` URI decoding), wgpu 22 (existing pinned version — texture/sampler APIs used here match that version, not a newer one).

**Spec:** `docs/superpowers/specs/2026-09-26-image-support-design.md`

## Global Constraints

- `DecodedImage`/`ImageCommand` live in `ferris-scene` (already dependency-free) so `ferris-loader` (producer) and `ferris-layout`/`ferris-paint` (consumers) never create a dependency cycle with each other (spec's Arquitetura).
- The existing public `layout()` function's signature and behavior are UNCHANGED — every one of its ~25 existing test call sites in `ferris-layout/src/layout.rs` continues to compile and pass with zero edits. The new `layout_with_images` function is what real callers (the compositor) use going forward; `layout()` becomes a thin wrapper calling it with an empty `ImageMap` (this plan's own refinement of the spec, not a spec change).
- An `<img>` with no matching entry in the `ImageMap` (failed fetch/decode, missing `src`, or unsupported format) gets a 0×0 box and `image: None` — no placeholder icon, no propagated error, matches the user's explicit choice during brainstorming.
- `data:` URIs in `src` are decoded directly (base64, no network fetch); everything else resolves against the page's base `Source` via the already-existing `resolve_href`.
- Only PNG and JPEG are decoded (the `image` crate's `png`+`jpeg` features); no `<picture>`/`srcset`, no aspect-ratio preservation when only one of `width`/`height` is set in CSS, no HTML `width=`/`height=` attribute reading, no image cache across navigations — all explicitly documented spec limitations, not gaps to fix here.
- GPU pipeline code (`ImagePipeline`'s `new`/`prepare`/`render`) has no unit test, matching the existing, unchanged precedent set by `QuadPipeline`/`TextLayer` in this same codebase (their own pipeline-construction code has never had a unit test either — only the pure extraction functions `build_quad_instances`/`text_layer.prepare`'s command-filtering do). Verified only by the mandatory manual GPU-binary run.

## Review Focus

- A `<img>` whose `src` points at a file that does not exist, or at bytes that fail to decode (corrupted, unsupported format), must not propagate an error or stop the rest of the page's images from loading — the page must still render everything else normally. → Task 2.
- A `data:image/png;base64,...` URI must decode without ever attempting a network/file fetch (a malformed `data:` URI must degrade the same as a fetch failure — skipped, not a panic). → Task 2.
- Two different `<img>` tags with the exact same `src` text must reuse the same decoded image (not fetch/decode twice) — the `ImageMap`'s keying by raw `src` text should naturally deduplicate this; a test should pin it explicitly since it's easy to break by accident (e.g. if a future change made `extract_images` key by something non-deterministic per occurrence). → Task 2.
- An `<img>` with CSS `width`/`height` both specified must use those values verbatim, never the intrinsic decoded size — should be pinned in the same task as the intrinsic-size behavior so a future change to one path doesn't silently break the other. → Task 3.
- Two different decoded images used in the same `Frame` must get two separate GPU textures uploaded, not overwrite each other in the pipeline's cache — pinned via `build_image_instances`' pure extraction (which images end up in the frame, in order) plus the mandatory manual verification actually showing two distinct images on screen at once. → Task 4 (pure part) + Task 5 (manual, GPU part).

---

### Task 1: `DecodedImage`/`ImageCommand` in `ferris-scene`

**Files:**
- Modify: `ferris-scene/src/lib.rs`

**Interfaces:**
- Consumes: nothing new (still zero dependencies).
- Produces: `pub struct DecodedImage { pub width: u32, pub height: u32, pub rgba: std::sync::Arc<[u8]> }`, `pub struct ImageCommand { pub x: f32, pub y: f32, pub width: f32, pub height: f32, pub image: DecodedImage }` with `pub fn ImageCommand::scaled(&self, factor: f32) -> ImageCommand`, `DrawCommand::Image(ImageCommand)` (new 3rd variant), `pub type ImageMap = std::collections::HashMap<String, DecodedImage>` — consumed by Task 2 (`ferris-loader`, produces `ImageMap` values), Task 3 (`ferris-layout`/`ferris-paint`), Task 4 (`ferris-compositor`'s new pipeline + the 3 existing exhaustive `match`/`filter_map` sites on `DrawCommand` that must add an `Image` arm to keep compiling).

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block at the bottom of `ferris-scene/src/lib.rs` (after `text_scaled_multiplies_position_and_size_not_content_or_color`'s closing `}`, still inside `mod tests { ... }`):
```rust
    #[test]
    fn image_scaled_multiplies_position_and_size_not_pixel_dimensions() {
        let image = DecodedImage { width: 10, height: 20, rgba: std::sync::Arc::from(vec![0u8; 10 * 20 * 4]) };
        let cmd = ImageCommand { x: 1.0, y: 2.0, width: 3.0, height: 4.0, image: image.clone() };
        let scaled = cmd.scaled(2.0);
        assert_eq!(scaled.x, 2.0);
        assert_eq!(scaled.y, 4.0);
        assert_eq!(scaled.width, 6.0);
        assert_eq!(scaled.height, 8.0);
        assert_eq!(scaled.image.width, 10, "pixel dimensions are never scaled, only the on-screen box");
        assert_eq!(scaled.image.height, 20);
    }

    #[test]
    fn image_scaled_reuses_the_same_underlying_pixel_arc_no_copy() {
        let image = DecodedImage { width: 1, height: 1, rgba: std::sync::Arc::from(vec![1u8, 2, 3, 4]) };
        let cmd = ImageCommand { x: 0.0, y: 0.0, width: 1.0, height: 1.0, image: image.clone() };
        let scaled = cmd.scaled(1.5);
        assert!(std::sync::Arc::ptr_eq(&scaled.image.rgba, &image.rgba), "scaling must clone the Arc handle, not the pixel bytes");
    }

    #[test]
    fn draw_command_image_variant_round_trips_through_a_frame() {
        let image = DecodedImage { width: 1, height: 1, rgba: std::sync::Arc::from(vec![255u8, 0, 0, 255]) };
        let mut frame = Frame::new();
        frame.push(DrawCommand::Image(ImageCommand { x: 0.0, y: 0.0, width: 1.0, height: 1.0, image }));
        assert_eq!(frame.commands.len(), 1);
        assert!(matches!(frame.commands[0], DrawCommand::Image(_)));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-scene`
Expected: FAIL to compile — `DecodedImage`/`ImageCommand`/`DrawCommand::Image` don't exist yet.

- [ ] **Step 3: Add the new types**

In `ferris-scene/src/lib.rs`, add this block right after `TextCommand`'s `impl` block (before the `#[derive(Debug, Clone, PartialEq)] pub enum DrawCommand` line):
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: std::sync::Arc<[u8]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageCommand {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub image: DecodedImage,
}

impl ImageCommand {
    pub fn scaled(&self, factor: f32) -> ImageCommand {
        ImageCommand {
            x: self.x * factor,
            y: self.y * factor,
            width: self.width * factor,
            height: self.height * factor,
            image: self.image.clone(),
        }
    }
}

pub type ImageMap = std::collections::HashMap<String, DecodedImage>;
```
Then change the `DrawCommand` enum from:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect(RectCommand),
    Text(TextCommand),
}
```
to:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect(RectCommand),
    Text(TextCommand),
    Image(ImageCommand),
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --package ferris-scene`
Expected: FAIL to compile still — adding a 3rd `DrawCommand` variant breaks 3 exhaustive `match`/`filter_map` call sites elsewhere in the workspace that don't yet handle it. This is expected at this exact point in the plan (those sites are outside this task's file list; they get fixed one task at a time later). Confirm the compiler errors are ONLY in `ferris-compositor` (files `chrome.rs`, `renderer/quad.rs`, `renderer/mod.rs`), not in `ferris-scene` itself — run `cargo test --package ferris-scene` specifically (not `--workspace`) to isolate this crate's own tests:
Expected: `test result: ok. 11 passed; 0 failed` (8 pre-existing + 3 new).

- [ ] **Step 5: Commit**

```bash
git add ferris-scene/src/lib.rs
git commit -m "feat: add DecodedImage/ImageCommand and a 3rd DrawCommand::Image variant"
```

---

### Task 2: Fetch + decode images (`ferris-loader`)

**Files:**
- Modify: `ferris-loader/Cargo.toml`
- Modify: `ferris-loader/src/lib.rs`

**Interfaces:**
- Consumes: `ferris_scene::{DecodedImage, ImageMap}` (Task 1); existing `resolve_href`, `fetch_text`, `Source`, `LoadError` (all already in this file).
- Produces: `pub fn load_page(source: &Source) -> Result<(Element, Stylesheet, ImageMap), LoadError>` (signature changes — was `Result<(Element, Stylesheet), LoadError>`); `fn extract_images(root: &Element, base: &Source) -> ImageMap` (private) — consumed by Task 5 (`main.rs`, which calls `load_page` and now must destructure 3 items instead of 2).

- [ ] **Step 1: Add the new dependencies**

Modify `ferris-loader/Cargo.toml` — it currently reads:
```toml
[dependencies]
ferris-dom = { path = "../ferris-dom" }
ferris-css = { path = "../ferris-css" }
ferris-style = { path = "../ferris-style" }
ureq = "2"
url = "2"
```
Change to:
```toml
[dependencies]
ferris-dom = { path = "../ferris-dom" }
ferris-css = { path = "../ferris-css" }
ferris-style = { path = "../ferris-style" }
ferris-scene = { path = "../ferris-scene" }
ureq = "2"
url = "2"
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
base64 = "0.22"
```

- [ ] **Step 2: Run a build to confirm the new dependencies resolve**

Run: `cargo build --package ferris-loader`
Expected: clean build (nothing uses the new crates yet).

- [ ] **Step 3: Write the failing tests**

Append to the `mod tests` block at the bottom of `ferris-loader/src/lib.rs` (after `load_page_local_file_prepends_default_stylesheet_and_appends_page_css`'s closing `}`, still inside `mod tests { ... }`):
```rust
    fn tiny_png_bytes(r: u8, g: u8, b: u8) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([r, g, b, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn extract_images_decodes_a_local_png_file() {
        let png_path = write_temp_file_bytes("image_task2_a.png", &tiny_png_bytes(200, 0, 0));
        let html_path = write_temp_file("page_task2_a.html", "");
        let html = format!(r#"<html><body><img src="{}"></body></html>"#, png_path.file_name().unwrap().to_str().unwrap());
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(html_path);

        let images = extract_images(&root, &base);

        let decoded = images.get(png_path.file_name().unwrap().to_str().unwrap()).expect("image must be present");
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.rgba.len(), 2 * 2 * 4);
    }

    #[test]
    fn extract_images_decodes_a_data_uri_without_any_fetch() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(tiny_png_bytes(0, 200, 0));
        let html = format!(r#"<html><body><img src="data:image/png;base64,{b64}"></body></html>"#);
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(std::path::PathBuf::from("/irrelevant/page.html"));

        let images = extract_images(&root, &base);

        let key = format!("data:image/png;base64,{b64}");
        let decoded = images.get(&key).expect("data: URI image must be present");
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
    }

    #[test]
    fn extract_images_skips_a_missing_file_without_propagating_error() {
        let html_path = write_temp_file("page_task2_c.html", "");
        let html = r#"<html><body><img src="does-not-exist-anywhere.png"><p>still here</p></body></html>"#;
        let root = ferris_dom::parser::parse_document(html);
        let base = Source::File(html_path);

        let images = extract_images(&root, &base);

        assert!(images.is_empty(), "a missing image file must be skipped, not panic or propagate an error");
    }

    #[test]
    fn extract_images_skips_bytes_that_fail_to_decode() {
        let bad_path = write_temp_file("image_task2_bad.png", "this is not a real png file");
        let html_path = write_temp_file("page_task2_d.html", "");
        let html = format!(r#"<html><body><img src="{}"></body></html>"#, bad_path.file_name().unwrap().to_str().unwrap());
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(html_path);

        let images = extract_images(&root, &base);

        assert!(images.is_empty(), "corrupted image bytes must be skipped, not panic");
    }

    #[test]
    fn extract_images_ignores_an_img_with_no_src_attribute() {
        let html = r#"<html><body><img></body></html>"#;
        let root = ferris_dom::parser::parse_document(html);
        let base = Source::File(std::path::PathBuf::from("/irrelevant/page.html"));

        let images = extract_images(&root, &base);

        assert!(images.is_empty());
    }

    #[test]
    fn extract_images_deduplicates_two_tags_with_the_same_src() {
        let png_path = write_temp_file_bytes("image_task2_e.png", &tiny_png_bytes(0, 0, 200));
        let html_path = write_temp_file("page_task2_e.html", "");
        let file_name = png_path.file_name().unwrap().to_str().unwrap();
        let html = format!(r#"<html><body><img src="{file_name}"><img src="{file_name}"></body></html>"#);
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(html_path);

        let images = extract_images(&root, &base);

        assert_eq!(images.len(), 1, "two tags with the same src must produce exactly one map entry");
    }

    #[test]
    fn load_page_returns_an_image_map_alongside_the_element_and_stylesheet() {
        let png_path = write_temp_file_bytes("image_task2_f.png", &tiny_png_bytes(10, 20, 30));
        let html_path = write_temp_file(
            "page_task2_f.html",
            &format!(r#"<html><body><img src="{}"></body></html>"#, png_path.file_name().unwrap().to_str().unwrap()),
        );
        let source = Source::File(html_path);

        let (root, _stylesheet, images) = load_page(&source).unwrap();

        assert_eq!(root.tag_name, "html");
        assert_eq!(images.len(), 1);
    }
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test --package ferris-loader`
Expected: FAIL to compile — `extract_images` doesn't exist yet, `load_page` still returns a 2-tuple, and `write_temp_file_bytes` (a small new test helper, see Step 5) doesn't exist yet either.

- [ ] **Step 5: Implement `write_temp_file_bytes`, `fetch_bytes`, `decode_data_uri`, `extract_images`, and update `load_page`**

Add this test helper next to the existing `write_temp_file` helper inside `mod tests`:
```rust
    fn write_temp_file_bytes(name: &str, contents: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join("ferris_loader_tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }
```

Add this above `load_page` (outside `mod tests`, in the main body of the file, near `fetch_text`):
```rust
fn fetch_bytes(source: &Source) -> Result<Vec<u8>, LoadError> {
    match source {
        Source::File(path) => std::fs::read(path).map_err(|e| LoadError::Fetch(format!("{}: {e}", path.display()))),
        Source::Url(url) => {
            let response = ureq::get(url).call().map_err(|e| LoadError::Fetch(format!("{url}: {e}")))?;
            let mut buf = Vec::new();
            response.into_reader().read_to_end(&mut buf).map_err(|e| LoadError::Fetch(format!("{url}: {e}")))?;
            Ok(buf)
        }
    }
}

/// Decodes a `data:image/...;base64,<...>` URI directly, with no fetch at
/// all. Returns `None` for anything that doesn't parse as base64 after the
/// `base64,` marker — malformed `data:` URIs degrade the same as any other
/// fetch failure: the image is skipped, not propagated as an error.
fn decode_data_uri(src: &str) -> Option<Vec<u8>> {
    let (_, b64) = src.split_once("base64,")?;
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

fn extract_images(root: &Element, base: &Source) -> ferris_scene::ImageMap {
    let mut images = ferris_scene::ImageMap::new();
    collect_images(root, base, &mut images);
    images
}

fn collect_images(el: &Element, base: &Source, images: &mut ferris_scene::ImageMap) {
    if el.tag_name == "img" {
        if let Some(src) = el.attributes.get("src") {
            if !images.contains_key(src) {
                let bytes = if src.starts_with("data:") {
                    decode_data_uri(src)
                } else {
                    resolve_href(base, src).and_then(|resolved| fetch_bytes(&resolved).ok())
                };
                if let Some(bytes) = bytes {
                    if let Ok(decoded) = image::load_from_memory(&bytes) {
                        let rgba_image = decoded.to_rgba8();
                        let (width, height) = (rgba_image.width(), rgba_image.height());
                        images.insert(
                            src.clone(),
                            ferris_scene::DecodedImage { width, height, rgba: std::sync::Arc::from(rgba_image.into_raw()) },
                        );
                    }
                }
            }
        }
    }

    for child in &el.children {
        if let Node::Element(child_el) = child {
            collect_images(child_el, base, images);
        }
    }
}
```
Note: `fetch_bytes`'s `Source::Url` branch needs `std::io::Read` in scope for `.read_to_end(...)` — add `use std::io::Read;` near the top of the file if it isn't already imported.

Change `load_page`'s signature and body from:
```rust
pub fn load_page(source: &Source) -> Result<(Element, Stylesheet), LoadError> {
    let html = fetch_text(source)?;
    let root = ferris_dom::parser::parse_document(&html);

    let mut css_sources = vec![DEFAULT_STYLESHEET_CSS.to_string()];
    css_sources.extend(extract_stylesheets(&root, source));
    let combined_css = css_sources.join("\n");

    let css_tokens = ferris_css::tokenizer::Tokenizer::tokenize(&combined_css);
    let stylesheet = ferris_css::parser::Parser::parse(&css_tokens);

    Ok((root, stylesheet))
}
```
to:
```rust
pub fn load_page(source: &Source) -> Result<(Element, Stylesheet, ferris_scene::ImageMap), LoadError> {
    let html = fetch_text(source)?;
    let root = ferris_dom::parser::parse_document(&html);

    let mut css_sources = vec![DEFAULT_STYLESHEET_CSS.to_string()];
    css_sources.extend(extract_stylesheets(&root, source));
    let combined_css = css_sources.join("\n");

    let css_tokens = ferris_css::tokenizer::Tokenizer::tokenize(&combined_css);
    let stylesheet = ferris_css::parser::Parser::parse(&css_tokens);

    let images = extract_images(&root, source);

    Ok((root, stylesheet, images))
}
```
This also breaks the ONE existing test that destructures `load_page`'s result — `load_page_local_file_prepends_default_stylesheet_and_appends_page_css` currently has `let (root, stylesheet) = load_page(&source).unwrap();` — change it to `let (root, stylesheet, _images) = load_page(&source).unwrap();` (that test isn't about images, so the third value is intentionally unused).

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --package ferris-loader`
Expected: `test result: ok. 20 passed; 0 failed` (13 pre-existing + 7 new).

- [ ] **Step 7: Run the whole workspace build**

Run: `cargo build --workspace`
Expected: still fails to compile in `ferris-compositor` (main.rs still calls the old 2-tuple `load_page`) — expected at this point, fixed in Task 5. Confirm the ONLY compile errors are in `ferris-compositor`, none in `ferris-loader`/`ferris-scene`/`ferris-layout`/`ferris-paint`.

- [ ] **Step 8: Commit**

```bash
git add ferris-loader/Cargo.toml ferris-loader/src/lib.rs
git commit -m "feat: fetch and decode <img> sources (file, URL, data: URI) in ferris-loader"
```

---

### Task 3: Intrinsic image sizing (`ferris-layout`) and painting (`ferris-paint`)

**Files:**
- Modify: `ferris-layout/Cargo.toml`
- Modify: `ferris-layout/src/layout.rs`
- Modify: `ferris-paint/src/paint.rs`

**Interfaces:**
- Consumes: `ferris_scene::{DecodedImage, ImageMap}` (Task 1); `ferris_loader`'s `ImageMap` values (Task 2, same type, no new dependency needed — `ferris-layout` depends on `ferris-scene` directly, not on `ferris-loader`).
- Produces: `pub fn layout_with_images<'a>(root: &'a StyledNode<'a>, viewport_width: f32, viewport_height: f32, images: &ferris_scene::ImageMap) -> Option<LayoutBox<'a>>` (new, additive); `LayoutBox.image: Option<ferris_scene::DecodedImage>` (new field); `ferris-paint`'s `paint()` now emits `DrawCommand::Image` — consumed by Task 5 (`main.rs`, which switches from `layout::layout(...)` to `layout::layout_with_images(...)`).

- [ ] **Step 1: Add the new dependency**

Modify `ferris-layout/Cargo.toml` — it currently reads:
```toml
[dependencies]
ferris-dom = { path = "../ferris-dom" }
ferris-style = { path = "../ferris-style" }
ferris-text = { path = "../ferris-text" }

[dev-dependencies]
ferris-css = { path = "../ferris-css" }
```
Change to:
```toml
[dependencies]
ferris-dom = { path = "../ferris-dom" }
ferris-style = { path = "../ferris-style" }
ferris-text = { path = "../ferris-text" }
ferris-scene = { path = "../ferris-scene" }

[dev-dependencies]
ferris-css = { path = "../ferris-css" }
```

- [ ] **Step 2: Write the failing tests**

Append to the `mod tests` block at the bottom of `ferris-layout/src/layout.rs` (after `trailing_text_after_a_block_child_is_not_silently_dropped`'s closing `}`, still inside `mod tests { ... }`):
```rust
    fn tiny_image(width: u32, height: u32) -> ferris_scene::DecodedImage {
        ferris_scene::DecodedImage { width, height, rgba: std::sync::Arc::from(vec![0u8; (width * height * 4) as usize]) }
    }

    #[test]
    fn img_with_no_css_size_uses_the_decoded_intrinsic_dimensions() {
        let mut el = Element::new("img");
        el.attributes.insert("src".to_string(), "photo.png".to_string());
        let node = leaf_styled_node(&el, &[]);
        let mut images = ferris_scene::ImageMap::new();
        images.insert("photo.png".to_string(), tiny_image(120, 80));

        let root = layout_with_images(&node, 800.0, 600.0, &images).unwrap();

        assert_eq!(root.width, 120.0);
        assert_eq!(root.height, 80.0);
        assert_eq!(root.image.as_ref().unwrap().width, 120);
    }

    #[test]
    fn img_with_explicit_css_width_and_height_ignores_the_intrinsic_size() {
        let mut el = Element::new("img");
        el.attributes.insert("src".to_string(), "photo.png".to_string());
        let node = leaf_styled_node(&el, &[("width", "50px"), ("height", "30px")]);
        let mut images = ferris_scene::ImageMap::new();
        images.insert("photo.png".to_string(), tiny_image(120, 80));

        let root = layout_with_images(&node, 800.0, 600.0, &images).unwrap();

        assert_eq!(root.width, 50.0);
        assert_eq!(root.height, 30.0);
    }

    #[test]
    fn img_with_no_matching_image_map_entry_has_a_zero_size_box_and_no_image() {
        let mut el = Element::new("img");
        el.attributes.insert("src".to_string(), "missing.png".to_string());
        let node = leaf_styled_node(&el, &[]);
        let images = ferris_scene::ImageMap::new();

        let root = layout_with_images(&node, 800.0, 600.0, &images).unwrap();

        assert_eq!(root.width, 0.0);
        assert_eq!(root.height, 0.0);
        assert!(root.image.is_none());
    }

    #[test]
    fn non_img_elements_never_get_an_image_even_with_a_matching_map_entry() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[]);
        let mut images = ferris_scene::ImageMap::new();
        images.insert("".to_string(), tiny_image(10, 10)); // a div has no `src`, so this must never match

        let root = layout_with_images(&node, 800.0, 600.0, &images).unwrap();

        assert!(root.image.is_none());
    }

    #[test]
    fn layout_without_images_behaves_exactly_like_before_this_change() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("width", "100px"), ("height", "50px")]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert_eq!(root.width, 100.0);
        assert_eq!(root.height, 50.0);
        assert!(root.image.is_none());
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --package ferris-layout`
Expected: FAIL to compile — `layout_with_images` doesn't exist yet, `LayoutBox` has no `image` field, `Element::attributes` insertion needs confirming (it's already a public `HashMap<String, String>` field, used the same way in `ferris-loader`'s own tests).

- [ ] **Step 4: Add the `image` field to `LayoutBox` and implement `layout_with_images`**

Change the `LayoutBox` struct definition from:
```rust
pub struct LayoutBox<'a> {
    pub styled_node: &'a StyledNode<'a>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub margin: Edges,
    pub border: Edges,
    pub padding: Edges,
    pub lines: Vec<Vec<InlineRun<'a>>>,
    pub children: Vec<LayoutBox<'a>>,
}
```
to:
```rust
pub struct LayoutBox<'a> {
    pub styled_node: &'a StyledNode<'a>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub margin: Edges,
    pub border: Edges,
    pub padding: Edges,
    pub lines: Vec<Vec<InlineRun<'a>>>,
    pub children: Vec<LayoutBox<'a>>,
    pub image: Option<ferris_scene::DecodedImage>,
}
```

Change the public `layout` function from:
```rust
pub fn layout<'a>(root: &'a StyledNode<'a>, viewport_width: f32, viewport_height: f32) -> Option<LayoutBox<'a>> {
    let containing_block = ContainingBlock {
        content_x: 0.0,
        content_y: 0.0,
        content_width: viewport_width,
        viewport_height,
    };
    layout_block(root, containing_block, true)
}
```
to:
```rust
/// Lays out `root` with no images available — every `<img>` gets a 0x0 box
/// and `image: None`. Every existing caller/test of this function keeps
/// working unchanged; real page rendering uses `layout_with_images` below.
pub fn layout<'a>(root: &'a StyledNode<'a>, viewport_width: f32, viewport_height: f32) -> Option<LayoutBox<'a>> {
    layout_with_images(root, viewport_width, viewport_height, &ferris_scene::ImageMap::new())
}

/// Lays out `root`, sizing any `<img>` element using CSS `width`/`height`
/// when specified, or `images`' intrinsic decoded dimensions otherwise. An
/// `<img>` with no matching entry in `images` (missing `src`, failed
/// fetch/decode) gets a 0x0 box and `image: None` — never panics.
pub fn layout_with_images<'a>(root: &'a StyledNode<'a>, viewport_width: f32, viewport_height: f32, images: &ferris_scene::ImageMap) -> Option<LayoutBox<'a>> {
    let containing_block = ContainingBlock {
        content_x: 0.0,
        content_y: 0.0,
        content_width: viewport_width,
        viewport_height,
    };
    layout_block(root, containing_block, true, images)
}
```

Change `layout_block`'s signature from `fn layout_block<'a>(node: &'a StyledNode<'a>, containing_block: ContainingBlock, is_root: bool) -> Option<LayoutBox<'a>> {` to `fn layout_block<'a>(node: &'a StyledNode<'a>, containing_block: ContainingBlock, is_root: bool, images: &ferris_scene::ImageMap) -> Option<LayoutBox<'a>> {`, and update its ONE recursive call site inside the same function — currently `if let Some(child_box) = layout_block(child, child_containing_block, false) {` — to `if let Some(child_box) = layout_block(child, child_containing_block, false, images) {`.

Inside `layout_block`, right after `let available_width = ...` and before `let width_len = ...`, insert the image lookup:
```rust
    let image = if node.element.tag_name == "img" {
        node.element.attributes.get("src").and_then(|src| images.get(src)).cloned()
    } else {
        None
    };
```

Change the `width`/`height` resolution to prefer the intrinsic image size as the `Auto` fallback instead of `available_width`/`0.0` when this node has an image. Replace:
```rust
    let width_len = parse_length(node.style.get("width").map(String::as_str));
    let width = resolved_length_or(width_len, containing_block.content_width, available_width).max(0.0);
```
with:
```rust
    let width_len = parse_length(node.style.get("width").map(String::as_str));
    let width_default = image.as_ref().map(|img| img.width as f32).unwrap_or(available_width);
    let width = resolved_length_or(width_len, containing_block.content_width, width_default).max(0.0);
```

And near the bottom, where `height` is finally computed, change:
```rust
    let height_len = parse_length(node.style.get("height").map(String::as_str));
    let auto_height = (cursor_y - content_y).max(0.0);
    let height = match height_len {
        Length::Percent(p) if is_root => (containing_block.viewport_height * (p / 100.0)).max(0.0),
        Length::Percent(_) => auto_height,
        other => resolved_length_or(other, 0.0, auto_height).max(0.0),
    };

    Some(LayoutBox { styled_node: node, x: content_x, y: content_y, width, height, margin, border, padding, lines, children })
```
to:
```rust
    let height_len = parse_length(node.style.get("height").map(String::as_str));
    let auto_height = image.as_ref().map(|img| img.height as f32).unwrap_or_else(|| (cursor_y - content_y).max(0.0));
    let height = match height_len {
        Length::Percent(p) if is_root => (containing_block.viewport_height * (p / 100.0)).max(0.0),
        Length::Percent(_) => auto_height,
        other => resolved_length_or(other, 0.0, auto_height).max(0.0),
    };

    Some(LayoutBox { styled_node: node, x: content_x, y: content_y, width, height, margin, border, padding, lines, children, image })
```
(Note: for a non-`<img>` node, `image` is always `None`, and `auto_height`'s fallback is exactly the old `(cursor_y - content_y).max(0.0)` behavior — this changes nothing for any existing non-image test.)

- [ ] **Step 5: Fix the one other `LayoutBox` literal in this file**

There is exactly one other place in `ferris-layout/src/layout.rs` that constructs a bare `LayoutBox { ... }` literal — there is none; the only production construction is the one just changed above. (Confirmed via `grep -n "LayoutBox {" ferris-layout/src/layout.rs` returning exactly the one line already changed in Step 4.)

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --package ferris-layout`
Expected: `test result: ok. 42 passed; 0 failed` (37 pre-existing + 5 new).

- [ ] **Step 7: Update `ferris-paint` to emit `DrawCommand::Image`**

Modify `ferris-paint/src/paint.rs`. Every `LayoutBox { ... }` literal in this file's tests needs one new field, `image: None,` (this crate doesn't test image-emission with a real image-bearing box until the test below is added — the 4 pre-existing literals are all non-image fixtures, so they just get the field added with `None`). There are exactly 4 in this file:
1. Inside the `leaf_box` helper (around line 61):
```rust
    fn leaf_box<'a>(styled_node: &'a StyledNode<'a>, x: f32, y: f32, width: f32, height: f32, edges: Edges) -> LayoutBox<'a> {
        LayoutBox {
            styled_node,
            x,
            y,
            width,
            height,
            margin: Edges::default(),
            border: edges,
            padding: edges,
            lines: Vec::new(),
            children: Vec::new(),
            image: None,
        }
    }
```
2 and 3. Inside `lines_produce_one_text_command_per_run_with_correct_x_and_y_offset` and `empty_lines_produces_no_text_command` (both build a `LayoutBox { ... }` directly with a `lines: ...,` and `children: Vec::new(),` — add `image: None,` right after `children: Vec::new(),` in both).
4. Inside `recursion_visits_all_descendants_background_before_children`'s `parent_box` (same pattern — add `image: None,` after `children: vec![child_box],`).

Then add the actual emission logic. In `paint_node`, right after the existing background-color block (`if let Some(color) = parse_color(...) { frame.push(DrawCommand::Rect(...)); }`) and before the `if !node.lines.is_empty() { ... }` block, add:
```rust
    if let Some(image) = &node.image {
        frame.push(DrawCommand::Image(ferris_scene::ImageCommand {
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
            image: image.clone(),
        }));
    }
```

Add this test to `mod tests` (after `recursion_visits_all_descendants_background_before_children`'s closing `}`):
```rust
    #[test]
    fn image_bearing_box_emits_an_image_command_at_its_content_box_position() {
        let element = Element::new("img");
        let styled = StyledNode { element: &element, style: HashMap::new(), children: Vec::new() };
        let mut node = leaf_box(&styled, 10.0, 20.0, 120.0, 80.0, Edges::default());
        node.image = Some(ferris_scene::DecodedImage { width: 120, height: 80, rgba: std::sync::Arc::from(vec![0u8; 120 * 80 * 4]) });

        let frame = paint(&node);

        assert_eq!(frame.commands.len(), 1);
        let DrawCommand::Image(img) = &frame.commands[0] else { panic!("expected an image command") };
        assert_eq!(img.x, 10.0);
        assert_eq!(img.y, 20.0);
        assert_eq!(img.width, 120.0);
        assert_eq!(img.height, 80.0);
    }
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test --package ferris-paint`
Expected: `test result: ok. 4 passed; 0 failed` (3 pre-existing + 1 new).

- [ ] **Step 9: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: still fails to compile in `ferris-compositor` (main.rs still calls the old-signature `load_page`/`layout`) — expected, fixed in Task 5. Confirm the only compile errors are in `ferris-compositor`.

- [ ] **Step 10: Commit**

```bash
git add ferris-layout/Cargo.toml ferris-layout/src/layout.rs ferris-paint/src/paint.rs
git commit -m "feat: size <img> boxes by CSS or intrinsic dimensions, paint emits DrawCommand::Image"
```

---

### Task 4: GPU texture pipeline (`ferris-compositor`)

**Files:**
- Create: `ferris-compositor/src/renderer/image.rs`
- Modify: `ferris-compositor/src/renderer/mod.rs`
- Modify: `ferris-compositor/src/renderer/quad.rs`
- Modify: `ferris-compositor/src/chrome.rs`
- Modify: `ferris-compositor/src/scene.rs`
- Modify: `ferris-compositor/src/tabs.rs`

**Interfaces:**
- Consumes: `ferris_scene::{DrawCommand, Frame, ImageCommand, DecodedImage}` (Task 1).
- Produces: `pub fn build_image_instances(frame: &scene::Frame, scale_factor: f32) -> Vec<ImageCommand>` (pure, tested); `pub struct ImagePipeline` with `new`/`prepare`/`render` (no unit test, same precedent as `QuadPipeline`/`TextLayer`) — consumed by Task 5 (`main.rs` doesn't call this directly, but `Renderer::render_frame`, which this task wires up, is called by `main.rs` unchanged).

- [ ] **Step 1: Fix the 3 exhaustive `DrawCommand` matches broken since Task 1**

`ferris-compositor/src/renderer/quad.rs`'s `build_quad_instances` currently has:
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
Add one arm: change `DrawCommand::Text(_) => None,` to `DrawCommand::Text(_) | DrawCommand::Image(_) => None,`.

`ferris-compositor/src/renderer/mod.rs`'s `render_frame` currently has:
```rust
        let texts: Vec<scene::TextCommand> = frame.commands.iter().filter_map(|c| match c {
            scene::DrawCommand::Text(t) => Some(t.scaled(self.scale_factor)),
            scene::DrawCommand::Rect(_) => None,
        }).collect();
```
Change `scene::DrawCommand::Rect(_) => None,` to `scene::DrawCommand::Rect(_) | scene::DrawCommand::Image(_) => None,`.

`ferris-compositor/src/chrome.rs`'s `translate_frame` currently has:
```rust
pub fn translate_frame(frame: &scene::Frame, dy: f32) -> scene::Frame {
    let commands = frame
        .commands
        .iter()
        .map(|cmd| match cmd {
            scene::DrawCommand::Rect(r) => scene::DrawCommand::Rect(scene::RectCommand { y: r.y + dy, ..*r }),
            scene::DrawCommand::Text(t) => scene::DrawCommand::Text(scene::TextCommand { y: t.y + dy, ..t.clone() }),
        })
        .collect();
    scene::Frame { commands }
}
```
Add an `Image` arm (images must also shift when the chrome bar/tab strip translate the page frame, or every image on a real page would render at the wrong Y):
```rust
pub fn translate_frame(frame: &scene::Frame, dy: f32) -> scene::Frame {
    let commands = frame
        .commands
        .iter()
        .map(|cmd| match cmd {
            scene::DrawCommand::Rect(r) => scene::DrawCommand::Rect(scene::RectCommand { y: r.y + dy, ..*r }),
            scene::DrawCommand::Text(t) => scene::DrawCommand::Text(scene::TextCommand { y: t.y + dy, ..t.clone() }),
            scene::DrawCommand::Image(i) => scene::DrawCommand::Image(scene::ImageCommand { y: i.y + dy, ..i.clone() }),
        })
        .collect();
    scene::Frame { commands }
}
```

`ferris-compositor/src/scene.rs` and `ferris-compositor/src/tabs.rs` each have a test using `matches!(c, DrawCommand::Rect(_))`/`matches!(c, DrawCommand::Text(_))` — these use `matches!`, which doesn't require exhaustiveness, so they still compile unchanged; no edit needed in either file (confirmed — only the 3 `match` sites above are exhaustive and need fixing).

- [ ] **Step 2: Run a build to confirm the 3 fixes compile**

Run: `cargo build --package ferris-compositor --lib`
Expected: clean build (the new `Image` arms compile; `ImagePipeline` doesn't exist yet, but nothing calls it yet either).

**Note on wgpu API naming:** this task's code uses `wgpu::ImageCopyTexture`/`wgpu::ImageDataLayout` (the texture-upload struct names matching wgpu 22, the version pinned in `ferris-compositor/Cargo.toml`, `wgpu = "22"`). If `cargo build` reports these names don't exist, check `cargo tree -p wgpu` for the resolved version and use whatever that version calls the same two structs (e.g. newer wgpu releases renamed them to `TexelCopyTextureInfo`/`TexelCopyBufferLayout`) — same fields, same purpose, just a rename; not a design change.

- [ ] **Step 3: Write the failing test for `build_image_instances`**

Create the test module inside a NEW file `ferris-compositor/src/renderer/image.rs` — write the whole file now (implementation included, since this is the smallest reasonable unit: the pure function plus its test, then the GPU pipeline below in the same file):
```rust
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::scene::{DecodedImage, DrawCommand, Frame, ImageCommand};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ImageUniform {
    position: [f32; 2],
    size: [f32; 2],
}

const IMAGE_VERTICES: &[[f32; 2]] = &[
    [0.0, 0.0], [1.0, 0.0], [0.0, 1.0],
    [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
];

const IMAGE_SHADER: &str = r#"
struct Globals {
    viewport_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct ImageUniform {
    position: vec2<f32>,
    size: vec2<f32>,
};
@group(1) @binding(0) var<uniform> image_uniform: ImageUniform;
@group(1) @binding(1) var image_texture: texture_2d<f32>;
@group(1) @binding(2) var image_sampler: sampler;

struct VertexInput {
    @location(0) corner: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let pixel_pos = image_uniform.position + vertex.corner * image_uniform.size;
    let ndc_x = (pixel_pos.x / globals.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / globals.viewport_size.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = vertex.corner;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(image_texture, image_sampler, in.uv);
}
"#;

/// Extracts and scales the `Image` commands from `frame`, in order,
/// ignoring `Rect`/`Text` — pure, no GPU needed, mirrors
/// `quad::build_quad_instances`.
pub fn build_image_instances(frame: &Frame, scale_factor: f32) -> Vec<ImageCommand> {
    frame
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::Image(img) => Some(img.scaled(scale_factor)),
            DrawCommand::Rect(_) | DrawCommand::Text(_) => None,
        })
        .collect()
}

struct CachedImage {
    position_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    cache: std::collections::HashMap<usize, CachedImage>,
    draw_order: Vec<usize>,
}

impl ImagePipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image_shader"),
            source: wgpu::ShaderSource::Wgsl(IMAGE_SHADER.into()),
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("image_vertex_buffer"),
            contents: bytemuck::cast_slice(IMAGE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image_globals_buffer"),
            size: std::mem::size_of::<[f32; 2]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image_globals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image_globals_bind_group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buffer.as_entire_binding() }],
        });

        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image_texture_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image_pipeline_layout"),
            bind_group_layouts: &[&globals_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 }],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_layout],
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

        Self {
            pipeline,
            vertex_buffer,
            globals_buffer,
            globals_bind_group,
            texture_bind_group_layout,
            sampler,
            cache: std::collections::HashMap::new(),
            draw_order: Vec::new(),
        }
    }

    /// Uploads a GPU texture for `image` the first time this exact `Arc`
    /// pointer is seen; later calls with the same pointer (same decoded
    /// image, still alive in the current page's `Frame`) reuse it. Returns
    /// the cache key so `prepare` can update just the position uniform.
    fn get_or_create(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, image: &DecodedImage) -> usize {
        let key = std::sync::Arc::as_ptr(&image.rgba) as *const u8 as usize;
        if !self.cache.contains_key(&key) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("image_texture"),
                size: wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &image.rgba,
                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * image.width), rows_per_image: Some(image.height) },
                wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 },
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let position_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image_position_buffer"),
                size: std::mem::size_of::<ImageUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("image_texture_bind_group"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: position_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });

            self.cache.insert(key, CachedImage { position_buffer, bind_group });
        }
        key
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame, scale_factor: f32, viewport_width: f32, viewport_height: f32) {
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[viewport_width, viewport_height]));

        self.draw_order.clear();
        for scaled in build_image_instances(frame, scale_factor) {
            let key = self.get_or_create(device, queue, &scaled.image);
            let cached = self.cache.get(&key).expect("just inserted or already present");
            queue.write_buffer(&cached.position_buffer, 0, bytemuck::cast_slice(&[scaled.x, scaled.y, scaled.width, scaled.height]));
            self.draw_order.push(key);
        }
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>) {
        if self.draw_order.is_empty() {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for key in &self.draw_order {
            let cached = self.cache.get(key).expect("draw_order only contains keys inserted this frame");
            render_pass.set_bind_group(1, &cached.bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{RectCommand, TextCommand};

    fn tiny_image() -> DecodedImage {
        DecodedImage { width: 1, height: 1, rgba: std::sync::Arc::from(vec![1u8, 2, 3, 4]) }
    }

    #[test]
    fn extracts_only_image_commands_in_order() {
        let mut frame = Frame::new();
        frame.push(DrawCommand::Rect(RectCommand { x: 0.0, y: 0.0, width: 1.0, height: 1.0, color: [0.0; 4], corner_radius: 0.0 }));
        frame.push(DrawCommand::Image(ImageCommand { x: 1.0, y: 2.0, width: 10.0, height: 20.0, image: tiny_image() }));
        frame.push(DrawCommand::Text(TextCommand { x: 0.0, y: 0.0, content: "ignored".into(), size: 12.0, color: [0.0; 4] }));
        frame.push(DrawCommand::Image(ImageCommand { x: 5.0, y: 6.0, width: 30.0, height: 40.0, image: tiny_image() }));

        let instances = build_image_instances(&frame, 1.0);
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].x, 1.0);
        assert_eq!(instances[0].y, 2.0);
        assert_eq!(instances[1].x, 5.0);
    }

    #[test]
    fn empty_frame_yields_no_image_instances() {
        let frame = Frame::new();
        assert!(build_image_instances(&frame, 1.0).is_empty());
    }

    #[test]
    fn applies_scale_factor_to_position_and_size_not_pixel_dimensions() {
        let mut frame = Frame::new();
        frame.push(DrawCommand::Image(ImageCommand { x: 10.0, y: 20.0, width: 30.0, height: 40.0, image: tiny_image() }));

        let instances = build_image_instances(&frame, 2.0);
        assert_eq!(instances[0].x, 20.0);
        assert_eq!(instances[0].y, 40.0);
        assert_eq!(instances[0].width, 60.0);
        assert_eq!(instances[0].height, 80.0);
        assert_eq!(instances[0].image.width, 1, "pixel dimensions of the decoded image are never scaled");
    }
}
```

- [ ] **Step 4: Register the new module and wire `ImagePipeline` into `Renderer`**

Modify `ferris-compositor/src/renderer/mod.rs` — add `pub mod image;` next to the existing `pub mod quad;`/`pub mod text;` lines.

Add `image_pipeline: image::ImagePipeline` to the `Renderer` struct (next to `quad_pipeline`/`text_layer`), construct it in `Renderer::new` right after `let text_layer = text::TextLayer::new(&device, &queue, format);` with `let image_pipeline = image::ImagePipeline::new(&device, format);`, and add it to the final `Self { ... }` struct literal.

In `render_frame`, right after the existing `self.text_layer.prepare(...)` call, add:
```rust
        self.image_pipeline.prepare(&self.device, &self.queue, frame, self.scale_factor, self.config.width as f32, self.config.height as f32);
```
And inside the render pass block, right after `self.text_layer.render(&mut pass);`, add:
```rust
            self.image_pipeline.render(&mut pass);
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --package ferris-compositor`
Expected: `test result: ok. 3 passed; 0 failed` for the new `renderer::image` module (matches `renderer::quad`'s own 3-test pattern), plus all pre-existing `ferris-compositor` tests unaffected.

- [ ] **Step 6: Run the whole workspace build**

Run: `cargo build --workspace`
Expected: still fails in `ferris-compositor`'s `main.rs` (still calls the old-signature `load_page`/`layout`) — expected, fixed in the final Task 5.

- [ ] **Step 7: Commit**

```bash
git add ferris-compositor/src/renderer/image.rs ferris-compositor/src/renderer/mod.rs ferris-compositor/src/renderer/quad.rs ferris-compositor/src/chrome.rs
git commit -m "feat: add a GPU texture pipeline for <img>, wired into Renderer::render_frame"
```

---

### Task 5: Wire images into `main.rs` and verify manually

**Files:**
- Modify: `ferris-compositor/src/main.rs`

**Interfaces:**
- Consumes: `ferris_loader::load_page` (Task 2, now returns a 3-tuple including `ImageMap`), `ferris_layout::layout::layout_with_images` (Task 3), everything else unchanged from sub-project 2.10.
- Produces: nothing consumed by later tasks — this is the final task of the plan.

- [ ] **Step 1: Read the current file before editing**

Read `ferris-compositor/src/main.rs` in full to confirm it still matches the state left by sub-project 2.10 (three call sites destructure `load_page`'s 2-tuple and call `layout::layout(...)`: `resumed()`, `navigate_interactive()`, and `build_page_frame`'s signature itself).

- [ ] **Step 2: Update `build_page_frame` to accept and use an `ImageMap`**

Change:
```rust
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
```
to:
```rust
fn build_page_frame(root: &ferris_dom::dom::Element, stylesheet: &ferris_css::stylesheet::Stylesheet, images: &ferris_scene::ImageMap, viewport_width: f32, viewport_height: f32) -> scene::Frame {
    let styled = resolve_styles(root, stylesheet);
    match layout::layout_with_images(&styled, viewport_width, viewport_height, images) {
        Some(layout_box) => paint(&layout_box),
        None => {
            log::warn!("page root has no visible layout (display:none); showing an empty frame");
            scene::Frame::default()
        }
    }
}
```

- [ ] **Step 3: Update the 2 call sites that call `ferris_loader::load_page`**

In `resumed()`, change:
```rust
                match ferris_loader::load_page(&self.initial_source) {
                    Ok((root, stylesheet)) => {
                        let chrome = Chrome::new(self.initial_source.clone(), chrome::source_display_text(&self.initial_source));
                        let height = gpu.logical_height() - chrome.bar_height - self.tab_strip.height;
                        let page_frame = Some(build_page_frame(&root, &stylesheet, gpu.logical_width(), height));
                        self.tabs.push(Tab { chrome, page_frame });
                        self.active_tab = 0;
                    }
                    Err(ferris_loader::LoadError::Fetch(msg)) => {
                        log::error!("failed to load initial page: {msg}");
                        std::process::exit(1);
                    }
                }
```
to:
```rust
                match ferris_loader::load_page(&self.initial_source) {
                    Ok((root, stylesheet, images)) => {
                        let chrome = Chrome::new(self.initial_source.clone(), chrome::source_display_text(&self.initial_source));
                        let height = gpu.logical_height() - chrome.bar_height - self.tab_strip.height;
                        let page_frame = Some(build_page_frame(&root, &stylesheet, &images, gpu.logical_width(), height));
                        self.tabs.push(Tab { chrome, page_frame });
                        self.active_tab = 0;
                    }
                    Err(ferris_loader::LoadError::Fetch(msg)) => {
                        log::error!("failed to load initial page: {msg}");
                        std::process::exit(1);
                    }
                }
```

In `navigate_interactive`, change:
```rust
    fn navigate_interactive(&mut self, source: ferris_loader::Source) -> bool {
        match ferris_loader::load_page(&source) {
            Ok((root, stylesheet)) => {
                let bar_height = self.active_tab().map(|t| t.chrome.bar_height).unwrap_or(0.0);
                if let Some(gpu) = &self.gpu {
                    let height = gpu.logical_height() - bar_height - self.tab_strip.height;
                    let frame = build_page_frame(&root, &stylesheet, gpu.logical_width(), height);
                    if let Some(tab) = self.active_tab_mut() {
                        tab.page_frame = Some(frame);
                    }
                }
                if let Some(tab) = self.active_tab_mut() {
                    let text = chrome::source_display_text(&source);
                    tab.chrome.address_bar.set_text(&text);
                }
                true
            }
            Err(ferris_loader::LoadError::Fetch(msg)) => {
                log::warn!("navigation failed: {msg}");
                if let Some(tab) = self.active_tab_mut() {
                    tab.chrome.address_bar.set_error(Some(msg));
                }
                false
            }
        }
    }
```
to:
```rust
    fn navigate_interactive(&mut self, source: ferris_loader::Source) -> bool {
        match ferris_loader::load_page(&source) {
            Ok((root, stylesheet, images)) => {
                let bar_height = self.active_tab().map(|t| t.chrome.bar_height).unwrap_or(0.0);
                if let Some(gpu) = &self.gpu {
                    let height = gpu.logical_height() - bar_height - self.tab_strip.height;
                    let frame = build_page_frame(&root, &stylesheet, &images, gpu.logical_width(), height);
                    if let Some(tab) = self.active_tab_mut() {
                        tab.page_frame = Some(frame);
                    }
                }
                if let Some(tab) = self.active_tab_mut() {
                    let text = chrome::source_display_text(&source);
                    tab.chrome.address_bar.set_text(&text);
                }
                true
            }
            Err(ferris_loader::LoadError::Fetch(msg)) => {
                log::warn!("navigation failed: {msg}");
                if let Some(tab) = self.active_tab_mut() {
                    tab.chrome.address_bar.set_error(Some(msg));
                }
                false
            }
        }
    }
```

- [ ] **Step 4: Fix the one test in this file that calls `build_page_frame` directly**

`build_page_frame_on_display_none_root_returns_empty_frame_without_panicking` currently calls `build_page_frame(&root, &stylesheet, 800.0, 600.0)`. Change it to pass an empty images map: `build_page_frame(&root, &stylesheet, &ferris_scene::ImageMap::new(), 800.0, 600.0)`.

- [ ] **Step 5: Run the workspace build and test suite**

Run: `cargo build --workspace`
Expected: clean build, no warnings — this is the point where every crate touched by this plan finally compiles together.

Run: `cargo test --workspace`
Expected: 333 passed (314 baseline + 3 from Task 1 + 7 from Task 2 + 5 from Task 3's `ferris-layout` + 1 from Task 3's `ferris-paint` + 3 from Task 4's `renderer::image` = 314+3+7+5+1+3 = 333).

- [ ] **Step 6: Manual verification — a real local image and a real URL image**

Run `cargo build --release -p ferris-compositor` once, then run it against real test pages using the techniques already established in this project (PowerShell `Start-Process`/background run + `GetClientRect`+`ClientToScreen`+`CopyFromScreen` for screenshots):

- Create a small real PNG file in your scratchpad (e.g. write a 100x100 solid-color PNG using the `image` crate directly in a throwaway script, or use any real PNG you have access to) and an HTML file referencing it: `<html><body><img src="test.png"></body></html>`. Run `cargo run --release -p ferris-compositor -- <path to that html>` and confirm, via screenshot, that the image renders on screen at the expected position, at roughly its real pixel dimensions (no CSS `width`/`height` set, so intrinsic sizing applies).
- Run `cargo run --release -p ferris-compositor -- <path to a real HTML page with an `<img src="https://...">` pointing at a real, small, publicly reachable image>` (or a page you already know has one) and confirm the image loads and renders over a real network fetch.
- Confirm neither run panics, and confirm the existing non-image parts of each page (text, background colors) still render correctly alongside the image (the new `ImagePipeline` runs alongside, not instead of, `QuadPipeline`/`TextLayer`).

Note in the task's completion report exactly what was seen at each of these checks — a task reviewer for this task must ask the implementer to describe what appeared for both manual-verification runs, and treat an implementer that skipped either as not having completed the task's actual deliverable, same discipline as every prior sub-project's final GUI task in this project.

- [ ] **Step 7: Commit**

```bash
git add ferris-compositor/src/main.rs
git commit -m "feat: wire decoded images through main.rs (load_page -> layout_with_images -> paint)"
```

---
