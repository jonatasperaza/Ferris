# CSS Parser → Stylesheet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a new `ferris-css` crate (third member of the existing Cargo workspace) that parses a pragmatic CSS subset — type/class/id/attribute selectors, the four standard combinators, opaque property/value declarations, and `@media`-nested rules — into a navigable `Stylesheet` structure, with zero GPU/DOM dependency.

**Architecture:** A context-free `Tokenizer` converts a CSS `&str` into a flat `Vec<Token>` (the same token stream regardless of whether it's currently inside a selector or a declaration value — interpretation is entirely the parser's job, mirroring how the real CSS Syntax spec tokenizes). A `Parser` consumes that stream to build `Stylesheet`/`Rule`/`Selector`/`Declaration` trees, reconstructing opaque declaration values and `@media` conditions by concatenating token text, and implementing one pragmatic error-recovery rule (a malformed rule is dropped whole, without derailing the rest of the sheet) in place of the full CSS spec's error-recovery algorithm.

**Tech Stack:** Rust 2021, no external dependencies (pure `std`) — matches `ferris-dom`'s pattern exactly.

**Spec:** `docs/superpowers/specs/2026-09-24-css-parser-stylesheet-design.md`

## Global Constraints

- New workspace crate `ferris-css`, zero external dependencies.
- Selector scope: type, class (`.foo`), id (`#foo`), attribute (`[attr]` / `[attr=value]`), combined with descendant (whitespace), child (`>`), next-sibling (`+`), and subsequent-sibling (`~`) combinators.
- Declaration `property` and `value` are both opaque `String`s — no validation of known property names, no structured/typed values.
- `@media (condition) { nested rules }` is supported one level deep; nested `@media` inside `@media`, and any at-rule other than `@media`, is out of scope (an unknown at-rule's keyword token is simply skipped, producing no `Rule`).
- Error recovery: a malformed rule (unclosed brace running to EOF, or a declaration missing its `:`) is dropped whole; the parser resumes at the next rule boundary and keeps going. This is the one pragmatic rule — not the full CSS spec's error-recovery algorithm.
- No HTML entity/CSS escape decoding, no pseudo-classes/pseudo-elements/functional selectors, no specificity, no selector-to-DOM matching, no cascade — all explicitly out of scope per the spec.

## Review Focus

Five input classes the spec implies but no task's tests would catch by accident — each gets an explicit test in the task that owns the code (all fall naturally under Task 4, where `Parser::parse`'s core selector/declaration behavior is first established and tested):

- **A quoted attribute-selector value containing characters that look structural** (e.g. `[data-x="a=b"]`) — the tokenizer already keeps quoted strings atomic, but the parser must use the `Str` token's content as-is, not re-parse it for `=`/`]`. Owned by Task 4.
- **Whitespace around the selector-list comma** (e.g. `h1 , h2`) must NOT be misread as a descendant combinator on either side. Owned by Task 4.
- **An empty declaration block** (`div {}`, no declarations at all) must produce a `StyleRule` with an empty `declarations` list, not an error or a panic. Owned by Task 4.
- **A semicolon inside a quoted declaration value** (e.g. `content: "a;b";`) must not prematurely terminate the declaration — the tokenizer's atomic `Str` token already protects this, but it needs an explicit regression test at the declaration-parsing level. Owned by Task 4.
- **Empty or whitespace-only input** must produce `Stylesheet { rules: vec![] }` without panicking. Owned by Task 4.

---

### Task 1: Add `ferris-css` as the third workspace member

**Files:**
- Modify: `Cargo.toml` (root workspace manifest)
- Create: `ferris-css/Cargo.toml`
- Create: `ferris-css/src/lib.rs`

**Interfaces:**
- Consumes: nothing — this is pure scaffolding, no code logic.
- Produces: a working three-member Cargo workspace (`ferris-compositor`, `ferris-dom`, `ferris-css`). Tasks 2-5 add modules inside `ferris-css/src/`.

- [ ] **Step 1: Update the root workspace manifest**

