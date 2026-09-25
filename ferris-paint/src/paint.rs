use ferris_scene::{DrawCommand, Frame, RectCommand, TextCommand};
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

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
        // width: content width + (padding.left + border.left) + (padding.right + border.right)
        assert_eq!(rect.width, 200.0 + (5.0 + 5.0) + (3.0 + 3.0));
        // height: content height + (padding.top + border.top) + (padding.bottom + border.bottom)
        assert_eq!(rect.height, 80.0 + (2.0 + 2.0) + (4.0 + 4.0));
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
