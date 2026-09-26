use crate::ast::*;
use crate::tokenizer::{Keyword, Punct, Token};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

pub struct Parser;

impl Parser {
    pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
        ParserState::new(tokens).parse_program()
    }
}

struct ParserState<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> ParserState<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError { message: message.into(), position: self.pos }
    }

    fn expect_punct(&mut self, p: Punct) -> Result<(), ParseError> {
        if *self.peek() == Token::Punct(p) {
            self.advance();
            Ok(())
        } else {
            let found = self.peek().clone();
            Err(self.error(format!("esperado {p:?}, encontrado {found:?}")))
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        match self.peek().clone() {
            Token::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(self.error(format!("esperado identificador, encontrado {other:?}"))),
        }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut body = Vec::new();
        while *self.peek() != Token::Eof {
            body.push(self.parse_statement()?);
        }
        Ok(Program { body })
    }

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expression()?;
        if *self.peek() == Token::Punct(Punct::Semicolon) {
            self.advance();
        }
        Ok(Stmt::Expression(expr))
    }

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Number(n) => {
                self.advance();
                Ok(Expr::Number(n))
            }
            Token::StringLiteral(s) => {
                self.advance();
                Ok(Expr::StringLiteral(s))
            }
            Token::Identifier(name) => {
                self.advance();
                Ok(Expr::Identifier(name))
            }
            Token::Keyword(Keyword::True) => {
                self.advance();
                Ok(Expr::Boolean(true))
            }
            Token::Keyword(Keyword::False) => {
                self.advance();
                Ok(Expr::Boolean(false))
            }
            Token::Keyword(Keyword::Null) => {
                self.advance();
                Ok(Expr::Null)
            }
            Token::Keyword(Keyword::Undefined) => {
                self.advance();
                Ok(Expr::Undefined)
            }
            Token::Keyword(Keyword::This) => {
                self.advance();
                Ok(Expr::Identifier("this".to_string()))
            }
            Token::Punct(Punct::LParen) => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect_punct(Punct::RParen)?;
                Ok(expr)
            }
            Token::Punct(Punct::LBracket) => self.parse_array_literal(),
            Token::Punct(Punct::LBrace) => self.parse_object_literal(),
            other => Err(self.error(format!("esperado uma expressão, encontrado {other:?}"))),
        }
    }

    fn parse_array_literal(&mut self) -> Result<Expr, ParseError> {
        self.advance(); // '['
        let mut elements = Vec::new();
        while *self.peek() != Token::Punct(Punct::RBracket) && *self.peek() != Token::Eof {
            elements.push(self.parse_expression()?);
            if *self.peek() == Token::Punct(Punct::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_punct(Punct::RBracket)?;
        Ok(Expr::Array(elements))
    }

    fn parse_object_literal(&mut self) -> Result<Expr, ParseError> {
        self.advance(); // '{'
        let mut properties = Vec::new();
        while *self.peek() != Token::Punct(Punct::RBrace) && *self.peek() != Token::Eof {
            let key = self.parse_property_key()?;
            self.expect_punct(Punct::Colon)?;
            let value = self.parse_expression()?;
            properties.push((key, value));
            if *self.peek() == Token::Punct(Punct::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_punct(Punct::RBrace)?;
        Ok(Expr::Object(properties))
    }

    fn parse_property_key(&mut self) -> Result<String, ParseError> {
        match self.peek().clone() {
            Token::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            Token::StringLiteral(s) => {
                self.advance();
                Ok(s)
            }
            Token::Number(n) => {
                self.advance();
                Ok(n.to_string())
            }
            other => Err(self.error(format!("esperado chave de propriedade, encontrado {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    fn parse(src: &str) -> Result<Program, ParseError> {
        Parser::parse(&Tokenizer::tokenize(src))
    }

    #[test]
    fn parses_a_number_literal_statement() {
        let program = parse("42;").unwrap();
        assert_eq!(program.body, vec![Stmt::Expression(Expr::Number(42.0))]);
    }

    #[test]
    fn parses_a_string_literal_statement() {
        let program = parse("\"hi\";").unwrap();
        assert_eq!(program.body, vec![Stmt::Expression(Expr::StringLiteral("hi".to_string()))]);
    }

    #[test]
    fn parses_booleans_null_and_undefined() {
        assert_eq!(parse("true;").unwrap().body, vec![Stmt::Expression(Expr::Boolean(true))]);
        assert_eq!(parse("false;").unwrap().body, vec![Stmt::Expression(Expr::Boolean(false))]);
        assert_eq!(parse("null;").unwrap().body, vec![Stmt::Expression(Expr::Null)]);
        assert_eq!(parse("undefined;").unwrap().body, vec![Stmt::Expression(Expr::Undefined)]);
    }

    #[test]
    fn parses_this_as_an_identifier() {
        let program = parse("this;").unwrap();
        assert_eq!(program.body, vec![Stmt::Expression(Expr::Identifier("this".to_string()))]);
    }

    #[test]
    fn parses_an_identifier_statement() {
        let program = parse("myVar;").unwrap();
        assert_eq!(program.body, vec![Stmt::Expression(Expr::Identifier("myVar".to_string()))]);
    }

    #[test]
    fn parses_a_parenthesized_expression() {
        let program = parse("(42);").unwrap();
        assert_eq!(program.body, vec![Stmt::Expression(Expr::Number(42.0))]);
    }

    #[test]
    fn parses_an_empty_and_a_filled_array_literal() {
        assert_eq!(parse("[];").unwrap().body, vec![Stmt::Expression(Expr::Array(vec![]))]);
        assert_eq!(
            parse("[1, 2, 3];").unwrap().body,
            vec![Stmt::Expression(Expr::Array(vec![Expr::Number(1.0), Expr::Number(2.0), Expr::Number(3.0)]))]
        );
    }

    #[test]
    fn parses_an_object_literal_with_identifier_string_and_number_keys() {
        let program = parse("({a: 1, \"b\": 2, 3: 4});").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::Object(vec![
                ("a".to_string(), Expr::Number(1.0)),
                ("b".to_string(), Expr::Number(2.0)),
                ("3".to_string(), Expr::Number(4.0)),
            ]))]
        );
    }

    #[test]
    fn multiple_statements_in_one_program() {
        let program = parse("1; 2;").unwrap();
        assert_eq!(program.body, vec![Stmt::Expression(Expr::Number(1.0)), Stmt::Expression(Expr::Number(2.0))]);
    }

    #[test]
    fn trailing_semicolon_is_optional() {
        let program = parse("42").unwrap();
        assert_eq!(program.body, vec![Stmt::Expression(Expr::Number(42.0))]);
    }

    #[test]
    fn unterminated_array_literal_is_a_parse_error_not_a_panic() {
        let result = parse("[1, 2");
        assert!(result.is_err(), "missing ']' before EOF must be a parse error");
    }

    #[test]
    fn unterminated_object_literal_is_a_parse_error_not_a_panic() {
        let result = parse("({a: 1");
        assert!(result.is_err(), "missing '}}' before EOF must be a parse error");
    }
}
