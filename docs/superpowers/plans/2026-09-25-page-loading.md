# Page Loading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load a real page — a local file or an `http(s)://` URL — instead of the compiled-in fixture, extracting CSS from `<link rel="stylesheet">` and `<style>` tags and applying a small user-agent default stylesheet that hides metadata tags (`head`/`style`/`script`/`link`/`meta`/`title`) from ever rendering as visible text.

**Architecture:** A new crate `ferris-loader` fetches HTML (file or HTTP via `ureq`), parses it via a new additive `ferris_dom::parser::parse_document` helper, walks the tree collecting stylesheet sources in document order (resolving relative `href`s via the `url` crate for URLs or `Path::join` for files, skipping any single `<link>` that fails to fetch), and hands back `(Element, Stylesheet)` ready for the existing `resolve_styles` → `layout` → `paint` pipeline. `ferris-compositor/src/main.rs` uses it when a command-line argument is given, falling back to the embedded fixture otherwise.

**Tech Stack:** Rust, `ureq` (blocking HTTP client, no async runtime), `url` (URL parsing/joining), Cargo workspace path dependencies.

**Spec:** `docs/superpowers/specs/2026-09-25-page-loading-design.md`

## Global Constraints

- New crate `ferris-loader`, ninth workspace member, depends on `ferris-dom`, `ferris-css`, `ferris-style` (path) plus `ureq` and `url` (external, both new to this workspace).
- `ferris_dom::parser::parse_document(html: &str) -> Element` is additive to the already-published `ferris-dom` crate — never panics; empty or tagless input returns an empty `Element::new("html")`.
- `ferris_loader::Source` is `File(PathBuf) | Url(String)`; `parse_source(arg: &str) -> Source` treats an `http://`/`https://`-prefixed argument as a URL, anything else as a file path.
- `ferris_loader::LoadError::Fetch(String)` wraps any I/O/HTTP failure as a formatted message — never exposes `ureq`'s or `std::io`'s error types in the public API.
- The default user-agent stylesheet (`"head, style, script, link, meta, title { display: none; }"`) is always the FIRST source in the combined CSS, before any of the page's own `<link>`/`<style>` content, so page rules can still override it if they explicitly want to (though none of the current pipeline exercises that).
- Stylesheet sources are collected in DOCUMENT ORDER (a DFS pre-order walk of the parsed tree) — this matters for CSS cascade source-order tie-breaking (already established behavior since piece 2.3).
- A `<link rel="stylesheet">` that fails to resolve or fetch is skipped silently (not propagated as an error) — only the main page's own fetch failure is fatal.
- `ferris-compositor/src/main.rs`: no CLI argument → unchanged fixture-embedding behavior (backward compatible); an argument → `ferris_loader::{parse_source, load_page}`, with a fatal error logged and the process exited on `Err`.

## Review Focus

