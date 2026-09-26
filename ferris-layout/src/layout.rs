use std::collections::HashMap;

use ferris_dom::dom::Node;
use ferris_style::StyledNode;

use crate::length::{parse_length, resolve_font_size, Length};

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
    pub lines: Vec<Vec<InlineRun<'a>>>,
    pub children: Vec<LayoutBox<'a>>,
    pub image: Option<ferris_scene::DecodedImage>,
}

/// One styled, contiguous, already-positioned run of text within a single
/// wrapped line — e.g. the `"bold"` segment of `<p>Hello <b>bold</b>
/// world</p>` gets its own `InlineRun` with `style` pointing at `<b>`'s
/// resolved style and `x_offset` equal to the measured width of `"Hello "`
/// that precedes it on the same line.
pub struct InlineRun<'a> {
    pub text: String,
    pub style: &'a HashMap<String, String>,
    pub x_offset: f32,
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
    let side = |suffix: &str| -> f32 {
        // `border-{side}` (e.g. `border-top`) is a CSS shorthand combining
        // width+style+color, not a width value — the actual longhand
        // property for width alone is `border-{side}-width`. margin/padding
        // have no such shorthand-vs-longhand split (no separate "implicit
        // width" longhand distinct from the shorthand), so they keep the
        // plain `{property}-{side}` key.
        let key = if property == "border" {
            format!("border-{suffix}-width")
        } else {
            format!("{property}-{suffix}")
        };
        let len = parse_length(style.get(&key).map(String::as_str));
        match len {
            Length::Px(n) => n,
            Length::Percent(_) | Length::Auto => 0.0,
        }
    };
    Edges {
        top: side("top"),
        right: side("right"),
        bottom: side("bottom"),
        left: side("left"),
    }
}

/// Determines whether `tag_name`/`style` should participate in inline flow
/// (sharing a wrapped line with surrounding text) rather than stacking as
/// its own block box. An explicit `display: inline`/`display: block` always
/// wins; absent that, a fixed list of known inline HTML tags defaults to
/// inline and everything else defaults to block.
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

/// Resolves a single `Length` against `basis` (the relevant containing dimension).
/// `Length::Auto` returns `default_if_auto` — callers pass the "fill available
/// space" computation for width, or `0.0` as a placeholder for height (Task 3
/// overrides height's `Auto` case with the real sum-of-children logic, this
/// helper alone can't know the children's heights).
fn resolved_length_or(len: Length, basis: f32, default_if_auto: f32) -> f32 {
    match len {
        Length::Px(n) => n,
        Length::Percent(p) => basis * (p / 100.0),
        Length::Auto => default_if_auto,
    }
}

/// Computes the layout tree for `root` against a viewport of
/// `viewport_width` x `viewport_height`. This is the crate's public entry
/// point (consumed directly by piece 2.5).
///
/// Returns `None` when `root` itself is `display: none` — real, valid CSS
/// (e.g. `html { display: none; }`) can put the whole document in that
/// state, and this crate never panics on valid input (same philosophy as
/// `ferris-css`/`ferris-style`). Callers that know their root is never
/// hidden may `.unwrap()`; callers building a general-purpose consumer
/// (piece 2.5) should handle `None` as "nothing to paint."
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

/// Walks `node`'s direct children in true DOM order (mixing `Node::Text` and
/// `Node::Element`), appending every text/inline-element content's RAW
/// (uncollapsed) text into `text_out` and recording one span per contiguous
/// source run into `spans_out`. Recurses into inline descendants via
/// `flatten_inline_content_subtree` (which never stops early, see below).
/// Tracks the index of the FIRST block-level direct child found (if any) as
/// the resume point for the caller's block-stacking loop, but — unlike an
/// earlier version of this function — does NOT stop scanning when it finds
/// one: it keeps appending any further Text/inline content into the SAME
/// leading `text_out`/`spans_out` buffers, so no text is ever silently
/// dropped just because a block sibling sits between two runs of inline
/// content. This matches piece 2.6's original "pool all direct text
/// regardless of position" guarantee while still adding 2.7's inline
/// flattening/coloring on top of it. `display:none` is skipped at any
/// depth, same as before. Returns the index into `node.children` (the
/// `StyledNode`-only, element-filtered list) of the first block child, or
/// `node.children.len()` if every direct child was inline/text.
fn flatten_inline_content<'a>(
    node: &'a StyledNode<'a>,
    text_out: &mut String,
    spans_out: &mut Vec<(usize, usize, &'a HashMap<String, String>)>,
) -> usize {
    let mut element_child_idx = 0;
    let mut resume_idx: Option<usize> = None;

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
                    flatten_inline_content_subtree(child_styled, text_out, spans_out);
                } else if resume_idx.is_none() {
                    resume_idx = Some(element_child_idx - 1);
                }
                // A block child found AFTER the first one: nothing extra to
                // do here — node.children[resume_idx..] in the caller's
                // block-stacking loop already covers every block child from
                // the first one onward, inclusive.
            }
        }
    }

    resume_idx.unwrap_or(element_child_idx)
}

