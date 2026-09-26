#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    StringLiteral(String),
    Identifier(String),
    Keyword(Keyword),
    Punct(Punct),
    Op(Op),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Keyword {
    Var, Let, Const, Function, Return, If, Else, For, While,
    Break, Continue, True, False, Null, Undefined, New, This, Typeof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Punct {
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Semicolon, Comma, Dot, Colon, Question,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Plus, Minus, Star, Slash, Percent,
    Assign, Eq, StrictEq, NotEq, StrictNotEq,
    Lt, Gt, LtEq, GtEq,
    AndAnd, OrOr, Not,
    Inc, Dec,
    PlusEq, MinusEq, StarEq, SlashEq,
}

pub struct Tokenizer;

impl Tokenizer {
    pub fn tokenize(src: &str) -> Vec<Token> {
        let chars: Vec<char> = src.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];

            if c.is_whitespace() {
                i += 1;
                continue;
            }

            if c == '/' && chars.get(i + 1) == Some(&'/') {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }

            if c == '/' && chars.get(i + 1) == Some(&'*') {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
                continue;
            }

            if c.is_ascii_digit() {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let value = text.parse::<f64>().unwrap_or(0.0);
                tokens.push(Token::Number(value));
                continue;
            }

            if c == '"' || c == '\'' {
                let quote = c;
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != quote {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                i = (i + 1).min(chars.len());
                tokens.push(Token::StringLiteral(text));
                continue;
            }

            if c.is_alphabetic() || c == '_' || c == '$' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                tokens.push(match text.as_str() {
                    "var" => Token::Keyword(Keyword::Var),
                    "let" => Token::Keyword(Keyword::Let),
                    "const" => Token::Keyword(Keyword::Const),
                    "function" => Token::Keyword(Keyword::Function),
                    "return" => Token::Keyword(Keyword::Return),
                    "if" => Token::Keyword(Keyword::If),
                    "else" => Token::Keyword(Keyword::Else),
                    "for" => Token::Keyword(Keyword::For),
                    "while" => Token::Keyword(Keyword::While),
                    "break" => Token::Keyword(Keyword::Break),
                    "continue" => Token::Keyword(Keyword::Continue),
                    "true" => Token::Keyword(Keyword::True),
                    "false" => Token::Keyword(Keyword::False),
                    "null" => Token::Keyword(Keyword::Null),
                    "undefined" => Token::Keyword(Keyword::Undefined),
                    "new" => Token::Keyword(Keyword::New),
                    "this" => Token::Keyword(Keyword::This),
                    "typeof" => Token::Keyword(Keyword::Typeof),
                    _ => Token::Identifier(text),
                });
                continue;
            }

            let three: String = chars[i..(i + 3).min(chars.len())].iter().collect();
            if three == "===" {
                tokens.push(Token::Op(Op::StrictEq));
                i += 3;
                continue;
            }
            if three == "!==" {
                tokens.push(Token::Op(Op::StrictNotEq));
                i += 3;
                continue;
            }

            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            let two_char = match two.as_str() {
                "==" => Some(Token::Op(Op::Eq)),
                "!=" => Some(Token::Op(Op::NotEq)),
                "<=" => Some(Token::Op(Op::LtEq)),
                ">=" => Some(Token::Op(Op::GtEq)),
                "&&" => Some(Token::Op(Op::AndAnd)),
                "||" => Some(Token::Op(Op::OrOr)),
                "++" => Some(Token::Op(Op::Inc)),
                "--" => Some(Token::Op(Op::Dec)),
                "+=" => Some(Token::Op(Op::PlusEq)),
                "-=" => Some(Token::Op(Op::MinusEq)),
                "*=" => Some(Token::Op(Op::StarEq)),
                "/=" => Some(Token::Op(Op::SlashEq)),
                _ => None,
            };
            if let Some(tok) = two_char {
                tokens.push(tok);
                i += 2;
                continue;
            }

            match c {
                '(' => tokens.push(Token::Punct(Punct::LParen)),
                ')' => tokens.push(Token::Punct(Punct::RParen)),
                '{' => tokens.push(Token::Punct(Punct::LBrace)),
                '}' => tokens.push(Token::Punct(Punct::RBrace)),
                '[' => tokens.push(Token::Punct(Punct::LBracket)),
                ']' => tokens.push(Token::Punct(Punct::RBracket)),
                ';' => tokens.push(Token::Punct(Punct::Semicolon)),
                ',' => tokens.push(Token::Punct(Punct::Comma)),
                '.' => tokens.push(Token::Punct(Punct::Dot)),
                ':' => tokens.push(Token::Punct(Punct::Colon)),
                '?' => tokens.push(Token::Punct(Punct::Question)),
                '+' => tokens.push(Token::Op(Op::Plus)),
                '-' => tokens.push(Token::Op(Op::Minus)),
                '*' => tokens.push(Token::Op(Op::Star)),
                '/' => tokens.push(Token::Op(Op::Slash)),
                '%' => tokens.push(Token::Op(Op::Percent)),
                '=' => tokens.push(Token::Op(Op::Assign)),
                '<' => tokens.push(Token::Op(Op::Lt)),
                '>' => tokens.push(Token::Op(Op::Gt)),
                '!' => tokens.push(Token::Op(Op::Not)),
                _ => {}
            }
            i += 1;
        }

        tokens.push(Token::Eof);
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_an_integer_number() {
        assert_eq!(Tokenizer::tokenize("42"), vec![Token::Number(42.0), Token::Eof]);
    }

    #[test]
    fn tokenizes_a_decimal_number() {
        assert_eq!(Tokenizer::tokenize("3.5"), vec![Token::Number(3.5), Token::Eof]);
    }

    #[test]
    fn tokenizes_double_and_single_quoted_strings() {
        assert_eq!(Tokenizer::tokenize("\"hi\""), vec![Token::StringLiteral("hi".to_string()), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("'hi'"), vec![Token::StringLiteral("hi".to_string()), Token::Eof]);
    }

    #[test]
    fn tokenizes_an_identifier() {
        assert_eq!(Tokenizer::tokenize("myVar_1"), vec![Token::Identifier("myVar_1".to_string()), Token::Eof]);
    }

    #[test]
    fn tokenizes_every_keyword() {
        let pairs = [
            ("var", Keyword::Var), ("let", Keyword::Let), ("const", Keyword::Const),
            ("function", Keyword::Function), ("return", Keyword::Return),
            ("if", Keyword::If), ("else", Keyword::Else), ("for", Keyword::For),
            ("while", Keyword::While), ("break", Keyword::Break), ("continue", Keyword::Continue),
            ("true", Keyword::True), ("false", Keyword::False), ("null", Keyword::Null),
            ("undefined", Keyword::Undefined), ("new", Keyword::New), ("this", Keyword::This),
            ("typeof", Keyword::Typeof),
        ];
        for (text, kw) in pairs {
            assert_eq!(Tokenizer::tokenize(text), vec![Token::Keyword(kw), Token::Eof], "keyword {text}");
        }
    }

    #[test]
    fn tokenizes_all_single_char_punctuation() {
        assert_eq!(
            Tokenizer::tokenize("(){}[];,.:?"),
            vec![
                Token::Punct(Punct::LParen), Token::Punct(Punct::RParen),
                Token::Punct(Punct::LBrace), Token::Punct(Punct::RBrace),
                Token::Punct(Punct::LBracket), Token::Punct(Punct::RBracket),
                Token::Punct(Punct::Semicolon), Token::Punct(Punct::Comma),
                Token::Punct(Punct::Dot), Token::Punct(Punct::Colon),
                Token::Punct(Punct::Question),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn tokenizes_multi_char_operators_longest_match_first() {
        assert_eq!(Tokenizer::tokenize("==="), vec![Token::Op(Op::StrictEq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("!=="), vec![Token::Op(Op::StrictNotEq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("=="), vec![Token::Op(Op::Eq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("!="), vec![Token::Op(Op::NotEq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("<="), vec![Token::Op(Op::LtEq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize(">="), vec![Token::Op(Op::GtEq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("&&"), vec![Token::Op(Op::AndAnd), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("||"), vec![Token::Op(Op::OrOr), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("++"), vec![Token::Op(Op::Inc), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("--"), vec![Token::Op(Op::Dec), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("+="), vec![Token::Op(Op::PlusEq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("-="), vec![Token::Op(Op::MinusEq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("*="), vec![Token::Op(Op::StarEq), Token::Eof]);
        assert_eq!(Tokenizer::tokenize("/="), vec![Token::Op(Op::SlashEq), Token::Eof]);
    }

    #[test]
    fn tokenizes_single_char_operators() {
        assert_eq!(
            Tokenizer::tokenize("+ - * / % = < > !"),
            vec![
                Token::Op(Op::Plus), Token::Op(Op::Minus), Token::Op(Op::Star),
                Token::Op(Op::Slash), Token::Op(Op::Percent), Token::Op(Op::Assign),
                Token::Op(Op::Lt), Token::Op(Op::Gt), Token::Op(Op::Not),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn skips_whitespace_and_comments() {
        assert_eq!(
            Tokenizer::tokenize("  1 // a comment\n  + /* block\ncomment */ 2  "),
            vec![Token::Number(1.0), Token::Op(Op::Plus), Token::Number(2.0), Token::Eof]
        );
    }

    #[test]
    fn unterminated_string_does_not_panic_and_still_reaches_eof() {
        let tokens = Tokenizer::tokenize("\"never closed");
        assert_eq!(tokens, vec![Token::StringLiteral("never closed".to_string()), Token::Eof]);
    }

    #[test]
    fn malformed_number_with_two_dots_does_not_panic() {
        let tokens = Tokenizer::tokenize("1.2.3");
        assert!(matches!(tokens[0], Token::Number(_)), "must still produce a Number token, not panic");
        assert_eq!(*tokens.last().unwrap(), Token::Eof);
    }

    #[test]
    fn unknown_character_is_skipped_without_panicking() {
        assert_eq!(Tokenizer::tokenize("1 @ 2"), vec![Token::Number(1.0), Token::Number(2.0), Token::Eof]);
    }
}