- An HTML file with no `<html>`/no element content at all (empty file, or a file containing only free text) must not panic anywhere in the load path — `parse_document` returning an empty synthetic `<html>` element is the documented behavior; covered by Task 1's `parse_document_on_empty_input_returns_empty_html_element` and `parse_document_on_text_only_input_does_not_panic`.
- A `<link rel="stylesheet" href="...">` pointing at a file that doesn't exist (typo, moved file) must not crash the whole page load — only that one stylesheet is skipped; covered by Task 3's `extract_stylesheets_skips_link_that_fails_to_resolve_or_fetch`.
- Multiple `<link>`/`<style>` sources in a real document must be concatenated in the SAME order they appear in the source (not, say, all `<link>`s first then all `<style>`s) — covered by Task 3's `extract_stylesheets_preserves_document_order_style_then_link`.
- A `<link rel="STYLESHEET">` (uppercase, or any case variant) must still be recognized as a stylesheet link — real-world HTML is inconsistently cased; covered by Task 3's `extract_stylesheets_collects_link_tag_via_local_file` using an uppercase `rel` attribute value.
- The default user-agent stylesheet must actually suppress `<style>`'s own CSS text from rendering as literal page text when run through the real end-to-end pipeline (the bug this piece was specifically designed to close) — covered by Task 4's manual verification (a fixture/test page whose `<style>` block's raw text would be visibly wrong if not hidden).

---

### Task 1: `ferris_dom::parser::parse_document`

**Files:**
- Modify: `ferris-dom/src/parser.rs`

**Interfaces:**
- Consumes: `crate::tokenizer::Tokenizer::tokenize` and `Parser::parse` (already exist, unmodified).
- Produces: `pub fn parse_document(html: &str) -> Element`, used by Tasks 3 and 4.

- [ ] **Step 1: Read the current file before editing**

Read `ferris-dom/src/parser.rs` in full — this task only ADDS a new public function and its tests; `Parser::parse` and its own 6 existing tests are untouched.

- [ ] **Step 2: Write the failing tests**

Add these to the existing `#[cfg(test)] mod tests` block:

```rust
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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --package ferris-dom parser:: -- --nocapture`
Expected: compile error (`parse_document` not found).

- [ ] **Step 4: Implement `parse_document`**

Add near the top of `ferris-dom/src/parser.rs`, alongside the existing `use` lines, `use crate::tokenizer::Tokenizer;` if not already imported (check first — `parser.rs` currently imports `crate::tokenizer::Token`, not `Tokenizer` itself). Then add the function, outside `impl Parser` (as a free function in the module, after the `impl Parser` block):

```rust
/// Tokenizes and parses `html`, then unwraps the synthetic "document" root
/// `Parser::parse` always wraps everything in, returning the first real
/// element found — or an empty `<html>` element if there is none (empty
/// input, or input with only free text and no tags). Never panics.
pub fn parse_document(html: &str) -> Element {
    let tokens = Tokenizer::tokenize(html);
    let Node::Element(document) = Parser::parse(&tokens) else {
        return Element::new("html");
    };
    document
        .children
        .into_iter()
        .find_map(|child| match child {
            Node::Element(el) => Some(el),
            Node::Text(_) | Node::Comment(_) => None,
        })
        .unwrap_or_else(|| Element::new("html"))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --package ferris-dom parser::`
Expected: 9 passed (6 pre-existing + 3 new).

- [ ] **Step 6: Run the whole crate and workspace test suites**

Run: `cargo test --package ferris-dom`
Expected: 25 passed (9 parser + 10 tokenizer + 2 dom + 4 integration).

Run: `cargo test --workspace`
Expected: workspace baseline before this plan is 227 (post-2.7, confirmed via `git log`/the pushed state) + 3 = 230.

- [ ] **Step 7: Commit**

```bash
git add ferris-dom/src/parser.rs
git commit -m "feat: add parse_document, a tokenize+parse+unwrap-root convenience for ferris-dom"
```

---

### Task 2: `ferris-loader` scaffold — `Source`, `parse_source`, `resolve_href`

**Files:**
- Create: `ferris-loader/Cargo.toml`
- Create: `ferris-loader/src/lib.rs`

**Interfaces:**
- Consumes: nothing (first task for this crate, pure logic — no file I/O, no network calls yet).
- Produces: `pub enum Source { File(PathBuf), Url(String) }`, `pub fn parse_source(arg: &str) -> Source`, and a private `fn resolve_href(base: &Source, href: &str) -> Option<Source>`, all used by Task 3.

- [ ] **Step 1: Add `ferris-loader` to the workspace**

Read `Cargo.toml` at the repo root first to confirm the current `members` list, then add `"ferris-loader"`. As of this plan it reads:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style", "ferris-layout", "ferris-paint", "ferris-scene", "ferris-text"]
resolver = "2"
```

Change to:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style", "ferris-layout", "ferris-paint", "ferris-scene", "ferris-text", "ferris-loader"]
resolver = "2"
```

- [ ] **Step 2: Create the crate manifest**

Create `ferris-loader/Cargo.toml`:

```toml
[package]
name = "ferris-loader"
version = "0.1.0"
edition = "2021"

[dependencies]
ferris-dom = { path = "../ferris-dom" }
ferris-css = { path = "../ferris-css" }
ferris-style = { path = "../ferris-style" }
ureq = "2"
url = "2"
```

- [ ] **Step 3: Write the failing tests**

Create `ferris-loader/src/lib.rs`:

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    File(PathBuf),
    Url(String),
}

