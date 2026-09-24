# Layout-Compositor Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `ferris-paint`, a new crate that turns a `ferris-layout::LayoutBox` tree into a `ferris-compositor::scene::Frame`, and wire it into `ferris-compositor/src/main.rs` so the window shows a real HTML+CSS page instead of the animated demo — the project's first visible milestone.

**Architecture:** A `color.rs` module parses opaque CSS color strings (hex + a fixed keyword list) into `[f32; 4]`. A `paint.rs` module recursively walks a `LayoutBox` tree, pushing a background `RectCommand` (border-box area) and a `TextCommand` (direct text content, unwrapped) per node. `main.rs` runs the full parse→style→layout→paint pipeline once against an embedded HTML+CSS fixture and stores the resulting `Frame`, reusing it every redraw instead of calling `scene::build_test_scene`.

**Tech Stack:** Rust, Cargo workspace path dependencies on `ferris-layout` and `ferris-compositor`. No external dependencies in `ferris-paint`.

**Spec:** `docs/superpowers/specs/2026-09-24-layout-compositor-bridge-design.md`

## Global Constraints

- New crate `ferris-paint`, sixth workspace member, path-depends only on `ferris-layout` and `ferris-compositor` — no external crates.
- `parse_color` supports `#rgb`/`#rrggbb` hex and exactly these keywords (case-insensitive): `black`, `silver`, `gray`, `white`, `maroon`, `red`, `purple`, `fuchsia`, `green`, `lime`, `olive`, `yellow`, `navy`, `blue`, `teal`, `aqua`, `orange`, `transparent`. Anything else (absent, malformed, `rgb()`/`hsl()`, unrecognized keyword) resolves to `None` — never panics.
- `background-color` absent/unparseable → no `RectCommand` pushed for that node (transparent, matches CSS default). `color` (text) absent/unparseable → falls back to `[0.0, 0.0, 0.0, 1.0]` (black, CSS's initial value for that property).
- Background rects are painted at the **border-box** (padding + border + content), not the margin box, using the node's own `margin`/`border`/`padding` `Edges`.
- `corner_radius` on every `RectCommand` this crate produces is always `0.0` — `border-radius` is not interpreted.
- Border width already affects layout math (piece 2.4) but is never painted — no `border-color`/`border-style` exist anywhere in the pipeline yet.
- Text: concatenate all direct `Node::Text` children of `styled_node.element` (skip `Node::Comment`), no line-wrapping, no measurement — one `TextCommand` per node with non-empty (post-trim) text, positioned at the node's content-box origin (`x`, `y`).
- `font-size`: only an explicit `px` value is used; anything else (absent, `%`, `auto`) falls back to `16.0`.
- `main.rs` computes the page `Frame` once (in `App::resumed`, after `Renderer::new` succeeds) against the window's initial logical size, and reuses it on every redraw — no re-layout on window resize.
- The FPS overlay `TextCommand` currently appended each redraw in `main.rs` is preserved, appended after the page `Frame`'s own commands.
- `scene::build_test_scene` and its existing tests in `ferris-compositor/src/scene.rs` are left untouched — only what `main.rs` calls for the drawn `Frame` changes.

## Review Focus

- A `background-color` value using an unsupported format (e.g. `"rgb(255,0,0)"`, `"hsl(0,100%,50%)"`) must resolve to `None` (no rect drawn), not panic and not silently draw a wrong/garbage color — covered in Task 1's `parse_color` tests.
- An element with non-zero `margin`/`border`/`padding` must have its background rect positioned and sized at the border-box, not the content-box or margin-box — a background that visually excludes the element's own padding would look wrong even though layout math (already tested in piece 2.4) is correct — covered in Task 2's `paint_node` tests.
- An element with a `Node::Comment` sitting between two `Node::Text` children (e.g. `"Hello <!-- x --> world"`) must still concatenate the surrounding text correctly, skipping only the comment — covered in Task 2's text-extraction tests.
- A CSS `color`/`background-color` keyword written in mixed case (e.g. `"Red"`, `"BLUE"`) must still parse — CSS keywords are case-insensitive and real-world CSS is inconsistently cased — covered in Task 1's `parse_color` tests.
- An element with `font-size` set to a non-`px` value (`%`, `auto`, or simply absent) must fall back to the documented default (`16.0`), not silently produce a wrong/zero size or panic — covered in Task 2's `paint_node` tests.

---

### Task 1: `color.rs` — CSS color parsing

**Files:**
- Create: `ferris-paint/Cargo.toml`
- Create: `ferris-paint/src/lib.rs`
- Create: `ferris-paint/src/color.rs`

**Interfaces:**
- Consumes: nothing (first task, pure string parsing, no dependency on `ferris-layout`/`ferris-compositor` types).
- Produces: `pub fn parse_color(value: Option<&str>) -> Option<[f32; 4]>`, used by Task 2.

- [ ] **Step 1: Add `ferris-paint` to the workspace**

Read `Cargo.toml` at the repo root first to confirm the current `members` list, then add `"ferris-paint"` to it. As of this plan it reads:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style", "ferris-layout"]
resolver = "2"
```

Change to:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style", "ferris-layout", "ferris-paint"]
resolver = "2"
```

- [ ] **Step 2: Create the crate manifest**

Create `ferris-paint/Cargo.toml`:

```toml
[package]
name = "ferris-paint"
version = "0.1.0"
edition = "2021"

[dependencies]
ferris-layout = { path = "../ferris-layout" }
ferris-compositor = { path = "../ferris-compositor" }
```

- [ ] **Step 3: Write the failing tests for `parse_color`**

Create `ferris-paint/src/color.rs`:

```rust
pub fn parse_color(value: Option<&str>) -> Option<[f32; 4]> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_value_is_none() {
        assert_eq!(parse_color(None), None);
    }

    #[test]
    fn empty_string_is_none() {
        assert_eq!(parse_color(Some("")), None);
    }

    #[test]
    fn parses_six_digit_hex() {
        assert_eq!(parse_color(Some("#ff0000")), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color(Some("#00ff00")), Some([0.0, 1.0, 0.0, 1.0]));
        assert_eq!(parse_color(Some("#0000ff")), Some([0.0, 0.0, 1.0, 1.0]));
    }

    #[test]
    fn parses_six_digit_hex_uppercase() {
        assert_eq!(parse_color(Some("#FF0000")), Some([1.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn parses_three_digit_hex_by_duplicating_each_digit() {
        // #f00 -> #ff0000
        assert_eq!(parse_color(Some("#f00")), Some([1.0, 0.0, 0.0, 1.0]));
        // #08f -> #0088ff
        let c = parse_color(Some("#08f")).unwrap();
        assert!((c[0] - 0.0).abs() < 0.001);
        assert!((c[1] - (0x88 as f32 / 255.0)).abs() < 0.001);
        assert!((c[2] - 1.0).abs() < 0.001);
    }

    #[test]
    fn hex_wrong_length_is_none() {
        assert_eq!(parse_color(Some("#ff00")), None);
        assert_eq!(parse_color(Some("#ff00000")), None);
    }

    #[test]
    fn hex_invalid_characters_is_none() {
        assert_eq!(parse_color(Some("#zzzzzz")), None);
    }

    #[test]
    fn parses_all_sixteen_css1_keywords_plus_orange_and_transparent() {
        let cases: &[(&str, [f32; 4])] = &[
            ("black", [0.0, 0.0, 0.0, 1.0]),
            ("silver", [0.75294125, 0.75294125, 0.75294125, 1.0]),
            ("gray", [0.5019608, 0.5019608, 0.5019608, 1.0]),
            ("white", [1.0, 1.0, 1.0, 1.0]),
            ("maroon", [0.5019608, 0.0, 0.0, 1.0]),
            ("red", [1.0, 0.0, 0.0, 1.0]),
            ("purple", [0.5019608, 0.0, 0.5019608, 1.0]),
            ("fuchsia", [1.0, 0.0, 1.0, 1.0]),
            ("green", [0.0, 0.5019608, 0.0, 1.0]),
            ("lime", [0.0, 1.0, 0.0, 1.0]),
            ("olive", [0.5019608, 0.5019608, 0.0, 1.0]),
            ("yellow", [1.0, 1.0, 0.0, 1.0]),
            ("navy", [0.0, 0.0, 0.5019608, 1.0]),
            ("blue", [0.0, 0.0, 1.0, 1.0]),
            ("teal", [0.0, 0.5019608, 0.5019608, 1.0]),
            ("aqua", [0.0, 1.0, 1.0, 1.0]),
            ("orange", [1.0, 0.64705884, 0.0, 1.0]),
            ("transparent", [0.0, 0.0, 0.0, 0.0]),
        ];
        for (name, expected) in cases {
            let got = parse_color(Some(name)).unwrap_or_else(|| panic!("{name} should parse"));
            for i in 0..4 {
                assert!(
                    (got[i] - expected[i]).abs() < 0.001,
                    "{name}: channel {i} got {got:?}, expected {expected:?}"
                );
            }
        }
    }

    #[test]
    fn keyword_matching_is_case_insensitive() {
        assert_eq!(parse_color(Some("RED")), parse_color(Some("red")));
        assert_eq!(parse_color(Some("Blue")), parse_color(Some("blue")));
        assert_eq!(parse_color(Some("TRANSPARENT")), parse_color(Some("transparent")));
    }

    #[test]
    fn unrecognized_keyword_is_none() {
        assert_eq!(parse_color(Some("rebeccapurple")), None);
        assert_eq!(parse_color(Some("banana")), None);
    }

    #[test]
    fn unsupported_function_syntax_is_none() {
        assert_eq!(parse_color(Some("rgb(255, 0, 0)")), None);
        assert_eq!(parse_color(Some("hsl(0, 100%, 50%)")), None);
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --package ferris-paint color:: -- --nocapture`
Expected: compile error or panic on `todo!()`.

- [ ] **Step 5: Implement `parse_color`**

Replace the `todo!()` body in `ferris-paint/src/color.rs`:

```rust
pub fn parse_color(value: Option<&str>) -> Option<[f32; 4]> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(hex) = raw.strip_prefix('#') {
        return parse_hex(hex);
    }

    parse_keyword(raw)
}

fn parse_hex(hex: &str) -> Option<[f32; 4]> {
    let expanded: String = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.to_string(),
        _ => return None,
    };

    let r = u8::from_str_radix(&expanded[0..2], 16).ok()?;
    let g = u8::from_str_radix(&expanded[2..4], 16).ok()?;
    let b = u8::from_str_radix(&expanded[4..6], 16).ok()?;

    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
}

fn parse_keyword(raw: &str) -> Option<[f32; 4]> {
    let rgb = match raw.to_ascii_lowercase().as_str() {
        "black" => [0, 0, 0],
        "silver" => [192, 192, 192],
        "gray" => [128, 128, 128],
        "white" => [255, 255, 255],
        "maroon" => [128, 0, 0],
        "red" => [255, 0, 0],
        "purple" => [128, 0, 128],
        "fuchsia" => [255, 0, 255],
        "green" => [0, 128, 0],
        "lime" => [0, 255, 0],
        "olive" => [128, 128, 0],
        "yellow" => [255, 255, 0],
        "navy" => [0, 0, 128],
        "blue" => [0, 0, 255],
        "teal" => [0, 128, 128],
        "aqua" => [0, 255, 255],
        "orange" => [255, 165, 0],
        "transparent" => return Some([0.0, 0.0, 0.0, 0.0]),
        _ => return None,
    };
    Some([rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, 1.0])
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --package ferris-paint color::`
Expected: 11 passed.

- [ ] **Step 7: Add the crate's `lib.rs`**

Create `ferris-paint/src/lib.rs`:

```rust
pub mod color;
```

- [ ] **Step 8: Build the whole workspace to confirm the new member compiles cleanly**

Run: `cargo build --workspace`
Expected: clean build, no warnings, `ferris-paint` now listed among the built packages.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml ferris-paint/Cargo.toml ferris-paint/src/lib.rs ferris-paint/src/color.rs
git commit -m "feat: add ferris-paint crate with CSS color parsing"
```

---

### Task 2: `paint.rs` — LayoutBox → Frame

**Files:**
- Create: `ferris-paint/src/paint.rs`
- Modify: `ferris-paint/src/lib.rs`

**Interfaces:**
- Consumes: `crate::color::parse_color` (Task 1); `ferris_layout::layout::LayoutBox<'a>` (`x`, `y`, `width`, `height: f32`, `margin`, `border`, `padding: ferris_layout::layout::Edges`, `children: Vec<LayoutBox<'a>>`, `styled_node: &'a ferris_style::StyledNode<'a>`); `ferris_style::StyledNode<'a>` (`element: &'a ferris_dom::dom::Element`, `style: HashMap<String, String>`); `ferris_dom::dom::{Element, Node}` (`Element.children: Vec<Node>`, `Node::{Text(String), Comment(String), Element(Element)}`); `ferris_layout::length::{Length, parse_length}`; `ferris_compositor::scene::{Frame, DrawCommand, RectCommand, TextCommand}`.
- Produces: `pub fn paint(root: &LayoutBox) -> Frame`, used directly by `main.rs` in Task 3.

- [ ] **Step 1: Write the failing tests**

Create `ferris-paint/src/paint.rs`:

```rust
use std::collections::HashMap;

use ferris_compositor::scene::{DrawCommand, Frame, RectCommand, TextCommand};
use ferris_dom::dom::Node;
use ferris_layout::layout::LayoutBox;
use ferris_layout::length::{parse_length, Length};

use crate::color::parse_color;

pub fn paint(root: &LayoutBox) -> Frame {
    let mut frame = Frame::new();
    paint_node(root, &mut frame);
    frame
}

fn paint_node(node: &LayoutBox, frame: &mut Frame) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    use ferris_dom::dom::Element;
    use ferris_layout::layout::Edges;
    use ferris_style::StyledNode;

    fn style_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

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
            children: Vec::new(),
        }
    }

    #[test]
    fn background_color_produces_a_rect_at_the_border_box() {
        let element = Element::new("div");
        let styled = StyledNode {
            element: &element,
            style: style_with(&[("background-color", "#ff0000")]),
            children: Vec::new(),
        };
        let edges = Edges { top: 2.0, right: 3.0, bottom: 4.0, left: 5.0 };
        let node = leaf_box(&styled, 100.0, 50.0, 200.0, 80.0, edges);

        let frame = paint(&node);
        assert_eq!(frame.commands.len(), 1);
        let DrawCommand::Rect(rect) = &frame.commands[0] else { panic!("expected a rect") };
        // border-box = content box expanded by this node's own border+padding
        // (both set to `edges` in this test fixture) on every side.
        assert_eq!(rect.x, 100.0 - 5.0 - 5.0); // left: padding.left + border.left
        assert_eq!(rect.y, 50.0 - 2.0 - 2.0); // top: padding.top + border.top
        assert_eq!(rect.width, 200.0 + (5.0 + 5.0) * 2.0); // + left/right padding+border on both sides
        assert_eq!(rect.height, 80.0 + (2.0 + 2.0) * 2.0);
        assert_eq!(rect.color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(rect.corner_radius, 0.0);
    }

    #[test]
    fn no_background_color_produces_no_rect() {
        let element = Element::new("div");
        let styled = StyledNode { element: &element, style: HashMap::new(), children: Vec::new() };
        let node = leaf_box(&styled, 0.0, 0.0, 100.0, 50.0, Edges::default());

        let frame = paint(&node);
        assert!(frame.commands.is_empty());
    }

    #[test]
    fn direct_text_child_produces_a_text_command_at_content_origin() {
        let mut element = Element::new("p");
        element.children.push(Node::Text("Hello".to_string()));
        let styled = StyledNode { element: &element, style: HashMap::new(), children: Vec::new() };
        let node = leaf_box(&styled, 10.0, 20.0, 100.0, 30.0, Edges::default());

        let frame = paint(&node);
        assert_eq!(frame.commands.len(), 1);
        let DrawCommand::Text(text) = &frame.commands[0] else { panic!("expected text") };
        assert_eq!(text.content, "Hello");
        assert_eq!(text.x, 10.0);
        assert_eq!(text.y, 20.0);
    }

    #[test]
    fn multiple_text_children_around_a_comment_are_concatenated_skipping_the_comment() {
        let mut element = Element::new("p");
        element.children.push(Node::Text("Hello ".to_string()));
        element.children.push(Node::Comment("note".to_string()));
        element.children.push(Node::Text("world".to_string()));
        let styled = StyledNode { element: &element, style: HashMap::new(), children: Vec::new() };
        let node = leaf_box(&styled, 0.0, 0.0, 100.0, 30.0, Edges::default());

        let frame = paint(&node);
        let DrawCommand::Text(text) = &frame.commands[0] else { panic!("expected text") };
        assert_eq!(text.content, "Hello world");
    }

    #[test]
    fn whitespace_only_text_produces_no_text_command() {
        let mut element = Element::new("div");
        element.children.push(Node::Text("   ".to_string()));
        let styled = StyledNode { element: &element, style: HashMap::new(), children: Vec::new() };
        let node = leaf_box(&styled, 0.0, 0.0, 100.0, 30.0, Edges::default());

        let frame = paint(&node);
        assert!(frame.commands.is_empty());
    }

    #[test]
    fn font_size_in_px_is_used_directly() {
        let mut element = Element::new("p");
        element.children.push(Node::Text("Hi".to_string()));
        let styled = StyledNode { element: &element, style: style_with(&[("font-size", "24px")]), children: Vec::new() };
        let node = leaf_box(&styled, 0.0, 0.0, 100.0, 30.0, Edges::default());

        let frame = paint(&node);
        let DrawCommand::Text(text) = &frame.commands[0] else { panic!("expected text") };
        assert_eq!(text.size, 24.0);
    }

    #[test]
    fn font_size_absent_or_non_px_falls_back_to_sixteen() {
        let mut element_absent = Element::new("p");
        element_absent.children.push(Node::Text("Hi".to_string()));
        let styled_absent = StyledNode { element: &element_absent, style: HashMap::new(), children: Vec::new() };
        let node_absent = leaf_box(&styled_absent, 0.0, 0.0, 100.0, 30.0, Edges::default());
        let frame_absent = paint(&node_absent);
        let DrawCommand::Text(text_absent) = &frame_absent.commands[0] else { panic!("expected text") };
        assert_eq!(text_absent.size, 16.0);

        let mut element_percent = Element::new("p");
        element_percent.children.push(Node::Text("Hi".to_string()));
        let styled_percent = StyledNode { element: &element_percent, style: style_with(&[("font-size", "150%")]), children: Vec::new() };
        let node_percent = leaf_box(&styled_percent, 0.0, 0.0, 100.0, 30.0, Edges::default());
        let frame_percent = paint(&node_percent);
        let DrawCommand::Text(text_percent) = &frame_percent.commands[0] else { panic!("expected text") };
        assert_eq!(text_percent.size, 16.0);
    }

    #[test]
    fn text_color_falls_back_to_black_when_absent_or_unparseable() {
        let mut element = Element::new("p");
        element.children.push(Node::Text("Hi".to_string()));
        let styled = StyledNode { element: &element, style: style_with(&[("color", "not-a-color")]), children: Vec::new() };
        let node = leaf_box(&styled, 0.0, 0.0, 100.0, 30.0, Edges::default());

        let frame = paint(&node);
        let DrawCommand::Text(text) = &frame.commands[0] else { panic!("expected text") };
        assert_eq!(text.color, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn text_color_uses_the_parsed_css_color_when_valid() {
        let mut element = Element::new("p");
        element.children.push(Node::Text("Hi".to_string()));
        let styled = StyledNode { element: &element, style: style_with(&[("color", "blue")]), children: Vec::new() };
        let node = leaf_box(&styled, 0.0, 0.0, 100.0, 30.0, Edges::default());

        let frame = paint(&node);
        let DrawCommand::Text(text) = &frame.commands[0] else { panic!("expected text") };
        assert_eq!(text.color, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn recursion_visits_all_descendants_background_before_children() {
        let child_element = Element::new("span");
        let child_styled = StyledNode { element: &child_element, style: style_with(&[("background-color", "blue")]), children: Vec::new() };
        let child_box = leaf_box(&child_styled, 5.0, 5.0, 10.0, 10.0, Edges::default());

        let parent_element = Element::new("div");
        let parent_styled = StyledNode { element: &parent_element, style: style_with(&[("background-color", "red")]), children: vec![] };
        let parent_box = LayoutBox {
            styled_node: &parent_styled,
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            margin: Edges::default(),
            border: Edges::default(),
            padding: Edges::default(),
            children: vec![child_box],
        };

        let frame = paint(&parent_box);
        assert_eq!(frame.commands.len(), 2);
        let DrawCommand::Rect(first) = &frame.commands[0] else { panic!("expected rect") };
        let DrawCommand::Rect(second) = &frame.commands[1] else { panic!("expected rect") };
        assert_eq!(first.color, [1.0, 0.0, 0.0, 1.0], "parent's background must be painted before the child's");
        assert_eq!(second.color, [0.0, 0.0, 1.0, 1.0]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-paint paint:: -- --nocapture`
Expected: compile error or panic on `todo!()` in `paint_node`.

- [ ] **Step 3: Implement `paint_node`**

Replace the `todo!()` body:

```rust
fn paint_node(node: &LayoutBox, frame: &mut Frame) {
    let bx = node.x - node.padding.left - node.border.left;
    let by = node.y - node.padding.top - node.border.top;
    let bw = node.width + node.padding.left + node.padding.right + node.border.left + node.border.right;
    let bh = node.height + node.padding.top + node.padding.bottom + node.border.top + node.border.bottom;

    if let Some(color) = parse_color(node.styled_node.style.get("background-color").map(String::as_str)) {
        frame.push(DrawCommand::Rect(RectCommand { x: bx, y: by, width: bw, height: bh, color, corner_radius: 0.0 }));
    }

    let text: String = node
        .styled_node
        .element
        .children
        .iter()
        .filter_map(|child| match child {
            Node::Text(s) => Some(s.as_str()),
            Node::Comment(_) | Node::Element(_) => None,
        })
        .collect();

    if !text.trim().is_empty() {
        let size = match parse_length(node.styled_node.style.get("font-size").map(String::as_str)) {
            Length::Px(n) => n,
            Length::Percent(_) | Length::Auto => 16.0,
        };
        let color = parse_color(node.styled_node.style.get("color").map(String::as_str))
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        frame.push(DrawCommand::Text(TextCommand { x: node.x, y: node.y, content: text, size, color }));
    }

    for child in &node.children {
        paint_node(child, frame);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-paint paint::`
Expected: 10 passed.

- [ ] **Step 5: Add the module to `lib.rs`**

Modify `ferris-paint/src/lib.rs`:

```rust
pub mod color;
pub mod paint;
```

- [ ] **Step 6: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: all prior crates' tests still pass, plus `ferris-paint`'s 21 tests (11 color + 10 paint). Prior workspace baseline is 166 (post-2.4, after its final-review fix wave — confirmed via `git log`/the pushed state). Workspace total after this task: 166 + 21 = 187.

- [ ] **Step 7: Commit**

```bash
git add ferris-paint/src/lib.rs ferris-paint/src/paint.rs
git commit -m "feat: add paint() to turn a LayoutBox tree into a scene::Frame"
```

---

### Task 3: Integration tests against the real parser pipeline

**Files:**
- Create: `ferris-paint/tests/integration.rs`

**Interfaces:**
- Consumes: `ferris_dom::tokenizer::Tokenizer`, `ferris_dom::parser::Parser` (Task 3's helper mirrors the established `parse_root_element` pattern from `ferris-layout/tests/integration.rs`); `ferris_css::tokenizer::Tokenizer`, `ferris_css::parser::Parser`; `ferris_style::resolve_styles`; `ferris_layout::layout::layout`; `ferris_paint::paint::paint` (Task 2).
- Produces: nothing new for later tasks.

- [ ] **Step 1: Write the integration tests**

Create `ferris-paint/tests/integration.rs`:

```rust
// Note on parser entry points: `ferris_dom::parser::Parser::parse` and
// `ferris_css::parser::Parser::parse` take `&[Token]`, not `&str` — each
// crate tokenizes first, then parses. This matches the established pattern
// in `ferris-style/tests/integration.rs` and `ferris-layout/tests/integration.rs`.

use ferris_compositor::scene::DrawCommand;
use ferris_css::parser::Parser as CssParser;
use ferris_css::stylesheet::Stylesheet;
use ferris_css::tokenizer::Tokenizer as CssTokenizer;
use ferris_dom::dom::{Element, Node};
use ferris_dom::parser::Parser as DomParser;
use ferris_dom::tokenizer::Tokenizer as DomTokenizer;
use ferris_layout::layout::layout;
use ferris_paint::paint::paint;
use ferris_style::resolve_styles;

fn parse_css(css: &str) -> Stylesheet {
    let tokens = CssTokenizer::tokenize(css);
    CssParser::parse(&tokens)
}

/// Parses `html` and returns the first actual element in the document
/// (skipping the parser's synthetic "document" wrapper element, and any
/// leading/trailing whitespace-only text nodes produced by indented test
/// fixtures).
fn parse_root_element(html: &str) -> Element {
    let tokens = DomTokenizer::tokenize(html);
    let Node::Element(document) = DomParser::parse(&tokens) else {
        panic!("expected document element")
    };
    document
        .children
        .into_iter()
        .find_map(|child| match child {
            Node::Element(el) => Some(el),
            Node::Text(_) | Node::Comment(_) => None,
        })
        .expect("expected at least one root element in the document")
}

#[test]
fn realistic_page_paints_background_rects_and_text_in_the_right_places() {
    let html = r#"
        <div id="page">
            <header id="head">Ferris</header>
            <main id="content">
                <p id="para1">Hello</p>
            </main>
        </div>
    "#;
    let css = r#"
        #page { width: 800px; background-color: white; }
        #head { height: 60px; background-color: #003366; color: yellow; font-size: 20px; }
        #content { padding-top: 10px; }
        #para1 { height: 20px; color: black; }
    "#;

    let page = parse_root_element(html);
    let stylesheet = parse_css(css);
    let styled = resolve_styles(&page, &stylesheet);
    let layout_box = layout(&styled, 1024.0, 768.0).unwrap();
    let frame = paint(&layout_box);

    let rects: Vec<_> = frame.commands.iter().filter_map(|c| match c {
        DrawCommand::Rect(r) => Some(r),
        DrawCommand::Text(_) => None,
    }).collect();
    let texts: Vec<_> = frame.commands.iter().filter_map(|c| match c {
        DrawCommand::Text(t) => Some(t),
        DrawCommand::Rect(_) => None,
    }).collect();

    // #page and #head both have background-color; #content and #para1 don't.
    assert_eq!(rects.len(), 2);
    assert_eq!(rects[0].color, [1.0, 1.0, 1.0, 1.0], "#page is white");
    assert_eq!(rects[0].width, 800.0);
    let head_rgb = [0x00 as f32 / 255.0, 0x33 as f32 / 255.0, 0x66 as f32 / 255.0];
    assert!((rects[1].color[0] - head_rgb[0]).abs() < 0.01);
    assert!((rects[1].color[1] - head_rgb[1]).abs() < 0.01);
    assert!((rects[1].color[2] - head_rgb[2]).abs() < 0.01);

    // #head and #para1 both have direct text.
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0].content, "Ferris");
    assert_eq!(texts[0].size, 20.0);
    assert_eq!(texts[0].color, [1.0, 1.0, 0.0, 1.0], "#head text is yellow");
    assert_eq!(texts[1].content, "Hello");
    assert_eq!(texts[1].color, [0.0, 0.0, 0.0, 1.0], "#para1 text is black");
}

// --- Review Focus: an unsupported color format (rgb()/hsl()) must not
// panic and must not draw a rect, via the real parser pipeline ---
#[test]
fn unsupported_color_format_produces_no_rect_end_to_end() {
    let html = r#"<div id="box">text</div>"#;
    let css = "#box { background-color: rgb(10, 20, 30); }";

    let page = parse_root_element(html);
    let stylesheet = parse_css(css);
    let styled = resolve_styles(&page, &stylesheet);
    let layout_box = layout(&styled, 1024.0, 768.0).unwrap();
    let frame = paint(&layout_box);

    let has_rect = frame.commands.iter().any(|c| matches!(c, DrawCommand::Rect(_)));
    assert!(!has_rect, "an unparseable background-color must not produce a rect");
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --package ferris-paint --test integration`
Expected: PASS if Tasks 1-2 were implemented correctly (this task adds no new production code — it's a verification task exercising the real parser pipeline end to end). If an assertion fails, hand-trace the CSS/box-model math yourself before changing either the test's expected value or the production code — Tasks 1-2's code is already reviewed at this point, but a miscalculated expected value in this task's own draft is equally possible (this happened in piece 2.4's own Task 4).

- [ ] **Step 3: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: all tests pass — prior crates unchanged, `ferris-paint` at 21 unit + 2 integration = 23 tests, workspace total 166 + 23 = 189.

- [ ] **Step 4: Run clippy on the new crate**

Run: `cargo clippy --package ferris-paint --all-targets`
Expected: no new warnings.

- [ ] **Step 5: Commit**

```bash
git add ferris-paint/tests/integration.rs
git commit -m "test: add end-to-end paint integration tests against the real parser pipeline"
```

---

### Task 4: Wire the real pipeline into `ferris-compositor/src/main.rs`

**Files:**
- Modify: `ferris-compositor/Cargo.toml`
- Create: `ferris-compositor/assets/fixture.html`
- Create: `ferris-compositor/assets/fixture.css`
- Modify: `ferris-compositor/src/main.rs`

**Interfaces:**
- Consumes: `ferris_paint::paint::paint` (Task 2); `ferris_layout::layout::layout`; `ferris_style::resolve_styles`; `ferris_dom::{tokenizer::Tokenizer, parser::Parser}`; `ferris_css::{tokenizer::Tokenizer, parser::Parser}`; `Renderer::logical_width`/`logical_height` (already exist, `ferris-compositor/src/renderer/mod.rs:122,128`).
- Produces: nothing consumed by later tasks — this is the final task of the plan.

- [ ] **Step 1: Add the fixture files**

Create `ferris-compositor/assets/fixture.html`:

```html
<div id="page">
    <header id="head">Ferris</header>
    <main id="content">
        <p id="intro">A browser built from scratch in Rust.</p>
        <div id="card">
            <p id="card-text">Layout, style, and paint, all working together.</p>
        </div>
    </main>
</div>
```

Create `ferris-compositor/assets/fixture.css`:

```css
#page {
    width: 800px;
    background-color: white;
}
#head {
    height: 60px;
    background-color: #003366;
    color: white;
    font-size: 28px;
}
#content {
    padding-top: 20px;
    padding-left: 20px;
    padding-right: 20px;
}
#intro {
    height: 24px;
    color: #222222;
    font-size: 18px;
}
#card {
    margin-top: 16px;
    padding-top: 16px;
    padding-bottom: 16px;
    padding-left: 16px;
    padding-right: 16px;
    background-color: #eeeeee;
}
#card-text {
    height: 20px;
    color: black;
    font-size: 16px;
}
```

- [ ] **Step 2: Add `ferris-paint` (and its transitive parser/style/layout crates) as dependencies of `ferris-compositor`**

Read `ferris-compositor/Cargo.toml` first to see its current `[dependencies]` block, then add the path dependencies needed for the pipeline (`ferris-dom`, `ferris-css`, `ferris-style`, `ferris-layout`, `ferris-paint`) alongside whatever is already there (do not remove existing dependencies — `wgpu`, `winit`, `glyphon`, etc.):

```toml
ferris-dom = { path = "../ferris-dom" }
ferris-css = { path = "../ferris-css" }
ferris-style = { path = "../ferris-style" }
ferris-layout = { path = "../ferris-layout" }
ferris-paint = { path = "../ferris-paint" }
```

- [ ] **Step 3: Add a `page_frame` field to `App` and build it in `resumed`**

Modify `ferris-compositor/src/main.rs`. First, add the new imports near the top (alongside the existing `use ferris_compositor::{perf, renderer, scene};`):

```rust
use ferris_css::parser::Parser as CssParser;
use ferris_css::tokenizer::Tokenizer as CssTokenizer;
use ferris_dom::dom::Node;
use ferris_dom::parser::Parser as DomParser;
use ferris_dom::tokenizer::Tokenizer as DomTokenizer;
use ferris_paint::paint::paint;
use ferris_style::resolve_styles;
```

Add a `page_frame: Option<scene::Frame>` field to the `App` struct (`ferris-compositor/src/main.rs:12-19`):

```rust
struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Renderer>,
    start: std::time::Instant,
    frame_timer: perf::FrameTimer,
    last_frame_start: Option<std::time::Instant>,
    occluded: bool,
    page_frame: Option<scene::Frame>,
}
```

Add it to `Default::default()` (`ferris-compositor/src/main.rs:21-32`):

```rust
impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            gpu: None,
            start: std::time::Instant::now(),
            frame_timer: perf::FrameTimer::new(120),
            last_frame_start: None,
            occluded: false,
            page_frame: None,
        }
    }
}
```

Add a private helper function (outside `impl App`, near the top-level of `main.rs`) that runs the pipeline:

```rust
fn build_page_frame(viewport_width: f32, viewport_height: f32) -> scene::Frame {
    let html = include_str!("../assets/fixture.html");
    let css = include_str!("../assets/fixture.css");

    let html_tokens = DomTokenizer::tokenize(html);
    let Node::Element(document) = DomParser::parse(&html_tokens) else {
        panic!("fixture.html: expected a document element");
    };
    let root = document
        .children
        .into_iter()
        .find_map(|child| match child {
            Node::Element(el) => Some(el),
            Node::Text(_) | Node::Comment(_) => None,
        })
        .expect("fixture.html: expected at least one root element");

    let css_tokens = CssTokenizer::tokenize(css);
    let stylesheet = CssParser::parse(&css_tokens);

    let styled = resolve_styles(&root, &stylesheet);
    let layout_box = layout::layout(&styled, viewport_width, viewport_height)
        .expect("fixture.html's root must not be display:none");

    paint(&layout_box)
}
```

This needs one more import — `ferris_layout::layout` — add it alongside the others from this step:

```rust
use ferris_layout::layout;
```

In `App::resumed` (`ferris-compositor/src/main.rs:35-56`), after `self.gpu = Some(gpu); self.window = Some(window);` succeeds, build and store the page frame using the just-created `gpu`'s logical dimensions:

```rust
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
                self.page_frame = Some(build_page_frame(gpu.logical_width(), gpu.logical_height()));
                self.gpu = Some(gpu);
                self.window = Some(window);
            }
            None => {
                log::error!("GPU initialization failed, exiting");
                event_loop.exit();
            }
        }
    }
