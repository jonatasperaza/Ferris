use ferris_css::parser::Parser as CssParser;
use ferris_css::tokenizer::Tokenizer as CssTokenizer;
use ferris_dom::dom::Node;
use ferris_dom::parser::Parser as DomParser;
use ferris_dom::tokenizer::Tokenizer as DomTokenizer;
use ferris_style::resolve_styles;

fn parse_html(html: &str) -> Node {
    let tokens = DomTokenizer::tokenize(html);
    DomParser::parse(&tokens)
}

fn parse_css(css: &str) -> ferris_css::stylesheet::Stylesheet {
    let tokens = CssTokenizer::tokenize(css);
    CssParser::parse(&tokens)
}

#[test]
fn resolves_composite_selector_specificity_and_important_together() {
    let html = r#"<div class="card"><p id="lead">Hello</p><span>World</span></div>"#;
    let Node::Element(root) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(div) = &root.children[0] else { panic!("expected div") };

    let css = r#"
        p { color: blue; }
        #lead { color: green; }
        .card p { color: red !important; }
    "#;
    let stylesheet = parse_css(css);

    let styled_div = resolve_styles(div, &stylesheet);
    let styled_p = &styled_div.children[0];
    assert_eq!(styled_p.element.tag_name, "p");
    assert_eq!(
        styled_p.style.get("color"),
        Some(&"red".to_string()),
        "important descendant-combinator rule should win over higher-specificity #lead"
    );
}

#[test]
fn resolves_next_sibling_combinator_against_real_parsed_html() {
    let html = r#"<div><h1>Title</h1><p>First</p></div>"#;
    let Node::Element(root) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(div) = &root.children[0] else { panic!("expected div") };

    let css = "h1 + p { margin-top: 0; }";
    let stylesheet = parse_css(css);

    let styled_div = resolve_styles(div, &stylesheet);
    let styled_p = &styled_div.children[1];
    assert_eq!(styled_p.element.tag_name, "p");
    assert_eq!(styled_p.style.get("margin-top"), Some(&"0".to_string()));
}

// --- Review Focus: class/id/attribute-VALUE matching is case-sensitive across the
// ferris-dom/ferris-css crate boundary (only tag/attribute NAMES are lowercased by
// either crate — values and class/id content are preserved as-written) ---
#[test]
fn class_value_matching_is_case_sensitive_across_the_dom_css_boundary() {
    let html = r#"<div class="Card">content</div>"#;
    let Node::Element(root) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(div) = &root.children[0] else { panic!("expected div") };

    let matching_css = parse_css(".Card { color: red; }");
    let styled = resolve_styles(div, &matching_css);
    assert_eq!(styled.style.get("color"), Some(&"red".to_string()), "exact-case class selector must match");

    let wrong_case_css = parse_css(".card { color: blue; }");
    let styled_wrong = resolve_styles(div, &wrong_case_css);
    assert!(styled_wrong.style.is_empty(), "class matching is case-sensitive — .card must not match class=\"Card\"");
}
