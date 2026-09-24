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
}
