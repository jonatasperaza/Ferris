# Inline Elements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make inline elements (`span`, `a`, `em`, `strong`, `b`, `i`, `small`, `code`, `sub`, `sup`, `u`, `mark`, or anything with `display: inline`) share the same wrapped line as surrounding text, with each styled segment (a run) painted at its own x-offset and color, instead of every element becoming its own block.

**Architecture:** `ferris-text` gains two low-level primitives (`wrap_line_spans`, byte-range version of the existing `wrap_lines`; `measure_width`, single-line width measurement) that `ferris-layout` uses to flatten a block's leading run of text+inline-element content into one whitespace-collapsed string tagged with per-source-run byte spans, wrap that string once (uniform font-size, matching piece 2.6), then re-split each wrapped line back into styled `InlineRun`s by intersecting the wrap boundaries against the original run spans. `ferris-paint` emits one `TextCommand` per run instead of one per line.

**Tech Stack:** Rust, `cosmic-text` (via `ferris-text`, already a workspace dependency since piece 2.6).

**Spec:** `docs/superpowers/specs/2026-09-25-inline-elements-design.md`

## Global Constraints

- No new crate. Extends `ferris-text` and `ferris-layout` (both already published), adjusts `ferris-paint`.
- `LayoutBox.text_lines: Vec<String>` (piece 2.6) is replaced by `LayoutBox.lines: Vec<Vec<InlineRun<'a>>>` — same conceptual field, richer element type.
- `InlineRun<'a> { text: String, style: &'a HashMap<String, String>, x_offset: f32 }` — `style` is a reference to whichever element's resolved style this run's text came from (the block itself, or a nested inline element), never a copy; `x_offset` is pixels from the start of the line to this run's left edge.
- Inline elements have **no `font-size` of their own** — the whole line uses the containing block's `font_size` (via `resolve_font_size`, unchanged from piece 2.6). A `font-size` declared on an inline element is ignored for layout/paint purposes.
- Inline elements have **no box model of their own** — no margin/border/padding/background-color on inline elements; only `color` (text color) is read from an inline element's style.
- Inline content is always laid out before any block-level direct children — direct generalization of piece 2.6's "text always before children" rule. A block-level direct child stops the inline-flattening pass; any further inline-looking content that appears in the DOM AFTER that block child is laid out as its own separate block box (falls through to the existing block-stacking recursion unchanged, which itself runs its own inline-flattening pass on that child's own descendants) — it does not get folded back into the block's own line-box group. This is a documented simplification, not a bug.
- Whitespace collapsing happens on the FULL flattened sequence (all direct text + inline-descendant text concatenated in DOM order), never per-fragment before concatenation — collapsing a fragment in isolation would destroy the single space that separates it from its neighbors (e.g. `"Hello "` collapsed alone becomes `"Hello"`, losing the separator).
- `display: none` on any element in the inline-flattening walk (at any nesting depth) excludes it and its descendants from the flattened text entirely, without breaking the concatenation of the content around it.
- `is_inline_display(tag_name, style)`: `style["display"]` explicit `"inline"`/`"block"` wins; otherwise a fixed list of known inline HTML tags (`a`, `span`, `em`, `strong`, `b`, `i`, `small`, `code`, `sub`, `sup`, `u`, `mark`) returns `true`, anything else defaults to block.

## Review Focus

- Two adjacent inline runs of different styles on the same wrapped line must be painted at non-overlapping x-positions, the second starting exactly where the first's measured width ends — the whole point of this piece; covered by Task 2's `second_run_on_a_line_starts_at_the_first_runs_measured_width` and Task 3's rewritten multi-run paint test.
- A `display:none` element sitting between two inline text runs must not break the concatenation of the text around it (the text before and after must still end up adjacent, correctly spaced) — covered by Task 2's `display_none_inline_child_is_skipped_without_breaking_surrounding_text`.
- A block-level element appearing as a direct child in the middle of otherwise-inline content must stop the inline flow at exactly that point and be laid out as its own stacked block afterward, without swallowing or duplicating any of the inline content before or after it — covered by Task 2's `block_child_interrupts_inline_flow_and_stacks_separately`.
- Whitespace collapsing across a run boundary (not just within one run) must not lose the separating space nor duplicate it — the worked example `<p>Hello <b>bold</b> world</p>` from the spec, hand-traced there to `[(0,6,p), (6,10,b), (10,16,p)]` over the collapsed string `"Hello bold world"` — covered by Task 2's `hello_bold_world_produces_three_runs_with_correct_styles_and_offsets` and Task 2's direct `collapse_whitespace_with_spans` unit tests.
- An inline element's `color` must actually reach the painted output distinctly from its surrounding text's color, through the real parser pipeline (not just hand-built fixtures) — covered by Task 4's integration test using real HTML+CSS with `b { color: red }`.

---

### Task 1: `ferris-text` — byte-range wrapping and single-line width measurement

