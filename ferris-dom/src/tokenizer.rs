use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    TagOpen { name: String, attributes: HashMap<String, String>, self_closing: bool },
    TagClose { name: String },
    Text(String),
    Comment(String),
}

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "source", "track", "wbr",
];

fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name.to_ascii_lowercase().as_str())
}

fn find_tag_end(chars: &[char], start: usize) -> usize {
    let mut i = start;
    let mut in_quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        match in_quote {
            Some(q) => {
                if c == q {
                    in_quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_quote = Some(c);
                } else if c == '>' {
                    return i;
                }
            }
        }
        i += 1;
    }
    chars.len()
}

fn find_sequence(chars: &[char], start: usize, seq: &[char]) -> Option<usize> {
    (start..=chars.len().saturating_sub(seq.len())).find(|&i| chars[i..].starts_with(seq))
}

fn parse_tag_content(content: &str) -> (String, HashMap<String, String>) {
    let mut chars = content.chars().peekable();
    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            break;
        }
        name.push(c);
        chars.next();
    }

    let mut attributes = HashMap::new();
    loop {
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
        if chars.peek().is_none() {
            break;
        }

        let mut attr_name = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '=' {
                break;
            }
            attr_name.push(c);
            chars.next();
        }
        if attr_name.is_empty() {
            break;
        }

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        if chars.peek() == Some(&'=') {
            chars.next();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    chars.next();
                } else {
                    break;
                }
            }
            let mut value = String::new();
            if chars.peek() == Some(&'"') || chars.peek() == Some(&'\'') {
                let quote = *chars.peek().unwrap();
                chars.next();
                while let Some(&c) = chars.peek() {
                    if c == quote {
                        chars.next();
                        break;
                    }
                    value.push(c);
                    chars.next();
                }
            } else {
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    value.push(c);
                    chars.next();
                }
            }
            attributes.insert(attr_name, value);
        } else {
            attributes.insert(attr_name, String::new());
        }
    }

    (name, attributes)
}

pub struct Tokenizer;

