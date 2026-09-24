# HTML Parser → DOM Tree Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a new `ferris-dom` crate (in a converted Cargo workspace alongside `ferris-compositor`) that parses a pragmatic HTML5 subset — elements, attributes, text, comments, arbitrary nesting, void elements — into a navigable `Node` tree, with zero GPU/rendering dependency.

**Architecture:** A `Tokenizer` converts an HTML `&str` into a flat `Vec<Token>`; a `Parser` consumes that token stream with a stack of open elements to build the final `Node` tree. Each stage is independently unit-testable without the other. Two pragmatic error-recovery rules replace the full HTML5 spec algorithm: unclosed tags auto-close at end-of-input, and mismatched closing tags are ignored.

**Tech Stack:** Rust 2021, no external dependencies (pure `std`) — this crate does not touch `wgpu`/`winit`/`glyphon`.

**Spec:** `docs/superpowers/specs/2026-09-23-html-parser-dom-design.md`

## Global Constraints

- New workspace crate `ferris-dom`, zero external dependencies for this sub-project.
- HTML entities (`&amp;`, `&lt;`, etc.) are NOT decoded — stored literal, exactly as written in the source.
- DOCTYPE and `<script>`/`<style>` special-casing are out of scope — `<script>`/`<style>` content is tokenized as ordinary HTML, not raw text.
- **Void elements are always self-closing**, regardless of whether the source has a literal trailing `/>`: `area`, `base`, `br`, `col`, `embed`, `hr`, `img`, `input`, `link`, `meta`, `source`, `track`, `wbr` (case-insensitive match against the tag name). This makes the spec's explicit examples (`<br>`, `<img>`, `<input>`) work correctly even when written without a trailing slash — without this, `<br>` followed by sibling content would incorrectly nest that content inside `<br>` until EOF auto-closed it.
- The root node `Parser::parse` returns is always `Node::Element` with `tag_name` `"document"`, wrapping whatever top-level nodes were parsed as its `children` — this is the concrete choice for the spec's "único item restante na base da pilha" (the one item left at the base of the stack).
- Tag names are stored exactly as written (not lowercased) — only the void-element check is case-insensitive.

## Review Focus

Five input classes the spec implies but no task's tests would catch by accident — each gets an explicit test in the task that owns the code:

- **Attribute value containing a literal `>` character** (e.g. `<div title="a > b">`) — a naive scan for the tag's closing `>` would stop early, inside the quoted value, corrupting the tag and everything after it. Owned by Task 3 (tokenizer must scan quote-aware).
- **Empty input string** (`""`) — must return an empty `Node::Element { tag_name: "document", .. }`, not panic on an empty stack. Owned by Task 4 (parser).
- **Self-closing tag with attributes AND an explicit trailing `/`** (e.g. `<input type="text" />`) — both the attribute parsing and the self-closing detection must work together, not just each in isolation. Owned by Task 3.
- **Case-insensitive void-element matching** (`<BR>`, `<Img>`) — real HTML tag names are case-insensitive; the void-element check must not silently fail on uppercase/mixed-case tags while the stored `tag_name` stays as-written. Owned by Task 3.
- **Text immediately adjacent to a tag with no whitespace** (e.g. `Hello<b>world</b>`) — the text-buffer-flush-on-`<` logic must not swallow, duplicate, or drop the boundary character. Owned by Task 3.

---

### Task 1: Convert to a Cargo workspace, scaffold `ferris-dom`

**Files:**
- Modify: `Cargo.toml` (root — becomes the workspace manifest)
- Move: `Cargo.toml` → `ferris-compositor/Cargo.toml` (the existing package manifest)
- Move: `src/` → `ferris-compositor/src/`
- Create: `ferris-dom/Cargo.toml`
- Create: `ferris-dom/src/lib.rs`

