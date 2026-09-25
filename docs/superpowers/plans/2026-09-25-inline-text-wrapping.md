# Text Measurement and Line Wrapping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `ferris-text`, a crate that measures and wraps text using real font metrics (via `cosmic-text`, the shaping engine `glyphon` already uses), and wire it into `ferris-layout` so element boxes grow to fit wrapped text, replacing `ferris-paint`'s current single-line, un-measured text handling.

**Architecture:** `ferris-text` exposes a pure `wrap_lines`/`line_height` API backed by a lazily-initialized shared `cosmic-text` `FontSystem` (no GPU, no `wgpu`, no `glyphon`). `ferris-layout`'s `layout_block` extracts and collapses an element's direct text (logic moved from `ferris-paint`), measures/wraps it via `ferris-text`, and folds the resulting line count into the element's height before its block children stack. `ferris-paint` stops doing any text logic of its own — it just turns the already-wrapped `LayoutBox.text_lines` into one `TextCommand` per line.

**Tech Stack:** Rust, `cosmic-text` 0.12 (already resolved as a transitive dependency of `glyphon` in this workspace — pin the same major/minor to avoid a duplicate version), Cargo workspace path dependencies.

**Spec:** `docs/superpowers/specs/2026-09-25-inline-text-wrapping-design.md`

## Global Constraints

- New crate `ferris-text`, seventh workspace member, depends only on `cosmic-text = "0.12"` — no `wgpu`, no `glyphon`, no GPU dependency of any kind.
- `ferris-layout` gains a dependency on `ferris-text`.
- `LayoutBox` gains a new field `text_lines: Vec<String>` (additive — every existing field stays).
- `resolve_font_size` (validates `font-size`: only a finite, positive `px` value is used directly, everything else — absent, `%`, `auto`, zero, negative, non-finite — falls back to `16.0`) lives once in `ferris_layout::length`, used by both `layout_block` (to measure) and `ferris-paint` (to size the `TextCommand`) — never duplicated.
- Text extraction (concatenate direct `Node::Text` children, skip `Node::Comment`) and whitespace collapsing (`split_whitespace().collect::<Vec<_>>().join(" ")`) move from `ferris-paint` into `ferris-layout` — `ferris-paint` no longer imports `ferris_dom::dom::Node` or does any text-content logic.
- Line height is `font_size * 1.2` everywhere (`ferris_text::line_height`) — same convention the renderer (`ferris-compositor/src/renderer/text.rs`) already uses for `Metrics::new(cmd.size, cmd.size * 1.2)`.
- Text direct to an element is always laid out (and painted) BEFORE that element's block children, regardless of DOM order — documented simplification, not a bug to fix here.
- `wrap_lines` never panics: empty text returns `vec![]`; `max_width <= 0` is clamped to a small positive minimum before being handed to `cosmic-text`; a single word wider than `max_width` still produces at least one line (never loops forever, never drops content).
- `ferris-text`'s `cosmic-text` API usage in this plan is the plan author's best understanding from memory, not confirmed against the live crate docs. If a call doesn't compile or doesn't behave as described (most likely: whether `layout_runs()` returns every wrapped line or only a viewport-limited subset when no height bound is set), the implementer must read the actual `cosmic-text` 0.12.1 source (already vendored locally — check `%CARGO_HOME%/registry/src/*/cosmic-text-0.12.1/src/` or run `cargo doc --open -p cosmic-text` from this workspace) and adjust, documenting the deviation in their report, the same way prior tasks in this project have handled brief code that didn't match a real external API exactly.

## Review Focus

