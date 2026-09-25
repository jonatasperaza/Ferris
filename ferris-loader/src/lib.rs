use std::path::PathBuf;

use ferris_dom::dom::{Element, Node};
use ferris_css::stylesheet::Stylesheet;

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

#[derive(Debug)]
pub enum LoadError {
    Fetch(String),
}

const DEFAULT_STYLESHEET_CSS: &str = "head, style, script, link, meta, title { display: none; }";

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
}