**Files:**
- Modify: `ferris-text/src/lib.rs`

**Interfaces:**
- Consumes: nothing new (extends the existing crate, no new external dependency).
- Produces: `pub fn wrap_line_spans(text: &str, font_size: f32, max_width: f32) -> Vec<(usize, usize)>` and `pub fn measure_width(text: &str, font_size: f32) -> f32`, both used by Task 2. `wrap_lines` is refactored to be implemented in terms of `wrap_line_spans` — its own public signature and behavior are unchanged (all 9 of its existing tests must keep passing unmodified).

- [ ] **Step 1: Read the current file before editing**

Read `ferris-text/src/lib.rs` in full — this task refactors `wrap_lines`'s body without changing its signature or any of its 9 existing tests, and adds two new functions alongside it.

- [ ] **Step 2: Write the failing tests**

Add these to the existing `#[cfg(test)] mod tests` block in `ferris-text/src/lib.rs` (do not remove or modify any existing test):

```rust
    #[test]
    fn wrap_line_spans_byte_ranges_match_wrap_lines_output() {
        let text = "this is a long sentence that should not fit on a single line inside a very narrow box";
        let spans = wrap_line_spans(text, 16.0, 80.0);
        let via_spans: Vec<&str> = spans.iter().map(|&(start, end)| &text[start..end]).collect();
        let via_wrap_lines = wrap_lines(text, 16.0, 80.0);
        assert_eq!(via_spans, via_wrap_lines, "slicing text by wrap_line_spans' byte ranges must reproduce exactly what wrap_lines returns");
    }

    #[test]
    fn wrap_line_spans_empty_text_returns_empty_vec() {
        assert!(wrap_line_spans("", 16.0, 800.0).is_empty());
    }

    #[test]
    fn wrap_line_spans_non_positive_max_width_does_not_panic() {
        let spans = wrap_line_spans("hello world", 16.0, 0.0);
        assert!(!spans.is_empty());
    }

    #[test]
    fn measure_width_of_empty_string_is_zero() {
        assert_eq!(measure_width("", 16.0), 0.0);
    }

    #[test]
    fn measure_width_longer_text_is_wider() {
        let short = measure_width("hi", 16.0);
        let long = measure_width("hello world this is longer", 16.0);
        assert!(long > short, "longer text ({long}) should measure wider than shorter text ({short})");
    }

    #[test]
    fn measure_width_larger_font_is_wider() {
        let small = measure_width("hello", 12.0);
        let large = measure_width("hello", 48.0);
        assert!(large > small, "the same text at a larger font size ({large}) should measure wider than at a smaller one ({small})");
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --package ferris-text -- --nocapture`
Expected: compile error (`wrap_line_spans`/`measure_width` not found).

- [ ] **Step 4: Refactor `wrap_lines` into `wrap_line_spans`, and add `measure_width`**

This is the plan author's best understanding of the `cosmic-text` API, following the exact same pattern already proven correct in piece 2.6 (verify against the real crate — vendored source under `%USERPROFILE%\.cargo\registry\src\*\cosmic-text-0.12.1\src\` or `cargo doc --open -p cosmic-text` — if anything here doesn't compile or behave as described; piece 2.6's own final review already empirically confirmed `LayoutRun` has a `line_w: f32` field via a probe, which `measure_width` below relies on).

Replace the existing `wrap_lines` function with:

```rust
pub fn wrap_line_spans(text: &str, font_size: f32, max_width: f32) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return Vec::new();
    }

    let width = if max_width > 0.0 { max_width } else { 1.0 };
    let mut font_system = match font_system().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let metrics = Metrics::new(font_size, line_height(font_size));
    let mut buffer = Buffer::new(&mut font_system, metrics);
    buffer.set_size(&mut font_system, Some(width), None);
    buffer.set_text(&mut font_system, text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    buffer.shape_until_scroll(&mut font_system, false);

    let mut spans = Vec::new();
    for run in buffer.layout_runs() {
        if run.glyphs.is_empty() {
            continue;
        }
        let start = run.glyphs.iter().map(|g| g.start).min().unwrap();
        let end = run.glyphs.iter().map(|g| g.end).max().unwrap();
        spans.push((start, end));
    }

    if spans.is_empty() {
        spans.push((0, text.len()));
    }

    spans
}

pub fn wrap_lines(text: &str, font_size: f32, max_width: f32) -> Vec<String> {
    wrap_line_spans(text, font_size, max_width)
        .into_iter()
        .map(|(start, end)| text[start..end].to_string())
        .collect()
}

/// Measures the rendered pixel width of `text` laid out on a single,
/// unbounded line (no wrapping) at `font_size`. Used to position `InlineRun`s
/// left-to-right within an already-wrapped line — measurement, not wrapping.
pub fn measure_width(text: &str, font_size: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    let mut font_system = match font_system().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let metrics = Metrics::new(font_size, line_height(font_size));
    let mut buffer = Buffer::new(&mut font_system, metrics);
    // No width bound: a single unwrapped measurement line, same technique
    // that fixed piece 2.6's double-wrap renderer bug (see
    // ferris-compositor/src/renderer/text.rs).
    buffer.set_size(&mut font_system, None, None);
    buffer.set_text(&mut font_system, text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    buffer.shape_until_scroll(&mut font_system, false);

    buffer.layout_runs().map(|run| run.line_w).fold(0.0f32, f32::max)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --package ferris-text`