pub fn parse_source(arg: &str) -> Source {
    if arg.starts_with("http://") || arg.starts_with("https://") {
        Source::Url(arg.to_string())
    } else {
        Source::File(PathBuf::from(arg))
    }
}

fn resolve_href(base: &Source, href: &str) -> Option<Source> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_source_recognizes_http_url() {
        assert_eq!(parse_source("http://example.com/page.html"), Source::Url("http://example.com/page.html".to_string()));
    }

    #[test]
    fn parse_source_recognizes_https_url() {
        assert_eq!(parse_source("https://example.com/page.html"), Source::Url("https://example.com/page.html".to_string()));
    }

    #[test]
    fn parse_source_treats_relative_path_as_file() {
        assert_eq!(parse_source("page.html"), Source::File(PathBuf::from("page.html")));
    }

    #[test]
    fn parse_source_treats_absolute_path_as_file() {
        assert_eq!(parse_source("/home/user/page.html"), Source::File(PathBuf::from("/home/user/page.html")));
    }

    #[test]
    fn resolve_href_file_base_joins_relative_path() {
        let base = Source::File(PathBuf::from("/pages/index.html"));
        let resolved = resolve_href(&base, "style.css").unwrap();
        assert_eq!(resolved, Source::File(PathBuf::from("/pages/style.css")));
    }

    #[test]
    fn resolve_href_url_base_joins_relative_path() {
        let base = Source::Url("https://example.com/pages/index.html".to_string());
        let resolved = resolve_href(&base, "style.css").unwrap();
        assert_eq!(resolved, Source::Url("https://example.com/pages/style.css".to_string()));
    }

    #[test]
    fn resolve_href_url_base_absolute_href_uses_the_absolute_target() {
        let base = Source::Url("https://example.com/pages/index.html".to_string());
        let resolved = resolve_href(&base, "https://cdn.other.com/style.css").unwrap();
        assert_eq!(resolved, Source::Url("https://cdn.other.com/style.css".to_string()));
    }

    #[test]
    fn resolve_href_url_base_malformed_href_returns_none() {
        let base = Source::Url("not a valid url at all".to_string());
        assert!(resolve_href(&base, "style.css").is_none());
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --package ferris-loader -- --nocapture`
Expected: compile error or panic on `todo!()`.

- [ ] **Step 5: Implement `resolve_href`**

```rust
fn resolve_href(base: &Source, href: &str) -> Option<Source> {
    match base {
        Source::File(path) => {
            let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            Some(Source::File(dir.join(href)))
        }
        Source::Url(base_url) => {
            let parsed_base = url::Url::parse(base_url).ok()?;
            let joined = parsed_base.join(href).ok()?;
            Some(Source::Url(joined.to_string()))
        }
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --package ferris-loader`
Expected: 8 passed.

- [ ] **Step 7: Build the whole workspace to confirm the new member compiles cleanly**

Run: `cargo build --workspace`
Expected: clean build, no warnings, `ferris-loader` now listed among the built packages (this is the first build that pulls in `ureq`/`url` and their own dependency trees — expect a longer build the first time).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml ferris-loader/Cargo.toml ferris-loader/src/lib.rs Cargo.lock
git commit -m "feat: add ferris-loader crate with Source/parse_source/resolve_href"
```

---

### Task 3: Fetching and stylesheet extraction — `load_page`

**Files:**
- Modify: `ferris-loader/src/lib.rs`

**Interfaces:**
- Consumes: `Source`, `resolve_href` (Task 2); `ferris_dom::parser::parse_document` (Task 1); `ferris_dom::dom::{Element, Node}`; `ferris_css::{tokenizer::Tokenizer, parser::Parser, stylesheet::Stylesheet}`.
- Produces: `pub enum LoadError { Fetch(String) }`, `pub fn load_page(source: &Source) -> Result<(Element, Stylesheet), LoadError>`, used by Task 4.

- [ ] **Step 1: Read the current file before editing**

Read `ferris-loader/src/lib.rs` in full (from Task 2) before adding to it.

- [ ] **Step 2: Write the failing tests**

Add these imports near the top of `ferris-loader/src/lib.rs` (alongside the existing `use std::path::PathBuf;`):

```rust
use ferris_dom::dom::{Element, Node};
use ferris_css::stylesheet::Stylesheet;
```

Add the `LoadError` type and the (initially `todo!()`) function stubs, after `resolve_href`:

```rust
#[derive(Debug)]
pub enum LoadError {
    Fetch(String),
}

const DEFAULT_STYLESHEET_CSS: &str = "head, style, script, link, meta, title { display: none; }";

fn fetch_text(source: &Source) -> Result<String, LoadError> {
    todo!()
}

fn extract_stylesheets(root: &Element, base: &Source) -> Vec<String> {
    todo!()
}

pub fn load_page(source: &Source) -> Result<(Element, Stylesheet), LoadError> {
    todo!()
}
```

Add these tests inside the existing `#[cfg(test)] mod tests` block. These tests use real temporary files on disk (via `std::env::temp_dir()` plus a unique subdirectory per test, cleaned up isn't required — this matches the project's existing `%TEMP%`-based scratch conventions) rather than mocking `ureq`, since this crate has no existing mocking infrastructure and file-based `Source::File` testing exercises the exact same code paths as a URL would, minus the network call itself (which Task 4's manual verification covers):

```rust
    fn write_temp_file(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("ferris_loader_tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn extract_stylesheets_collects_style_tag_content() {
        let root = ferris_dom::parser::parse_document("<html><head><style>body { color: red; }</style></head></html>");
        let base = Source::File(PathBuf::from("/irrelevant/page.html"));
        let sheets = extract_stylesheets(&root, &base);
        assert_eq!(sheets, vec!["body { color: red; }".to_string()]);
    }

    #[test]
    fn extract_stylesheets_collects_link_tag_via_local_file() {
        let css_path = write_temp_file("linked_task3_a.css", "p { color: blue; }");
        let html_path = write_temp_file("page_task3_a.html", "");
        let html = format!(
            r#"<html><head><link rel="STYLESHEET" href="{}"></head></html>"#,
            css_path.file_name().unwrap().to_str().unwrap()
        );
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(html_path);
        let sheets = extract_stylesheets(&root, &base);
        assert_eq!(sheets, vec!["p { color: blue; }".to_string()], "rel=\"STYLESHEET\" (uppercase) must still be recognized");
    }

    #[test]
    fn extract_stylesheets_preserves_document_order_style_then_link() {
        let css_path = write_temp_file("linked_task3_b.css", "SECOND");
        let html_path = write_temp_file("page_task3_b.html", "");
        let html = format!(
            r#"<html><head><style>FIRST</style><link rel="stylesheet" href="{}"></head></html>"#,
            css_path.file_name().unwrap().to_str().unwrap()
        );
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(html_path);
        let sheets = extract_stylesheets(&root, &base);
        assert_eq!(sheets, vec!["FIRST".to_string(), "SECOND".to_string()]);
    }

    #[test]
    fn extract_stylesheets_skips_link_that_fails_to_resolve_or_fetch() {
        let html_path = write_temp_file("page_task3_c.html", "");
        let html = r#"<html><head><style>KEEP</style><link rel="stylesheet" href="does-not-exist-anywhere.css"></head></html>"#;
        let root = ferris_dom::parser::parse_document(html);
        let base = Source::File(html_path);
        let sheets = extract_stylesheets(&root, &base);
        assert_eq!(sheets, vec!["KEEP".to_string()], "a failing link must be skipped, not propagate an error or stop collection");
    }

    #[test]
    fn load_page_local_file_prepends_default_stylesheet_and_appends_page_css() {
        let html_path = write_temp_file("page_task3_d.html", "<html><head><style>body { color: red; }</style></head><p>hi</p></html>");
        let source = Source::File(html_path);
        let (root, stylesheet) = load_page(&source).unwrap();
        assert_eq!(root.tag_name, "html");
        assert_eq!(stylesheet.rules.len(), 2, "expected the default UA rule plus the page's one rule");
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --package ferris-loader -- --nocapture`
Expected: compile error or panic on `todo!()`.

- [ ] **Step 4: Implement `fetch_text`, `extract_stylesheets`, `load_page`**

```rust
fn fetch_text(source: &Source) -> Result<String, LoadError> {
    match source {
        Source::File(path) => std::fs::read_to_string(path).map_err(|e| LoadError::Fetch(format!("{}: {e}", path.display()))),
        Source::Url(url) => {
            let response = ureq::get(url).call().map_err(|e| LoadError::Fetch(format!("{url}: {e}")))?;
            response.into_string().map_err(|e| LoadError::Fetch(format!("{url}: {e}")))
        }
    }
}

fn extract_stylesheets(root: &Element, base: &Source) -> Vec<String> {
    let mut sheets = Vec::new();
    collect_stylesheets(root, base, &mut sheets);
    sheets
}

fn collect_stylesheets(el: &Element, base: &Source, sheets: &mut Vec<String>) {
    if el.tag_name == "style" {
        let text: String = el
            .children
            .iter()
            .filter_map(|c| match c {
                Node::Text(s) => Some(s.as_str()),
                Node::Comment(_) | Node::Element(_) => None,
            })
            .collect();
        if !text.trim().is_empty() {
            sheets.push(text);
        }
    } else if el.tag_name == "link" {
        let is_stylesheet = el
            .attributes
            .get("rel")
            .map(|r| r.eq_ignore_ascii_case("stylesheet"))
            .unwrap_or(false);
        if is_stylesheet {
            if let Some(href) = el.attributes.get("href") {
                if let Some(resolved) = resolve_href(base, href) {
                    if let Ok(css) = fetch_text(&resolved) {
                        sheets.push(css);
                    }
                    // A link that fails to resolve or fetch is skipped
                    // silently, per this crate's documented resilience
                    // contract — only the main page's own fetch is fatal.
                }
            }
        }
    }

    for child in &el.children {
        if let Node::Element(child_el) = child {
            collect_stylesheets(child_el, base, sheets);
        }
    }
}

pub fn load_page(source: &Source) -> Result<(Element, Stylesheet), LoadError> {
    let html = fetch_text(source)?;
    let root = ferris_dom::parser::parse_document(&html);

    let mut css_sources = vec![DEFAULT_STYLESHEET_CSS.to_string()];
    css_sources.extend(extract_stylesheets(&root, source));
    let combined_css = css_sources.join("\n");

    let css_tokens = ferris_css::tokenizer::Tokenizer::tokenize(&combined_css);
    let stylesheet = ferris_css::parser::Parser::parse(&css_tokens);

    Ok((root, stylesheet))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --package ferris-loader`
Expected: 13 passed (8 pre-existing + 5 new).

- [ ] **Step 6: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: 230 (from Task 1) + 8 (Task 2's `ferris-loader` tests — Task 2 only checked `cargo build --workspace`, not a workspace test count, so this is the first workspace-level checkpoint that includes them) + 5 (this task's new tests) = 243.

- [ ] **Step 7: Commit**

```bash
git add ferris-loader/src/lib.rs
git commit -m "feat: implement load_page — fetch, stylesheet extraction, and default UA stylesheet"
```

---

### Task 4: Wire `ferris-loader` into `ferris-compositor` + mandatory manual verification

**Files:**
- Modify: `ferris-compositor/Cargo.toml`
- Modify: `ferris-compositor/src/main.rs`

**Interfaces:**
- Consumes: `ferris_loader::{Source, LoadError, parse_source, load_page}` (Tasks 2-3); `ferris_dom::parser::parse_document` (Task 1) for the fixture fallback path.
- Produces: nothing consumed by later tasks — this is the final task of the plan.

- [ ] **Step 1: Add the `ferris-loader` dependency**

Read `ferris-compositor/Cargo.toml` first to confirm its current `[dependencies]` block, then add:

```toml
ferris-loader = { path = "../ferris-loader" }
```

- [ ] **Step 2: Read the current `main.rs` before editing**

Read `ferris-compositor/src/main.rs` in full. This task: (a) adds a `source: Option<ferris_loader::Source>` field to `App`, (b) changes `build_page_frame` to take an already-parsed `&Element`/`&Stylesheet` instead of doing its own fixture-specific parsing, (c) moves the fixture-parsing logic into a fallback branch inside `resumed()`, (d) parses a CLI argument in `main()`.

- [ ] **Step 3: Add the `source` field to `App`**

Change:
```rust
struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Renderer>,
    frame_timer: perf::FrameTimer,
    last_frame_start: Option<std::time::Instant>,
    occluded: bool,
    page_frame: Option<scene::Frame>,
}
```
to:
```rust
struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Renderer>,
    frame_timer: perf::FrameTimer,
    last_frame_start: Option<std::time::Instant>,
    occluded: bool,
    page_frame: Option<scene::Frame>,
    source: Option<ferris_loader::Source>,
}
```
and add `source: None,` to `impl Default for App`'s struct literal (alongside `page_frame: None,`).

- [ ] **Step 4: Change `build_page_frame`'s signature**

Replace the entire current `build_page_frame` function:
```rust
fn build_page_frame(viewport_width: f32, viewport_height: f32) -> scene::Frame {
    let html = include_str!("../assets/fixture.html");
    let css = include_str!("../assets/fixture.css");

    let html_tokens = DomTokenizer::tokenize(html);
    let Node::Element(document) = DomParser::parse(&html_tokens) else {
        panic!("fixture.html: expected a document element");
    };
    let root = document
        .children
        .into_iter()
        .find_map(|child| match child {
            Node::Element(el) => Some(el),
            Node::Text(_) | Node::Comment(_) => None,
        })
        .expect("fixture.html: expected at least one root element");

    let css_tokens = CssTokenizer::tokenize(css);
    let stylesheet = CssParser::parse(&css_tokens);

    let styled = resolve_styles(&root, &stylesheet);
    let layout_box = layout::layout(&styled, viewport_width, viewport_height)
        .expect("fixture.html's root must not be display:none");

    paint(&layout_box)
}
```
with:
```rust
fn build_page_frame(root: &ferris_dom::dom::Element, stylesheet: &ferris_css::stylesheet::Stylesheet, viewport_width: f32, viewport_height: f32) -> scene::Frame {
    let styled = resolve_styles(root, stylesheet);
    let layout_box = layout::layout(&styled, viewport_width, viewport_height)
        .expect("page root must not be display:none");
    paint(&layout_box)
}
```

- [ ] **Step 5: Update the imports**

The old `DomTokenizer`/`DomParser`/`Node` imports (`use ferris_dom::dom::Node;`, `use ferris_dom::parser::Parser as DomParser;`, `use ferris_dom::tokenizer::Tokenizer as DomTokenizer;`) are no longer used anywhere in `main.rs` once Step 6 below moves the fixture-parsing logic to use `parse_document` instead — remove those three `use` lines. `CssTokenizer`/`CssParser` imports stay (still used for the fixture fallback's CSS parsing in Step 6).

- [ ] **Step 6: Determine `(root, stylesheet)` in `resumed`, replacing the old direct `build_page_frame` call**

In `App::resumed`, replace:
```rust
            Some(gpu) => {
                self.page_frame = Some(build_page_frame(gpu.logical_width(), gpu.logical_height()));
                self.gpu = Some(gpu);
                self.window = Some(window);
            }
```
with:
```rust
            Some(gpu) => {
                let loaded = match &self.source {
                    Some(source) => match ferris_loader::load_page(source) {
                        Ok(page) => Some(page),
                        Err(ferris_loader::LoadError::Fetch(msg)) => {
                            log::error!("failed to load page: {msg}");
                            None
                        }
                    },
                    None => {
                        let html = include_str!("../assets/fixture.html");
                        let css = include_str!("../assets/fixture.css");
                        let root = ferris_dom::parser::parse_document(html);
                        let css_tokens = CssTokenizer::tokenize(css);
                        let stylesheet = CssParser::parse(&css_tokens);
                        Some((root, stylesheet))
                    }
                };

                let Some((root, stylesheet)) = loaded else {
                    event_loop.exit();
                    return;
                };

                self.page_frame = Some(build_page_frame(&root, &stylesheet, gpu.logical_width(), gpu.logical_height()));
                self.gpu = Some(gpu);
                self.window = Some(window);
            }
```

- [ ] **Step 7: Parse the CLI argument in `main`**

Replace:
```rust
fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("event loop error");
}
```
with:
```rust
fn main() {
    env_logger::init();
    let source = std::env::args().nth(1).map(|arg| ferris_loader::parse_source(&arg));
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { source, ..App::default() };
    event_loop.run_app(&mut app).expect("event loop error");
}
```

- [ ] **Step 8: Build and run the whole workspace**

Run: `cargo build --workspace`
Expected: clean build, no warnings.

Run: `cargo test --workspace`
Expected: 243/243 (unchanged from Task 3 — this task adds no new automated tests, per the spec's documented testing approach for GUI code).

- [ ] **Step 9: Manual verification — run without an argument (fixture fallback still works)**

Run: `cargo run --release -p ferris-compositor` (no argument)

Confirm the window opens and shows the exact same fixture page pieces 2.5-2.7 already verified (white background, navy header, wrapped paragraphs, colored inline text) — this confirms the fallback path wasn't broken by this task's refactor.

- [ ] **Step 10: Manual verification — run with a local file argument**

Create a small standalone test page somewhere outside the repo's tracked files (e.g. in your scratchpad), for instance:
```html
<html>
<head>
<style>#box { background-color: #336699; color: white; }</style>
</head>
<body>
<div id="box">Loaded from a real file, not the embedded fixture.</div>
</body>
</html>
```
Run: `cargo run --release -p ferris-compositor -- <path to that file>`

Confirm, by looking at the window (or via the screenshot technique already established in pieces 2.4-2.7):
- The page's own content renders (the blue box with white text).
- Nothing from `<style>`'s own CSS text ("`#box { background-color: ...`") appears anywhere as visible page text — this is the direct proof the default user-agent stylesheet's `display:none` on `style` is actually working end to end, the bug this whole piece exists to close.

- [ ] **Step 11: Manual verification — run with a real URL**

Run: `cargo run --release -p ferris-compositor -- https://example.com` (or any other simple, real, publicly reachable page).

Confirm the window opens and shows SOMETHING derived from that real page's content (exact rendering fidelity isn't the bar here — this project's CSS/layout support is still a pragmatic subset — the bar is: the HTTP fetch succeeds, real HTML from a real server gets parsed and something appears on screen, no panic, no crash). If the process exits with a fetch error, investigate whether it's a real bug in `fetch_text`'s URL branch or an environment issue (no network access, corporate proxy, etc.) before concluding this task is done — same discipline as every prior manual-verification step in this project: if the binary doesn't actually run successfully, say so explicitly, don't claim success.

Close the window (or kill the process) once confirmed. Note in the task's completion report exactly what was seen at each of Steps 9-11 — a task reviewer for this task must ask the implementer to describe what appeared for all three runs, and treat an implementer that skipped any of them as not having completed the task's actual deliverable, same discipline as pieces 2.4-2.7.

- [ ] **Step 12: Commit**

```bash
git add ferris-compositor/Cargo.toml ferris-compositor/src/main.rs
git commit -m "feat: load a real page from a file or URL argument, falling back to the embedded fixture"
```

---