- A paragraph long enough to wrap into many lines inside a narrow box must produce a box whose height actually reflects every line (not just the first one) — the spec's whole point; covered by Task 2's `long_text_in_narrow_box_wraps_into_multiple_lines_and_grows_height` and Task 4's real-pipeline integration test.
- An element with NO direct text (only block children, or genuinely empty) must get `text_lines: vec![]` and contribute zero extra height from this feature — a regression here would silently break every existing layout test from piece 2.4; covered by Task 2's `element_without_direct_text_has_empty_text_lines`.
- A single word wider than the available box width (a long URL, an unbroken string) must not hang or panic `wrap_lines` — covered by Task 1's `wrap_lines_single_long_word_does_not_hang_and_returns_at_least_one_line`.
- `font-size: 0` or a negative `font-size` must not reach `cosmic-text` at all (it previously caused a real panic/hang inside `glyphon`/`cosmic-text`, found and partially fixed in piece 2.5 — this plan moves that same validation to the measurement path too, which piece 2.5's fix didn't cover since text wasn't measured at layout time then) — covered by Task 2's `resolve_font_size_zero_falls_back_to_sixteen`/`resolve_font_size_negative_falls_back_to_sixteen`.
- The real GPU-rendered output must actually show wrapped text inside its box, not just pass unit tests against pure data structures — GPU/text-shaping bugs in this project have only ever been caught by actually running the compositor and looking (pieces 2.4's box math was pure-logic-testable, but 2.5's shader alpha bug and this plan's whole subject, real font shaping, are not) — covered by Task 4's mandatory manual visual verification step.

---

### Task 1: `ferris-text` — text measurement and line wrapping

**Files:**
- Create: `ferris-text/Cargo.toml`
- Create: `ferris-text/src/lib.rs`

**Interfaces:**
- Consumes: nothing (first task, depends only on the external `cosmic-text` crate).
- Produces: `pub fn wrap_lines(text: &str, font_size: f32, max_width: f32) -> Vec<String>` and `pub fn line_height(font_size: f32) -> f32`, used by Task 2.

- [ ] **Step 1: Add `ferris-text` to the workspace**

Read `Cargo.toml` at the repo root first to confirm the current `members` list, then add `"ferris-text"` to it. As of this plan it reads:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style", "ferris-layout", "ferris-paint", "ferris-scene"]
resolver = "2"
```

Change to:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style", "ferris-layout", "ferris-paint", "ferris-scene", "ferris-text"]
resolver = "2"
```

- [ ] **Step 2: Create the crate manifest**

Create `ferris-text/Cargo.toml`:

```toml
[package]
name = "ferris-text"
version = "0.1.0"
edition = "2021"

[dependencies]
cosmic-text = "0.12"
```

- [ ] **Step 3: Write the failing tests**

Create `ferris-text/src/lib.rs`:

```rust
use std::sync::{Mutex, OnceLock};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

pub fn line_height(font_size: f32) -> f32 {
    font_size * 1.2
}

fn font_system() -> &'static Mutex<FontSystem> {
    static FONT_SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    FONT_SYSTEM.get_or_init(|| Mutex::new(FontSystem::new()))
}

pub fn wrap_lines(text: &str, font_size: f32, max_width: f32) -> Vec<String> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_height_is_font_size_times_1_2() {
        assert_eq!(line_height(10.0), 12.0);
        assert_eq!(line_height(20.0), 24.0);
    }

    #[test]
    fn short_text_fits_on_one_line() {
        let lines = wrap_lines("hi", 16.0, 800.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "hi");
    }

    #[test]
    fn empty_text_returns_empty_vec() {
        let lines = wrap_lines("", 16.0, 800.0);
        assert!(lines.is_empty());
    }

    #[test]
    fn long_text_wraps_into_multiple_lines_in_a_narrow_box() {
        let text = "this is a long sentence that should not fit on a single line inside a very narrow box";
        let lines = wrap_lines(text, 16.0, 80.0);
        assert!(lines.len() > 1, "expected more than one line, got {}: {:?}", lines.len(), lines);
        // No word should have been silently dropped: rejoining every returned
        // line with a single space between them must reconstruct every word
        // originally present (word order and set preserved, even if the exact
        // whitespace between wrapped lines differs from the source).
        let rejoined: Vec<&str> = lines.iter().flat_map(|l| l.split_whitespace()).collect();
        let original: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(rejoined, original, "wrapping must not drop or reorder words");
    }

    #[test]
    fn single_long_word_does_not_hang_and_returns_at_least_one_line() {
        let long_word = "a".repeat(500);
        let lines = wrap_lines(&long_word, 16.0, 10.0);
        assert!(!lines.is_empty(), "a word wider than max_width must still produce at least one line");
    }

    #[test]
    fn non_positive_max_width_does_not_panic() {
        let lines_zero = wrap_lines("hello world", 16.0, 0.0);
        assert!(!lines_zero.is_empty());
        let lines_negative = wrap_lines("hello world", 16.0, -50.0);
        assert!(!lines_negative.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-text -- --nocapture`
Expected: compile error or panic on `todo!()`.

- [ ] **Step 3: Implement `wrap_lines`**

Replace the `todo!()` body in `ferris-text/src/lib.rs`. This is the plan author's best understanding of the `cosmic-text` 0.12 API from memory — per the Global Constraints, verify against the real crate (vendored source or `cargo doc`) if this doesn't compile or if `long_text_wraps_into_multiple_lines_in_a_narrow_box` fails because `layout_runs()` only returns a viewport-limited subset of lines rather than every wrapped line:

```rust
pub fn wrap_lines(text: &str, font_size: f32, max_width: f32) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let width = if max_width > 0.0 { max_width } else { 1.0 };
    let mut font_system = font_system().lock().expect("font system mutex poisoned");

    let metrics = Metrics::new(font_size, line_height(font_size));
    let mut buffer = Buffer::new(&mut font_system, metrics);
    // No height bound: we want every wrapped line, not just what would fit in
    // a fixed viewport (unlike the renderer's own use of Buffer, which bounds
    // both width and height for on-screen clipping).
    buffer.set_size(&mut font_system, Some(width), None);
    buffer.set_text(&mut font_system, text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    buffer.shape_until_scroll(&mut font_system, false);

    let mut lines = Vec::new();
    for run in buffer.layout_runs() {
        if run.glyphs.is_empty() {
            continue;
        }
        let start = run.glyphs.first().unwrap().start;
        let end = run.glyphs.last().unwrap().end;
        lines.push(run.text[start..end].to_string());
    }

    if lines.is_empty() {
        // Defensive fallback: if layout_runs() somehow produced nothing for
        // non-empty input (should not happen per cosmic-text's contract, but
        // this crate never panics or silently drops all content), fall back
        // to the whole text as a single unwrapped line.
        lines.push(text.to_string());
    }

    lines
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-text`
Expected: 6 passed.

- [ ] **Step 5: Build the whole workspace to confirm the new member compiles cleanly**

Run: `cargo build --workspace`
Expected: clean build, no warnings, `ferris-text` now listed among the built packages.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml ferris-text/Cargo.toml ferris-text/src/lib.rs Cargo.lock
git commit -m "feat: add ferris-text crate for real font-metric line wrapping"
```

---

### Task 2: Wire text measurement into `ferris-layout`

**Files:**
- Modify: `ferris-layout/Cargo.toml`
- Modify: `ferris-layout/src/length.rs`
- Modify: `ferris-layout/src/layout.rs`

**Interfaces:**
- Consumes: `ferris_text::{wrap_lines, line_height}` (Task 1); `ferris_dom::dom::Node` (already available — `ferris-layout` already depends on `ferris-dom`).
- Produces: `pub fn resolve_font_size(style: &std::collections::HashMap<String, String>) -> f32` in `ferris_layout::length`, and `LayoutBox.text_lines: Vec<String>`, both used by Task 3.

- [ ] **Step 1: Add the `ferris-text` dependency**

Read `ferris-layout/Cargo.toml` first to confirm its current `[dependencies]` block (it already has `ferris-dom` and `ferris-style`), then add:

```toml
ferris-text = { path = "../ferris-text" }
```

- [ ] **Step 2: Write the failing tests for `resolve_font_size`**

Read `ferris-layout/src/length.rs` first to see its current content (the `Length` enum and `parse_length`, from piece 2.4). Add `resolve_font_size` and its tests at the end of the file, inside the existing `#[cfg(test)] mod tests` block (append these functions/tests, do not replace anything already there):

```rust
use std::collections::HashMap;

pub fn resolve_font_size(style: &HashMap<String, String>) -> f32 {
    todo!()
}
```

(add the `use std::collections::HashMap;` near the top of the file alongside any existing `use` lines, and add the `resolve_font_size` function itself after `parse_length`, outside the test module)

```rust
    #[test]
    fn resolve_font_size_px_used_directly() {
        let style = style_with(&[("font-size", "24px")]);
        assert_eq!(resolve_font_size(&style), 24.0);
    }

    #[test]
    fn resolve_font_size_absent_falls_back_to_sixteen() {
        let style = HashMap::new();
        assert_eq!(resolve_font_size(&style), 16.0);
    }

    #[test]
    fn resolve_font_size_percent_falls_back_to_sixteen() {
        let style = style_with(&[("font-size", "150%")]);
        assert_eq!(resolve_font_size(&style), 16.0);
    }

    #[test]
    fn resolve_font_size_zero_falls_back_to_sixteen() {
        let style = style_with(&[("font-size", "0px")]);
        assert_eq!(resolve_font_size(&style), 16.0);
    }

    #[test]
    fn resolve_font_size_negative_falls_back_to_sixteen() {
        let style = style_with(&[("font-size", "-12px")]);
        assert_eq!(resolve_font_size(&style), 16.0);
    }
```

`style_with` is a local test helper already defined in `length.rs`'s test module from piece 2.4 — if it isn't (check first), add it:
```rust
    fn style_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --package ferris-layout length:: -- --nocapture`
Expected: compile error or panic on `todo!()`.

- [ ] **Step 4: Implement `resolve_font_size`**

```rust
pub fn resolve_font_size(style: &HashMap<String, String>) -> f32 {
    match parse_length(style.get("font-size").map(String::as_str)) {
        Length::Px(n) if n.is_finite() && n > 0.0 => n,
        _ => 16.0,
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --package ferris-layout length::`
Expected: 20 passed (15 pre-existing + 5 new).

- [ ] **Step 6: Write the failing tests for `layout_block`'s text handling**

Read `ferris-layout/src/layout.rs` first to see the current `layout_block` implementation and its test module (from piece 2.4, including the `leaf_styled_node` helper). Add these tests inside the existing `mod tests` block:

```rust
    #[test]
    fn single_short_line_of_text_fits_on_one_line() {
        let mut el = Element::new("p");
        el.children.push(Node::Text("hi".to_string()));
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert_eq!(root.text_lines, vec!["hi".to_string()]);
        assert_eq!(root.height, ferris_text::line_height(16.0));
    }

    #[test]
    fn long_text_in_narrow_box_wraps_into_multiple_lines_and_grows_height() {
        let mut el = Element::new("p");
        el.children.push(Node::Text(
            "this is a long sentence that should not fit on a single line inside a very narrow box".to_string(),
        ));
        let node = leaf_styled_node(&el, &[("width", "80px")]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert!(root.text_lines.len() > 1, "expected multiple wrapped lines, got {:?}", root.text_lines);
        let expected_height = root.text_lines.len() as f32 * ferris_text::line_height(16.0);
        assert_eq!(root.height, expected_height);
    }

    #[test]
    fn element_without_direct_text_has_empty_text_lines() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert!(root.text_lines.is_empty());
    }
```

Note: `Node` (from `ferris_dom::dom::Node`) is already imported in `layout.rs`'s test module from piece 2.4's own tests (which push `Node::Text`/build children directly) — if the current file doesn't already have `use ferris_dom::dom::Node;` in scope for the test module, add it.

- [ ] **Step 7: Run tests to verify they fail**

Run: `cargo test --package ferris-layout layout:: -- --nocapture`
Expected: the 3 new tests fail (compile error if `text_lines` doesn't exist yet on `LayoutBox`, or assertion failure).

- [ ] **Step 8: Add `text_lines` to `LayoutBox` and wire the measurement into `layout_block`**

In `ferris-layout/src/layout.rs`, add the import near the top (alongside the existing `use` lines):

```rust
use ferris_dom::dom::Node;
```

Add `text_lines: Vec<String>` to the `LayoutBox` struct definition (keep every existing field):

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
    pub text_lines: Vec<String>,
    pub children: Vec<LayoutBox<'a>>,
}
```

In `layout_block`, insert text extraction and measurement right after `content_y` is computed and before the children loop begins (read the current function first to find this exact spot — it is immediately after the two lines computing `content_x`/`content_y` and immediately before `let mut children = Vec::new();`):

```rust
    let raw_text: String = node
        .element
        .children
        .iter()
        .filter_map(|child| match child {
            Node::Text(s) => Some(s.as_str()),
            Node::Comment(_) | Node::Element(_) => None,
        })
        .collect();
    let collapsed_text = raw_text.split_whitespace().collect::<Vec<_>>().join(" ");

    let font_size = resolve_font_size(&node.style);
    let text_lines = if collapsed_text.is_empty() {
        Vec::new()
    } else {
        ferris_text::wrap_lines(&collapsed_text, font_size, width)
    };
    let text_block_height = text_lines.len() as f32 * ferris_text::line_height(font_size);
```

Change the children loop's cursor initialization from:
```rust
    let mut children = Vec::new();
    let mut cursor_y = content_y;
```
to:
```rust
    let mut children = Vec::new();
    let mut cursor_y = content_y + text_block_height;
```

Add `text_lines` to the final `Some(LayoutBox { ... })` construction at the end of the function (keep every existing field):

```rust
    Some(LayoutBox { styled_node: node, x: content_x, y: content_y, width, height, margin, border, padding, text_lines, children })
```

- [ ] **Step 9: Run tests to verify they pass**

Run: `cargo test --package ferris-layout layout::`
Expected: all layout.rs tests pass (23 pre-existing + 3 new = 26).

- [ ] **Step 10: Run the whole crate and workspace test suites**

Run: `cargo test --package ferris-layout`
Expected: 49 passed (26 layout + 20 length + 3 integration — the 3 pre-existing `tests/integration.rs` tests should be unaffected, since none of their fixtures include direct text long enough to wrap; if any of them fail, hand-trace whether the fixture's text now legitimately produces `text_lines`/height different from what the test asserted, versus a real bug in this task's code, before changing either side).

Run: `cargo test --workspace`
Expected: workspace baseline before this plan was 194 (post-2.5, confirmed via `git log`/the pushed state) + Task 1's 6 (ferris-text) + this task's 8 (5 length + 3 layout) = 208.

- [ ] **Step 11: Commit**

```bash
git add ferris-layout/Cargo.toml ferris-layout/src/length.rs ferris-layout/src/layout.rs Cargo.lock
git commit -m "feat: measure and wrap direct text during layout, growing box height"
```

---

### Task 3: Simplify `ferris-paint` to consume pre-wrapped `text_lines`

**Files:**
- Modify: `ferris-paint/Cargo.toml`
- Modify: `ferris-paint/src/paint.rs`

**Interfaces:**
- Consumes: `LayoutBox.text_lines: Vec<String>` and `ferris_layout::length::resolve_font_size` (Task 2); `ferris_text::line_height` (Task 1).
- Produces: nothing new for later tasks — `paint()`'s public signature is unchanged.

- [ ] **Step 1: Add the `ferris-text` dependency**

Read `ferris-paint/Cargo.toml` first, then add:

```toml
ferris-text = { path = "../ferris-text" }
```

- [ ] **Step 2: Read the current `paint.rs` before editing**

Read `ferris-paint/src/paint.rs` in full — this task both removes text-extraction logic that moved to `ferris-layout` in Task 2, and removes 8 tests whose behavior now lives in `ferris-layout`'s own test suite (Task 2 already covers the equivalent behavior there — this is not a coverage reduction, it's the same coverage moving to where the logic now lives). Confirm you can see each test named below before deleting it; if any is missing or already different, stop and report rather than guessing.

- [ ] **Step 3: Replace `paint_node`'s text handling**

Replace the entire text-related block inside `paint_node` (currently: extracting `raw_text` from `node.styled_node.element.children`, collapsing it, and — if non-empty — resolving `font-size` via a local `parse_length` match and pushing one `TextCommand`) with:

```rust
    if !node.text_lines.is_empty() {
        let size = resolve_font_size(&node.styled_node.style);
        let line_h = ferris_text::line_height(size);
        let color = parse_color(node.styled_node.style.get("color").map(String::as_str))
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        for (i, line) in node.text_lines.iter().enumerate() {
            frame.push(DrawCommand::Text(TextCommand {
                x: node.x,
                y: node.y + i as f32 * line_h,
                content: line.clone(),
                size,
                color,
            }));
        }
    }
```

The background-rect logic above this block, and the `for child in &node.children { paint_node(child, frame); }` recursion below it, are unchanged — only the text block in the middle is replaced.

- [ ] **Step 4: Fix the imports**

At the top of `paint.rs`, remove `use ferris_dom::dom::Node;` and `use ferris_layout::length::{parse_length, Length};` (both now unused — `Node` was only for the removed text-extraction filter_map, `parse_length`/`Length` were only for the removed inline font-size match). Add:

```rust
use ferris_layout::length::resolve_font_size;
```

- [ ] **Step 5: Update the test module's `leaf_box` helper and existing literal for the new `LayoutBox` field**

`LayoutBox` (Task 2) now has a mandatory `text_lines: Vec<String>` field, so both places in `paint.rs`'s test module that construct a `LayoutBox` literal need it added. In the `leaf_box` helper function:

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
            text_lines: Vec::new(),
            children: Vec::new(),
        }
    }
```

And in `recursion_visits_all_descendants_background_before_children`'s direct `LayoutBox { ... }` literal (the one that doesn't go through `leaf_box`), add `text_lines: Vec::new(),` alongside its other fields.

- [ ] **Step 6: Remove the 8 tests whose logic moved to `ferris-layout`**

Delete these test functions entirely from `paint.rs`'s test module (equivalent coverage now lives in `ferris-layout/src/layout.rs`'s and `ferris-layout/src/length.rs`'s test modules, added in Task 2):
- `direct_text_child_produces_a_text_command_at_content_origin`
- `multiple_text_children_around_a_comment_are_concatenated_skipping_the_comment`
- `whitespace_only_text_produces_no_text_command`
- `font_size_in_px_is_used_directly`
- `font_size_absent_or_non_px_falls_back_to_sixteen`
- `font_size_zero_falls_back_to_sixteen`
- `font_size_negative_falls_back_to_sixteen`
- `text_with_internal_newlines_and_indentation_is_collapsed_to_single_spaces`

Keep everything else: `background_color_produces_a_rect_at_the_border_box`, `no_background_color_produces_no_rect`, `text_color_falls_back_to_black_when_absent_or_unparseable`, `text_color_uses_the_parsed_css_color_when_valid`, `recursion_visits_all_descendants_background_before_children` (all 5 need only the `text_lines: Vec::new()` field added via Step 5's helper/literal update — `text_color_falls_back_to_black_when_absent_or_unparseable` and `text_color_uses_the_parsed_css_color_when_valid` currently build their `LayoutBox` via a `p` element with a pushed `Node::Text("Hi")` child; since `paint_node` no longer reads element children for text, change these two tests to set `text_lines: vec!["Hi".to_string()]` directly in their `leaf_box`-built node instead of relying on the `Element`'s children — `leaf_box` takes a `&StyledNode`, so after calling `leaf_box(...)` to get the base `LayoutBox`, either construct the box's `text_lines` field directly with a full literal, or add an optional pattern like constructing the `Element`/`StyledNode` exactly as before but then overriding just the `text_lines` field on the returned `LayoutBox` — simplest is to stop pushing `Node::Text` on the `Element` in these two tests and instead build the `LayoutBox` literal directly with `text_lines: vec!["Hi".to_string()]`, mirroring `recursion_visits_all_descendants_background_before_children`'s pattern of a raw literal instead of `leaf_box`).

- [ ] **Step 7: Write the 2 new tests for the `text_lines`-based rendering**

Add these to the test module:

```rust
    #[test]
    fn text_lines_produce_one_text_command_per_line_with_correct_y_offset() {
        let element = Element::new("p");
        let styled = StyledNode { element: &element, style: style_with(&[("font-size", "20px")]), children: Vec::new() };
        let node = LayoutBox {
            styled_node: &styled,
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 48.0,
            margin: Edges::default(),
            border: Edges::default(),
            padding: Edges::default(),
            text_lines: vec!["Hello".to_string(), "world".to_string()],
            children: Vec::new(),
        };

        let frame = paint(&node);
        assert_eq!(frame.commands.len(), 2);
        let DrawCommand::Text(first) = &frame.commands[0] else { panic!("expected text") };
        let DrawCommand::Text(second) = &frame.commands[1] else { panic!("expected text") };
        assert_eq!(first.content, "Hello");
        assert_eq!(first.x, 10.0);
        assert_eq!(first.y, 20.0);
        assert_eq!(second.content, "world");
        assert_eq!(second.x, 10.0);
        assert_eq!(second.y, 20.0 + ferris_text::line_height(20.0));
    }

    #[test]
    fn empty_text_lines_produces_no_text_command() {
        let element = Element::new("div");
        let styled = StyledNode { element: &element, style: style_with(&[("font-size", "20px")]), children: Vec::new() };
        let node = LayoutBox {
            styled_node: &styled,
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 0.0,
            margin: Edges::default(),
            border: Edges::default(),
            padding: Edges::default(),
            text_lines: Vec::new(),
            children: Vec::new(),
        };

        let frame = paint(&node);
        assert!(frame.commands.is_empty());
    }
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --package ferris-paint paint::`
Expected: 7 passed (5 kept + 2 new).

- [ ] **Step 9: Run the whole crate and workspace test suites**

Run: `cargo test --package ferris-paint`
Expected: 22 passed (7 paint + 13 color + 2 integration — the 2 pre-existing `tests/integration.rs` tests may need attention: their fixture's text is short enough that it should still produce exactly one line each even through real `wrap_lines`, but confirm by running rather than assuming; if `realistic_page_paints_background_rects_and_text_in_the_right_places` fails, hand-trace whether it's this task's bug or whether the assertion needs updating for a legitimately-different-but-correct result, same discipline as every prior task in this project).

Run: `cargo test --workspace`
Expected: 208 (from Task 2) − 6 (net change in `ferris-paint`'s own test count: 13 removed across the deleted 8 tests replaced by nothing, plus 2 added = −6... concretely: `ferris-paint` goes from 28 to 22, a change of −6) = 202.

- [ ] **Step 10: Commit**

```bash
git add ferris-paint/Cargo.toml ferris-paint/src/paint.rs Cargo.lock
git commit -m "refactor: consume pre-wrapped text_lines from ferris-layout instead of extracting text in ferris-paint"
```

---

### Task 4: Integration test + mandatory manual visual verification

**Files:**
- Modify: `ferris-paint/tests/integration.rs`
- Modify: `ferris-compositor/assets/fixture.html`

**Interfaces:**
- Consumes: the full pipeline as already wired in piece 2.5 (`ferris_dom`, `ferris_css`, `ferris_style::resolve_styles`, `ferris_layout::layout::layout`, `ferris_paint::paint::paint`) — no new production code, this task only adds a test and a content change.
- Produces: nothing consumed by later tasks — this is the final task of the plan.

- [ ] **Step 1: Write the integration test for real text wrapping**

Read `ferris-paint/tests/integration.rs` first to see its current helpers (`parse_css`, `parse_root_element`) and existing 2 tests, from piece 2.5. Append this test:

```rust
// --- Review Focus: real HTML+CSS with a long paragraph in a narrow box must
// wrap into multiple TextCommands with correctly stacked y-offsets, through
// the real parser+style+layout+paint pipeline end to end ---
#[test]
fn long_paragraph_in_a_narrow_box_wraps_into_multiple_text_commands() {
    let html = r#"<div id="box"><p id="para">this is a long sentence that should not fit on a single line inside a very narrow box</p></div>"#;
    let css = r#"
        #box { width: 100px; }
        #para { font-size: 16px; }
    "#;

    let page = parse_root_element(html);
    let stylesheet = parse_css(css);
    let styled = resolve_styles(&page, &stylesheet);
    let layout_box = layout(&styled, 1024.0, 768.0).unwrap();
    let frame = paint(&layout_box);

    let texts: Vec<_> = frame.commands.iter().filter_map(|c| match c {
        DrawCommand::Text(t) => Some(t),
        DrawCommand::Rect(_) => None,
    }).collect();

    assert!(texts.len() > 1, "expected the long paragraph to wrap into multiple TextCommands, got {}", texts.len());

    // y-offsets must be strictly increasing by exactly one line height between
    // consecutive lines.
    let line_h = ferris_text::line_height(16.0);
    for i in 1..texts.len() {
        assert!(
            (texts[i].y - texts[i - 1].y - line_h).abs() < 0.01,
            "line {} should be exactly one line-height below line {}",
            i,
            i - 1
        );
    }

    // No word should have been dropped across the wrapped TextCommands.
    let rejoined: Vec<&str> = texts.iter().flat_map(|t| t.content.split_whitespace()).collect();
    let original: Vec<&str> =
        "this is a long sentence that should not fit on a single line inside a very narrow box".split_whitespace().collect();
    assert_eq!(rejoined, original);
}
```

The test above calls `ferris_text::line_height(16.0)` fully-qualified — no new `use` import is needed for it, but `ferris-text` must already be a resolvable path dependency for this test file (a regular `[dependencies]` entry works fine for integration tests too), which Step 2 below confirms.

- [ ] **Step 2: Add `ferris-text` as a dev-dependency of `ferris-paint`**

Read `ferris-paint/Cargo.toml` (it already has `ferris-text` as a regular `[dependencies]` entry from Task 3, since `paint.rs` itself calls `ferris_text::line_height`) — confirm this is already present as a normal dependency, which means the integration test in `tests/` can already reference `ferris_text::` without any further `Cargo.toml` change (Rust's integration tests can use any of the crate's own regular dependencies). No edit needed here if Task 3 already added it; just confirm.

- [ ] **Step 3: Run the new test to verify it passes**

Run: `cargo test --package ferris-paint --test integration long_paragraph_in_a_narrow_box_wraps_into_multiple_text_commands`
Expected: PASS. If it fails, hand-trace real `cosmic-text` wrapping behavior for this exact string/width/font-size combination before changing the test's expectations — this is exercising real font shaping, not a value you can simply recompute by hand the way pure box-model arithmetic could be in earlier pieces.

- [ ] **Step 4: Run the whole crate and workspace test suites**

Run: `cargo test --package ferris-paint`
Expected: 23 passed (22 from Task 3 + 1 new integration test).

Run: `cargo test --workspace`
Expected: 202 (from Task 3) + 1 = 203.

Run: `cargo clippy --workspace --all-targets`
Expected: no new warnings (the pre-existing 2 warnings in `ferris-style/src/cascade.rs` and any other already-known pre-existing warnings from prior pieces are expected and out of scope).

- [ ] **Step 5: Extend the compositor fixture with a paragraph long enough to wrap**

This step exists purely to give the manual visual verification (Step 6) something to look at — a real paragraph that will visibly wrap inside the fixture's existing layout, without touching `ferris-compositor/src/main.rs`, `fixture.css`, or any other file from piece 2.5.

Read `ferris-compositor/assets/fixture.html` first (from piece 2.5) to confirm its current exact content, then replace the `#intro` paragraph's text (currently `"A browser built from scratch in Rust."`) with a longer sentence that will wrap at the fixture's actual rendered width (`#content` is `#page`'s 800px width minus its 20px left/right padding = 760px available, at the default 16px font size — a sentence of roughly 200+ characters comfortably wraps into 3+ lines at that width with a proportional sans-serif font):

```html
        <p id="intro">A browser built from scratch in Rust, with its own HTML parser, its own CSS parser, its own style resolution, its own block and text layout engine, and its own bridge into a real GPU compositor — no engine borrowed from anywhere else.</p>
```

- [ ] **Step 6: Manual verification — run the binary and look at the window**

This step cannot be automated; per the spec, it is the actual acceptance test for this plan's visual claim.

Run: `cargo build --release -p ferris-compositor` then run the resulting binary (or `cargo run --release -p ferris-compositor`).

Confirm, by looking at the opened window (or by capturing a screenshot and reading it, the same technique already established in piece 2.5's Task 4 and its fix rounds — PowerShell `Start-Process` + `GetWindowRect`/`CopyFromScreen`, then the Read tool on the resulting PNG):

- The `#intro` paragraph's now-much-longer sentence visibly wraps across multiple lines, staying inside its box (not overflowing past `#page`'s white background into the surrounding dark clear color).
- The `#card`/`#card-text` element below it is still positioned correctly, pushed further down than before (since `#intro`'s box is now taller than one line) — this is the direct visual proof that layout's height calculation, not just paint's line-splitting, is actually working end to end.
- No overlapping text, no missing lines, no visibly wrong wrapping (a word split mid-word, for instance, would indicate a real bug worth investigating before treating this task as done).

Close the window (or kill the process) once confirmed. Delete any temporary screenshot file created for this check. Note in the task's completion report exactly what was seen — a task reviewer for this task must ask the implementer to describe what appeared, and treat an implementer that skipped running the binary as not having completed the task's actual deliverable, same discipline as piece 2.5's Task 4.

- [ ] **Step 7: Commit**

```bash
git add ferris-paint/tests/integration.rs ferris-compositor/assets/fixture.html
git commit -m "test: add real-pipeline text-wrapping integration test and extend the fixture to demonstrate wrapping visually"
```

---