Expected: 15 passed (9 pre-existing, unmodified, plus 6 new).

- [ ] **Step 6: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: all prior crates' tests still pass. Workspace baseline before this plan is 206 (post-2.6, confirmed via `git log`/the pushed state). Workspace total after this task: 206 + 6 = 212.

- [ ] **Step 7: Commit**

```bash
git add ferris-text/src/lib.rs
git commit -m "feat: add wrap_line_spans and measure_width to ferris-text"
```

---

### Task 2: Flatten and lay out inline content in `ferris-layout`

**Files:**
- Modify: `ferris-layout/src/layout.rs`

**Interfaces:**
- Consumes: `ferris_text::{wrap_line_spans, measure_width, line_height}` (Task 1); `ferris_dom::dom::{Node, Element}`; `ferris_style::StyledNode`; `crate::length::resolve_font_size` (already exists, piece 2.6).
- Produces: `pub struct InlineRun<'a> { pub text: String, pub style: &'a HashMap<String, String>, pub x_offset: f32 }`, `LayoutBox.lines: Vec<Vec<InlineRun<'a>>>` (replaces `text_lines: Vec<String>`), used by Task 3.

- [ ] **Step 1: Read the current file before editing**

Read `ferris-layout/src/layout.rs` in full. This task replaces the "collect direct `Node::Text` children, collapse, wrap" block inside `layout_block` (currently lines computing `raw_text`/`collapsed_text`/`text_lines`/`text_block_height`, and the children loop that follows immediately after) with the new inline-flattening pipeline, and renames the `text_lines` field to `lines` throughout the file (struct definition, the final `Some(LayoutBox { ... })` construction, and every test that constructs a `LayoutBox`/calls `layout()` and inspects the field).

- [ ] **Step 2: Write the failing tests**

