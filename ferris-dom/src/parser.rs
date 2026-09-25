use crate::dom::{Element, Node};
use crate::tokenizer::{Token, Tokenizer};

pub struct Parser;

impl Parser {
    pub fn parse(tokens: &[Token]) -> Node {
        let mut stack: Vec<Element> = vec![Element::new("document")];

        for token in tokens {
            match token {
                Token::TagOpen { name, attributes, self_closing } => {
                    let mut el = Element::new(name.clone());
                    el.attributes = attributes.clone();
                    if *self_closing {
                        stack.last_mut().unwrap().children.push(Node::Element(el));
                    } else {
                        stack.push(el);
                    }
                }
                Token::TagClose { name } => {
                    if stack.len() > 1 && stack.last().unwrap().tag_name == *name {
                        let finished = stack.pop().unwrap();
                        stack.last_mut().unwrap().children.push(Node::Element(finished));
                    }
                    // else: mismatched closing tag, ignored (spec error-recovery rule 2)
                }
                Token::Text(text) => {
                    stack.last_mut().unwrap().children.push(Node::Text(text.clone()));
                }
                Token::Comment(text) => {
                    stack.last_mut().unwrap().children.push(Node::Comment(text.clone()));
                }
            }
        }

        // spec error-recovery rule 1: auto-close any remaining open elements
        while stack.len() > 1 {
            let finished = stack.pop().unwrap();
            stack.last_mut().unwrap().children.push(Node::Element(finished));
        }

        Node::Element(stack.pop().unwrap())
    }
}

/// Tokenizes and parses `html`, then unwraps the synthetic "document" root
/// `Parser::parse` always wraps everything in. Collects every top-level
/// element (in order): with none, returns an empty `<html>` element (empty
/// input, or input with only free text and no tags); with exactly one,
/// returns it directly, unwrapped; with two or more (e.g. a `<head>...</head>
/// <body>...</body>` document with no enclosing `<html>`, which is valid
/// HTML5), synthesizes an `<html>` element whose children are all of them,
/// in order, so no top-level content is silently dropped. Never panics.
pub fn parse_document(html: &str) -> Element {
    let tokens = Tokenizer::tokenize(html);
    let Node::Element(document) = Parser::parse(&tokens) else {
        return Element::new("html");
    };
    let mut top_level_elements: Vec<Element> = document
        .children
        .into_iter()
        .filter_map(|child| match child {
            Node::Element(el) => Some(el),
            Node::Text(_) | Node::Comment(_) => None,
        })
        .collect();

    match top_level_elements.len() {
        0 => Element::new("html"),
        1 => top_level_elements.pop().unwrap(),
        _ => {
            let mut wrapper = Element::new("html");
            wrapper.children = top_level_elements.into_iter().map(Node::Element).collect();
            wrapper
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parses_single_element_with_text() {
        let tokens = vec![
            Token::TagOpen { name: "p".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::Text("hi".to_string()),
            Token::TagClose { name: "p".to_string() },
        ];
        let Node::Element(doc) = Parser::parse(&tokens) else { panic!("expected document element") };
        assert_eq!(doc.tag_name, "document");
        assert_eq!(doc.children.len(), 1);
        let Node::Element(p) = &doc.children[0] else { panic!("expected p element") };
        assert_eq!(p.tag_name, "p");
        assert_eq!(p.children, vec![Node::Text("hi".to_string())]);
    }

    #[test]
    fn parses_nested_elements_preserving_attributes() {
        let mut attrs = HashMap::new();
        attrs.insert("class".to_string(), "a".to_string());
        let tokens = vec![
            Token::TagOpen { name: "div".to_string(), attributes: attrs.clone(), self_closing: false },
            Token::TagOpen { name: "span".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::TagClose { name: "span".to_string() },
            Token::TagClose { name: "div".to_string() },
        ];
        let Node::Element(doc) = Parser::parse(&tokens) else { panic!("expected document element") };
        let Node::Element(div) = &doc.children[0] else { panic!("expected div element") };
        assert_eq!(div.attributes, attrs);
        assert_eq!(div.children.len(), 1);
    }

    #[test]
    fn auto_closes_unclosed_tag_at_end_of_input() {
        let tokens = vec![
            Token::TagOpen { name: "div".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::Text("hi".to_string()),
        ];
        let Node::Element(doc) = Parser::parse(&tokens) else { panic!("expected document element") };
        assert_eq!(doc.children.len(), 1);
        let Node::Element(div) = &doc.children[0] else { panic!("expected div element") };
        assert_eq!(div.tag_name, "div");
        assert_eq!(div.children, vec![Node::Text("hi".to_string())]);
    }

    #[test]
    fn ignores_mismatched_closing_tag() {
        let tokens = vec![
            Token::TagOpen { name: "div".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::TagClose { name: "span".to_string() },
            Token::Text("hi".to_string()),
            Token::TagClose { name: "div".to_string() },
        ];
        let Node::Element(doc) = Parser::parse(&tokens) else { panic!("expected document element") };
        assert_eq!(doc.children.len(), 1);
        let Node::Element(div) = &doc.children[0] else { panic!("expected div element") };
        assert_eq!(div.children, vec![Node::Text("hi".to_string())]);
    }

    #[test]
    fn self_closing_element_does_not_capture_following_siblings() {
        let tokens = vec![
            Token::TagOpen { name: "br".to_string(), attributes: HashMap::new(), self_closing: true },
            Token::TagOpen { name: "p".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::TagClose { name: "p".to_string() },
        ];
        let Node::Element(doc) = Parser::parse(&tokens) else { panic!("expected document element") };
        assert_eq!(doc.children.len(), 2);
    }

    // --- Review Focus: empty input ---
    #[test]
    fn empty_token_list_yields_empty_document_element() {
        let tokens: Vec<Token> = vec![];
        let Node::Element(doc) = Parser::parse(&tokens) else { panic!("expected document element") };
        assert_eq!(doc.tag_name, "document");
        assert!(doc.children.is_empty());
    }

    #[test]
    fn parse_document_returns_the_first_real_element() {
        let root = parse_document("<div>hello</div>");
        assert_eq!(root.tag_name, "div");
    }

    #[test]
    fn parse_document_on_empty_input_returns_empty_html_element() {
        let root = parse_document("");
        assert_eq!(root.tag_name, "html");
        assert!(root.children.is_empty());
    }

    #[test]
    fn parse_document_on_text_only_input_does_not_panic() {
        let root = parse_document("just some free text, no tags at all");
        assert_eq!(root.tag_name, "html");
        assert!(root.children.is_empty());
    }

    #[test]
    fn parse_document_wraps_multiple_top_level_elements_in_synthetic_html() {
        let root = parse_document("<head><title>t</title></head><body><p>hi</p></body>");
        assert_eq!(root.tag_name, "html");
        assert_eq!(root.children.len(), 2);
        let Node::Element(head) = &root.children[0] else { panic!("expected head element") };
        assert_eq!(head.tag_name, "head");
        let Node::Element(body) = &root.children[1] else { panic!("expected body element") };
        assert_eq!(body.tag_name, "body");
    }
}
