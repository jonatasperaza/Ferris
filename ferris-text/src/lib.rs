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
    // Recover from a poisoned lock rather than propagating the poison: a
    // std::sync::Mutex only marks itself poisoned as a precaution when a
    // previous holder panicked mid-access, but it never corrupts the data it
    // guards. Using `.into_inner()` on the poison error recovers the (still
    // valid) FontSystem instead of cascading a panic in *every* subsequent
    // caller for the rest of the process, which is what `.expect()` here
    // used to do after any single panicking call.
    let mut font_system = match font_system().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

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
        // Use min/max over all glyphs rather than first()/last(): cosmic-text
        // returns glyphs in VISUAL (display) order, not logical (source
        // text) order, when a line contains bidi-reordered text (e.g. RTL
        // Hebrew/Arabic mixed with LTR text). first()/last() would then pick
        // the wrong byte range, causing either a panic (start > end) or
        // silent truncation of the run's text.
        let start = run.glyphs.iter().map(|g| g.start).min().unwrap();
        let end = run.glyphs.iter().map(|g| g.end).max().unwrap();
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

    // Regression tests for C1: bidi (RTL/LTR mixed) text caused wrap_lines to
    // compute wrapped-line byte spans from run.glyphs.first()/last(), which
    // assumes glyphs are returned in logical (source-text) order. cosmic-text
    // actually returns glyphs in VISUAL order after bidi reordering, so that
    // assumption broke in two ways: a panic in narrow boxes where the
    // "first" visual glyph's start byte was greater than the "last" visual
    // glyph's end byte, and silent text loss in wide boxes where the byte
    // range simply picked up the wrong (too-small) span.

    #[test]
    fn narrow_box_with_mixed_rtl_text_does_not_panic() {
        // Before the fix, this reproduced a real panic in the GPU pipeline:
        // "byte range starts at X but ends at Y" from run.text[start..end],
        // because cosmic-text returns glyphs in visual (not logical) order
        // once bidi reordering kicks in for the Hebrew span.
        let lines = wrap_lines("Say שלום to everyone.", 16.0, 30.0);
        assert!(!lines.is_empty(), "must produce at least one line without panicking");
    }

    #[test]
    fn wide_box_with_mixed_rtl_text_does_not_lose_words() {
        // Before the fix, this did not panic (no start > end byte range at
        // this width) but silently dropped the Hebrew word entirely:
        // "hello world שלום" wrapped down to just "hello world ש".
        let lines = wrap_lines("hello world שלום", 16.0, 800.0);
        let joined = lines.join(" ");
        assert!(joined.contains("hello"), "expected 'hello' to survive wrapping, got {:?}", lines);
        assert!(joined.contains("world"), "expected 'world' to survive wrapping, got {:?}", lines);
        assert!(joined.contains("שלום"), "expected the full Hebrew word 'שלום' to survive wrapping without truncation, got {:?}", lines);
    }

    #[test]
    fn font_system_mutex_recovers_after_a_problematic_call() {
        // Even in case some other bidi edge case still manages to panic
        // while the font system mutex is held, a later plain-ASCII call
        // must keep working rather than panicking forever because the
        // mutex was left poisoned. This exercises the RTL case first (which
        // used to poison the mutex on panic pre-fix) and then confirms a
        // completely unrelated, normal call still succeeds afterward.
        let _ = wrap_lines("Say שלום to everyone.", 16.0, 30.0);
        let lines = wrap_lines("hello world", 16.0, 800.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "hello world");
    }
}