/// Always-recursing variant used once we're already inside an inline
/// ancestor's own subtree. Unlike `flatten_inline_content`, this NEVER
/// checks `is_inline_display` and NEVER stops early: once inside an inline
/// context there is no block-stacking loop left to hand a block-level
/// descendant off to, so any further descendant — block or not — just has
/// its own text folded in here instead of being silently dropped. This is
/// the fix for a real bug: a block element nested inside an inline element
/// (e.g. `<a><div>card</div></a>`, a common real-world pattern) used to
/// vanish entirely, along with everything after it inside that inline
/// element, because the caller ignored this function's old early-return
/// value.
fn flatten_inline_content_subtree<'a>(
    node: &'a StyledNode<'a>,
    text_out: &mut String,
    spans_out: &mut Vec<(usize, usize, &'a HashMap<String, String>)>,
) {
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
                flatten_inline_content_subtree(child_styled, text_out, spans_out);
            }
        }
    }
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

/// Recursively lays out `node` and its children within `containing_block`.
/// Width is resolved top-down (from the containing block into this node),
/// height is resolved bottom-up (from this node's laid-out children), and
/// children are stacked vertically (block layout). Returns `None` when
/// `node` is `display: none`, which excludes the whole subtree — the caller
/// simply never recurses into a `None` result, so a hidden node's children
/// never get laid out regardless of their own `display` value.
fn layout_block<'a>(
    node: &'a StyledNode<'a>,
    containing_block: ContainingBlock,
    is_root: bool,
    images: &ferris_scene::ImageMap,
) -> Option<LayoutBox<'a>> {
    if node.style.get("display").map(String::as_str) == Some("none") {
        return None;
    }

    let margin = resolve_edges(&node.style, "margin");
    let border = resolve_edges(&node.style, "border");
    let padding = resolve_edges(&node.style, "padding");

    let edges_width = margin.left + margin.right + border.left + border.right + padding.left + padding.right;
    let available_width = (containing_block.content_width - edges_width).max(0.0);

    let image = if node.element.tag_name == "img" {
        node.element.attributes.get("src").and_then(|src| images.get(src)).cloned()
    } else {
        None
    };

    let width_len = parse_length(node.style.get("width").map(String::as_str));
    let width_default = if node.element.tag_name == "img" {
        image.as_ref().map(|img| img.width as f32).unwrap_or(0.0)
    } else {
        available_width
    };
    let width = resolved_length_or(width_len, containing_block.content_width, width_default).max(0.0);

    let content_x = containing_block.content_x + margin.left + border.left + padding.left;
    let content_y = containing_block.content_y + margin.top + border.top + padding.top;

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

    let mut children = Vec::new();
    let mut cursor_y = content_y + text_block_height;
    for child in &node.children[resume_idx..] {
        let child_containing_block = ContainingBlock {
            content_x,
            content_y: cursor_y,
            content_width: width,
            viewport_height: containing_block.viewport_height,
        };
        if let Some(child_box) = layout_block(child, child_containing_block, false, images) {
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
    let auto_height = if node.element.tag_name == "img" {
        image.as_ref().map(|img| img.height as f32).unwrap_or(0.0)
    } else {
        (cursor_y - content_y).max(0.0)
    };
    let height = match height_len {
        Length::Percent(p) if is_root => (containing_block.viewport_height * (p / 100.0)).max(0.0),
        Length::Percent(_) => auto_height,
        other => resolved_length_or(other, 0.0, auto_height).max(0.0),
    };

    Some(LayoutBox { styled_node: node, x: content_x, y: content_y, width, height, margin, border, padding, lines, children, image })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
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
        assert_eq!(
            edges,
            Edges {
                top: 1.0,
                right: 2.0,
                bottom: 3.0,
                left: 4.0
            }
        );
    }

    #[test]
    fn resolve_edges_defaults_absent_sides_to_zero() {
        let style = style_with(&[("margin-top", "5px")]);
        let edges = resolve_edges(&style, "margin");
        assert_eq!(
            edges,
            Edges {
                top: 5.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0
            }
        );
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
        assert_eq!(
            resolved_length_or(Length::Percent(-10.0), 200.0, 0.0),
            -20.0
        );
    }

    use ferris_dom::dom::Element;

    fn leaf_styled_node<'a>(element: &'a Element, style: &[(&str, &str)]) -> StyledNode<'a> {
        StyledNode { element, style: style_with(style), children: Vec::new() }
    }

    #[test]
    fn explicit_px_width_and_height_are_used_directly() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("width", "100px"), ("height", "50px")]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert_eq!(root.width, 100.0);
        assert_eq!(root.height, 50.0);
    }

    #[test]
    fn auto_width_fills_available_containing_block_space() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0).unwrap();
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
                ("border-left-width", "2px"),
                ("border-right-width", "2px"),
                ("padding-left", "5px"),
                ("padding-right", "5px"),
            ],
        );
        let root = layout(&node, 800.0, 600.0).unwrap();
        // 800 - 10 - 10 - 2 - 2 - 5 - 5 = 766
        assert_eq!(root.width, 766.0);
    }

    #[test]
    fn oversized_margins_clamp_content_width_to_zero_not_negative() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("margin-left", "500px"), ("margin-right", "500px")]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert_eq!(root.width, 0.0);
    }

    #[test]
    fn auto_height_with_no_children_is_zero() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert_eq!(root.height, 0.0);
    }

    #[test]
    fn explicit_negative_height_clamps_to_zero() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("height", "-20px")]);
        let root = layout(&node, 800.0, 600.0).unwrap();
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
        let root = layout(&parent, 800.0, 600.0).unwrap();
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
        let root = layout(&parent, 800.0, 600.0).unwrap();
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
        let root = layout(&parent, 800.0, 600.0).unwrap();
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
        let root = layout(&root_node, 800.0, 600.0).unwrap();
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
        let root = layout(&root_node, 800.0, 600.0).unwrap();
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].y, 0.0); // a
        assert_eq!(root.children[1].y, 10.0); // c, immediately after a — b contributed 0
        assert_eq!(root.height, 30.0);
    }

    #[test]
    fn height_percent_resolves_against_viewport_at_root() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[("height", "50%")]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert_eq!(root.height, 300.0);
    }

    #[test]
    fn height_percent_below_root_resolves_to_auto_not_a_percent_of_viewport() {
        let child_el = Element::new("p");
        let child = leaf_styled_node(&child_el, &[("height", "50%")]);
        let parent_el = Element::new("div");
        let parent = StyledNode { element: &parent_el, style: HashMap::new(), children: vec![child] };
        let root = layout(&parent, 800.0, 600.0).unwrap();
        // child's height:50% is not root, so it resolves as Auto -> 0 (no children of its own)
        assert_eq!(root.children[0].height, 0.0);
        assert_eq!(root.height, 0.0);
    }

    // --- I1: layout() must not panic when the root is display:none ---
    #[test]
    fn layout_returns_none_when_root_is_display_none() {
        let el = Element::new("html");
        let node = leaf_styled_node(&el, &[("display", "none")]);
        assert!(layout(&node, 800.0, 600.0).is_none());
    }

    // --- I3: border width must come from the border-{side}-width longhand,
    // not the border-{side} shorthand ---
    #[test]
    fn resolve_edges_border_reads_the_width_longhand() {
        let style = style_with(&[("border-top-width", "7px")]);
        let edges = resolve_edges(&style, "border");
        assert_eq!(edges.top, 7.0);
    }

    #[test]
    fn resolve_edges_border_shorthand_alone_does_not_resolve_width() {
        // `border-top: 4px` is the non-standard/shorthand-only form used
        // before this fix; it must NOT resolve to a border width anymore
        // (the fix intentionally changes this behavior).
        let style = style_with(&[("border-top", "4px")]);
        let edges = resolve_edges(&style, "border");
        assert_eq!(edges.top, 0.0);
    }

    #[test]
    fn single_short_line_of_text_fits_on_one_line() {
        let mut el = Element::new("p");
        el.children.push(Node::Text("hi".to_string()));
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert_eq!(root.lines.len(), 1);
        assert_eq!(root.lines[0].len(), 1);
        assert_eq!(root.lines[0][0].text, "hi");
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
        assert!(root.lines.len() > 1, "expected multiple wrapped lines, got {:?}", root.lines.iter().map(|l| l.iter().map(|r| r.text.clone()).collect::<Vec<_>>()).collect::<Vec<_>>());
        let expected_height = root.lines.len() as f32 * ferris_text::line_height(16.0);
        assert_eq!(root.height, expected_height);
    }

    #[test]
    fn element_without_direct_text_has_empty_lines() {
        let el = Element::new("div");
        let node = leaf_styled_node(&el, &[]);
        let root = layout(&node, 800.0, 600.0).unwrap();
        assert!(root.lines.is_empty());
    }

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

    #[test]
    fn block_inside_inline_element_is_not_silently_dropped() {
        // Regression for C1: <a><div>Card title</div><p>Card body</p></a>
        let mut title_el = Element::new("div");
        title_el.children.push(Node::Text("Card title".to_string()));
        let title_styled = StyledNode { element: &title_el, style: HashMap::new(), children: Vec::new() };

        let mut body_el = Element::new("p");
        body_el.children.push(Node::Text("Card body".to_string()));
        let body_styled = StyledNode { element: &body_el, style: HashMap::new(), children: Vec::new() };

        let mut a_el = Element::new("a");
        a_el.children.push(Node::Element(title_el.clone()));
        a_el.children.push(Node::Element(body_el.clone()));
        let a_styled = StyledNode { element: &a_el, style: HashMap::new(), children: vec![title_styled, body_styled] };

        let mut outer_el = Element::new("div");
        outer_el.children.push(Node::Element(a_el.clone()));
        let outer_styled = StyledNode { element: &outer_el, style: HashMap::new(), children: vec![a_styled] };

        let root = layout(&outer_styled, 800.0, 600.0).unwrap();
        assert!(!root.lines.is_empty(), "content inside the block-in-inline subtree must not vanish");
        let joined: String = root.lines.iter().flat_map(|line| line.iter().map(|r| r.text.as_str())).collect();
        assert!(joined.contains("Card title"), "expected 'Card title' to survive, got {joined:?}");
        assert!(joined.contains("Card body"), "expected 'Card body' to survive, got {joined:?}");
    }

    #[test]
    fn text_after_a_block_sibling_is_not_silently_dropped() {
        // Regression for C2: <p>line one<br>line two</p>
        let br_el = Element::new("br");
        let br_styled = StyledNode { element: &br_el, style: HashMap::new(), children: Vec::new() };

        let mut p_el = Element::new("p");
        p_el.children.push(Node::Text("line one".to_string()));
        p_el.children.push(Node::Element(br_el.clone()));
        p_el.children.push(Node::Text("line two".to_string()));
        let p_styled = StyledNode { element: &p_el, style: HashMap::new(), children: vec![br_styled] };

        let root = layout(&p_styled, 800.0, 600.0).unwrap();
        let joined: String = root.lines.iter().flat_map(|line| line.iter().map(|r| r.text.as_str())).collect();
        assert!(joined.contains("line one"), "expected 'line one' to survive, got {joined:?}");
        assert!(joined.contains("line two"), "expected 'line two' to survive (previously silently dropped), got {joined:?}");
    }

    #[test]
    fn trailing_text_after_a_block_child_is_not_silently_dropped() {
        // Regression for C2: <div><h1>Title</h1>Some trailing text</div>
        let mut h1_el = Element::new("h1");
        h1_el.children.push(Node::Text("Title".to_string()));
        let h1_styled = StyledNode { element: &h1_el, style: HashMap::new(), children: Vec::new() };

        let mut div_el = Element::new("div");
        div_el.children.push(Node::Element(h1_el.clone()));
        div_el.children.push(Node::Text("Some trailing text".to_string()));
        let div_styled = StyledNode { element: &div_el, style: HashMap::new(), children: vec![h1_styled] };

        let root = layout(&div_styled, 800.0, 600.0).unwrap();
        let joined: String = root.lines.iter().flat_map(|line| line.iter().map(|r| r.text.as_str())).collect();
        assert!(joined.contains("Some trailing text"), "expected the trailing text to survive (previously silently dropped), got {joined:?}");
        assert_eq!(root.children.len(), 1, "the h1 must still be stacked as its own block child");
        assert_eq!(root.children[0].lines[0][0].text, "Title");
    }

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
}
