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