impl Tokenizer {
    pub fn tokenize(input: &str) -> Vec<Token> {
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;
        let mut tokens = Vec::new();
        let mut text_buf = String::new();

        while i < chars.len() {
            if chars[i] == '<' {
                if !text_buf.is_empty() {
                    tokens.push(Token::Text(text_buf.clone()));
                    text_buf.clear();
                }

                if chars[i..].starts_with(&['<', '!', '-', '-']) {
                    let start = i + 4;
                    if let Some(end) = find_sequence(&chars, start, &['-', '-', '>']) {
                        let comment_text: String = chars[start..end].iter().collect();
                        tokens.push(Token::Comment(comment_text));
                        i = end + 3;
                    } else {
                        let comment_text: String = chars[start..].iter().collect();
                        tokens.push(Token::Comment(comment_text));
                        i = chars.len();
                    }
                    continue;
                }

                if i + 1 < chars.len() && chars[i + 1] == '/' {
                    let start = i + 2;
                    let end = find_tag_end(&chars, start);
                    let name: String = chars[start..end].iter().collect::<String>().trim().to_string();
                    tokens.push(Token::TagClose { name });
                    i = if end < chars.len() { end + 1 } else { end };
                    continue;
                }

                let end = find_tag_end(&chars, i + 1);
                let mut tag_content: String = chars[i + 1..end].iter().collect();
                let explicit_slash = tag_content.trim_end().ends_with('/');
                if explicit_slash {
                    tag_content = tag_content.trim_end().trim_end_matches('/').to_string();
                }
                let (name, attributes) = parse_tag_content(&tag_content);
                let self_closing = explicit_slash || is_void_element(&name);
                tokens.push(Token::TagOpen { name, attributes, self_closing });
                i = if end < chars.len() { end + 1 } else { end };
                continue;
            }

            text_buf.push(chars[i]);
            i += 1;
        }

        if !text_buf.is_empty() {
            tokens.push(Token::Text(text_buf));
        }

        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_simple_open_and_close_tag() {
        let tokens = Tokenizer::tokenize("<div></div>");
        assert_eq!(tokens, vec![
            Token::TagOpen { name: "div".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::TagClose { name: "div".to_string() },
        ]);
    }

    #[test]
    fn tokenizes_tag_with_multiple_attributes() {
        let tokens = Tokenizer::tokenize(r#"<div class="a" id='b'></div>"#);
        let mut expected_attrs = HashMap::new();
        expected_attrs.insert("class".to_string(), "a".to_string());
        expected_attrs.insert("id".to_string(), "b".to_string());
        assert_eq!(tokens[0], Token::TagOpen { name: "div".to_string(), attributes: expected_attrs, self_closing: false });
    }

    #[test]
    fn tokenizes_text_between_tags() {
        let tokens = Tokenizer::tokenize("<p>Hello world</p>");
        assert_eq!(tokens[1], Token::Text("Hello world".to_string()));
    }

    #[test]
    fn tokenizes_comment() {
        let tokens = Tokenizer::tokenize("<!-- a note -->");
        assert_eq!(tokens, vec![Token::Comment(" a note ".to_string())]);
    }

    #[test]
    fn tokenizes_nested_tags_in_sequence() {
        let tokens = Tokenizer::tokenize("<div><p>hi</p></div>");
        assert_eq!(tokens, vec![
            Token::TagOpen { name: "div".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::TagOpen { name: "p".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::Text("hi".to_string()),
            Token::TagClose { name: "p".to_string() },
            Token::TagClose { name: "div".to_string() },
        ]);
    }

    // --- Review Focus: attribute value containing a literal '>' character ---
    #[test]
    fn attribute_value_containing_greater_than_does_not_truncate_the_tag() {
        let tokens = Tokenizer::tokenize(r#"<div title="a > b">text</div>"#);
        match &tokens[0] {
            Token::TagOpen { name, attributes, .. } => {
                assert_eq!(name, "div");
                assert_eq!(attributes.get("title"), Some(&"a > b".to_string()));
            }
            other => panic!("expected TagOpen, got {other:?}"),
        }
        assert_eq!(tokens[1], Token::Text("text".to_string()));
        assert_eq!(tokens[2], Token::TagClose { name: "div".to_string() });
    }

    // --- Review Focus: self-closing tag with attributes AND explicit trailing '/' ---
    #[test]
    fn self_closing_tag_with_attributes_and_explicit_slash() {
        let tokens = Tokenizer::tokenize(r#"<input type="text" />"#);
        match &tokens[0] {
            Token::TagOpen { name, attributes, self_closing } => {
                assert_eq!(name, "input");
                assert_eq!(attributes.get("type"), Some(&"text".to_string()));
                assert!(self_closing);
            }
            other => panic!("expected TagOpen, got {other:?}"),
        }
    }

    // --- Review Focus: case-insensitive void-element matching ---
    #[test]
    fn void_element_self_closes_without_explicit_slash_case_insensitively() {
        let lower = Tokenizer::tokenize("<br>");
        assert_eq!(lower, vec![Token::TagOpen { name: "br".to_string(), attributes: HashMap::new(), self_closing: true }]);

        let upper = Tokenizer::tokenize("<BR>");
        match &upper[0] {
            Token::TagOpen { name, self_closing, .. } => {
                assert_eq!(name, "BR", "tag name is preserved as-written, not lowercased");
                assert!(self_closing, "void-element check must match case-insensitively");
            }
            other => panic!("expected TagOpen, got {other:?}"),
        }
    }

    // --- Review Focus: text immediately adjacent to a tag with no whitespace ---
    #[test]
    fn text_immediately_adjacent_to_tag_is_not_dropped_or_duplicated() {
        let tokens = Tokenizer::tokenize("Hello<b>world</b>");
        assert_eq!(tokens, vec![
            Token::Text("Hello".to_string()),
            Token::TagOpen { name: "b".to_string(), attributes: HashMap::new(), self_closing: false },
            Token::Text("world".to_string()),
            Token::TagClose { name: "b".to_string() },
        ]);
    }
}