```

- [ ] **Step 4: Use `page_frame` in `RedrawRequested` instead of `build_test_scene`**

In the `WindowEvent::RedrawRequested` arm (`ferris-compositor/src/main.rs:74-113`), replace:

```rust
                let elapsed = self.start.elapsed().as_secs_f32();
                let (w, h) = (gpu.logical_width(), gpu.logical_height());
                let mut frame = scene::build_test_scene(elapsed, w, h);
```

with:

```rust
                let mut frame = self.page_frame.clone().unwrap_or_default();
```

Leave the rest of the `RedrawRequested` arm (the FPS overlay `TextCommand` push, and `gpu.render_frame(&frame)`) exactly as-is — only the line that builds `frame` changes. `scene::Frame` already derives `Clone` (confirm by reading `ferris-compositor/src/scene.rs:57` — `#[derive(Debug, Clone, Default)]`), so `.clone()` is available without further changes, and `.unwrap_or_default()` handles the case where `resumed` never ran (defensive, should not happen in practice since `RedrawRequested` only fires after `resumed`).

- [ ] **Step 5: Build and run the whole workspace**

Run: `cargo build --workspace`
Expected: clean build, no warnings.

Run: `cargo test --workspace`
Expected: 189/189 (unchanged from Task 3 — this task adds no new automated tests, per the spec's documented testing approach for GUI code).

- [ ] **Step 6: Manual verification — run the binary and look at the window**

This step cannot be automated; it is the task's actual acceptance test, per the spec.

Run: `cargo run -p ferris-compositor`

Confirm, by looking at the opened window:
- A white background area roughly 800px wide (the `#page` element).
- A dark blue bar near the top with white text reading "Ferris" (the `#head` element).
- Below it, dark gray/black text reading "A browser built from scratch in Rust." (the `#intro` element).
- Below that, a light gray box containing dark text reading "Layout, style, and paint, all working together." (the `#card`/`#card-text` elements).
- The FPS overlay text still visible (e.g. "120.0 fps | ..." near the top-left, overlapping or near the header — this is expected, it was not repositioned).
- No animated bouncing rectangles (the old demo is gone from what's drawn, though `scene::build_test_scene` still exists in the codebase and its own tests still pass).

Close the window (or Ctrl+C the process) once confirmed. Note in the task's completion report exactly what was seen, since this step has no automated pass/fail signal for a reviewer to check independently — a task reviewer for this task must ask the implementer to describe what appeared, and treat an implementer that skipped running the binary as not having completed the task's actual deliverable.

- [ ] **Step 7: Commit**

```bash
git add ferris-compositor/Cargo.toml ferris-compositor/assets/fixture.html ferris-compositor/assets/fixture.css ferris-compositor/src/main.rs
git commit -m "feat: render a real HTML+CSS fixture page in the compositor window"
```

---
