use crate::dom::{Element, Node};
use crate::tokenizer::Token;

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
}
