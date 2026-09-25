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