**Interfaces:**
- Consumes: nothing — this is pure repo restructuring, no code logic changes.
- Produces: a working two-member Cargo workspace (`ferris-compositor`, `ferris-dom`). Tasks 2-4 add modules inside `ferris-dom/src/`.

- [ ] **Step 1: Move the existing package into `ferris-compositor/`**

```bash
cd "C:\Users\User\Documents\Ferris"
mkdir ferris-compositor
git mv src ferris-compositor/src
git mv Cargo.toml ferris-compositor/Cargo.toml
```

Note: `Cargo.lock` at the repo root is NOT moved — in a Cargo workspace, the lock file lives at the workspace root and covers every member crate. It will be automatically updated in place once the workspace builds.

- [ ] **Step 2: Create the new root workspace manifest**

Create `Cargo.toml` at the repo root (this is a new file — the old root `Cargo.toml` was just moved away in Step 1):

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom"]
resolver = "2"
```

- [ ] **Step 3: Create the `ferris-dom` crate**

Create `ferris-dom/Cargo.toml`:

```toml
[package]
name = "ferris-dom"
version = "0.1.0"
edition = "2021"

[dependencies]
```

Create `ferris-dom/src/lib.rs`:

```rust
// Modules are added by later tasks in this plan (dom, tokenizer, parser).
```

- [ ] **Step 4: Build and verify no regressions**

Run: `cargo build --workspace`
Expected: compiles clean — both `ferris-compositor` (moved, unchanged code) and `ferris-dom` (new, empty) build successfully. If this fails on `ferris-compositor` specifically, the move in Step 1 likely broke a relative path somewhere — `ferris-compositor`'s own code doesn't reference paths outside itself, so this should just work.

Run: `cargo test --workspace`
Expected: `ferris-compositor`'s existing 24 tests all still pass (unchanged code, just relocated); `ferris-dom` reports 0 tests (empty crate so far).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml ferris-compositor ferris-dom
git commit -m "refactor: convert repo to Cargo workspace, scaffold ferris-dom crate"
```

---

### Task 2: DOM tree types

**Files:**
- Create: `ferris-dom/src/dom.rs`
- Modify: `ferris-dom/src/lib.rs` (add `pub mod dom;`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `dom::Node` (enum: `Element(Element)`, `Text(String)`, `Comment(String)`), `dom::Element { tag_name: String, attributes: HashMap<String, String>, children: Vec<Node> }` with `Element::new(tag_name: impl Into<String>) -> Self`. Used by Task 3's tests (constructing expected trees isn't needed there, but `Node`/`Element` are referenced) and Task 4's `Parser::parse` (builds and returns `Node` trees).

- [ ] **Step 1: Write the failing test**