Read the current root `Cargo.toml` first to confirm its exact contents, then change the `members` list to add `"ferris-css"`:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css"]
resolver = "2"
```

- [ ] **Step 2: Create the `ferris-css` crate**

Create `ferris-css/Cargo.toml`:

```toml
[package]
name = "ferris-css"
version = "0.1.0"
edition = "2021"

[dependencies]
```

Create `ferris-css/src/lib.rs`:

```rust
// Modules are added by later tasks in this plan (stylesheet, tokenizer, parser).
```

- [ ] **Step 3: Build and verify no regressions**

Run: `cargo build --workspace`
Expected: compiles clean — `ferris-compositor`, `ferris-dom` (unchanged), and the new empty `ferris-css` all build.

Run: `cargo test --workspace`
Expected: `ferris-compositor`'s 24 tests and `ferris-dom`'s 22 tests still pass, unchanged; `ferris-css` reports 0 tests (empty crate so far).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml ferris-css
git commit -m "feat: add ferris-css crate as third workspace member"
```

---

### Task 2: Stylesheet data types

**Files:**
- Create: `ferris-css/src/stylesheet.rs`
- Modify: `ferris-css/src/lib.rs` (add `pub mod stylesheet;`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `stylesheet::{Stylesheet, Rule, StyleRule, MediaRule, Declaration, Selector, SelectorComponent, SimpleSelector, AttributeSelector, Combinator}` — the complete type vocabulary Tasks 3-5 build and populate. Exact field names below are load-bearing for every later task.

- [ ] **Step 1: Write the failing tests**

Create `ferris-css/src/stylesheet.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    Style(StyleRule),
    Media(MediaRule),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaRule {
    pub condition: String,
    pub rules: Vec<StyleRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub property: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Selector {
    pub components: Vec<SelectorComponent>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectorComponent {
    Simple(SimpleSelector),
    Combinator(Combinator),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimpleSelector {
    pub type_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<AttributeSelector>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSelector {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Combinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stylesheet_has_no_rules() {
        assert_eq!(Stylesheet::default().rules, Vec::new());
    }

    #[test]
    fn default_style_rule_has_no_selectors_or_declarations() {
        let rule = StyleRule::default();
        assert!(rule.selectors.is_empty());
        assert!(rule.declarations.is_empty());
    }

    #[test]
    fn default_simple_selector_has_no_type_id_classes_or_attributes() {
        let selector = SimpleSelector::default();
        assert_eq!(selector.type_name, None);
        assert_eq!(selector.id, None);
        assert!(selector.classes.is_empty());
        assert!(selector.attributes.is_empty());
    }
}
```

This file has no `todo!()` — all these types are pure data with derived behavior, nothing to implement beyond the struct/enum definitions themselves, so there's no meaningful RED step for the types. The 3 tests below still need to pass, which is your GREEN signal.

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --package ferris-css stylesheet::tests`
Expected: 3 passed.

- [ ] **Step 3: Register the module**

In `ferris-css/src/lib.rs`, replace the placeholder comment with:

```rust
pub mod stylesheet;
```

- [ ] **Step 4: Build and commit**

Run: `cargo build --workspace` — expected clean.

```bash
git add ferris-css/src/stylesheet.rs ferris-css/src/lib.rs
git commit -m "feat: add stylesheet data types (Stylesheet, Rule, Selector, Declaration, etc.)"
```

---

### Task 3: Tokenizer

**Files:**
- Create: `ferris-css/src/tokenizer.rs`
- Modify: `ferris-css/src/lib.rs` (add `pub mod tokenizer;`)

**Interfaces:**
- Consumes: nothing new (self-contained, no dependency on `stylesheet.rs`).
- Produces: `tokenizer::Token` (enum: `Ident(String)`, `Hash(String)`, `Str(String)`, `Delim(char)`, `Comma`, `Colon`, `Semicolon`, `OpenBrace`, `CloseBrace`, `AtKeyword(String)`, `Whitespace`), `tokenizer::Tokenizer::tokenize(input: &str) -> Vec<Token>`. Used by Tasks 4-5's `Parser::parse(tokens: &[Token])` and the integration tests.

- [ ] **Step 1: Write the failing tests**

Create `ferris-css/src/tokenizer.rs`:

```rust
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
        todo!()
    }
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-css tokenizer::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement `Tokenizer::tokenize`**

