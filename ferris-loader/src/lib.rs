use std::io::Read;
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

fn fetch_bytes(source: &Source) -> Result<Vec<u8>, LoadError> {
    match source {
        Source::File(path) => std::fs::read(path).map_err(|e| LoadError::Fetch(format!("{}: {e}", path.display()))),
        Source::Url(url) => {
            let response = ureq::get(url).call().map_err(|e| LoadError::Fetch(format!("{url}: {e}")))?;
            let mut buf = Vec::new();
            response.into_reader().read_to_end(&mut buf).map_err(|e| LoadError::Fetch(format!("{url}: {e}")))?;
            Ok(buf)
        }
    }
}

/// Decodes a `data:image/...;base64,<...>` URI directly, with no fetch at
/// all. Returns `None` for anything that doesn't parse as base64 after the
/// `base64,` marker — malformed `data:` URIs degrade the same as any other
/// fetch failure: the image is skipped, not propagated as an error.
fn decode_data_uri(src: &str) -> Option<Vec<u8>> {
    let (_, b64) = src.split_once("base64,")?;
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

fn extract_images(root: &Element, base: &Source) -> ferris_scene::ImageMap {
    let mut images = ferris_scene::ImageMap::new();
    collect_images(root, base, &mut images);
    images
}

fn collect_images(el: &Element, base: &Source, images: &mut ferris_scene::ImageMap) {
    if el.tag_name == "img" {
        if let Some(src) = el.attributes.get("src") {
            if !images.contains_key(src) {
                let bytes = if src.starts_with("data:") {
                    decode_data_uri(src)
                } else {
                    resolve_href(base, src).and_then(|resolved| fetch_bytes(&resolved).ok())
                };
                if let Some(bytes) = bytes {
                    if let Ok(decoded) = image::load_from_memory(&bytes) {
                        let rgba_image = decoded.to_rgba8();
                        let (width, height) = (rgba_image.width(), rgba_image.height());
                        images.insert(
                            src.clone(),
                            ferris_scene::DecodedImage { width, height, rgba: std::sync::Arc::from(rgba_image.into_raw()) },
                        );
                    }
                }
            }
        }
    }

    for child in &el.children {
        if let Node::Element(child_el) = child {
            collect_images(child_el, base, images);
        }
    }
}

pub fn load_page(source: &Source) -> Result<(Element, Stylesheet, ferris_scene::ImageMap), LoadError> {
    let html = fetch_text(source)?;
    let root = ferris_dom::parser::parse_document(&html);

    let mut css_sources = vec![DEFAULT_STYLESHEET_CSS.to_string()];
    css_sources.extend(extract_stylesheets(&root, source));
    let combined_css = css_sources.join("\n");

    let css_tokens = ferris_css::tokenizer::Tokenizer::tokenize(&combined_css);
    let stylesheet = ferris_css::parser::Parser::parse(&css_tokens);

    let images = extract_images(&root, source);

    Ok((root, stylesheet, images))
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
        let (root, stylesheet, _images) = load_page(&source).unwrap();
        assert_eq!(root.tag_name, "html");
        assert_eq!(stylesheet.rules.len(), 2, "expected the default UA rule plus the page's one rule");
    }

    fn write_temp_file_bytes(name: &str, contents: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join("ferris_loader_tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn tiny_png_bytes(r: u8, g: u8, b: u8) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([r, g, b, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn extract_images_decodes_a_local_png_file() {
        let png_path = write_temp_file_bytes("image_task2_a.png", &tiny_png_bytes(200, 0, 0));
        let html_path = write_temp_file("page_task2_a.html", "");
        let html = format!(r#"<html><body><img src="{}"></body></html>"#, png_path.file_name().unwrap().to_str().unwrap());
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(html_path);

        let images = extract_images(&root, &base);

        let decoded = images.get(png_path.file_name().unwrap().to_str().unwrap()).expect("image must be present");
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.rgba.len(), 2 * 2 * 4);
    }

    #[test]
    fn extract_images_decodes_a_data_uri_without_any_fetch() {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(tiny_png_bytes(0, 200, 0));
        let html = format!(r#"<html><body><img src="data:image/png;base64,{b64}"></body></html>"#);
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(std::path::PathBuf::from("/irrelevant/page.html"));

        let images = extract_images(&root, &base);

        let key = format!("data:image/png;base64,{b64}");
        let decoded = images.get(&key).expect("data: URI image must be present");
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
    }

    #[test]
    fn extract_images_skips_a_malformed_data_uri_without_panicking() {
        let html = r#"<html><body><img src="data:image/png;base64,not-valid-base64!!!"></body></html>"#;
        let root = ferris_dom::parser::parse_document(html);
        let base = Source::File(std::path::PathBuf::from("/irrelevant/page.html"));

        let images = extract_images(&root, &base);

        assert!(images.is_empty(), "a malformed data: URI must be skipped, same as any other decode failure");
    }

    #[test]
    fn extract_images_skips_a_missing_file_without_propagating_error() {
        let html_path = write_temp_file("page_task2_c.html", "");
        let html = r#"<html><body><img src="does-not-exist-anywhere.png"><p>still here</p></body></html>"#;
        let root = ferris_dom::parser::parse_document(html);
        let base = Source::File(html_path);

        let images = extract_images(&root, &base);

        assert!(images.is_empty(), "a missing image file must be skipped, not panic or propagate an error");
    }

    #[test]
    fn extract_images_skips_bytes_that_fail_to_decode() {
        let bad_path = write_temp_file("image_task2_bad.png", "this is not a real png file");
        let html_path = write_temp_file("page_task2_d.html", "");
        let html = format!(r#"<html><body><img src="{}"></body></html>"#, bad_path.file_name().unwrap().to_str().unwrap());
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(html_path);

        let images = extract_images(&root, &base);

        assert!(images.is_empty(), "corrupted image bytes must be skipped, not panic");
    }

    #[test]
    fn extract_images_ignores_an_img_with_no_src_attribute() {
        let html = r#"<html><body><img></body></html>"#;
        let root = ferris_dom::parser::parse_document(html);
        let base = Source::File(std::path::PathBuf::from("/irrelevant/page.html"));

        let images = extract_images(&root, &base);

        assert!(images.is_empty());
    }

    #[test]
    fn extract_images_deduplicates_two_tags_with_the_same_src() {
        let png_path = write_temp_file_bytes("image_task2_e.png", &tiny_png_bytes(0, 0, 200));
        let html_path = write_temp_file("page_task2_e.html", "");
        let file_name = png_path.file_name().unwrap().to_str().unwrap();
        let html = format!(r#"<html><body><img src="{file_name}"><img src="{file_name}"></body></html>"#);
        let root = ferris_dom::parser::parse_document(&html);
        let base = Source::File(html_path);

        let images = extract_images(&root, &base);

        assert_eq!(images.len(), 1, "two tags with the same src must produce exactly one map entry");
    }

    #[test]
    fn load_page_returns_an_image_map_alongside_the_element_and_stylesheet() {
        let png_path = write_temp_file_bytes("image_task2_f.png", &tiny_png_bytes(10, 20, 30));
        let html_path = write_temp_file(
            "page_task2_f.html",
            &format!(r#"<html><body><img src="{}"></body></html>"#, png_path.file_name().unwrap().to_str().unwrap()),
        );
        let source = Source::File(html_path);

        let (root, _stylesheet, images) = load_page(&source).unwrap();

        assert_eq!(root.tag_name, "html");
        assert_eq!(images.len(), 1);
    }
}
