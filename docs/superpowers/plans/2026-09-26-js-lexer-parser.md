# JS Lexer and Parser (2.12) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A new `ferris-js` crate that tokenizes an ES5-ish JavaScript source string into `Vec<Token>` and parses those tokens into a `Program` AST, returning `Result<Program, ParseError>` — no execution, no DOM, no integration with any other crate yet.

**Architecture:** Same tokenizer+parser split already used by `ferris-dom` (HTML) and `ferris-css` (CSS): `tokenizer.rs` (pure `&str -> Vec<Token>`, never fails), `ast.rs` (the `Program`/`Expr`/`Stmt` node types), `parser.rs` (`Parser::parse(&[Token]) -> Result<Program, ParseError>`, built via recursive-descent + operator-precedence climbing over an internal `ParserState`). Unlike the HTML/CSS parsers, this one can genuinely return `Err` on real syntax errors (no error-recovery convention exists for JS), but is still forbidden from panicking.

**Tech Stack:** Rust, zero external dependencies (same leaf-crate pattern as `ferris-css`/`ferris-dom`).

**Spec:** `docs/superpowers/specs/2026-09-26-js-lexer-parser-design.md`

## Global Constraints

- `ferris-js` is added as a new workspace member in the root `Cargo.toml`, `edition = "2021"`, zero dependencies — same leaf-crate shape as `ferris-css`/`ferris-dom` (spec's Arquitetura).
- `Tokenizer::tokenize` never panics on malformed input (unterminated string, malformed number, unknown character) — it always produces some `Vec<Token>` ending in `Token::Eof` (spec's Tratamento de erro).
- `Parser::parse` never panics either. Genuine syntax errors return `Err(ParseError { message, position })` — the one parser in this project allowed to fail, but every access to the token slice goes through `ParserState::peek()`/`advance()` (both bounds-safe, returning `Token::Eof` past the end), never direct indexing or `.unwrap()` on a token lookup (spec's Tratamento de erro).
- `this` is tokenized as `Keyword::This` but parsed as `Expr::Identifier("this".to_string())` — the spec's `Expr` enum has no dedicated `This` variant; real `this`-binding semantics are deferred to the interpreter sub-projects (2.13+). This is a plan-level refinement, not a spec change.
- No integration with any other Ferris crate or the page-loading pipeline happens in this sub-project. `ferris-js` compiles and tests standalone; nothing else in the workspace references it yet (that's 2.16).
- Explicitly out of scope for the whole crate (per spec): classes, arrow functions, template literals, destructuring, spread/rest, async/await, generators, modules, regex literals, `try`/`catch`.

## Review Focus

- An unterminated string literal (missing closing quote before EOF) must not panic the tokenizer — it must still emit a `StringLiteral` token with whatever text preceded EOF, followed by `Token::Eof`. → Task 1.
- A malformed numeric literal that fails to parse as `f64` (e.g. `"1.2.3"` with two dots) must not panic on the parse step — it must degrade to some numeric value instead of crashing. → Task 1.
- An unterminated array or object literal (missing closing `]`/`}` before EOF) must make `Parser::parse` return `Err`, not loop forever or panic on out-of-bounds access. → Task 2.
- Assignment must be right-associative (`a = b = 3` parses as `a = (b = 3)`, not `(a = b) = 3`) — an easy direction to get backwards when writing precedence-climbing code. → Task 4.
- An unclosed block (`{` with no matching `}` before EOF, e.g. in an `if`/`while`/`function` body) must make `Parser::parse` return `Err`, not panic or infinite-loop. → Task 5.

---

### Task 1: Crate scaffold + `Token`/`Keyword`/`Punct`/`Op` + full `Tokenizer`

**Files:**
- Create: `ferris-js/Cargo.toml`
- Create: `ferris-js/src/lib.rs`
- Create: `ferris-js/src/tokenizer.rs`
- Modify: `Cargo.toml` (root workspace `members`)

**Interfaces:**
- Consumes: nothing (leaf crate).
- Produces: `pub enum Token { Number(f64), StringLiteral(String), Identifier(String), Keyword(Keyword), Punct(Punct), Op(Op), Eof }` (derives `Debug, Clone, PartialEq`), `pub enum Keyword { Var, Let, Const, Function, Return, If, Else, For, While, Break, Continue, True, False, Null, Undefined, New, This, Typeof }` (derives `Debug, Clone, Copy, PartialEq`), `pub enum Punct { LParen, RParen, LBrace, RBrace, LBracket, RBracket, Semicolon, Comma, Dot, Colon, Question }` (derives `Debug, Clone, Copy, PartialEq`), `pub enum Op { Plus, Minus, Star, Slash, Percent, Assign, Eq, StrictEq, NotEq, StrictNotEq, Lt, Gt, LtEq, GtEq, AndAnd, OrOr, Not, Inc, Dec, PlusEq, MinusEq, StarEq, SlashEq }` (derives `Debug, Clone, Copy, PartialEq`), `pub struct Tokenizer; impl Tokenizer { pub fn tokenize(src: &str) -> Vec<Token> }` — consumed by every later task (all call `Tokenizer::tokenize` to produce input for `Parser::parse`).

- [ ] **Step 1: Create the crate and wire it into the workspace**

Create `ferris-js/Cargo.toml`:
```toml
[package]
name = "ferris-js"
version = "0.1.0"
edition = "2021"

[dependencies]
```

Modify root `Cargo.toml`'s `members` list to add `"ferris-js"`:
```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style", "ferris-layout", "ferris-paint", "ferris-scene", "ferris-text", "ferris-loader", "ferris-js"]
resolver = "2"
```

Create `ferris-js/src/lib.rs`:
```rust
pub mod tokenizer;
```

Create `ferris-js/src/tokenizer.rs` with just enough to compile (no logic yet):
```rust
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
    pub fn tokenize(_src: &str) -> Vec<Token> {
        vec![Token::Eof]
    }
}
```

Run: `cargo build --package ferris-js`
Expected: builds successfully (placeholder implementation).

- [ ] **Step 2: Write the failing tests**

Append to `ferris-js/src/tokenizer.rs`:
```rust
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
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --package ferris-js`
Expected: most tests FAIL (placeholder `tokenize` always returns `vec![Token::Eof]`).

- [ ] **Step 4: Implement the real tokenizer**

Replace the `Tokenizer` impl in `ferris-js/src/tokenizer.rs` (keep the enums and the `#[cfg(test)]` module unchanged):
```rust
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --package ferris-js`
Expected: PASS (12 tests).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml ferris-js/Cargo.toml ferris-js/src/lib.rs ferris-js/src/tokenizer.rs
git commit -m "feat: add ferris-js crate with a full ES5-ish tokenizer"
```

---

### Task 2: AST types, `ParseError`, `Parser` skeleton, primary expressions

**Files:**
- Create: `ferris-js/src/ast.rs`
- Create: `ferris-js/src/parser.rs`
- Modify: `ferris-js/src/lib.rs`

**Interfaces:**
- Consumes: `ferris_js::tokenizer::{Token, Keyword, Punct}` (Task 1).
- Produces: `pub struct Program { pub body: Vec<Stmt> }`, `pub enum BinaryOp {...}`, `pub enum LogicalOp {...}`, `pub enum UnaryOp {...}`, `pub enum AssignOp {...}`, `pub enum DeclKind {...}`, `pub enum Expr {...}` (all variants from the spec), `pub enum Stmt {...}` (all variants from the spec) — all in `ast.rs`, all deriving `Debug, Clone, PartialEq` (`BinaryOp`/`LogicalOp`/`UnaryOp`/`AssignOp`/`DeclKind` also derive `Copy`). `pub struct ParseError { pub message: String, pub position: usize }` (derives `Debug, Clone, PartialEq`) and `pub struct Parser; impl Parser { pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> }` in `parser.rs` — consumed by every later task (Tasks 3-7 extend `parser.rs`'s internal `ParserState` methods; the public `Parser::parse` signature never changes again).

- [ ] **Step 1: Write the failing tests**

Create `ferris-js/src/ast.rs`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Eq, StrictEq, NotEq, StrictNotEq,
    Lt, Gt, LtEq, GtEq,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogicalOp {
    And, Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg, Not, Typeof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AssignOp {
    Assign, AddAssign, SubAssign, MulAssign, DivAssign,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeclKind {
    Var, Let, Const,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    StringLiteral(String),
    Boolean(bool),
    Null,
    Undefined,
    Identifier(String),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Logical { op: LogicalOp, left: Box<Expr>, right: Box<Expr> },
    Unary { op: UnaryOp, argument: Box<Expr> },
    Assignment { op: AssignOp, target: Box<Expr>, value: Box<Expr> },
    Call { callee: Box<Expr>, arguments: Vec<Expr> },
    Member { object: Box<Expr>, property: Box<Expr>, computed: bool },
    Function { name: Option<String>, params: Vec<String>, body: Vec<Stmt> },
    Conditional { test: Box<Expr>, consequent: Box<Expr>, alternate: Box<Expr> },
    New { callee: Box<Expr>, arguments: Vec<Expr> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expression(Expr),
    VarDecl { kind: DeclKind, declarations: Vec<(String, Option<Expr>)> },
    FunctionDecl { name: String, params: Vec<String>, body: Vec<Stmt> },
    If { test: Expr, consequent: Box<Stmt>, alternate: Option<Box<Stmt>> },
    For { init: Option<Box<Stmt>>, test: Option<Expr>, update: Option<Expr>, body: Box<Stmt> },
    While { test: Expr, body: Box<Stmt> },
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Break,
    Continue,
}
```

Create `ferris-js/src/parser.rs` with the tests (implementation follows in Step 3):
```rust
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
        assert!(result.is_err(), "missing '}' before EOF must be a parse error");
    }
}
```

Modify `ferris-js/src/lib.rs`:
```rust
pub mod ast;
pub mod parser;
pub mod tokenizer;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-js`
Expected: FAIL to compile — `ParserState` has no methods yet (`parse_program` doesn't exist).

- [ ] **Step 3: Implement the parser skeleton and primary expressions**

Add to `ferris-js/src/parser.rs`, replacing the empty `struct ParserState<'a> { ... }` block with a full `impl`:
```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --package ferris-js`
Expected: PASS (all Task 1 tests + 12 new Task 2 tests = 24 tests).

- [ ] **Step 5: Commit**

```bash
git add ferris-js/src/ast.rs ferris-js/src/parser.rs ferris-js/src/lib.rs
git commit -m "feat: add ferris-js AST types and a parser skeleton with primary expressions"
```

---

### Task 3: Member access, function calls, `new`

**Files:**
- Modify: `ferris-js/src/parser.rs`

**Interfaces:**
- Consumes: `ParserState::{peek, advance, error, expect_punct, expect_identifier, parse_primary}` (Task 2).
- Produces: `ParserState::{parse_call_member, parse_arguments, parse_new}` and a `parse_expression` that now delegates to `parse_call_member` — consumed by Task 4 (which will insert precedence layers between `parse_expression` and `parse_call_member`).

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `ferris-js/src/parser.rs` (after `unterminated_object_literal_is_a_parse_error_not_a_panic`):
```rust
    #[test]
    fn parses_dot_member_access() {
        let program = parse("a.b;").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::Member {
                object: Box::new(Expr::Identifier("a".to_string())),
                property: Box::new(Expr::Identifier("b".to_string())),
                computed: false,
            })]
        );
    }

    #[test]
    fn parses_computed_bracket_member_access() {
        let program = parse("a[0];").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::Member {
                object: Box::new(Expr::Identifier("a".to_string())),
                property: Box::new(Expr::Number(0.0)),
                computed: true,
            })]
        );
    }

    #[test]
    fn parses_a_function_call_with_arguments() {
        let program = parse("f(1, 2);").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::Call {
                callee: Box::new(Expr::Identifier("f".to_string())),
                arguments: vec![Expr::Number(1.0), Expr::Number(2.0)],
            })]
        );
    }

    #[test]
    fn parses_chained_member_and_call() {
        let program = parse("a.b().c;").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::Member {
                object: Box::new(Expr::Call {
                    callee: Box::new(Expr::Member {
                        object: Box::new(Expr::Identifier("a".to_string())),
                        property: Box::new(Expr::Identifier("b".to_string())),
                        computed: false,
                    }),
                    arguments: vec![],
                }),
                property: Box::new(Expr::Identifier("c".to_string())),
                computed: false,
            })]
        );
    }

    #[test]
    fn parses_new_with_member_callee_and_arguments() {
        let program = parse("new Foo.Bar(1);").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::New {
                callee: Box::new(Expr::Member {
                    object: Box::new(Expr::Identifier("Foo".to_string())),
                    property: Box::new(Expr::Identifier("Bar".to_string())),
                    computed: false,
                }),
                arguments: vec![Expr::Number(1.0)],
            })]
        );
    }

    #[test]
    fn parses_new_with_no_parentheses_as_zero_arguments() {
        let program = parse("new Foo;").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::New {
                callee: Box::new(Expr::Identifier("Foo".to_string())),
                arguments: vec![],
            })]
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-js`
Expected: FAIL — `a.b;` currently parses `a` as a complete statement and then hits `.b` as an unexpected leftover token (or a similar mismatch), since `parse_expression` only calls `parse_primary` so far.

- [ ] **Step 3: Implement member/call/new parsing**

In `ferris-js/src/parser.rs`, replace the existing `fn parse_expression` and add the `Token::Keyword(Keyword::New)` arm to `parse_primary`'s `match`:
```rust
    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_call_member()
    }

    fn parse_call_member(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek().clone() {
                Token::Punct(Punct::Dot) => {
                    self.advance();
                    let name = self.expect_identifier()?;
                    expr = Expr::Member {
                        object: Box::new(expr),
                        property: Box::new(Expr::Identifier(name)),
                        computed: false,
                    };
                }
                Token::Punct(Punct::LBracket) => {
                    self.advance();
                    let property = self.parse_expression()?;
                    self.expect_punct(Punct::RBracket)?;
                    expr = Expr::Member { object: Box::new(expr), property: Box::new(property), computed: true };
                }
                Token::Punct(Punct::LParen) => {
                    let arguments = self.parse_arguments()?;
                    expr = Expr::Call { callee: Box::new(expr), arguments };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect_punct(Punct::LParen)?;
        let mut arguments = Vec::new();
        while *self.peek() != Token::Punct(Punct::RParen) && *self.peek() != Token::Eof {
            arguments.push(self.parse_expression()?);
            if *self.peek() == Token::Punct(Punct::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_punct(Punct::RParen)?;
        Ok(arguments)
    }

    fn parse_new(&mut self) -> Result<Expr, ParseError> {
        self.advance(); // 'new'
        let mut callee = self.parse_primary()?;
        loop {
            match self.peek().clone() {
                Token::Punct(Punct::Dot) => {
                    self.advance();
                    let name = self.expect_identifier()?;
                    callee = Expr::Member {
                        object: Box::new(callee),
                        property: Box::new(Expr::Identifier(name)),
                        computed: false,
                    };
                }
                Token::Punct(Punct::LBracket) => {
                    self.advance();
                    let property = self.parse_expression()?;
                    self.expect_punct(Punct::RBracket)?;
                    callee = Expr::Member { object: Box::new(callee), property: Box::new(property), computed: true };
                }
                _ => break,
            }
        }
        let arguments = if *self.peek() == Token::Punct(Punct::LParen) {
            self.parse_arguments()?
        } else {
            Vec::new()
        };
        Ok(Expr::New { callee: Box::new(callee), arguments })
    }
```

In `parse_primary`'s `match self.peek().clone() { ... }`, add a new arm right before the `Token::Punct(Punct::LParen)` arm:
```rust
            Token::Keyword(Keyword::New) => self.parse_new(),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --package ferris-js`
Expected: PASS (30 tests total).

- [ ] **Step 5: Commit**

```bash
git add ferris-js/src/parser.rs
git commit -m "feat: parse member access, function calls, and new expressions"
```

---

### Task 4: Unary, binary, logical, conditional, and assignment expressions

**Files:**
- Modify: `ferris-js/src/parser.rs`

**Interfaces:**
- Consumes: `ParserState::{peek, advance, error, expect_punct, parse_call_member}` (Tasks 2-3).
- Produces: `ParserState::{parse_assignment, parse_conditional, parse_logical_or, parse_logical_and, parse_equality, parse_relational, parse_additive, parse_multiplicative, parse_unary}`, and `parse_expression` now delegates to `parse_assignment` (the full precedence chain, top to bottom) — consumed by Task 5 (statements call `parse_expression`/`parse_assignment` for initializers, test conditions, and return values).

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `ferris-js/src/parser.rs`:
```rust
    #[test]
    fn parses_unary_not_neg_and_typeof() {
        assert_eq!(
            parse("!a;").unwrap().body,
            vec![Stmt::Expression(Expr::Unary { op: UnaryOp::Not, argument: Box::new(Expr::Identifier("a".to_string())) })]
        );
        assert_eq!(
            parse("-a;").unwrap().body,
            vec![Stmt::Expression(Expr::Unary { op: UnaryOp::Neg, argument: Box::new(Expr::Identifier("a".to_string())) })]
        );
        assert_eq!(
            parse("typeof a;").unwrap().body,
            vec![Stmt::Expression(Expr::Unary { op: UnaryOp::Typeof, argument: Box::new(Expr::Identifier("a".to_string())) })]
        );
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let program = parse("1 + 2 * 3;").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::Binary {
                op: BinaryOp::Add,
                left: Box::new(Expr::Number(1.0)),
                right: Box::new(Expr::Binary {
                    op: BinaryOp::Mul,
                    left: Box::new(Expr::Number(2.0)),
                    right: Box::new(Expr::Number(3.0)),
                }),
            })]
        );
    }

    #[test]
    fn parses_all_equality_and_relational_operators() {
        let cases = [
            ("a == b;", BinaryOp::Eq), ("a === b;", BinaryOp::StrictEq),
            ("a != b;", BinaryOp::NotEq), ("a !== b;", BinaryOp::StrictNotEq),
            ("a < b;", BinaryOp::Lt), ("a > b;", BinaryOp::Gt),
            ("a <= b;", BinaryOp::LtEq), ("a >= b;", BinaryOp::GtEq),
        ];
        for (src, op) in cases {
            assert_eq!(
                parse(src).unwrap().body,
                vec![Stmt::Expression(Expr::Binary {
                    op,
                    left: Box::new(Expr::Identifier("a".to_string())),
                    right: Box::new(Expr::Identifier("b".to_string())),
                })],
                "case {src}"
            );
        }
    }

    #[test]
    fn parses_logical_and_and_or() {
        assert_eq!(
            parse("a && b;").unwrap().body,
            vec![Stmt::Expression(Expr::Logical {
                op: LogicalOp::And,
                left: Box::new(Expr::Identifier("a".to_string())),
                right: Box::new(Expr::Identifier("b".to_string())),
            })]
        );
        assert_eq!(
            parse("a || b;").unwrap().body,
            vec![Stmt::Expression(Expr::Logical {
                op: LogicalOp::Or,
                left: Box::new(Expr::Identifier("a".to_string())),
                right: Box::new(Expr::Identifier("b".to_string())),
            })]
        );
    }

    #[test]
    fn parses_a_ternary_conditional() {
        let program = parse("a ? b : c;").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::Conditional {
                test: Box::new(Expr::Identifier("a".to_string())),
                consequent: Box::new(Expr::Identifier("b".to_string())),
                alternate: Box::new(Expr::Identifier("c".to_string())),
            })]
        );
    }

    #[test]
    fn parses_simple_and_compound_assignment() {
        assert_eq!(
            parse("a = 1;").unwrap().body,
            vec![Stmt::Expression(Expr::Assignment {
                op: AssignOp::Assign,
                target: Box::new(Expr::Identifier("a".to_string())),
                value: Box::new(Expr::Number(1.0)),
            })]
        );
        assert_eq!(
            parse("a += 1;").unwrap().body,
            vec![Stmt::Expression(Expr::Assignment {
                op: AssignOp::AddAssign,
                target: Box::new(Expr::Identifier("a".to_string())),
                value: Box::new(Expr::Number(1.0)),
            })]
        );
    }

    #[test]
    fn assignment_is_right_associative() {
        let program = parse("a = b = 3;").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Expression(Expr::Assignment {
                op: AssignOp::Assign,
                target: Box::new(Expr::Identifier("a".to_string())),
                value: Box::new(Expr::Assignment {
                    op: AssignOp::Assign,
                    target: Box::new(Expr::Identifier("b".to_string())),
                    value: Box::new(Expr::Number(3.0)),
                }),
            })],
            "a = b = 3 must parse as a = (b = 3), not (a = b) = 3"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-js`
Expected: FAIL — `parse_expression` currently only calls `parse_call_member`, so operators like `+`, `&&`, `?:`, `=` are left unconsumed and trip the "expected `;` or EOF" leftover-token path (or an equivalent mismatch).

- [ ] **Step 3: Implement the precedence chain**

In `ferris-js/src/parser.rs`, replace the existing `fn parse_expression` (added in Task 3) with:
```rust
    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_conditional()?;
        let op = match self.peek() {
            Token::Op(Op::Assign) => Some(AssignOp::Assign),
            Token::Op(Op::PlusEq) => Some(AssignOp::AddAssign),
            Token::Op(Op::MinusEq) => Some(AssignOp::SubAssign),
            Token::Op(Op::StarEq) => Some(AssignOp::MulAssign),
            Token::Op(Op::SlashEq) => Some(AssignOp::DivAssign),
            _ => None,
        };
        match op {
            Some(op) => {
                self.advance();
                let value = self.parse_assignment()?;
                Ok(Expr::Assignment { op, target: Box::new(left), value: Box::new(value) })
            }
            None => Ok(left),
        }
    }

    fn parse_conditional(&mut self) -> Result<Expr, ParseError> {
        let test = self.parse_logical_or()?;
        if *self.peek() == Token::Punct(Punct::Question) {
            self.advance();
            let consequent = self.parse_assignment()?;
            self.expect_punct(Punct::Colon)?;
            let alternate = self.parse_assignment()?;
            Ok(Expr::Conditional { test: Box::new(test), consequent: Box::new(consequent), alternate: Box::new(alternate) })
        } else {
            Ok(test)
        }
    }

    fn parse_logical_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_logical_and()?;
        while *self.peek() == Token::Op(Op::OrOr) {
            self.advance();
            let right = self.parse_logical_and()?;
            left = Expr::Logical { op: LogicalOp::Or, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while *self.peek() == Token::Op(Op::AndAnd) {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::Logical { op: LogicalOp::And, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_relational()?;
        loop {
            let op = match self.peek() {
                Token::Op(Op::Eq) => Some(BinaryOp::Eq),
                Token::Op(Op::StrictEq) => Some(BinaryOp::StrictEq),
                Token::Op(Op::NotEq) => Some(BinaryOp::NotEq),
                Token::Op(Op::StrictNotEq) => Some(BinaryOp::StrictNotEq),
                _ => None,
            };
            match op {
                Some(op) => {
                    self.advance();
                    let right = self.parse_relational()?;
                    left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
                }
                None => break,
            }
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                Token::Op(Op::Lt) => Some(BinaryOp::Lt),
                Token::Op(Op::Gt) => Some(BinaryOp::Gt),
                Token::Op(Op::LtEq) => Some(BinaryOp::LtEq),
                Token::Op(Op::GtEq) => Some(BinaryOp::GtEq),
                _ => None,
            };
            match op {
                Some(op) => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
                }
                None => break,
            }
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Op(Op::Plus) => Some(BinaryOp::Add),
                Token::Op(Op::Minus) => Some(BinaryOp::Sub),
                _ => None,
            };
            match op {
                Some(op) => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
                }
                None => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Op(Op::Star) => Some(BinaryOp::Mul),
                Token::Op(Op::Slash) => Some(BinaryOp::Div),
                Token::Op(Op::Percent) => Some(BinaryOp::Mod),
                _ => None,
            };
            match op {
                Some(op) => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
                }
                None => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        let op = match self.peek() {
            Token::Op(Op::Not) => Some(UnaryOp::Not),
            Token::Op(Op::Minus) => Some(UnaryOp::Neg),
            Token::Keyword(Keyword::Typeof) => Some(UnaryOp::Typeof),
            _ => None,
        };
        match op {
            Some(op) => {
                self.advance();
                let argument = self.parse_unary()?;
                Ok(Expr::Unary { op, argument: Box::new(argument) })
            }
            None => self.parse_call_member(),
        }
    }
```

Add `use crate::tokenizer::Op;` to the top `use` block of `ferris-js/src/parser.rs` (alongside the existing `use crate::tokenizer::{Keyword, Punct, Token};`, which can become `use crate::tokenizer::{Keyword, Op, Punct, Token};`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --package ferris-js`
Expected: PASS (37 tests total).

- [ ] **Step 5: Commit**

```bash
git add ferris-js/src/parser.rs
git commit -m "feat: parse unary, binary, logical, conditional, and assignment expressions"
```

---

### Task 5: Statements — var/let/const, block, if/else, return, break, continue

**Files:**
- Modify: `ferris-js/src/parser.rs`

**Interfaces:**
- Consumes: `ParserState::{peek, advance, error, expect_punct, expect_identifier, parse_expression, parse_assignment}` (Tasks 2-4).
- Produces: `ParserState::{parse_var_decl_clause, parse_var_decl_statement, parse_block, parse_if, parse_return, consume_optional_semicolon}`, and `parse_statement`'s dispatch now covers `var`/`let`/`const`/`if`/`return`/`break`/`continue`/`{` in addition to the expression-statement fallback from Task 2 — consumed by Task 6 (`for`'s init clause reuses `parse_var_decl_clause`; `parse_statement`'s dispatch gains `for`/`while`/`function` arms).

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `ferris-js/src/parser.rs`:
```rust
    #[test]
    fn parses_var_let_const_with_and_without_initializer() {
        assert_eq!(
            parse("var a;").unwrap().body,
            vec![Stmt::VarDecl { kind: DeclKind::Var, declarations: vec![("a".to_string(), None)] }]
        );
        assert_eq!(
            parse("let a = 1;").unwrap().body,
            vec![Stmt::VarDecl { kind: DeclKind::Let, declarations: vec![("a".to_string(), Some(Expr::Number(1.0)))] }]
        );
        assert_eq!(
            parse("const a = 1, b = 2;").unwrap().body,
            vec![Stmt::VarDecl {
                kind: DeclKind::Const,
                declarations: vec![
                    ("a".to_string(), Some(Expr::Number(1.0))),
                    ("b".to_string(), Some(Expr::Number(2.0))),
                ],
            }]
        );
    }

    #[test]
    fn parses_a_block_statement() {
        let program = parse("{ 1; 2; }").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::Block(vec![Stmt::Expression(Expr::Number(1.0)), Stmt::Expression(Expr::Number(2.0))])]
        );
    }

    #[test]
    fn parses_if_without_else() {
        let program = parse("if (a) { 1; }").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::If {
                test: Expr::Identifier("a".to_string()),
                consequent: Box::new(Stmt::Block(vec![Stmt::Expression(Expr::Number(1.0))])),
                alternate: None,
            }]
        );
    }

    #[test]
    fn parses_if_with_else() {
        let program = parse("if (a) { 1; } else { 2; }").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::If {
                test: Expr::Identifier("a".to_string()),
                consequent: Box::new(Stmt::Block(vec![Stmt::Expression(Expr::Number(1.0))])),
                alternate: Some(Box::new(Stmt::Block(vec![Stmt::Expression(Expr::Number(2.0))]))),
            }]
        );
    }

    #[test]
    fn parses_return_with_and_without_a_value() {
        assert_eq!(parse("return;").unwrap().body, vec![Stmt::Return(None)]);
        assert_eq!(parse("return 1;").unwrap().body, vec![Stmt::Return(Some(Expr::Number(1.0)))]);
    }

    #[test]
    fn parses_break_and_continue() {
        assert_eq!(parse("break;").unwrap().body, vec![Stmt::Break]);
        assert_eq!(parse("continue;").unwrap().body, vec![Stmt::Continue]);
    }

    #[test]
    fn unclosed_block_is_a_parse_error_not_a_panic() {
        let result = parse("if (a) { 1;");
        assert!(result.is_err(), "missing '}' before EOF must be a parse error, not a panic or infinite loop");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-js`
Expected: FAIL — `parse_statement` currently only produces `Stmt::Expression`, so `var a;`/`{ ... }`/`if (...) {...}`/`return;`/`break;`/`continue;` all mis-parse (e.g. `var` isn't a valid expression, so it hits the "expected an expression" error path instead of the intended `Stmt::VarDecl`).

- [ ] **Step 3: Implement the new statement kinds**

In `ferris-js/src/parser.rs`, replace the existing `fn parse_statement` (from Task 2) with a dispatching version, and add the new helper methods:
```rust
    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        match self.peek() {
            Token::Keyword(Keyword::Var) => self.parse_var_decl_statement(DeclKind::Var),
            Token::Keyword(Keyword::Let) => self.parse_var_decl_statement(DeclKind::Let),
            Token::Keyword(Keyword::Const) => self.parse_var_decl_statement(DeclKind::Const),
            Token::Keyword(Keyword::If) => self.parse_if(),
            Token::Keyword(Keyword::Return) => self.parse_return(),
            Token::Keyword(Keyword::Break) => {
                self.advance();
                self.consume_optional_semicolon();
                Ok(Stmt::Break)
            }
            Token::Keyword(Keyword::Continue) => {
                self.advance();
                self.consume_optional_semicolon();
                Ok(Stmt::Continue)
            }
            Token::Punct(Punct::LBrace) => self.parse_block(),
            _ => {
                let expr = self.parse_expression()?;
                self.consume_optional_semicolon();
                Ok(Stmt::Expression(expr))
            }
        }
    }

    fn consume_optional_semicolon(&mut self) {
        if *self.peek() == Token::Punct(Punct::Semicolon) {
            self.advance();
        }
    }

    fn parse_var_decl_clause(&mut self, kind: DeclKind) -> Result<Stmt, ParseError> {
        self.advance(); // var/let/const
        let mut declarations = Vec::new();
        loop {
            let name = self.expect_identifier()?;
            let init = if *self.peek() == Token::Op(Op::Assign) {
                self.advance();
                Some(self.parse_assignment()?)
            } else {
                None
            };
            declarations.push((name, init));
            if *self.peek() == Token::Punct(Punct::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(Stmt::VarDecl { kind, declarations })
    }

    fn parse_var_decl_statement(&mut self, kind: DeclKind) -> Result<Stmt, ParseError> {
        let stmt = self.parse_var_decl_clause(kind)?;
        self.consume_optional_semicolon();
        Ok(stmt)
    }

    fn parse_block(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // '{'
        let mut stmts = Vec::new();
        while *self.peek() != Token::Punct(Punct::RBrace) && *self.peek() != Token::Eof {
            stmts.push(self.parse_statement()?);
        }
        self.expect_punct(Punct::RBrace)?;
        Ok(Stmt::Block(stmts))
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'if'
        self.expect_punct(Punct::LParen)?;
        let test = self.parse_expression()?;
        self.expect_punct(Punct::RParen)?;
        let consequent = self.parse_statement()?;
        let alternate = if *self.peek() == Token::Keyword(Keyword::Else) {
            self.advance();
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };
        Ok(Stmt::If { test, consequent: Box::new(consequent), alternate })
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'return'
        let value = if *self.peek() == Token::Punct(Punct::Semicolon)
            || *self.peek() == Token::Punct(Punct::RBrace)
            || *self.peek() == Token::Eof
        {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.consume_optional_semicolon();
        Ok(Stmt::Return(value))
    }
```

Also replace the two call sites that used `consume_optional_semicolon`'s inline predecessor: the `_ =>` fallback arm above already reflects this (it now calls `self.consume_optional_semicolon()` instead of the old inline `if *self.peek() == Token::Punct(Punct::Semicolon) { self.advance(); }`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --package ferris-js`
Expected: PASS (44 tests total).

- [ ] **Step 5: Commit**

```bash
git add ferris-js/src/parser.rs
git commit -m "feat: parse var/let/const, block, if/else, return, break, and continue statements"
```

---

### Task 6: `for`/`while` loops and function declarations/expressions

**Files:**
- Modify: `ferris-js/src/parser.rs`

**Interfaces:**
- Consumes: `ParserState::{peek, advance, error, expect_punct, expect_identifier, parse_expression, parse_statement, parse_var_decl_clause}` (Tasks 2-5).
- Produces: `ParserState::{parse_while, parse_for, parse_param_list, parse_function_decl, parse_function_expr}`; `parse_statement`'s dispatch gains `for`/`while`/`function` arms; `parse_primary`'s `match` gains a `Token::Keyword(Keyword::Function)` arm — this is the last task that changes `parser.rs`'s dispatch tables; Task 7 only adds tests.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `ferris-js/src/parser.rs`:
```rust
    #[test]
    fn parses_a_while_loop() {
        let program = parse("while (a) { 1; }").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::While {
                test: Expr::Identifier("a".to_string()),
                body: Box::new(Stmt::Block(vec![Stmt::Expression(Expr::Number(1.0))])),
            }]
        );
    }

    #[test]
    fn parses_a_full_for_loop() {
        let program = parse("for (let i = 0; i < 3; i = i + 1) { 1; }").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::For {
                init: Some(Box::new(Stmt::VarDecl {
                    kind: DeclKind::Let,
                    declarations: vec![("i".to_string(), Some(Expr::Number(0.0)))],
                })),
                test: Some(Expr::Binary {
                    op: BinaryOp::Lt,
                    left: Box::new(Expr::Identifier("i".to_string())),
                    right: Box::new(Expr::Number(3.0)),
                }),
                update: Some(Expr::Assignment {
                    op: AssignOp::Assign,
                    target: Box::new(Expr::Identifier("i".to_string())),
                    value: Box::new(Expr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Identifier("i".to_string())),
                        right: Box::new(Expr::Number(1.0)),
                    }),
                }),
                body: Box::new(Stmt::Block(vec![Stmt::Expression(Expr::Number(1.0))])),
            }]
        );
    }

    #[test]
    fn parses_a_for_loop_with_omitted_clauses() {
        let program = parse("for (;;) { break; }").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::For {
                init: None,
                test: None,
                update: None,
                body: Box::new(Stmt::Block(vec![Stmt::Break])),
            }]
        );
    }

    #[test]
    fn parses_a_named_function_declaration() {
        let program = parse("function add(a, b) { return a + b; }").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::FunctionDecl {
                name: "add".to_string(),
                params: vec!["a".to_string(), "b".to_string()],
                body: vec![Stmt::Return(Some(Expr::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Identifier("a".to_string())),
                    right: Box::new(Expr::Identifier("b".to_string())),
                }))],
            }]
        );
    }

    #[test]
    fn parses_an_anonymous_function_expression_assigned_to_a_variable() {
        let program = parse("const f = function(x) { return x; };").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::VarDecl {
                kind: DeclKind::Const,
                declarations: vec![(
                    "f".to_string(),
                    Some(Expr::Function {
                        name: None,
                        params: vec!["x".to_string()],
                        body: vec![Stmt::Return(Some(Expr::Identifier("x".to_string())))],
                    })
                )],
            }]
        );
    }

    #[test]
    fn parses_a_named_function_expression() {
        let program = parse("const f = function named() { return 1; };").unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::VarDecl {
                kind: DeclKind::Const,
                declarations: vec![(
                    "f".to_string(),
                    Some(Expr::Function {
                        name: Some("named".to_string()),
                        params: vec![],
                        body: vec![Stmt::Return(Some(Expr::Number(1.0)))],
                    })
                )],
            }]
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-js`
Expected: FAIL — `for`/`while`/`function` aren't in `parse_statement`'s dispatch yet (they fall into the expression-statement branch and error out as "expected an expression"), and `function` isn't in `parse_primary` either.

- [ ] **Step 3: Implement `for`/`while`/function parsing**

In `ferris-js/src/parser.rs`, add two new arms to `parse_statement`'s `match` (from Task 5), right after the `Token::Keyword(Keyword::Continue) => { ... }` arm:
```rust
            Token::Keyword(Keyword::For) => self.parse_for(),
            Token::Keyword(Keyword::While) => self.parse_while(),
            Token::Keyword(Keyword::Function) => self.parse_function_decl(),
```

Add a new arm to `parse_primary`'s `match` (from Task 2/3), right after the `Token::Keyword(Keyword::New) => self.parse_new(),` arm:
```rust
            Token::Keyword(Keyword::Function) => self.parse_function_expr(),
```

Add the new methods to the `impl<'a> ParserState<'a>` block:
```rust
    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'while'
        self.expect_punct(Punct::LParen)?;
        let test = self.parse_expression()?;
        self.expect_punct(Punct::RParen)?;
        let body = self.parse_statement()?;
        Ok(Stmt::While { test, body: Box::new(body) })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'for'
        self.expect_punct(Punct::LParen)?;
        let init: Option<Box<Stmt>> = if *self.peek() == Token::Punct(Punct::Semicolon) {
            None
        } else {
            let stmt = match self.peek() {
                Token::Keyword(Keyword::Var) => self.parse_var_decl_clause(DeclKind::Var)?,
                Token::Keyword(Keyword::Let) => self.parse_var_decl_clause(DeclKind::Let)?,
                Token::Keyword(Keyword::Const) => self.parse_var_decl_clause(DeclKind::Const)?,
                _ => Stmt::Expression(self.parse_expression()?),
            };
            Some(Box::new(stmt))
        };
        self.expect_punct(Punct::Semicolon)?;
        let test = if *self.peek() == Token::Punct(Punct::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_punct(Punct::Semicolon)?;
        let update = if *self.peek() == Token::Punct(Punct::RParen) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_punct(Punct::RParen)?;
        let body = self.parse_statement()?;
        Ok(Stmt::For { init, test, update, body: Box::new(body) })
    }

    fn parse_param_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut params = Vec::new();
        while *self.peek() != Token::Punct(Punct::RParen) && *self.peek() != Token::Eof {
            params.push(self.expect_identifier()?);
            if *self.peek() == Token::Punct(Punct::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(params)
    }

    fn parse_function_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect_punct(Punct::LBrace)?;
        let mut body = Vec::new();
        while *self.peek() != Token::Punct(Punct::RBrace) && *self.peek() != Token::Eof {
            body.push(self.parse_statement()?);
        }
        self.expect_punct(Punct::RBrace)?;
        Ok(body)
    }

    fn parse_function_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'function'
        let name = self.expect_identifier()?;
        self.expect_punct(Punct::LParen)?;
        let params = self.parse_param_list()?;
        self.expect_punct(Punct::RParen)?;
        let body = self.parse_function_body()?;
        Ok(Stmt::FunctionDecl { name, params, body })
    }

    fn parse_function_expr(&mut self) -> Result<Expr, ParseError> {
        self.advance(); // 'function'
        let name = if let Token::Identifier(n) = self.peek().clone() {
            self.advance();
            Some(n)
        } else {
            None
        };
        self.expect_punct(Punct::LParen)?;
        let params = self.parse_param_list()?;
        self.expect_punct(Punct::RParen)?;
        let body = self.parse_function_body()?;
        Ok(Expr::Function { name, params, body })
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --package ferris-js`
Expected: PASS (50 tests total).

- [ ] **Step 5: Commit**

```bash
git add ferris-js/src/parser.rs
git commit -m "feat: parse for/while loops and function declarations/expressions"
```

---

### Task 7: Error-path tests, a realistic combined snippet, and test-count reconciliation

**Files:**
- Modify: `ferris-js/src/parser.rs`

**Interfaces:**
- Consumes: everything from Tasks 1-6 (this task adds no new production code, only tests).
- Produces: nothing new for later tasks — this is the last task in this plan.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `ferris-js/src/parser.rs`:
```rust
    #[test]
    fn unclosed_paren_in_a_call_is_a_parse_error() {
        let result = parse("f(1, 2");
        assert!(result.is_err());
    }

    #[test]
    fn unexpected_token_where_an_expression_is_required_is_a_parse_error() {
        let result = parse(") 1;");
        assert!(result.is_err());
    }

    #[test]
    fn eof_in_the_middle_of_a_var_declaration_is_a_parse_error() {
        let result = parse("var a =");
        assert!(result.is_err());
    }

    #[test]
    fn parse_errors_report_a_position_and_never_panic() {
        let err = parse("(").unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[test]
    fn parses_a_realistic_combined_snippet() {
        let src = "
            function sumPositives(items) {
                var total = 0;
                for (var i = 0; i < items.length; i = i + 1) {
                    if (items[i] > 0) {
                        total += items[i];
                    } else {
                        continue;
                    }
                }
                return total;
            }
        ";
        let program = parse(src).unwrap();
        assert_eq!(
            program.body,
            vec![Stmt::FunctionDecl {
                name: "sumPositives".to_string(),
                params: vec!["items".to_string()],
                body: vec![
                    Stmt::VarDecl {
                        kind: DeclKind::Var,
                        declarations: vec![("total".to_string(), Some(Expr::Number(0.0)))],
                    },
                    Stmt::For {
                        init: Some(Box::new(Stmt::VarDecl {
                            kind: DeclKind::Var,
                            declarations: vec![("i".to_string(), Some(Expr::Number(0.0)))],
                        })),
                        test: Some(Expr::Binary {
                            op: BinaryOp::Lt,
                            left: Box::new(Expr::Identifier("i".to_string())),
                            right: Box::new(Expr::Member {
                                object: Box::new(Expr::Identifier("items".to_string())),
                                property: Box::new(Expr::Identifier("length".to_string())),
                                computed: false,
                            }),
                        }),
                        update: Some(Expr::Assignment {
                            op: AssignOp::Assign,
                            target: Box::new(Expr::Identifier("i".to_string())),
                            value: Box::new(Expr::Binary {
                                op: BinaryOp::Add,
                                left: Box::new(Expr::Identifier("i".to_string())),
                                right: Box::new(Expr::Number(1.0)),
                            }),
                        }),
                        body: Box::new(Stmt::Block(vec![Stmt::If {
                            test: Expr::Binary {
                                op: BinaryOp::Gt,
                                left: Box::new(Expr::Member {
                                    object: Box::new(Expr::Identifier("items".to_string())),
                                    property: Box::new(Expr::Identifier("i".to_string())),
                                    computed: true,
                                }),
                                right: Box::new(Expr::Number(0.0)),
                            },
                            consequent: Box::new(Stmt::Block(vec![Stmt::Expression(Expr::Assignment {
                                op: AssignOp::AddAssign,
                                target: Box::new(Expr::Identifier("total".to_string())),
                                value: Box::new(Expr::Member {
                                    object: Box::new(Expr::Identifier("items".to_string())),
                                    property: Box::new(Expr::Identifier("i".to_string())),
                                    computed: true,
                                }),
                            })])),
                            alternate: Some(Box::new(Stmt::Block(vec![Stmt::Continue]))),
                        }])),
                    },
                    Stmt::Return(Some(Expr::Identifier("total".to_string()))),
                ],
            }]
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-js`
Expected: FAIL only if any of Tasks 1-6 has a latent bug the earlier per-feature tests didn't happen to exercise (e.g. a precedence or dispatch edge case only visible when several constructs combine). If all of Tasks 1-6 were implemented as specified, these tests may already PASS on first run — that is an acceptable outcome for this task; proceed to Step 3 either way.

- [ ] **Step 3: Fix any gaps found, or confirm none exist**

If Step 2 failed: read the failure output, locate the specific `ParserState` method responsible (all method names are listed in Tasks 2-6's Interfaces blocks), and fix it there — do not add new methods; this task only closes gaps in existing behavior.

If Step 2 passed: no code change needed.

- [ ] **Step 4: Run the full test suite and reconcile the total**

Run: `cargo test --package ferris-js`
Expected: PASS. Count the total: 12 (Task 1) + 12 (Task 2) + 6 (Task 3) + 7 (Task 4) + 7 (Task 5) + 6 (Task 6) + 5 (Task 7) = 55 tests. If the real total differs, it means an earlier task's stated count in this plan was off by a small amount (this has happened in prior Ferris sub-projects and is not itself a bug) — recount by hand from the actual test output and note the real number in the commit message instead of silently trusting this plan's arithmetic.

Also run the whole workspace to confirm nothing else broke (this crate isn't referenced elsewhere yet, so this is a sanity check, not expected to find anything):
Run: `cargo test --workspace`
Expected: PASS, `ferris-js`'s tests included alongside every other crate's existing passing tests.

- [ ] **Step 5: Commit**

```bash
git add ferris-js/src/parser.rs
git commit -m "test: add ferris-js parse-error and realistic-snippet coverage"
```