Replace the `todo!()`:

```rust
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
```

Note: `.` is deliberately NOT an ident character — it must tokenize as `Delim('.')` so the parser can recognize `.classname` as a class selector. This means a numeric value like `1.5` tokenizes as three tokens (`Ident("1")`, `Delim('.')`, `Ident("5")`) rather than one — this is fine, because declaration values are reconstructed from raw token text with no gap where there was no `Whitespace` token between them, so `1.5` round-trips correctly as a string. You'll see this reconstruction logic in Task 4.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-css tokenizer::tests`
Expected: 11 passed.

- [ ] **Step 5: Register the module**

In `ferris-css/src/lib.rs`, add:

```rust
pub mod tokenizer;
```

- [ ] **Step 6: Build and commit**

Run: `cargo build --workspace` — expected clean. Run: `cargo test --package ferris-css` — expected 14 passed (3 from `stylesheet.rs` + 11 from `tokenizer.rs`).

```bash
git add ferris-css/src/tokenizer.rs ferris-css/src/lib.rs
git commit -m "feat: add CSS tokenizer"
```

---

### Task 4: Parser — selectors and declarations (well-formed input)

**Files:**
- Create: `ferris-css/src/parser.rs`
- Modify: `ferris-css/src/lib.rs` (add `pub mod parser;`)

**Interfaces:**
- Consumes: `stylesheet::{Stylesheet, Rule, StyleRule, Selector, SelectorComponent, SimpleSelector, AttributeSelector, Combinator, Declaration}` (Task 2), `tokenizer::{Token, Tokenizer}` (Task 3).
- Produces: `parser::Parser::parse(tokens: &[Token]) -> Stylesheet`. Also produces (as private, non-`pub` helpers other code in this file relies on, not exposed outside `parser.rs`) `parse_style_rule(tokens: &[Token], i: usize) -> (StyleRule, usize)`, `parse_selector_list`, `parse_selector`, `parse_simple_selector`, `parse_declarations`, `reconstruct_value`, `skip_whitespace`, `is_simple_selector_start`. Task 5 changes `parse_style_rule`'s signature to return `Option<(StyleRule, usize)>` and rewrites `Parser::parse` to add `@media` handling and error recovery — this task's version assumes well-formed input only (no `@media`, no malformed rules).

- [ ] **Step 1: Write the failing tests**

Create `ferris-css/src/parser.rs`:

