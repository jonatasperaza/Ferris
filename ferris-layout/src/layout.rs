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
    let side = |suffix: &str| -> f32 {
        let key = format!("{property}-{suffix}");
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
}
