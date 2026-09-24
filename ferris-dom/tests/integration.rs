use ferris_dom::dom::Node;
use ferris_dom::parser::Parser;
use ferris_dom::tokenizer::Tokenizer;

fn parse_html(html: &str) -> Node {
    let tokens = Tokenizer::tokenize(html);
    Parser::parse(&tokens)
}

#[test]
fn parses_realistic_nested_html_with_attributes_text_and_comment() {
    let html = r#"<div class="a"><p>Hello <b>world</b></p><!-- note --></div>"#;
    let Node::Element(doc) = parse_html(html) else { panic!("expected document element") };
    assert_eq!(doc.children.len(), 1);

    let Node::Element(div) = &doc.children[0] else { panic!("expected div element") };
    assert_eq!(div.tag_name, "div");
    assert_eq!(div.attributes.get("class").map(String::as_str), Some("a"));
    assert_eq!(div.children.len(), 2);

    let Node::Element(p) = &div.children[0] else { panic!("expected p element") };
    assert_eq!(p.tag_name, "p");
    assert_eq!(p.children.len(), 2);
    assert_eq!(p.children[0], Node::Text("Hello ".to_string()));

    let Node::Element(b) = &p.children[1] else { panic!("expected b element") };
    assert_eq!(b.tag_name, "b");
    assert_eq!(b.children, vec![Node::Text("world".to_string())]);

    assert_eq!(div.children[1], Node::Comment(" note ".to_string()));
}

#[test]
fn parses_html_with_void_elements_and_unclosed_tags() {
    let html = r#"<div><img src="x.png"><p>text"#;
    let Node::Element(doc) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(div) = &doc.children[0] else { panic!("expected div element") };
    assert_eq!(div.children.len(), 2);

    let Node::Element(img) = &div.children[0] else { panic!("expected img element") };
    assert_eq!(img.tag_name, "img");
    assert_eq!(img.attributes.get("src").map(String::as_str), Some("x.png"));

    let Node::Element(p) = &div.children[1] else { panic!("expected p element (auto-closed)") };
    assert_eq!(p.children, vec![Node::Text("text".to_string())]);
}

// --- Review Focus: mixed-case tag names must match case-insensitively so
// closing tags correctly pop their matching opening tag off the stack ---
#[test]
fn mixed_case_open_and_close_tags_produce_sibling_elements_not_misnesting() {
    let html = "<DIV>a</div><span>b</span>";
    let Node::Element(doc) = parse_html(html) else { panic!("expected document element") };
    assert_eq!(doc.children.len(), 2, "div and span must be siblings, not nested");

    let Node::Element(div) = &doc.children[0] else { panic!("expected div element") };
    assert_eq!(div.tag_name, "div", "tag name must be lowercased regardless of source casing");
    assert_eq!(div.children, vec![Node::Text("a".to_string())]);

    let Node::Element(span) = &doc.children[1] else { panic!("expected span element") };
    assert_eq!(span.tag_name, "span", "tag name must be lowercased regardless of source casing");
    assert_eq!(span.children, vec![Node::Text("b".to_string())]);
}

// --- Review Focus: a literal '<' in text (e.g. `1 < 2`) must not be treated
// as the start of a tag, which would otherwise corrupt the rest of the parse ---
#[test]
fn stray_less_than_in_text_does_not_corrupt_following_markup() {
    let html = "<p>1 < 2</p><p>x</p>";
    let Node::Element(doc) = parse_html(html) else { panic!("expected document element") };
    assert_eq!(doc.children.len(), 2, "both p elements must parse correctly");

    let Node::Element(first) = &doc.children[0] else { panic!("expected first p element") };
    assert_eq!(first.tag_name, "p");
    assert_eq!(first.children, vec![Node::Text("1 < 2".to_string())]);

    let Node::Element(second) = &doc.children[1] else { panic!("expected second p element") };
    assert_eq!(second.tag_name, "p");
    assert_eq!(second.children, vec![Node::Text("x".to_string())]);
}