```rust
use crate::stylesheet::{
    AttributeSelector, Combinator, Declaration, Rule, Selector, SelectorComponent,
    SimpleSelector, StyleRule, Stylesheet,
};
use crate::tokenizer::Token;

pub struct Parser;

impl Parser {
    pub fn parse(tokens: &[Token]) -> Stylesheet {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    fn tokens_for(css: &str) -> Vec<Token> {
        Tokenizer::tokenize(css)
    }

    #[test]
    fn parses_type_selector_with_one_declaration() {
        let tokens = tokens_for("div { color: red; }");
        let sheet = Parser::parse(&tokens);
        assert_eq!(sheet.rules.len(), 1);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.selectors.len(), 1);
        assert_eq!(
            rule.selectors[0].components,
            vec![SelectorComponent::Simple(SimpleSelector {
                type_name: Some("div".to_string()),
                ..Default::default()
            })]
        );
        assert_eq!(
            rule.declarations,
            vec![Declaration { property: "color".to_string(), value: "red".to_string() }]
        );
    }

    #[test]
    fn parses_composite_simple_selector_type_class_id_attribute() {
        let tokens = tokens_for(r#"div.foo#bar[data-x=1] { width: 10px; }"#);
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        let SelectorComponent::Simple(simple) = &rule.selectors[0].components[0] else {
            panic!("expected simple selector")
        };
        assert_eq!(simple.type_name, Some("div".to_string()));
        assert_eq!(simple.id, Some("bar".to_string()));
        assert_eq!(simple.classes, vec!["foo".to_string()]);
        assert_eq!(
            simple.attributes,
            vec![AttributeSelector { name: "data-x".to_string(), value: Some("1".to_string()) }]
        );
    }

    #[test]
    fn parses_selector_list_separated_by_comma() {
        let tokens = tokens_for("h1, h2 { margin: 0; }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.selectors.len(), 2);
    }

    #[test]
    fn parses_each_combinator() {
        let descendant = Parser::parse(&tokens_for("div p { color: red; }"));
        let Rule::Style(r) = &descendant.rules[0] else { panic!("expected style rule") };
        assert!(matches!(r.selectors[0].components[1], SelectorComponent::Combinator(Combinator::Descendant)));

        let child = Parser::parse(&tokens_for("div > p { color: red; }"));
        let Rule::Style(r) = &child.rules[0] else { panic!("expected style rule") };
        assert!(matches!(r.selectors[0].components[1], SelectorComponent::Combinator(Combinator::Child)));

        let next_sibling = Parser::parse(&tokens_for("div + p { color: red; }"));
        let Rule::Style(r) = &next_sibling.rules[0] else { panic!("expected style rule") };
        assert!(matches!(r.selectors[0].components[1], SelectorComponent::Combinator(Combinator::NextSibling)));

        let subsequent = Parser::parse(&tokens_for("div ~ p { color: red; }"));
        let Rule::Style(r) = &subsequent.rules[0] else { panic!("expected style rule") };
        assert!(matches!(r.selectors[0].components[1], SelectorComponent::Combinator(Combinator::SubsequentSibling)));
    }

    #[test]
    fn reconstructs_multi_word_comma_value() {
        let tokens = tokens_for("p { font-family: Arial, sans-serif; }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.declarations[0].value, "Arial, sans-serif");
    }

    #[test]
    fn parses_declaration_without_trailing_semicolon_before_close_brace() {
        let tokens = tokens_for("p { color: red }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.declarations[0].value, "red");
    }

    #[test]
    fn parses_multiple_declarations() {
        let tokens = tokens_for("p { color: red; width: 10px; }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.declarations.len(), 2);
    }

    // --- Review Focus: quoted attribute-selector value with structural-looking characters ---
    #[test]
    fn attribute_selector_quoted_value_with_equals_and_bracket_like_chars() {
        let tokens = tokens_for(r#"[data-x="a=b]c"] { color: red; }"#);
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        let SelectorComponent::Simple(simple) = &rule.selectors[0].components[0] else {
            panic!("expected simple selector")
        };
        assert_eq!(
            simple.attributes,
            vec![AttributeSelector { name: "data-x".to_string(), value: Some("a=b]c".to_string()) }]
        );
    }

    // --- Review Focus: whitespace around the selector-list comma isn't a descendant combinator ---
    #[test]
    fn whitespace_around_selector_list_comma_is_not_a_descendant_combinator() {
        let tokens = tokens_for("h1 , h2 { margin: 0; }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.selectors.len(), 2);
        assert_eq!(rule.selectors[0].components.len(), 1);
        assert_eq!(rule.selectors[1].components.len(), 1);
    }

    // --- Review Focus: empty declaration block ---
    #[test]
    fn empty_declaration_block_yields_empty_declarations() {
        let tokens = tokens_for("div {}");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert!(rule.declarations.is_empty());
    }

    // --- Review Focus: semicolon inside a quoted value doesn't end the declaration early ---
    #[test]
    fn semicolon_inside_quoted_value_does_not_end_declaration_early() {
        let tokens = tokens_for(r#"p { content: "a;b"; color: red; }"#);
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.declarations.len(), 2);
        assert_eq!(rule.declarations[0].value, "\"a;b\"");
        assert_eq!(rule.declarations[1].value, "red");
    }

    // --- Review Focus: empty/whitespace-only input ---
    #[test]
    fn empty_input_yields_empty_stylesheet() {
        assert_eq!(Parser::parse(&tokens_for("")), Stylesheet::default());
        assert_eq!(Parser::parse(&tokens_for("   \n  ")), Stylesheet::default());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-css parser::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement the parser**

Replace the `todo!()` and add the helper functions below `impl Parser`:

```rust
impl Parser {
    pub fn parse(tokens: &[Token]) -> Stylesheet {
        let mut rules = Vec::new();
        let mut i = 0;
        loop {
            skip_whitespace(tokens, &mut i);
            if i >= tokens.len() {
                break;
            }
            let (rule, next) = parse_style_rule(tokens, i);
            rules.push(Rule::Style(rule));
            i = next;
        }
        Stylesheet { rules }
    }
}

