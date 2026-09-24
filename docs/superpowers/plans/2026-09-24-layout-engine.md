# Layout Engine (Box Model) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `ferris-layout`, a new crate that turns a `ferris-style::StyledNode` tree into a `LayoutBox` tree carrying computed pixel positions and dimensions, using block-only CSS layout.

**Architecture:** A single top-down recursive pass (`layout_block`) resolves each element's content width from its parent's available space, then recurses into children (each stacking vertically below the last), then computes its own height as either an explicit value or the sum of its children's total box heights. A small `Length` type (`Px`/`Percent`/`Auto`) and a `parse_length` function convert the opaque CSS value strings already on `StyledNode.style` into numbers.

**Tech Stack:** Rust, Cargo workspace path dependencies on `ferris-dom` and `ferris-style`. No external dependencies.

**Spec:** `docs/superpowers/specs/2026-09-24-layout-engine-design.md`

## Global Constraints

- New crate `ferris-layout`, fifth workspace member, path-depends only on `ferris-dom` and `ferris-style` — no external crates, no dependency on `ferris-compositor`.
- Block layout only: every element is a vertically stacked block. No inline layout, no text measurement/wrapping.
- No property inheritance between elements in this piece.
- No margin-collapsing: adjacent vertical margins are summed, not maxed.
- `height: %` only resolves against the viewport (the root call); anywhere deeper in the tree it resolves to `Auto`. `width: %` always resolves against the immediate parent's content width — no restriction.
- No `box-sizing: border-box` support — explicit `width`/`height` always define the content box.
- Malformed/unknown/absent CSS length values never panic — they resolve to `Length::Auto` (or the category default: `0px` for margin/border/padding, `Auto` for width/height).
- Any content width/height that computes negative is clamped to `0.0`.
- `display: none` elements (and their children) are excluded from the `LayoutBox` tree entirely — never generate a zero-size box.
- `LayoutBox.x/y/width/height` represent the **content box** only; `margin`/`border`/`padding` are separate `Edges` fields.

## Review Focus

