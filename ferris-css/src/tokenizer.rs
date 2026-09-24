#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Hash(String),
    Str(String),
    Delim(char),
    Comma,
    Colon,
    Semicolon,
    OpenBrace,
    CloseBrace,
    AtKeyword(String),
    Whitespace,
}

pub struct Tokenizer;

impl Tokenizer {
    pub fn tokenize(input: &str) -> Vec<Token> {
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;
        let mut tokens = Vec::new();

        while i < chars.len() {
            let c = chars[i];

            if c.is_whitespace() {
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                tokens.push(Token::Whitespace);
                continue;
            }

            if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = if i + 1 < chars.len() { i + 2 } else { chars.len() };
                continue;
            }

            if c == '@' {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && is_ident_char(chars[j]) {
                    j += 1;
                }
                tokens.push(Token::AtKeyword(chars[start..j].iter().collect()));
                i = j;
                continue;
            }

            if c == '#' {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && is_ident_char(chars[j]) {
                    j += 1;
                }
                tokens.push(Token::Hash(chars[start..j].iter().collect()));
                i = j;
                continue;
            }

            if c == '"' || c == '\'' {
                let quote = c;
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j] != quote {
                    j += 1;
                }
                tokens.push(Token::Str(chars[start..j].iter().collect()));
                i = if j < chars.len() { j + 1 } else { j };
                continue;
            }

            if is_ident_char(c) {
                let start = i;
                let mut j = i;
                while j < chars.len() && is_ident_char(chars[j]) {
                    j += 1;
                }
                tokens.push(Token::Ident(chars[start..j].iter().collect()));
                i = j;
                continue;
            }

            match c {
                ',' => tokens.push(Token::Comma),
                ':' => tokens.push(Token::Colon),
                ';' => tokens.push(Token::Semicolon),
                '{' => tokens.push(Token::OpenBrace),
                '}' => tokens.push(Token::CloseBrace),
                other => tokens.push(Token::Delim(other)),
            }
            i += 1;
        }

        tokens
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '%'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_simple_type_selector() {
        let tokens = Tokenizer::tokenize("div");
        assert_eq!(tokens, vec![Token::Ident("div".to_string())]);
    }

    #[test]
    fn tokenizes_class_selector() {
        let tokens = Tokenizer::tokenize(".foo");
        assert_eq!(tokens, vec![Token::Delim('.'), Token::Ident("foo".to_string())]);
    }

    #[test]
    fn tokenizes_id_selector() {
        let tokens = Tokenizer::tokenize("#bar");
        assert_eq!(tokens, vec![Token::Hash("bar".to_string())]);
    }

    #[test]
    fn tokenizes_attribute_selector_without_value() {
        let tokens = Tokenizer::tokenize("[href]");
        assert_eq!(tokens, vec![
            Token::Delim('['), Token::Ident("href".to_string()), Token::Delim(']'),
        ]);
    }

    #[test]
    fn tokenizes_attribute_selector_with_value() {
        let tokens = Tokenizer::tokenize("[type=text]");
        assert_eq!(tokens, vec![
            Token::Delim('['), Token::Ident("type".to_string()), Token::Delim('='),
            Token::Ident("text".to_string()), Token::Delim(']'),
        ]);
    }

    #[test]
    fn tokenizes_each_combinator_delim() {
        assert_eq!(Tokenizer::tokenize(">"), vec![Token::Delim('>')]);
        assert_eq!(Tokenizer::tokenize("+"), vec![Token::Delim('+')]);
        assert_eq!(Tokenizer::tokenize("~"), vec![Token::Delim('~')]);
    }

    #[test]
    fn tokenizes_whitespace_as_single_coalesced_token() {
        let tokens = Tokenizer::tokenize("div   p");
        assert_eq!(tokens, vec![
            Token::Ident("div".to_string()), Token::Whitespace, Token::Ident("p".to_string()),
        ]);
    }

    #[test]
    fn tokenizes_declaration_with_multi_word_value() {
        let tokens = Tokenizer::tokenize("font-family: Arial, sans-serif;");
        assert_eq!(tokens, vec![
            Token::Ident("font-family".to_string()), Token::Colon, Token::Whitespace,
            Token::Ident("Arial".to_string()), Token::Comma, Token::Whitespace,
            Token::Ident("sans-serif".to_string()), Token::Semicolon,
        ]);
    }

    #[test]
    fn tokenizes_quoted_string() {
        let tokens = Tokenizer::tokenize(r#""hello world""#);
        assert_eq!(tokens, vec![Token::Str("hello world".to_string())]);
    }

    #[test]
    fn comment_is_discarded() {
        let tokens = Tokenizer::tokenize("div /* comment */ p");
        assert_eq!(tokens, vec![
            Token::Ident("div".to_string()), Token::Whitespace, Token::Whitespace, Token::Ident("p".to_string()),
        ]);
    }

    #[test]
    fn tokenizes_at_media_with_condition() {
        let tokens = Tokenizer::tokenize("@media (min-width: 600px) {");
        assert_eq!(tokens, vec![
            Token::AtKeyword("media".to_string()), Token::Whitespace,
            Token::Delim('('), Token::Ident("min-width".to_string()), Token::Colon, Token::Whitespace,
            Token::Ident("600px".to_string()), Token::Delim(')'), Token::Whitespace, Token::OpenBrace,
        ]);
    }
}