fn skip_whitespace(tokens: &[Token], i: &mut usize) {
    while *i < tokens.len() && tokens[*i] == Token::Whitespace {
        *i += 1;
    }
}

fn is_simple_selector_start(tokens: &[Token], i: usize) -> bool {
    matches!(
        tokens.get(i),
        Some(Token::Ident(_)) | Some(Token::Hash(_)) | Some(Token::Delim('.')) | Some(Token::Delim('['))
    )
}

fn parse_style_rule(tokens: &[Token], mut i: usize) -> (StyleRule, usize) {
    let (selectors, next) = parse_selector_list(tokens, i);
    i = next;
    skip_whitespace(tokens, &mut i);
    // Task 4 assumes an OpenBrace is here (well-formed input only);
    // Task 5 replaces this function to check and handle malformed input.
    i += 1; // consume {
    let (declarations, next) = parse_declarations(tokens, i);
    i = next;
    if tokens.get(i) == Some(&Token::CloseBrace) {
        i += 1;
    }
    (StyleRule { selectors, declarations }, i)
}

fn parse_selector_list(tokens: &[Token], mut i: usize) -> (Vec<Selector>, usize) {
    let mut selectors = Vec::new();
    loop {
        let (selector, next) = parse_selector(tokens, i);
        selectors.push(selector);
        i = next;
        skip_whitespace(tokens, &mut i);
        if i < tokens.len() && tokens[i] == Token::Comma {
            i += 1;
            skip_whitespace(tokens, &mut i);
        } else {
            break;
        }
    }
    (selectors, i)
}

fn parse_selector(tokens: &[Token], mut i: usize) -> (Selector, usize) {
    let mut components = Vec::new();
    skip_whitespace(tokens, &mut i);
    let (simple, next) = parse_simple_selector(tokens, i);
    components.push(SelectorComponent::Simple(simple));
    i = next;

    loop {
        let had_whitespace = i < tokens.len() && tokens[i] == Token::Whitespace;
        let mut j = i;
        if had_whitespace {
            skip_whitespace(tokens, &mut j);
        }

        let combinator = match tokens.get(j) {
            Some(Token::Delim('>')) => Some(Combinator::Child),
            Some(Token::Delim('+')) => Some(Combinator::NextSibling),
            Some(Token::Delim('~')) => Some(Combinator::SubsequentSibling),
            _ => None,
        };

        if let Some(combinator) = combinator {
            j += 1;
            skip_whitespace(tokens, &mut j);
            if !is_simple_selector_start(tokens, j) {
                break;
            }
            components.push(SelectorComponent::Combinator(combinator));
            let (simple, next) = parse_simple_selector(tokens, j);
            components.push(SelectorComponent::Simple(simple));
            i = next;
            continue;
        }

        if had_whitespace && is_simple_selector_start(tokens, j) {
            components.push(SelectorComponent::Combinator(Combinator::Descendant));
            let (simple, next) = parse_simple_selector(tokens, j);
            components.push(SelectorComponent::Simple(simple));
            i = next;
            continue;
        }

        break;
    }

    (Selector { components }, i)
}

