use std::sync::{Mutex, OnceLock};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

pub fn line_height(font_size: f32) -> f32 {
    font_size * 1.2
}

fn font_system() -> &'static Mutex<FontSystem> {
    static FONT_SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    FONT_SYSTEM.get_or_init(|| Mutex::new(FontSystem::new()))
}

pub fn wrap_lines(text: &str, font_size: f32, max_width: f32) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let width = if max_width > 0.0 { max_width } else { 1.0 };
    let mut font_system = font_system().lock().expect("font system mutex poisoned");

    let metrics = Metrics::new(font_size, line_height(font_size));
    let mut buffer = Buffer::new(&mut font_system, metrics);
    // No height bound: we want every wrapped line, not just what would fit in
    // a fixed viewport (unlike the renderer's own use of Buffer, which bounds
    // both width and height for on-screen clipping).
    buffer.set_size(&mut font_system, Some(width), None);
    buffer.set_text(&mut font_system, text, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
    buffer.shape_until_scroll(&mut font_system, false);

    let mut lines = Vec::new();
    for run in buffer.layout_runs() {
        if run.glyphs.is_empty() {
            continue;
        }
        let start = run.glyphs.first().unwrap().start;
        let end = run.glyphs.last().unwrap().end;
        lines.push(run.text[start..end].to_string());
    }

    if lines.is_empty() {
        // Defensive fallback: if layout_runs() somehow produced nothing for
        // non-empty input (should not happen per cosmic-text's contract, but
        // this crate never panics or silently drops all content), fall back
        // to the whole text as a single unwrapped line.
        lines.push(text.to_string());
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_height_is_font_size_times_1_2() {
        assert_eq!(line_height(10.0), 12.0);
        assert_eq!(line_height(20.0), 24.0);
    }

    #[test]
    fn short_text_fits_on_one_line() {
        let lines = wrap_lines("hi", 16.0, 800.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "hi");
    }

    #[test]
    fn empty_text_returns_empty_vec() {
        let lines = wrap_lines("", 16.0, 800.0);
        assert!(lines.is_empty());
    }

    #[test]
    fn long_text_wraps_into_multiple_lines_in_a_narrow_box() {
        let text = "this is a long sentence that should not fit on a single line inside a very narrow box";
        let lines = wrap_lines(text, 16.0, 80.0);
        assert!(lines.len() > 1, "expected more than one line, got {}: {:?}", lines.len(), lines);
        // No word should have been silently dropped: rejoining every returned
        // line with a single space between them must reconstruct every word
        // originally present (word order and set preserved, even if the exact
        // whitespace between wrapped lines differs from the source).
        let rejoined: Vec<&str> = lines.iter().flat_map(|l| l.split_whitespace()).collect();
        let original: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(rejoined, original, "wrapping must not drop or reorder words");
    }

    #[test]
    fn single_long_word_does_not_hang_and_returns_at_least_one_line() {
        let long_word = "a".repeat(500);
        let lines = wrap_lines(&long_word, 16.0, 10.0);
        assert!(!lines.is_empty(), "a word wider than max_width must still produce at least one line");
    }

    #[test]
    fn non_positive_max_width_does_not_panic() {
        let lines_zero = wrap_lines("hello world", 16.0, 0.0);
        assert!(!lines_zero.is_empty());
        let lines_negative = wrap_lines("hello world", 16.0, -50.0);
        assert!(!lines_negative.is_empty());
    }
}