- A `width`/`height` value using an unsupported unit (e.g. `"10vh"`, `"2rem"`) or a garbage string (e.g. `"banana"`) must resolve to `Auto`, not panic and not silently parse a wrong number — covered in Task 1's `parse_length` tests.
- An element whose margin+border+padding alone exceed the parent's available content width must produce a clamped `0.0` content width, not a negative number that would corrupt sibling positioning downstream — covered in Task 2's width-resolution tests.
- A `display:none` element with children that themselves have explicit `display` values must exclude the whole subtree (children never independently "resurrect" into the layout tree just because their own `display` isn't `none`) — covered in Task 3's traversal tests.
- Three or more block siblings, some with explicit height and some with `auto` (sum-of-children) height, mixed with `display:none` siblings interspersed between them, must stack correctly (the excluded siblings contribute no vertical offset at all) — covered in Task 3's stacking tests.
- A `height: %` on a deeply nested element (grandchild or deeper, not the root) must resolve to `Auto` rather than computing against an unrelated ancestor's auto-computed pixel height — covered in Task 4's integration tests, since it requires the full parser pipeline to build a realistic nested tree.

---

### Task 1: `Length` type and value parsing

**Files:**
- Create: `ferris-layout/Cargo.toml`
- Create: `ferris-layout/src/lib.rs`
- Create: `ferris-layout/src/length.rs`

**Interfaces:**
- Consumes: nothing (first task, pure parsing logic, no dependency on `ferris-style`/`ferris-dom` types).
- Produces: `pub enum Length { Px(f32), Percent(f32), Auto }` and `pub fn parse_length(value: Option<&str>) -> Length`, used by Task 2.

- [ ] **Step 1: Add `ferris-layout` to the workspace**

Read `Cargo.toml` at the repo root first to confirm the current `members` list, then add `"ferris-layout"` to it. As of this plan it reads:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style"]
resolver = "2"
```

Change to:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style", "ferris-layout"]
resolver = "2"
```

- [ ] **Step 2: Create the crate manifest**

Create `ferris-layout/Cargo.toml`:

```toml
[package]
name = "ferris-layout"
version = "0.1.0"
edition = "2021"

[dependencies]
ferris-dom = { path = "../ferris-dom" }
ferris-style = { path = "../ferris-style" }
```

- [ ] **Step 3: Write the failing tests for `parse_length`**

Create `ferris-layout/src/length.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
    Auto,
}

pub fn parse_length(value: Option<&str>) -> Length {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_px_value() {
        assert_eq!(parse_length(Some("10px")), Length::Px(10.0));
    }

    #[test]
    fn parses_px_value_with_decimal() {
        assert_eq!(parse_length(Some("10.5px")), Length::Px(10.5));
    }

    #[test]
    fn parses_px_value_with_surrounding_whitespace() {
        assert_eq!(parse_length(Some("  10px  ")), Length::Px(10.0));
    }

    #[test]
    fn parses_percent_value() {
        assert_eq!(parse_length(Some("50%")), Length::Percent(50.0));
    }

    #[test]
    fn parses_percent_value_with_decimal() {
        assert_eq!(parse_length(Some("33.33%")), Length::Percent(33.33));
    }

    #[test]
    fn parses_auto_keyword_case_insensitive() {
        assert_eq!(parse_length(Some("auto")), Length::Auto);
        assert_eq!(parse_length(Some("AUTO")), Length::Auto);
        assert_eq!(parse_length(Some("Auto")), Length::Auto);
    }

    #[test]
    fn absent_value_is_auto() {
        assert_eq!(parse_length(None), Length::Auto);
    }

    #[test]
    fn empty_string_is_auto() {
        assert_eq!(parse_length(Some("")), Length::Auto);
    }

    #[test]
    fn unsupported_unit_is_auto() {
        assert_eq!(parse_length(Some("10vh")), Length::Auto);
        assert_eq!(parse_length(Some("2rem")), Length::Auto);
        assert_eq!(parse_length(Some("1em")), Length::Auto);
    }

    #[test]
    fn garbage_string_is_auto() {
        assert_eq!(parse_length(Some("banana")), Length::Auto);
    }

    #[test]
    fn bare_number_with_no_unit_is_auto() {
        assert_eq!(parse_length(Some("10")), Length::Auto);
    }

    #[test]
    fn negative_px_value_parses_as_negative() {
        assert_eq!(parse_length(Some("-5px")), Length::Px(-5.0));
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --package ferris-layout length:: -- --nocapture`
Expected: compile error or panic on `todo!()` — 12 tests fail.

- [ ] **Step 5: Implement `parse_length`**

Replace the `todo!()` body in `ferris-layout/src/length.rs`:

```rust
pub fn parse_length(value: Option<&str>) -> Length {
    let Some(raw) = value else {
        return Length::Auto;
    };
    let trimmed = raw.trim();

    if trimmed.eq_ignore_ascii_case("auto") {
        return Length::Auto;
    }

    if let Some(number_part) = trimmed.strip_suffix("px") {
        if let Ok(n) = number_part.trim().parse::<f32>() {
            return Length::Px(n);
        }
        return Length::Auto;
    }

    if let Some(number_part) = trimmed.strip_suffix('%') {
        if let Ok(n) = number_part.trim().parse::<f32>() {
            return Length::Percent(n);
        }
        return Length::Auto;
    }

    Length::Auto
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --package ferris-layout length::`
Expected: 12 passed.

- [ ] **Step 7: Add the crate's `lib.rs`**

Create `ferris-layout/src/lib.rs`:

```rust
pub mod length;
```

- [ ] **Step 8: Build the whole workspace to confirm the new member compiles cleanly**

Run: `cargo build --workspace`
Expected: clean build, no warnings, `ferris-layout` now listed among the built packages.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml ferris-layout/Cargo.toml ferris-layout/src/lib.rs ferris-layout/src/length.rs
git commit -m "feat: add ferris-layout crate with CSS length parsing"
```

---

### Task 2: `Edges`, `LayoutBox`, and width/height resolution for a single node

**Files:**
- Create: `ferris-layout/src/layout.rs`
- Modify: `ferris-layout/src/lib.rs`

**Interfaces:**
- Consumes: `crate::length::{Length, parse_length}` (Task 1); `ferris_style::StyledNode` (already published, `element: &'a Element, style: HashMap<String,String>, children: Vec<StyledNode<'a>>`).
- Produces: `pub struct Edges { pub top: f32, pub right: f32, pub bottom: f32, pub left: f32 }`, `pub struct LayoutBox<'a> { pub styled_node: &'a StyledNode<'a>, pub x: f32, pub y: f32, pub width: f32, pub height: f32, pub margin: Edges, pub border: Edges, pub padding: Edges, pub children: Vec<LayoutBox<'a>> }`, and a private `struct ContainingBlock { content_x: f32, content_y: f32, content_width: f32 }` plus a private `fn resolve_edges(style: &HashMap<String, String>, property: &str) -> Edges` and `fn resolved_length_or(len: Length, basis: f32, default_if_auto: f32) -> f32` helper used by Task 3's `layout_block`. This task does NOT yet implement tree recursion or stacking (that's Task 3) — it builds and unit-tests the box-model math for a single node in isolation, driven directly by a `HashMap<String, String>` style map (not yet a full `StyledNode`, so tests don't need a real `Element`).

- [ ] **Step 1: Write the failing tests for edge and width resolution**

Create `ferris-layout/src/layout.rs`:

```rust
use std::collections::HashMap;

