// End-to-end integration tests: real HTML/CSS text goes in through the actual
// ferris-dom and ferris-css tokenizers/parsers, through ferris-style's cascade,
// through ferris-layout's box tree, and finally through ferris_paint::paint::paint.
// No hand-built LayoutBox fixtures here (those already live in
// ferris-paint/src/paint.rs's unit tests) — this file exists purely to prove
// the whole pipeline wires together.
//
// Note on parser entry points: `ferris_dom::parser::Parser::parse` and
// `ferris_css::parser::Parser::parse` take `&[Token]`, not `&str` — each
// crate tokenizes first, then parses. This matches the established pattern
// in `ferris-style/tests/integration.rs` and `ferris-layout/tests/integration.rs`.

use ferris_scene::DrawCommand;
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
