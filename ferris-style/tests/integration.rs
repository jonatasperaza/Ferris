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

// --- Review Focus (I-1 fix): a sibling combinator to the LEFT of an ancestor
// combinator (e.g. `.a + .b .f`) must resolve the sibling relationship against
// the ANCESTOR that the trailing simple selector belongs to (`.b`), not against
// the target element itself (`.f`). Exercised through the real HTML+CSS parsing
// pipeline, per the concrete repro from the bug report:
//   <div id="wrap">
//     <section class="a"><p class="x"></p></section>
//     <section class="b"><div class="inner"><p class="f"></p></div></section>
//   </div>
// `.a + .b .f` should match `p.f`: section.b is section.a's next sibling, and
// p.f is a descendant of section.b.
#[test]
fn sibling_combinator_before_ancestor_combinator_matches_via_real_parser() {
    let html = r#"<div id="wrap"><section class="a"><p class="x"></p></section><section class="b"><div class="inner"><p class="f"></p></div></section></div>"#;
    let Node::Element(root) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(wrap) = &root.children[0] else { panic!("expected div#wrap") };

    let css = ".a + .b .f { color: red; }";
    let stylesheet = parse_css(css);

    let styled_wrap = resolve_styles(wrap, &stylesheet);
    let styled_section_b = &styled_wrap.children[1];
    assert_eq!(styled_section_b.element.attributes.get("class"), Some(&"b".to_string()));
    let styled_inner = &styled_section_b.children[0];
    let styled_p_f = &styled_inner.children[0];
    assert_eq!(styled_p_f.element.attributes.get("class"), Some(&"f".to_string()));
    assert_eq!(
        styled_p_f.style.get("color"),
        Some(&"red".to_string()),
        ".a + .b .f should match: section.b is the next sibling of section.a, and p.f is a descendant of section.b"
    );
}

// --- Companion case: `~` (SubsequentSibling) to the left of `>` (Child) ---
// `.a ~ .b > .c` should match `span.c` when span.c's parent (div.b) has an
// earlier (not necessarily adjacent) sibling matching `.a`.
#[test]
fn subsequent_sibling_combinator_before_child_combinator_matches_via_real_parser() {
    let html = r#"<div id="wrap"><p class="a"></p><p class="other"></p><div class="b"><span class="c"></span></div></div>"#;
    let Node::Element(root) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(wrap) = &root.children[0] else { panic!("expected div#wrap") };

    let css = ".a ~ .b > .c { color: blue; }";
    let stylesheet = parse_css(css);

    let styled_wrap = resolve_styles(wrap, &stylesheet);
    let styled_div_b = &styled_wrap.children[2];
    assert_eq!(styled_div_b.element.attributes.get("class"), Some(&"b".to_string()));
    let styled_span_c = &styled_div_b.children[0];
    assert_eq!(styled_span_c.element.attributes.get("class"), Some(&"c".to_string()));
    assert_eq!(
        styled_span_c.style.get("color"),
        Some(&"blue".to_string()),
        ".a ~ .b > .c should match: div.b has an earlier sibling matching .a, and span.c is div.b's direct child"
    );
}