use ferris_style::StyledNode;

use crate::length::{parse_length, Length};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

pub struct LayoutBox<'a> {
    pub styled_node: &'a StyledNode<'a>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub margin: Edges,
    pub border: Edges,
    pub padding: Edges,
    pub children: Vec<LayoutBox<'a>>,
}

#[derive(Debug, Clone, Copy)]
struct ContainingBlock {
    content_x: f32,
    content_y: f32,
    content_width: f32,
    /// The original viewport height, threaded unchanged through every level of
    /// recursion. Needed only by `layout_block`'s root case (Task 3) to resolve
    /// `height: %` at the root against the viewport rather than against any
    /// ancestor's own computed height. Unused by this task's functions.
    viewport_height: f32,
}

/// Reads the four longhand properties for a box-model category (e.g. `"margin"`
/// reads `margin-top`/`margin-right`/`margin-bottom`/`margin-left`) and resolves
/// each against `basis` (the parent's content width — the only sensible basis
/// for a percentage margin/border/padding in block layout). Absent values default
/// to `0px`. Negative results are NOT clamped here (margins may be legitimately
/// negative in real CSS box-model math for edges); only width/height are clamped,
/// in `resolved_length_or`.
fn resolve_edges(style: &HashMap<String, String>, property: &str) -> Edges {
    todo!()
}