Add `use ferris_dom::dom::Element;` near the top of the file if not already present in scope outside the test module (the existing test module already imports it — check before duplicating). Add these to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn is_inline_display_recognizes_known_inline_tags() {
        let style = HashMap::new();
        for tag in ["a", "span", "em", "strong", "b", "i", "small", "code", "sub", "sup", "u", "mark"] {
            assert!(is_inline_display(tag, &style), "{tag} should be inline by default");
        }
    }

    #[test]
    fn is_inline_display_defaults_unknown_tags_to_block() {
        let style = HashMap::new();
        assert!(!is_inline_display("div", &style));
        assert!(!is_inline_display("p", &style));
        assert!(!is_inline_display("some-custom-tag", &style));
    }

    #[test]
    fn is_inline_display_explicit_inline_wins_over_a_normally_block_tag() {
        let style = style_with(&[("display", "inline")]);
        assert!(is_inline_display("div", &style));
    }

    #[test]
    fn is_inline_display_explicit_block_wins_over_a_normally_inline_tag() {
        let style = style_with(&[("display", "block")]);
        assert!(!is_inline_display("span", &style));
    }

    #[test]
    fn collapse_whitespace_with_spans_merges_double_space_and_adjacent_same_style_runs() {
        let a = style_with(&[("color", "red")]);
        let raw_spans: Vec<(usize, usize, &HashMap<String, String>)> = vec![(0, 5, &a), (5, 11, &a)];
        let (collapsed, spans) = collapse_whitespace_with_spans("hello  world", &raw_spans);
        assert_eq!(collapsed, "hello world");
        assert_eq!(spans, vec![(0, 11, &a)], "two adjacent same-style spans separated by collapsed whitespace must merge into one span");
    }

    #[test]
    fn collapse_whitespace_with_spans_trims_leading_and_trailing_whitespace() {
        let a = style_with(&[]);
        let raw_spans: Vec<(usize, usize, &HashMap<String, String>)> = vec![(0, 8, &a)];
        let (collapsed, spans) = collapse_whitespace_with_spans("  hi  ", &raw_spans);
        assert_eq!(collapsed, "hi");
        assert_eq!(spans, vec![(0, 2, &a)]);
    }

    #[test]
    fn hello_bold_world_produces_three_runs_with_correct_styles_and_offsets() {
        // Worked example from the spec: <p>Hello <b>bold</b> world</p>
        let mut b_el = Element::new("b");
        b_el.children.push(Node::Text("bold".to_string()));
        let b_styled = StyledNode { element: &b_el, style: HashMap::new(), children: Vec::new() };

        let mut p_el = Element::new("p");
        p_el.children.push(Node::Text("Hello ".to_string()));
        p_el.children.push(Node::Element(b_el.clone()));
        p_el.children.push(Node::Text(" world".to_string()));
        let p_styled = StyledNode { element: &p_el, style: HashMap::new(), children: vec![b_styled] };

        let root = layout(&p_styled, 800.0, 600.0).unwrap();
        assert_eq!(root.lines.len(), 1, "short text at 800px must fit on one line");
        let line = &root.lines[0];
        assert_eq!(line.len(), 3, "expected 3 runs: 'Hello ', 'bold', ' world'");
        assert_eq!(line[0].text, "Hello ");
        assert_eq!(line[1].text, "bold");
        assert_eq!(line[2].text, " world");
        assert_eq!(line[0].x_offset, 0.0);
        assert!(line[1].x_offset > 0.0, "second run must start after the first, not at 0");
        assert!(line[2].x_offset > line[1].x_offset, "third run must start after the second");
    }

    #[test]
    fn second_run_on_a_line_starts_at_the_first_runs_measured_width() {
        let mut b_el = Element::new("b");
        b_el.children.push(Node::Text("bold".to_string()));
        let b_styled = StyledNode { element: &b_el, style: HashMap::new(), children: Vec::new() };

        let mut p_el = Element::new("p");
        p_el.children.push(Node::Text("Hello ".to_string()));
        p_el.children.push(Node::Element(b_el.clone()));
        let p_styled = StyledNode { element: &p_el, style: HashMap::new(), children: vec![b_styled] };

        let root = layout(&p_styled, 800.0, 600.0).unwrap();
        let line = &root.lines[0];
        let expected_offset = ferris_text::measure_width("Hello ", 16.0);
        assert!(
            (line[1].x_offset - expected_offset).abs() < 0.01,
            "second run's x_offset ({}) should equal the first run's measured width ({})",
            line[1].x_offset,
            expected_offset
        );
    }

    #[test]
    fn display_none_inline_child_is_skipped_without_breaking_surrounding_text() {
        let mut hidden_el = Element::new("span");
        hidden_el.children.push(Node::Text("hidden".to_string()));
        let hidden_styled = StyledNode { element: &hidden_el, style: style_with(&[("display", "none")]), children: Vec::new() };

        let mut p_el = Element::new("p");
        p_el.children.push(Node::Text("before ".to_string()));
        p_el.children.push(Node::Element(hidden_el.clone()));
        p_el.children.push(Node::Text(" after".to_string()));
        let p_styled = StyledNode { element: &p_el, style: HashMap::new(), children: vec![hidden_styled] };

        let root = layout(&p_styled, 800.0, 600.0).unwrap();
        let line = &root.lines[0];
        let joined: String = line.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(joined, "before after");
        assert!(!joined.contains("hidden"));
    }

    #[test]
    fn block_child_interrupts_inline_flow_and_stacks_separately() {
        let mut span_el = Element::new("span");
        span_el.children.push(Node::Text("inline text".to_string()));
        let span_styled = StyledNode { element: &span_el, style: HashMap::new(), children: Vec::new() };

        let mut div_el = Element::new("div");
        div_el.children.push(Node::Text("block content".to_string()));
        let div_styled = StyledNode { element: &div_el, style: style_with(&[("height", "20px")]), children: Vec::new() };

        let mut p_el = Element::new("p");
        p_el.children.push(Node::Element(span_el.clone()));
        p_el.children.push(Node::Element(div_el.clone()));
        let p_styled = StyledNode { element: &p_el, style: HashMap::new(), children: vec![span_styled, div_styled] };

        let root = layout(&p_styled, 800.0, 600.0).unwrap();
        assert_eq!(root.lines.len(), 1, "the leading inline span must produce one line of inline content");
        assert_eq!(root.lines[0][0].text, "inline text");
        assert_eq!(root.children.len(), 1, "the div must be stacked as a block child, not flattened into the inline line");
        assert_eq!(root.children[0].height, 20.0);
        assert!(root.children[0].y > root.y, "the block child must be positioned below the inline content");
    }

    #[test]
    fn element_without_any_content_has_empty_lines_and_no_children() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert!(root.lines.is_empty());
        assert!(root.children.is_empty());
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --package ferris-layout layout:: -- --nocapture`
Expected: compile errors (`is_inline_display`, `collapse_whitespace_with_spans`, `InlineRun`, `.lines` field not found).

- [ ] **Step 4: Implement `is_inline_display`**

Add this private function to `ferris-layout/src/layout.rs` (near the top, after the `Edges` struct or alongside `resolve_edges`):

```rust
fn is_inline_display(tag_name: &str, style: &HashMap<String, String>) -> bool {
    match style.get("display").map(String::as_str) {
        Some("inline") => return true,
        Some("block") => return false,
        _ => {}
    }
    matches!(
        tag_name,
        "a" | "span" | "em" | "strong" | "b" | "i" | "small" | "code" | "sub" | "sup" | "u" | "mark"
    )
}
```

- [ ] **Step 5: Implement `InlineRun` and the flattening/collapsing/positioning pipeline**

Add `InlineRun` near `LayoutBox`'s definition:

```rust
pub struct InlineRun<'a> {
    pub text: String,
    pub style: &'a HashMap<String, String>,
    pub x_offset: f32,
}
```

Change `LayoutBox.text_lines: Vec<String>` to:

```rust
    pub lines: Vec<Vec<InlineRun<'a>>>,
