// End-to-end integration tests: real HTML/CSS text goes in through the actual
// ferris-dom and ferris-css tokenizers/parsers, through ferris-style's cascade,
// and finally through ferris_layout::layout::layout. No hand-built StyledNode
// fixtures here (those already live in ferris-layout/src/layout.rs's unit tests)
// — this file exists purely to prove the whole pipeline wires together.
//
// Note on parser entry points: `ferris_dom::parser::Parser::parse` and
// `ferris_css::parser::Parser::parse` take `&[Token]`, not `&str` — each crate
// tokenizes first, then parses. This matches the established pattern in
// `ferris-style/tests/integration.rs`, which this file mirrors.

use ferris_css::parser::Parser as CssParser;
use ferris_css::stylesheet::Stylesheet;
use ferris_css::tokenizer::Tokenizer as CssTokenizer;
use ferris_dom::dom::{Element, Node};
use ferris_dom::parser::Parser as DomParser;
use ferris_dom::tokenizer::Tokenizer as DomTokenizer;
use ferris_layout::layout::layout;
use ferris_style::resolve_styles;

fn parse_css(css: &str) -> Stylesheet {
    let tokens = CssTokenizer::tokenize(css);
    CssParser::parse(&tokens)
}

/// Parses `html` and returns the first actual element in the document
/// (skipping the parser's synthetic "document" wrapper element, and skipping
/// any leading/trailing whitespace text nodes produced by indented test
/// fixtures — `ferris_dom`'s tokenizer does not trim whitespace-only text).
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

    let page = parse_root_element(html);
    let stylesheet = parse_css(css);
    let styled = resolve_styles(&page, &stylesheet);
    let root = layout(&styled, 1024.0, 768.0).unwrap();

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
    // A LayoutBox's x/y is its OWN content-box origin — it already has this
    // node's own margin/border/padding-top folded in (see layout_block:
    // `content_y = containing_block.content_y + margin.top + border.top + padding.top`).
    // main's containing-block cursor arrives at 70 (header's total box height:
    // 60 + margin-bottom 10), and main itself adds its own padding-top (5px)
    // on top of that before content_y is captured as `main.y`.
    assert_eq!(main.y, 75.0, "70 (after header's total box height) + main's own padding-top (5)");

    let para1 = &main.children[0];
    let para2 = &main.children[1];
    // para1 is main's first child, so it starts exactly at main's content-box
    // origin — main.y already includes main's padding-top, so no further
    // offset is added here.
    assert_eq!(para1.y, main.y);
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

    let page = parse_root_element(html);
    let stylesheet = parse_css(css);
    let styled = resolve_styles(&page, &stylesheet);
    let root = layout(&styled, 1024.0, 768.0).unwrap();

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

    let page = parse_root_element(html);
    let stylesheet = parse_css(css);
    let styled = resolve_styles(&page, &stylesheet);
    let root = layout(&styled, 1024.0, 800.0).unwrap();

    assert_eq!(root.height, 400.0, "root height:50% resolves against the 800px viewport");
    // #child's height:50% is not the root, so it resolves to Auto (sum of its
    // own children — #grandchild has no explicit height, so 0) rather than 50%
    // of #page's computed 400px height.
    assert_eq!(root.children[0].height, 0.0);
}