Create `ferris-dom/src/dom.rs`:

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub tag_name: String,
    pub attributes: HashMap<String, String>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn new(tag_name: impl Into<String>) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_element_has_given_tag_name_and_empty_attributes_and_children() {
        let el = Element::new("div");
        assert_eq!(el.tag_name, "div");
        assert!(el.attributes.is_empty());
        assert!(el.children.is_empty());
    }

    #[test]
    fn new_accepts_both_str_and_string() {
        let from_str = Element::new("p");
        let from_string = Element::new(String::from("p"));
        assert_eq!(from_str.tag_name, from_string.tag_name);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --package ferris-dom dom::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement `Element::new`**

```rust
impl Element {
    pub fn new(tag_name: impl Into<String>) -> Self {
        Self { tag_name: tag_name.into(), attributes: HashMap::new(), children: Vec::new() }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --package ferris-dom dom::tests`
Expected: 2 passed.

- [ ] **Step 5: Register the module**

In `ferris-dom/src/lib.rs`, replace the placeholder comment with:

```rust
pub mod dom;
```

- [ ] **Step 6: Build and commit**

Run: `cargo build --workspace` — expected clean.

```bash
git add ferris-dom/src/dom.rs ferris-dom/src/lib.rs
git commit -m "feat: add DOM tree types (Node, Element)"
```

---

### Task 3: Tokenizer

**Files:**
- Create: `ferris-dom/src/tokenizer.rs`
- Modify: `ferris-dom/src/lib.rs` (add `pub mod tokenizer;`)

**Interfaces:**
- Consumes: nothing new (this module is self-contained — it does not depend on `dom.rs`).
- Produces: `tokenizer::Token` (enum: `TagOpen { name: String, attributes: HashMap<String, String>, self_closing: bool }`, `TagClose { name: String }`, `Text(String)`, `Comment(String)`), `tokenizer::Tokenizer::tokenize(input: &str) -> Vec<Token>`. Used by Task 4's `Parser::parse(tokens: &[Token])` and by the integration tests in Task 4.

This task covers all 5 Review Focus items — each gets its own test below.

- [ ] **Step 1: Write the failing tests**

Create `ferris-dom/src/tokenizer.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-dom tokenizer::tests`
Expected: FAIL to compile / panic on `todo!()` — `Tokenizer::tokenize` isn't implemented yet.

- [ ] **Step 3: Implement `Tokenizer::tokenize`**

Add these free functions above `impl Tokenizer` (they're implementation details, not part of the public interface):

```rust
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
```

Replace the `todo!()` in `Tokenizer::tokenize`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-dom tokenizer::tests`
Expected: 9 passed (5 from Step 1's original set + 4 Review Focus tests).

- [ ] **Step 5: Register the module**

In `ferris-dom/src/lib.rs`, add:

```rust
pub mod tokenizer;
```

- [ ] **Step 6: Build and commit**

Run: `cargo build --workspace` — expected clean. Run: `cargo test --package ferris-dom` — expected 11 passed (2 from `dom.rs` + 9 from `tokenizer.rs`).

```bash
git add ferris-dom/src/tokenizer.rs ferris-dom/src/lib.rs
git commit -m "feat: add HTML tokenizer with quote-aware scanning and void-element handling"
```

---

### Task 4: Parser + integration tests

**Files:**
- Create: `ferris-dom/src/parser.rs`
- Create: `ferris-dom/tests/integration.rs`
- Modify: `ferris-dom/src/lib.rs` (add `pub mod parser;`)

**Interfaces:**
- Consumes: `dom::{Node, Element}` (Task 2), `tokenizer::{Token, Tokenizer}` (Task 3).
- Produces: `parser::Parser::parse(tokens: &[Token]) -> Node`. This is the last task in this plan — nothing downstream depends on further changes here. The integration tests exercise `Tokenizer::tokenize` + `Parser::parse` together, which is the spec's stated success criterion.

- [ ] **Step 1: Write the failing tests**

Create `ferris-dom/src/parser.rs`:

```rust
use crate::dom::{Element, Node};
use crate::tokenizer::Token;

pub struct Parser;

impl Parser {
    pub fn parse(tokens: &[Token]) -> Node {
        todo!()
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-dom parser::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement `Parser::parse`**

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-dom parser::tests`
Expected: 6 passed.

- [ ] **Step 5: Register the module**

In `ferris-dom/src/lib.rs`, add:

```rust
pub mod parser;
```

- [ ] **Step 6: Write the integration tests (spec's stated success criterion)**

Create `ferris-dom/tests/integration.rs`:

```rust
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
```

- [ ] **Step 7: Run all tests to verify everything passes**

Run: `cargo test --package ferris-dom`
Expected: 19 passed total (2 `dom` + 9 `tokenizer` + 6 `parser` + 2 integration).

Run: `cargo test --workspace`
Expected: `ferris-compositor`'s 24 tests + `ferris-dom`'s 19 tests, all passing, nothing broken by the workspace conversion.

- [ ] **Step 8: Commit**

```bash
git add ferris-dom/src/parser.rs ferris-dom/src/lib.rs ferris-dom/tests/integration.rs
git commit -m "feat: add HTML parser with error-recovery rules and integration tests"
```