```

Add these three private functions (place them near `layout_block`, before its definition):

```rust
/// Walks `node`'s direct children in true DOM order (mixing `Node::Text` and
/// `Node::Element`), appending every leading run of text/inline-element
/// content's RAW (uncollapsed) text into `text_out` and recording one span
/// per contiguous source run — `(byte_start, byte_end, style)` within
/// `text_out` — into `spans_out`. Recurses into inline descendants (and
/// their own inline descendants, arbitrarily deep). Stops at the first
/// direct child that is a block-level element (not `display:none`, not
/// inline). Returns the index into `node.children` (the `StyledNode`-only,
/// element-filtered list — see `ferris_style::resolve_node`, which builds
/// it in the same relative order as the `Node::Element` entries in
/// `node.element.children`) of the first child NOT consumed by this pass —
/// i.e. where the block-child stacking loop should resume — or
/// `node.children.len()` if every direct child was inline/text.
fn flatten_inline_content<'a>(
    node: &'a StyledNode<'a>,
    text_out: &mut String,
    spans_out: &mut Vec<(usize, usize, &'a HashMap<String, String>)>,
) -> usize {
    let mut element_child_idx = 0;
    for raw_child in &node.element.children {
        match raw_child {
            Node::Text(s) => {
                let start = text_out.len();
                text_out.push_str(s);
                let end = text_out.len();
                if end > start {
                    spans_out.push((start, end, &node.style));
                }
            }
            Node::Comment(_) => {}
            Node::Element(_) => {
                let child_styled = &node.children[element_child_idx];
                element_child_idx += 1;
                if child_styled.style.get("display").map(String::as_str) == Some("none") {
                    continue;
                }
                if is_inline_display(&child_styled.element.tag_name, &child_styled.style) {
                    flatten_inline_content(child_styled, text_out, spans_out);
                } else {
                    return element_child_idx - 1;
                }
            }
        }
    }
    element_child_idx
}