/// Resolves a single `Length` against `basis` (the relevant containing dimension).
/// `Length::Auto` returns `default_if_auto` — callers pass the "fill available
/// space" computation for width, or `0.0` as a placeholder for height (Task 3
/// overrides height's `Auto` case with the real sum-of-children logic, this
/// helper alone can't know the children's heights).
fn resolved_length_or(len: Length, basis: f32, default_if_auto: f32) -> f32 {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn resolve_edges_reads_all_four_sides_in_px() {
        let style = style_with(&[
            ("margin-top", "1px"),
            ("margin-right", "2px"),
            ("margin-bottom", "3px"),
            ("margin-left", "4px"),
        ]);
        let edges = resolve_edges(&style, "margin");
        assert_eq!(edges, Edges { top: 1.0, right: 2.0, bottom: 3.0, left: 4.0 });
    }

    #[test]
    fn resolve_edges_defaults_absent_sides_to_zero() {
        let style = style_with(&[("margin-top", "5px")]);
        let edges = resolve_edges(&style, "margin");
        assert_eq!(edges, Edges { top: 5.0, right: 0.0, bottom: 0.0, left: 0.0 });
    }

    #[test]
    fn resolve_edges_resolves_percent_against_basis() {
        // Called with basis = 200.0 in the real call site (Task 3); this test
        // calls resolve_edges directly with a style using px only, since percent
        // resolution needs the basis threaded through resolved_length_or per side
        // — verified together with resolved_length_or below, then end-to-end in
        // Task 3/4's integration tests where the basis is the real parent width.
        let style = style_with(&[("padding-left", "10px")]);
        let edges = resolve_edges(&style, "padding");
        assert_eq!(edges.left, 10.0);
    }

    #[test]
    fn resolved_length_or_uses_px_value_directly() {
        assert_eq!(resolved_length_or(Length::Px(42.0), 100.0, 0.0), 42.0);
    }

    #[test]
    fn resolved_length_or_resolves_percent_against_basis() {
        assert_eq!(resolved_length_or(Length::Percent(50.0), 200.0, 0.0), 100.0);
    }

    #[test]
    fn resolved_length_or_uses_default_for_auto() {
        assert_eq!(resolved_length_or(Length::Auto, 200.0, 999.0), 999.0);
    }

    #[test]
    fn resolved_length_or_negative_percent_of_basis() {
        assert_eq!(resolved_length_or(Length::Percent(-10.0), 200.0, 0.0), -20.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-layout layout:: -- --nocapture`
Expected: compile error or panic on `todo!()`.

- [ ] **Step 3: Implement `resolve_edges` and `resolved_length_or`**

```rust
fn resolve_edges(style: &HashMap<String, String>, property: &str) -> Edges {
    let side = |suffix: &str| -> f32 {
        let key = format!("{property}-{suffix}");
        let len = parse_length(style.get(&key).map(String::as_str));
        match len {
            Length::Px(n) => n,
            Length::Percent(_) | Length::Auto => 0.0,
        }
    };
    Edges { top: side("top"), right: side("right"), bottom: side("bottom"), left: side("left") }
}

fn resolved_length_or(len: Length, basis: f32, default_if_auto: f32) -> f32 {
    match len {
        Length::Px(n) => n,
        Length::Percent(p) => basis * (p / 100.0),
        Length::Auto => default_if_auto,
    }
}
```

Note: `resolve_edges` treats a `Percent` margin/border/padding as `0.0` in this
task deliberately — percentage margin/border/padding is real CSS behavior but
adds a second basis-threading path this box model doesn't need for its stated
scope (only width/height percentages are required by the spec). This is a
scope decision, not a bug; if a future piece needs percentage margins, `side`
is the single place to extend.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-layout layout::`
Expected: 7 passed.

- [ ] **Step 5: Add the module to `lib.rs`**

Modify `ferris-layout/src/lib.rs`:

```rust
pub mod layout;
pub mod length;
```

- [ ] **Step 6: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: all prior crates' tests still pass, plus `ferris-layout`'s 19 tests (12 length + 7 layout).

- [ ] **Step 7: Commit**

```bash
git add ferris-layout/src/lib.rs ferris-layout/src/layout.rs
git commit -m "feat: add Edges/LayoutBox types and box-model length resolution"
```

---

### Task 3: `layout_block` recursive traversal (width-down, height-up, stacking)

**Files:**
- Modify: `ferris-layout/src/layout.rs`

**Interfaces:**
- Consumes: `ferris_style::StyledNode` (`element`, `style`, `children` fields); `Edges`, `LayoutBox`, `ContainingBlock`, `resolve_edges`, `resolved_length_or` (Task 2); `Length`, `parse_length` (Task 1).
- Produces: `pub fn layout<'a>(root: &'a StyledNode<'a>, viewport_width: f32, viewport_height: f32) -> LayoutBox<'a>`, used directly by consumers of this crate (piece 2.5). Internal: `fn layout_block<'a>(node: &'a StyledNode<'a>, containing_block: ContainingBlock, is_root: bool) -> Option<LayoutBox<'a>>` (returns `None` when `display: none`, so the caller filters it out of `children` — this is how Task 3's Review Focus case, excluding `display:none` subtrees regardless of the children's own `display` value, is structurally guaranteed: a `None` parent is never recursed into at all).

- [ ] **Step 1: Write the failing tests**

Append to `ferris-layout/src/layout.rs`, replacing the closing `}` of the `todo!()` bodies is not needed here (Task 2 already implemented those) — instead append below the existing `resolved_length_or` function, before the `#[cfg(test)]` module, and add new tests inside the existing `mod tests` block. First, add the public API:

```rust
pub fn layout<'a>(root: &'a StyledNode<'a>, viewport_width: f32, viewport_height: f32) -> LayoutBox<'a> {
    let containing_block = ContainingBlock {
        content_x: 0.0,
        content_y: 0.0,
        content_width: viewport_width,
        viewport_height,
    };
    // The root is never display:none in practice (StyledNode's root is the
    // document element), but layout_block's signature always returns Option for
    // uniformity — root callers unwrap because a display:none document root would
    // be a meaningless call in the first place, not a case this crate needs to
    // handle gracefully.
    layout_block(root, containing_block, true)
        .expect("root element must not be display:none")
}

fn layout_block<'a>(
    node: &'a StyledNode<'a>,
    containing_block: ContainingBlock,
    is_root: bool,
) -> Option<LayoutBox<'a>> {
    todo!()
}
```

Then add these tests inside `mod tests`, using real `Element`/`StyledNode` values (no parser needed yet — built directly, same pattern `ferris-style`'s own unit tests use):

```rust
    use ferris_dom::dom::Element;

    fn leaf_styled_node(element: &Element, style: &[(&str, &str)]) -> StyledNode<'_> {
        StyledNode { element, style: style_with(style), children: Vec::new() }
    }

    #[test]
    fn explicit_px_width_and_height_are_used_directly() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("width", "100px"), ("height", "50px")]);
        let root = layout(&node, 800.0, 600.0);
        assert_eq!(root.width, 100.0);
        assert_eq!(root.height, 50.0);
    }

    #[test]
    fn auto_width_fills_available_containing_block_space() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0);
        assert_eq!(root.width, 800.0);
    }

    #[test]
    fn auto_width_subtracts_margin_border_padding_from_available_space() {
        let el = Element::new("div");
        let node = leaf_styled_node(
            &el,
            &[
                ("margin-left", "10px"),
                ("margin-right", "10px"),
                ("border-left", "2px"),
                ("border-right", "2px"),
                ("padding-left", "5px"),
                ("padding-right", "5px"),
            ],
        );
        let root = layout(&node, 800.0, 600.0);
        // 800 - 10 - 10 - 2 - 2 - 5 - 5 = 766
        assert_eq!(root.width, 766.0);
    }

    #[test]
    fn oversized_margins_clamp_content_width_to_zero_not_negative() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("margin-left", "500px"), ("margin-right", "500px")]);
        let root = layout(&node, 800.0, 600.0);
        assert_eq!(root.width, 0.0);
    }

    #[test]
    fn auto_height_with_no_children_is_zero() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0);
        assert_eq!(root.height, 0.0);
    }

    #[test]
    fn explicit_negative_height_clamps_to_zero() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("height", "-20px")]);
        let root = layout(&node, 800.0, 600.0);
        assert_eq!(root.height, 0.0);
    }

    #[test]
    fn auto_height_sums_children_total_box_heights() {
        let parent_el = Element::new("div");
        let child1_el = Element::new("p");
        let child2_el = Element::new("p");
        let child1 = leaf_styled_node(&child1_el, &[("height", "30px"), ("margin-bottom", "5px")]);
        let child2 = leaf_styled_node(&child2_el, &[("height", "20px")]);
        let parent = StyledNode {
            element: &parent_el,
            style: HashMap::new(),
            children: vec![child1, child2],
        };
        let root = layout(&parent, 800.0, 600.0);
        // child1 total = 30 + 5 (margin-bottom) = 35; child2 total = 20; sum = 55
        assert_eq!(root.height, 55.0);
    }

    #[test]
    fn children_stack_vertically_with_correct_y_offsets() {
        let parent_el = Element::new("div");
        let child1_el = Element::new("p");
        let child2_el = Element::new("p");
        let child1 = leaf_styled_node(&child1_el, &[("height", "30px")]);
        let child2 = leaf_styled_node(&child2_el, &[("height", "20px")]);
        let parent = StyledNode {
            element: &parent_el,
            style: HashMap::new(),
            children: vec![child1, child2],
        };
        let root = layout(&parent, 800.0, 600.0);
        assert_eq!(root.children[0].y, 0.0);
        assert_eq!(root.children[1].y, 30.0);
    }

    #[test]
    fn display_none_element_is_excluded_from_tree() {
        let parent_el = Element::new("div");
        let child1_el = Element::new("p");
        let child2_el = Element::new("p");
        let child1 = leaf_styled_node(&child1_el, &[("display", "none"), ("height", "30px")]);
        let child2 = leaf_styled_node(&child2_el, &[("height", "20px")]);
        let parent = StyledNode {
            element: &parent_el,
            style: HashMap::new(),
            children: vec![child1, child2],
        };
        let root = layout(&parent, 800.0, 600.0);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.height, 20.0);
        assert_eq!(root.children[0].y, 0.0);
    }

    // --- Review Focus: display:none subtree excludes children regardless of
    // their own display value ---
    #[test]
    fn display_none_excludes_the_whole_subtree_even_if_children_are_display_block() {
        let hidden_parent_el = Element::new("div");
        let visible_sibling_el = Element::new("div");
        let grandchild_el = Element::new("p");
        let grandchild = leaf_styled_node(&grandchild_el, &[("display", "block"), ("height", "40px")]);
        let hidden_parent = StyledNode {
            element: &hidden_parent_el,
            style: style_with(&[("display", "none"), ("height", "999px")]),
            children: vec![grandchild],
        };
        let visible_sibling = leaf_styled_node(&visible_sibling_el, &[("height", "10px")]);
        let root_el = Element::new("div");
        let root_node = StyledNode {
            element: &root_el,
            style: HashMap::new(),
            children: vec![hidden_parent, visible_sibling],
        };
        let root = layout(&root_node, 800.0, 600.0);
        assert_eq!(root.children.len(), 1, "only the visible sibling should remain");
        assert_eq!(root.height, 10.0, "the display:none subtree's 999px must not count at all");
    }

    // --- Review Focus: display:none siblings contribute no vertical offset ---
    #[test]
    fn display_none_siblings_interspersed_do_not_affect_stacking() {
        let a_el = Element::new("div");
        let b_el = Element::new("div");
        let c_el = Element::new("div");
        let d_el = Element::new("div");
        let a = leaf_styled_node(&a_el, &[("height", "10px")]);
        let b = leaf_styled_node(&b_el, &[("display", "none"), ("height", "500px")]);
        let c = leaf_styled_node(&c_el, &[("height", "20px")]);
        let d = leaf_styled_node(&d_el, &[("display", "none"), ("height", "500px")]);
        let root_el = Element::new("div");
        let root_node = StyledNode {
            element: &root_el,
            style: HashMap::new(),
            children: vec![a, b, c, d],
        };
        let root = layout(&root_node, 800.0, 600.0);
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].y, 0.0); // a
        assert_eq!(root.children[1].y, 10.0); // c, immediately after a — b contributed 0
        assert_eq!(root.height, 30.0);
    }

    #[test]
    fn height_percent_resolves_against_viewport_at_root() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("height", "50%")]);
        let root = layout(&node, 800.0, 600.0);
        assert_eq!(root.height, 300.0);
    }

    #[test]
    fn height_percent_below_root_resolves_to_auto_not_a_percent_of_viewport() {
        let child_el = Element::new("p");
        let child = leaf_styled_node(&child_el, &[("height", "50%")]);
        let parent_el = Element::new("div");
        let parent = StyledNode { element: &parent_el, style: HashMap::new(), children: vec![child] };
        let root = layout(&parent, 800.0, 600.0);
        // child's height:50% is not root, so it resolves as Auto -> 0 (no children of its own)
        assert_eq!(root.children[0].height, 0.0);
        assert_eq!(root.height, 0.0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-layout layout:: -- --nocapture`
Expected: compile error or panic on `todo!()` in `layout_block`.

- [ ] **Step 3: Implement `layout_block`**

```rust
fn layout_block<'a>(
    node: &'a StyledNode<'a>,
    containing_block: ContainingBlock,
    is_root: bool,
) -> Option<LayoutBox<'a>> {
    if node.style.get("display").map(String::as_str) == Some("none") {
        return None;
    }

    let margin = resolve_edges(&node.style, "margin");
    let border = resolve_edges(&node.style, "border");
    let padding = resolve_edges(&node.style, "padding");

    let edges_width = margin.left + margin.right + border.left + border.right + padding.left + padding.right;
    let available_width = (containing_block.content_width - edges_width).max(0.0);

    let width_len = parse_length(node.style.get("width").map(String::as_str));
    let width = resolved_length_or(width_len, containing_block.content_width, available_width).max(0.0);

    let content_x = containing_block.content_x + margin.left + border.left + padding.left;
    let content_y = containing_block.content_y + margin.top + border.top + padding.top;

    let mut children = Vec::new();
    let mut cursor_y = content_y;
    for child in &node.children {
        let child_containing_block = ContainingBlock {
            content_x,
            content_y: cursor_y,
            content_width: width,
            viewport_height: containing_block.viewport_height,
        };
        if let Some(child_box) = layout_block(child, child_containing_block, false) {
            let child_total_height = child_box.margin.top
                + child_box.border.top
                + child_box.padding.top
                + child_box.height
                + child_box.padding.bottom
                + child_box.border.bottom
                + child_box.margin.bottom;
            cursor_y += child_total_height;
            children.push(child_box);
        }
    }

    let height_len = parse_length(node.style.get("height").map(String::as_str));
    let auto_height = (cursor_y - content_y).max(0.0);
    let height = match height_len {
        Length::Percent(p) if is_root => (containing_block.viewport_height * (p / 100.0)).max(0.0),
        Length::Percent(_) => auto_height,
        other => resolved_length_or(other, 0.0, auto_height).max(0.0),
    };

    Some(LayoutBox { styled_node: node, x: content_x, y: content_y, width, height, margin, border, padding, children })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-layout layout::`
Expected: all tests pass (7 from Task 2 + 13 new from this task = 20 in the `layout` module).

- [ ] **Step 5: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: all prior crates' tests pass, plus `ferris-layout`'s 32 tests (12 length + 20 layout).

- [ ] **Step 6: Commit**

```bash
git add ferris-layout/src/layout.rs
git commit -m "feat: add recursive block layout traversal (width-down, height-up, stacking)"
```

---

### Task 4: Integration tests against the real parser pipeline

**Files:**
- Create: `ferris-layout/tests/integration.rs`

**Interfaces:**
- Consumes: `ferris_dom::parser::Parser::parse`, `ferris_css::parser::Parser::parse`, `ferris_style::resolve_styles`, `ferris_layout::layout::layout` (all already published/implemented — Tasks 1-3 for this crate, prior sub-projects for the parsers).
- Produces: nothing new for later tasks — this is the final task of the plan.

- [ ] **Step 1: Write the integration tests**

Create `ferris-layout/tests/integration.rs`:

```rust
use ferris_css::parser::Parser as CssParser;
use ferris_dom::parser::Parser as HtmlParser;
use ferris_layout::layout::layout;
use ferris_style::resolve_styles;

#[test]
fn realistic_page_produces_expected_box_positions_and_sizes() {
    let html = r#"
        <div id="page">
            <header id="head">
                <h1 id="title">Ferris</h1>
            </header>
            <main id="content">
                <p id="para1">first</p>
                <p id="para2">second</p>
            </main>
        </div>
    "#;
    let css = r#"
        #page { width: 800px; }
        header { height: 60px; margin-bottom: 10px; }
        #title { height: 40px; }
        main { padding-top: 5px; padding-bottom: 5px; }
        p { height: 20px; margin-bottom: 8px; }
    "#;

    let dom = HtmlParser::parse(html);
    let stylesheet = CssParser::parse(css);
    let styled = resolve_styles(&dom, &stylesheet);
    let root = layout(&styled, 1024.0, 768.0);

    // #page: explicit width 800px, at the viewport origin.
    assert_eq!(root.x, 0.0);
    assert_eq!(root.y, 0.0);
    assert_eq!(root.width, 800.0);

    let header = &root.children[0];
    assert_eq!(header.height, 60.0);
    assert_eq!(header.y, 0.0);

    let title = &header.children[0];
    assert_eq!(title.height, 40.0);

    let main = &root.children[1];
    // main starts after header's total box height (60 + margin-bottom 10 = 70)
    assert_eq!(main.y, 70.0);

    let para1 = &main.children[0];
    let para2 = &main.children[1];
    // main has padding-top: 5px, so para1's content starts 5px into main's content box
    assert_eq!(para1.y, main.y + 5.0);
    // para2 starts after para1's total box height (20 + margin-bottom 8 = 28)
    assert_eq!(para2.y, para1.y + 28.0);
}

#[test]
fn display_none_element_from_real_css_excludes_subtree_end_to_end() {
    let html = r#"
        <div id="page">
            <div id="hidden-banner"><p id="banner-text">unseen</p></div>
            <p id="visible">seen</p>
        </div>
    "#;
    let css = r#"
        #hidden-banner { display: none; height: 500px; }
        #banner-text { height: 500px; }
        #visible { height: 20px; }
    "#;

    let dom = HtmlParser::parse(html);
    let stylesheet = CssParser::parse(css);
    let styled = resolve_styles(&dom, &stylesheet);
    let root = layout(&styled, 1024.0, 768.0);

    assert_eq!(root.children.len(), 1, "hidden-banner and its child must both be excluded");
    assert_eq!(root.children[0].y, 0.0);
    assert_eq!(root.height, 20.0);
}

// --- Review Focus: height:% resolves against the viewport at the root, and
// to Auto anywhere deeper, using the real parser pipeline end to end ---
#[test]
fn height_percent_resolves_only_at_root_through_real_pipeline() {
    let html = r#"<div id="page"><div id="child"><p id="grandchild"></p></div></div>"#;
    let css = r#"
        #page { height: 50%; }
        #child { height: 50%; }
    "#;

    let dom = HtmlParser::parse(html);
    let stylesheet = CssParser::parse(css);
    let styled = resolve_styles(&dom, &stylesheet);
    let root = layout(&styled, 1024.0, 800.0);

    assert_eq!(root.height, 400.0, "root height:50% resolves against the 800px viewport");
    // #child's height:50% is not the root, so it resolves to Auto (sum of its
    // own children — #grandchild has no explicit height, so 0) rather than 50%
    // of #page's computed 400px height.
    assert_eq!(root.children[0].height, 0.0);
}
```

- [ ] **Step 2: Run tests to verify they fail or pass**

Run: `cargo test --package ferris-layout --test integration`
Expected: PASS if Tasks 1-3 were implemented correctly (this task adds no new production code — it's a pure verification task exercising the real parser pipeline end to end). If any assertion fails, the bug is in Task 1-3's code, not this test — fix the production code, not the expected values, unless hand-tracing the real CSS box model shows the test's expected number is wrong.

- [ ] **Step 3: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: all tests pass — prior crates unchanged, `ferris-layout` at 32 unit + 3 integration = 35 tests, workspace total 125 (prior total after sub-project 2.3, including its post-final-review fix wave — confirmed via `git log`/the pushed state, not the pre-fix 122) + 35 = 160.

- [ ] **Step 4: Run clippy on the new crate**

Run: `cargo clippy --package ferris-layout --all-targets`
Expected: no warnings (or only warnings that also pre-exist in already-approved code from Tasks 1-3, which should be none since this task adds no production code).

- [ ] **Step 5: Commit**

```bash
git add ferris-layout/tests/integration.rs
git commit -m "test: add end-to-end layout integration tests against the real parser pipeline"
```

---