fn parse_simple_selector(tokens: &[Token], mut i: usize) -> (SimpleSelector, usize) {
    let mut selector = SimpleSelector::default();

    if let Some(Token::Ident(name)) = tokens.get(i) {
        selector.type_name = Some(name.clone());
        i += 1;
    }

    loop {
        match tokens.get(i) {
            Some(Token::Hash(id)) => {
                selector.id = Some(id.clone());
                i += 1;
            }
            Some(Token::Delim('.')) => {
                if let Some(Token::Ident(class)) = tokens.get(i + 1) {
                    selector.classes.push(class.clone());
                    i += 2;
                } else {
                    break;
                }
            }
            Some(Token::Delim('[')) => {
                if let Some(Token::Ident(attr_name)) = tokens.get(i + 1) {
                    let mut j = i + 2;
                    let mut value = None;
                    if tokens.get(j) == Some(&Token::Delim('=')) {
                        j += 1;
                        match tokens.get(j) {
                            Some(Token::Ident(v)) => {
                                value = Some(v.clone());
                                j += 1;
                            }
                            Some(Token::Str(v)) => {
                                value = Some(v.clone());
                                j += 1;
                            }
                            _ => {}
                        }
                    }
                    if tokens.get(j) == Some(&Token::Delim(']')) {
                        selector.attributes.push(AttributeSelector { name: attr_name.clone(), value });
                        i = j + 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    (selector, i)
}

fn parse_declarations(tokens: &[Token], mut i: usize) -> (Vec<Declaration>, usize) {
    let mut declarations = Vec::new();
    loop {
        skip_whitespace(tokens, &mut i);
        if i >= tokens.len() || tokens[i] == Token::CloseBrace {
            break;
        }
        let property = if let Some(Token::Ident(name)) = tokens.get(i) {
            i += 1;
            name.clone()
        } else {
            break;
        };
        skip_whitespace(tokens, &mut i);
        if tokens.get(i) != Some(&Token::Colon) {
            break;
        }
        i += 1;
        skip_whitespace(tokens, &mut i);

        let value_start = i;
        while i < tokens.len() && tokens[i] != Token::Semicolon && tokens[i] != Token::CloseBrace {
            i += 1;
        }
        let value = reconstruct_value(&tokens[value_start..i]);
        declarations.push(Declaration { property, value });

        if i < tokens.len() && tokens[i] == Token::Semicolon {
            i += 1;
        }
    }
    (declarations, i)
}

fn reconstruct_value(tokens: &[Token]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for token in tokens {
        match token {
            Token::Whitespace => parts.push(" ".to_string()),
            Token::Ident(s) => parts.push(s.clone()),
            Token::Str(s) => parts.push(format!("\"{s}\"")),
            Token::Hash(s) => parts.push(format!("#{s}")),
            Token::Comma => parts.push(",".to_string()),
            Token::Delim(c) => parts.push(c.to_string()),
            Token::Colon => parts.push(":".to_string()),
            Token::AtKeyword(s) => parts.push(format!("@{s}")),
            Token::OpenBrace => parts.push("{".to_string()),
            Token::CloseBrace => parts.push("}".to_string()),
            Token::Semicolon => parts.push(";".to_string()),
        }
    }
    parts.concat().trim().to_string()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-css parser::tests`
Expected: 12 passed.

- [ ] **Step 5: Register the module**

In `ferris-css/src/lib.rs`, add:

```rust
pub mod parser;
```

- [ ] **Step 6: Build and commit**

Run: `cargo build --workspace` — expected clean. Run: `cargo test --package ferris-css` — expected 26 passed (3 `stylesheet` + 11 `tokenizer` + 12 `parser`).

```bash
git add ferris-css/src/parser.rs ferris-css/src/lib.rs
git commit -m "feat: add CSS parser for selectors and declarations"
```

---

### Task 5: `@media` nesting, error recovery, and integration tests

**Files:**
- Modify: `ferris-css/src/parser.rs` (`parse_style_rule` signature changes, `Parser::parse` rewritten, new `skip_to_next_boundary` helper)
- Create: `ferris-css/tests/integration.rs`

**Interfaces:**
- Consumes: everything from Task 4, plus `stylesheet::MediaRule` (Task 2).
- Produces: `parser::Parser::parse(tokens: &[Token]) -> Stylesheet` (same public signature as Task 4 — only its internals and `parse_style_rule`'s now-private signature change). This is the last task in this plan.

- [ ] **Step 1: Write the failing tests**

Add these two tests to the `#[cfg(test)] mod tests { ... }` block at the bottom of `ferris-css/src/parser.rs` (alongside the 12 from Task 4 — don't remove any of them):

```rust
    #[test]
    fn parses_media_rule_with_nested_style_rules() {
        let tokens = tokens_for("@media (min-width: 600px) { p { color: red; } }");
        let sheet = Parser::parse(&tokens);
        assert_eq!(sheet.rules.len(), 1);
        let Rule::Media(media) = &sheet.rules[0] else { panic!("expected media rule") };
        assert_eq!(media.condition, "(min-width: 600px)");
        assert_eq!(media.rules.len(), 1);
        assert_eq!(media.rules[0].declarations[0].value, "red");
    }

    #[test]
    fn malformed_rule_is_skipped_without_breaking_the_rest_of_the_sheet() {
        // "div { color red; }" is missing the ':' after "color" — malformed.
        // The parser must drop it whole and still parse the well-formed "p" rule that follows.
        let tokens = tokens_for("div { color red; } p { color: blue; }");
        let sheet = Parser::parse(&tokens);
        let style_rules: Vec<&StyleRule> = sheet.rules.iter().filter_map(|r| match r {
            Rule::Style(s) => Some(s),
            Rule::Media(_) => None,
        }).collect();
        assert_eq!(style_rules.len(), 1, "expected only the well-formed p rule to survive");
        assert_eq!(style_rules[0].declarations, vec![Declaration { property: "color".to_string(), value: "blue".to_string() }]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-css parser::tests::parses_media_rule_with_nested_style_rules parser::tests::malformed_rule_is_skipped_without_breaking_the_rest_of_the_sheet`
Expected: FAIL — `@media` isn't recognized yet (falls through to `parse_style_rule` as if `@media` were a selector, producing wrong/garbage output or a panic), and the malformed-rule test fails because Task 4's `parse_style_rule` has no recovery — it will produce incorrect structure or panic on the unexpected token layout.

- [ ] **Step 3: Replace `parse_style_rule` and `Parser::parse` with error-recovery-aware versions**

In `ferris-css/src/parser.rs`, replace the entire `impl Parser { ... }` block AND the existing `fn parse_style_rule(...)` function with the following (everything else in the file — `skip_whitespace`, `is_simple_selector_start`, `parse_selector_list`, `parse_selector`, `parse_simple_selector`, `parse_declarations`, `reconstruct_value` — stays exactly as Task 4 left it):

```rust
impl Parser {
    pub fn parse(tokens: &[Token]) -> Stylesheet {
        let mut rules = Vec::new();
        let mut i = 0;
        loop {
            skip_whitespace(tokens, &mut i);
            if i >= tokens.len() {
                break;
            }
            let rule_start = i;

            if let Some(Token::AtKeyword(name)) = tokens.get(i) {
                if name == "media" {
                    i += 1;
                    skip_whitespace(tokens, &mut i);
                    let condition_start = i;
                    while i < tokens.len() && tokens[i] != Token::OpenBrace {
                        i += 1;
                    }
                    let condition = reconstruct_value(&tokens[condition_start..i]);
                    if i < tokens.len() {
                        i += 1; // consume {
                    }
                    let mut media_rules = Vec::new();
                    loop {
                        skip_whitespace(tokens, &mut i);
                        if i >= tokens.len() || tokens[i] == Token::CloseBrace {
                            break;
                        }
                        let nested_start = i;
                        match parse_style_rule(tokens, i) {
                            Some((rule, next)) => {
                                media_rules.push(rule);
                                i = next;
                            }
                            None => {
                                i = skip_to_next_boundary(tokens, nested_start);
                            }
                        }
                    }
                    if i < tokens.len() && tokens[i] == Token::CloseBrace {
                        i += 1; // consume outer }
                    }
                    rules.push(Rule::Media(MediaRule { condition, rules: media_rules }));
                    continue;
                } else {
                    // Unknown at-rule: out of scope, skip just the keyword and move on.
                    i += 1;
                    continue;
                }
            }

            match parse_style_rule(tokens, i) {
                Some((rule, next)) => {
                    rules.push(Rule::Style(rule));
                    i = next;
                }
                None => {
                    i = skip_to_next_boundary(tokens, rule_start);
                }
            }
        }
        Stylesheet { rules }
    }
}

fn parse_style_rule(tokens: &[Token], mut i: usize) -> Option<(StyleRule, usize)> {
    let (selectors, next) = parse_selector_list(tokens, i);
    i = next;
    skip_whitespace(tokens, &mut i);
    if tokens.get(i) != Some(&Token::OpenBrace) {
        return None;
    }
    i += 1;
    let (declarations, next) = parse_declarations(tokens, i);
    i = next;
    if tokens.get(i) != Some(&Token::CloseBrace) {
        return None;
    }
    i += 1;
    Some((StyleRule { selectors, declarations }, i))
}

/// Scans forward from `start` (the beginning of a rule that failed to parse), tracking
/// brace depth, and returns the index just past the `}` that closes the first `{` opened
/// at this level — or `tokens.len()` if no such closing brace exists before the end of
/// input. This is the spec's "malformed rule dropped whole, resume at the next boundary"
/// error-recovery rule.
fn skip_to_next_boundary(tokens: &[Token], start: usize) -> usize {
    let mut i = start;
    let mut depth: i32 = 0;
    while i < tokens.len() {
        match tokens[i] {
            Token::OpenBrace => depth += 1,
            Token::CloseBrace => {
                depth -= 1;
                i += 1;
                if depth <= 0 {
                    return i;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    i
}
```

You'll also need to add `MediaRule` to the `use crate::stylesheet::{...}` import list at the top of the file (it was not needed by Task 4, since Task 4 never constructed one).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-css parser::tests`
Expected: 14 passed (the 12 from Task 4 plus these 2).

- [ ] **Step 5: Write the integration tests (spec's stated success criterion)**

Create `ferris-css/tests/integration.rs`:

```rust
use ferris_css::parser::Parser;
use ferris_css::stylesheet::{Combinator, Rule, SelectorComponent, Stylesheet};
use ferris_css::tokenizer::Tokenizer;

fn parse_css(css: &str) -> Stylesheet {
    let tokens = Tokenizer::tokenize(css);
    Parser::parse(&tokens)
}

#[test]
fn parses_realistic_stylesheet_with_composite_selector_and_combinator() {
    let css = r#".card > h2, #hero { color: navy; font-family: Arial, sans-serif; }"#;
    let sheet = parse_css(css);
    assert_eq!(sheet.rules.len(), 1);
    let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
    assert_eq!(rule.selectors.len(), 2);
    assert_eq!(rule.selectors[0].components.len(), 3); // .card, >, h2
    assert!(matches!(
        rule.selectors[0].components[1],
        SelectorComponent::Combinator(Combinator::Child)
    ));
    assert_eq!(rule.declarations.len(), 2);
    assert_eq!(rule.declarations[1].value, "Arial, sans-serif");
}

#[test]
fn parses_stylesheet_with_media_query_and_malformed_rule_recovery() {
    let css = r#"
        div { color red; }
        p { color: blue; }
        @media (max-width: 480px) {
            .nav { display: none; }
        }
    "#;
    let sheet = parse_css(css);
    let style_rules: Vec<_> = sheet
        .rules
        .iter()
        .filter(|r| matches!(r, Rule::Style(_)))
        .collect();
    let media_rules: Vec<_> = sheet
        .rules
        .iter()
        .filter(|r| matches!(r, Rule::Media(_)))
        .collect();
    // The malformed "div { color red; }" (missing ':') is dropped; p and @media survive.
    assert_eq!(style_rules.len(), 1);
    assert_eq!(media_rules.len(), 1);
    let Rule::Media(media) = media_rules[0] else { unreachable!() };
    assert_eq!(media.condition, "(max-width: 480px)");
    assert_eq!(media.rules.len(), 1);
    assert_eq!(media.rules[0].declarations[0].value, "none");
}
```

- [ ] **Step 6: Run all tests to verify everything passes**

Run: `cargo test --package ferris-css`
Expected: 30 passed total (3 `stylesheet` + 11 `tokenizer` + 14 `parser` + 2 integration).

Run: `cargo test --workspace`
Expected: `ferris-compositor` (24) + `ferris-dom` (22) + `ferris-css` (30) = 76 passing, nothing broken by adding the new crate.

- [ ] **Step 7: Commit**

```bash
git add ferris-css/src/parser.rs ferris-css/tests/integration.rs
git commit -m "feat: add @media nesting, error recovery, and integration tests to CSS parser"
```