/// Collapses whitespace over the FULL raw sequence (never per-fragment —
/// see the spec's worked example for why), remapping `raw_spans`' byte
/// ranges into the collapsed string's coordinate space and merging adjacent
/// same-style spans that end up contiguous after collapsing.
fn collapse_whitespace_with_spans<'a>(
    raw_text: &str,
    raw_spans: &[(usize, usize, &'a HashMap<String, String>)],
) -> (String, Vec<(usize, usize, &'a HashMap<String, String>)>) {
    let mut collapsed = String::new();
    let mut collapsed_spans: Vec<(usize, usize, &'a HashMap<String, String>)> = Vec::new();
    let mut last_was_space = true;
    let mut span_idx = 0;

    for (byte_pos, ch) in raw_text.char_indices() {
        while span_idx + 1 < raw_spans.len() && byte_pos >= raw_spans[span_idx].1 {
            span_idx += 1;
        }
        let style = raw_spans[span_idx].2;

        if ch.is_whitespace() {
            if !last_was_space {
                let start = collapsed.len();
                collapsed.push(' ');
                let end = collapsed.len();
                push_or_extend_span(&mut collapsed_spans, start, end, style);
            }
            last_was_space = true;
        } else {
            let start = collapsed.len();
            collapsed.push(ch);
            let end = collapsed.len();
            push_or_extend_span(&mut collapsed_spans, start, end, style);
            last_was_space = false;
        }
    }

    if collapsed.ends_with(' ') {
        collapsed.pop();
        if let Some(last) = collapsed_spans.last_mut() {
            last.1 -= 1;
            if last.0 == last.1 {
                collapsed_spans.pop();
            }
        }
    }

    (collapsed, collapsed_spans)
}

fn push_or_extend_span<'a>(
    spans: &mut Vec<(usize, usize, &'a HashMap<String, String>)>,
    start: usize,
    end: usize,
    style: &'a HashMap<String, String>,
) {
    if let Some(last) = spans.last_mut() {
        if std::ptr::eq(last.2, style) && last.1 == start {
            last.1 = end;
            return;
        }
    }
    spans.push((start, end, style));
}

/// Builds the `InlineRun`s for one wrapped line by intersecting the line's
/// byte range (within the collapsed text) against every collapsed span,
/// accumulating each run's `x_offset` left to right via `measure_width`.
fn runs_for_line<'a>(
    collapsed_text: &str,
    collapsed_spans: &[(usize, usize, &'a HashMap<String, String>)],
    line_start: usize,
    line_end: usize,
    font_size: f32,
) -> Vec<InlineRun<'a>> {
    let mut runs = Vec::new();
    let mut x_offset = 0.0;
    for &(span_start, span_end, style) in collapsed_spans {
        let overlap_start = span_start.max(line_start);
        let overlap_end = span_end.min(line_end);
        if overlap_start < overlap_end {
            let text = collapsed_text[overlap_start..overlap_end].to_string();
            let width = ferris_text::measure_width(&text, font_size);
            runs.push(InlineRun { text, style, x_offset });
            x_offset += width;
        }
    }
    runs
}
```

- [ ] **Step 6: Wire the pipeline into `layout_block`**

Replace the existing text-handling block (the `raw_text`/`collapsed_text`/`text_lines`/`text_block_height` computation, currently sitting between the `content_y` calculation and the children loop) with:

```rust
    let mut raw_text = String::new();
    let mut raw_spans: Vec<(usize, usize, &HashMap<String, String>)> = Vec::new();
    let resume_idx = flatten_inline_content(node, &mut raw_text, &mut raw_spans);
    let (collapsed_text, collapsed_spans) = collapse_whitespace_with_spans(&raw_text, &raw_spans);

    let font_size = resolve_font_size(&node.style);
    let lines: Vec<Vec<InlineRun>> = if collapsed_text.is_empty() {
        Vec::new()
    } else {
        ferris_text::wrap_line_spans(&collapsed_text, font_size, width)
            .into_iter()
            .map(|(line_start, line_end)| {
                runs_for_line(&collapsed_text, &collapsed_spans, line_start, line_end, font_size)
            })
            .collect()
    };
    let text_block_height = lines.len() as f32 * ferris_text::line_height(font_size);
```

Change the children loop to only process the StyledNode children NOT already consumed by inline flattening — replace:
```rust
    let mut children = Vec::new();
    let mut cursor_y = content_y + text_block_height;
    for child in &node.children {
```
with:
```rust
    let mut children = Vec::new();
    let mut cursor_y = content_y + text_block_height;
    for child in &node.children[resume_idx..] {
```

Update the final `Some(LayoutBox { ... })` construction, renaming the field:
```rust
    Some(LayoutBox { styled_node: node, x: content_x, y: content_y, width, height, margin, border, padding, lines, children })
```

- [ ] **Step 7: Update every existing test that references `text_lines`**

Search the file's existing test module for every occurrence of `text_lines` (both in `LayoutBox { ... }` literal constructions and in assertions like `root.text_lines`) and rename to `lines`. For assertions that compared against a `Vec<String>` (e.g. `assert_eq!(root.text_lines, vec!["hi".to_string()])`), update them to compare against the new shape — for a single-run-per-line case, this becomes something like checking `root.lines.len()` and `root.lines[0][0].text` instead of comparing a flat `Vec<String>` directly. Read each existing assertion in context before changing it; the goal is to preserve exactly what each test originally verified (line count, height calculation, `display:none` exclusion, etc.), just expressed against the new `Vec<Vec<InlineRun>>` shape instead of `Vec<String>`.

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --package ferris-layout layout::`
Expected: all layout.rs tests pass (26 pre-existing, all updated for the field rename/retype but none removed, plus 11 new = 37).

- [ ] **Step 9: Run the whole crate and workspace test suites**

Run: `cargo test --package ferris-layout`
Expected: 37 layout + 20 length + 3 integration = 60 (the 3 pre-existing `tests/integration.rs` tests may need the same `text_lines`→`lines`/shape update if they inspect that field directly — check and update if so; if any fixture's wrapping behavior legitimately changes because inline content is now handled differently, hand-trace before changing an assertion, same discipline as every prior task in this project).

Run: `cargo test --workspace`
Expected: 212 (from Task 1) + 11 (this task's new tests; the field rename itself doesn't add or remove tests) = 223.

- [ ] **Step 10: Commit**

```bash
git add ferris-layout/src/layout.rs
git commit -m "feat: flatten and lay out inline content sharing lines with surrounding text"
```

---

### Task 3: Paint one `TextCommand` per `InlineRun`

**Files:**
- Modify: `ferris-paint/src/paint.rs`

**Interfaces:**
- Consumes: `LayoutBox.lines: Vec<Vec<InlineRun<'a>>>` and `InlineRun<'a> { text, style, x_offset }` (Task 2); `ferris_layout::length::resolve_font_size`; `ferris_text::line_height` (both already used since piece 2.6).
- Produces: nothing new for later tasks — `paint()`'s public signature is unchanged.

- [ ] **Step 1: Read the current file before editing**

Read `ferris-paint/src/paint.rs` in full — this task replaces the text-handling block inside `paint_node` (currently iterating `node.text_lines: Vec<String>`, one `TextCommand` per line) and updates every test that references `text_lines`.

- [ ] **Step 2: Replace `paint_node`'s text-handling block**

Replace the current block:
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
with:
```rust
    if !node.lines.is_empty() {
        let size = resolve_font_size(&node.styled_node.style);
        let line_h = ferris_text::line_height(size);
        for (i, line) in node.lines.iter().enumerate() {
            for run in line {
                let color = parse_color(run.style.get("color").map(String::as_str))
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                frame.push(DrawCommand::Text(TextCommand {
                    x: node.x + run.x_offset,
                    y: node.y + i as f32 * line_h,
                    content: run.text.clone(),
                    size,
                    color,
                }));
            }
        }
    }
```

- [ ] **Step 3: Update every existing test that constructs a `LayoutBox` or references `text_lines`**

Search the test module for `text_lines` and `LayoutBox { ... }` literals. Rename `text_lines` to `lines` everywhere. Wherever a test previously set `text_lines: vec!["Hi".to_string()]` (a single line, one implicit run using the node's own style), change it to `lines: vec![vec![InlineRun { text: "Hi".to_string(), style: &styled.style, x_offset: 0.0 }]]` (one line containing one run, sourced from that same test's own `styled: StyledNode` — read each test to find the right `style` variable name in scope). Add `use ferris_layout::layout::InlineRun;` to the test module's imports if not already present via a glob import.

- [ ] **Step 4: Rewrite the two `text_lines`-specific tests for the new run-based shape**

Replace `text_lines_produce_one_text_command_per_line_with_correct_y_offset` with:

```rust
    #[test]
    fn lines_produce_one_text_command_per_run_with_correct_x_and_y_offset() {
        let element = Element::new("p");
        let styled = StyledNode { element: &element, style: style_with(&[("font-size", "20px")]), children: Vec::new() };
        let red_style = style_with(&[("color", "red")]);
        let node = LayoutBox {
            styled_node: &styled,
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 48.0,
            margin: Edges::default(),
            border: Edges::default(),
            padding: Edges::default(),
            lines: vec![
                vec![
                    InlineRun { text: "Hello ".to_string(), style: &styled.style, x_offset: 0.0 },
                    InlineRun { text: "world".to_string(), style: &red_style, x_offset: 40.0 },
                ],
                vec![InlineRun { text: "second line".to_string(), style: &styled.style, x_offset: 0.0 }],
            ],
            children: Vec::new(),
        };

        let frame = paint(&node);
        assert_eq!(frame.commands.len(), 3);
        let DrawCommand::Text(first) = &frame.commands[0] else { panic!("expected text") };
        let DrawCommand::Text(second) = &frame.commands[1] else { panic!("expected text") };
        let DrawCommand::Text(third) = &frame.commands[2] else { panic!("expected text") };
        assert_eq!(first.content, "Hello ");
        assert_eq!(first.x, 10.0);
        assert_eq!(first.y, 20.0);
        assert_eq!(first.color, [0.0, 0.0, 0.0, 1.0], "no color set, falls back to black");
        assert_eq!(second.content, "world");
        assert_eq!(second.x, 10.0 + 40.0, "second run's x is the block's x plus its x_offset");
        assert_eq!(second.y, 20.0, "same line as the first run");
        assert_eq!(second.color, [1.0, 0.0, 0.0, 1.0], "the run's own style color, not the block's");
        assert_eq!(third.content, "second line");
        assert_eq!(third.y, 20.0 + ferris_text::line_height(20.0), "second line is one line-height below the first");
    }
```

Replace `empty_text_lines_produces_no_text_command` with:

```rust
    #[test]
    fn empty_lines_produces_no_text_command() {
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
            lines: Vec::new(),
            children: Vec::new(),
        };

        let frame = paint(&node);
        assert!(frame.commands.is_empty());
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --package ferris-paint paint::`
Expected: 7 passed (same count as before this task — no tests added or removed, all updated for the field rename; the two rewritten tests replace their predecessors one-for-one, and this task's Step 4 test also happens to cover the "different runs get different colors" behavior that would otherwise need a separate new test).

- [ ] **Step 6: Run the whole crate and workspace test suites**

Run: `cargo test --package ferris-paint`
Expected: 23 passed (7 paint + 13 color + 3 integration — the 3 pre-existing `tests/integration.rs` tests may need the same field-shape update if they inspect `.lines`/`.text_lines` directly, or may be entirely unaffected if they only inspect `Frame`/`DrawCommand` output; check by running, hand-trace before changing any assertion).

Run: `cargo test --workspace`
Expected: 223 (from Task 2) + 0 = 223 (this task renames/retypes existing tests, adds none net-new).

- [ ] **Step 7: Commit**

```bash
git add ferris-paint/src/paint.rs
git commit -m "refactor: paint one TextCommand per InlineRun instead of one per line"
```

---

### Task 4: Integration test + mandatory manual visual verification

**Files:**
- Modify: `ferris-paint/tests/integration.rs`
- Modify: `ferris-compositor/assets/fixture.html`
- Modify: `ferris-compositor/assets/fixture.css`

**Interfaces:**
- Consumes: the full pipeline as already wired (`ferris_dom`, `ferris_css`, `ferris_style::resolve_styles`, `ferris_layout::layout::layout`, `ferris_paint::paint::paint`) — no new production code, this task only adds a test and fixture content.
- Produces: nothing consumed by later tasks — this is the final task of the plan.

- [ ] **Step 1: Write the integration test for real inline styling**

Read `ferris-paint/tests/integration.rs` first to see its current helpers and existing 3 tests. Append:

```rust
// --- Review Focus: an inline element's color must reach the painted output
// distinctly from its surrounding text's color, through the real parser
// pipeline end to end ---
#[test]
fn inline_element_color_differs_from_surrounding_text_through_real_pipeline() {
    let html = r#"<div id="box"><p id="para">Hello <b id="bold">bold</b> world</p></div>"#;
    let css = r#"
        #box { width: 800px; }
        #bold { color: red; }
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

    assert_eq!(texts.len(), 3, "expected 3 runs: 'Hello ', 'bold', ' world'");
    assert_eq!(texts[0].content, "Hello ");
    assert_eq!(texts[0].color, [0.0, 0.0, 0.0, 1.0], "default black, no color set on #para");
    assert_eq!(texts[1].content, "bold");
    assert_eq!(texts[1].color, [1.0, 0.0, 0.0, 1.0], "#bold's own red color");
    assert_eq!(texts[2].content, " world");
    assert_eq!(texts[2].color, [0.0, 0.0, 0.0, 1.0], "back to black after the inline element");
    assert!(texts[1].x > texts[0].x, "the bold run must start after the first run");
    assert!(texts[2].x > texts[1].x, "the trailing run must start after the bold run");
}
```

- [ ] **Step 2: Run the new test to verify it passes**

Run: `cargo test --package ferris-paint --test integration inline_element_color_differs_from_surrounding_text_through_real_pipeline`
Expected: PASS. If it fails, hand-trace the real flattening/collapse/wrap behavior for this exact HTML/CSS before changing the test's expectations — Tasks 1-3's code is already reviewed at this point.

- [ ] **Step 3: Run the whole crate and workspace test suites**

Run: `cargo test --package ferris-paint`
Expected: 24 passed (23 from Task 3 + 1 new integration test).

Run: `cargo test --workspace`
Expected: 223 (from Task 3) + 1 = 224.

Run: `cargo clippy --workspace --all-targets`
Expected: no new warnings (pre-existing warnings from prior pieces are expected and out of scope).

- [ ] **Step 4: Extend the compositor fixture to demonstrate inline color visually**

Read `ferris-compositor/assets/fixture.html` and `fixture.css` first (from pieces 2.5/2.6) to confirm current exact content. Add a `<b>` (or `<span>`) with its own distinct color inside one of the existing paragraphs — for example, change the `#intro` paragraph to wrap one phrase in a colored inline element:

```html
        <p id="intro">A browser built from scratch in Rust, with its own HTML parser, its own CSS parser, its own style resolution, its own block and text layout engine, and its <b id="highlight">own bridge</b> into a real GPU compositor — no engine borrowed from anywhere else.</p>
```

Add to `fixture.css`:
```css
#highlight {
    color: #cc3333;
}
```

- [ ] **Step 5: Manual verification — run the binary and look at the window**

This step cannot be automated; per the spec, it is the actual acceptance test for this plan's visual claim.

Run: `cargo build --release -p ferris-compositor` then run the resulting binary (or capture a screenshot via the PowerShell `Start-Process`/`GetWindowRect`/`CopyFromScreen` technique already established in pieces 2.4-2.6, then use the Read tool on the resulting PNG).

Confirm, by looking at the window:
- The `#intro` paragraph still wraps across multiple lines as it did after piece 2.6.
- The phrase "own bridge" appears in a visibly different color (reddish, `#cc3333`) from the surrounding black text, sitting inline within the same line(s) as the text around it — not on its own separate line, not as its own block.
- No overlapping text, no dropped words, no broken line spacing.

Close the window (or kill the process) once confirmed. Delete any temporary screenshot file created for this check. Note in the task's completion report exactly what was seen — a task reviewer for this task must ask the implementer to describe what appeared, and treat an implementer that skipped running the binary as not having completed the task's actual deliverable, same discipline as pieces 2.4-2.6.

- [ ] **Step 6: Commit**

```bash
git add ferris-paint/tests/integration.rs ferris-compositor/assets/fixture.html ferris-compositor/assets/fixture.css
git commit -m "test: add real-pipeline inline-color integration test and demonstrate it visually in the fixture"
```

---
