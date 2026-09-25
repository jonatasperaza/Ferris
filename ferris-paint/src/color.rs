pub fn parse_color(value: Option<&str>) -> Option<[f32; 4]> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(hex) = raw.strip_prefix('#') {
        return parse_hex(hex);
    }

    parse_keyword(raw)
}

fn parse_hex(hex: &str) -> Option<[f32; 4]> {
    let expanded: String = match hex.chars().count() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.to_string(),
        _ => return None,
    };

    if !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let r = u8::from_str_radix(&expanded[0..2], 16).ok()?;
    let g = u8::from_str_radix(&expanded[2..4], 16).ok()?;
    let b = u8::from_str_radix(&expanded[4..6], 16).ok()?;

    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
}

fn parse_keyword(raw: &str) -> Option<[f32; 4]> {
    let rgb = match raw.to_ascii_lowercase().as_str() {
        "black" => [0, 0, 0],
        "silver" => [192, 192, 192],
        "gray" => [128, 128, 128],
        "white" => [255, 255, 255],
        "maroon" => [128, 0, 0],
        "red" => [255, 0, 0],
        "purple" => [128, 0, 128],
        "fuchsia" => [255, 0, 255],
        "green" => [0, 128, 0],
        "lime" => [0, 255, 0],
        "olive" => [128, 128, 0],
        "yellow" => [255, 255, 0],
        "navy" => [0, 0, 128],
        "blue" => [0, 0, 255],
        "teal" => [0, 128, 128],
        "aqua" => [0, 255, 255],
        "orange" => [255, 165, 0],
        "transparent" => return Some([0.0, 0.0, 0.0, 0.0]),
        _ => return None,
    };
    Some([rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, 1.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_value_is_none() {
        assert_eq!(parse_color(None), None);
    }

    #[test]
    fn empty_string_is_none() {
        assert_eq!(parse_color(Some("")), None);
    }

    #[test]
    fn parses_six_digit_hex() {
        assert_eq!(parse_color(Some("#ff0000")), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color(Some("#00ff00")), Some([0.0, 1.0, 0.0, 1.0]));
        assert_eq!(parse_color(Some("#0000ff")), Some([0.0, 0.0, 1.0, 1.0]));
    }

    #[test]
    fn parses_six_digit_hex_uppercase() {
        assert_eq!(parse_color(Some("#FF0000")), Some([1.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn parses_three_digit_hex_by_duplicating_each_digit() {
        // #f00 -> #ff0000
        assert_eq!(parse_color(Some("#f00")), Some([1.0, 0.0, 0.0, 1.0]));
        // #08f -> #0088ff
        let c = parse_color(Some("#08f")).unwrap();
        assert!((c[0] - 0.0).abs() < 0.001);
        assert!((c[1] - (0x88 as f32 / 255.0)).abs() < 0.001);
        assert!((c[2] - 1.0).abs() < 0.001);
    }

    #[test]
    fn hex_wrong_length_is_none() {
        assert_eq!(parse_color(Some("#ff00")), None);
        assert_eq!(parse_color(Some("#ff00000")), None);
    }

    #[test]
    fn hex_invalid_characters_is_none() {
        assert_eq!(parse_color(Some("#zzzzzz")), None);
    }

    #[test]
    fn hex_with_multibyte_characters_does_not_panic_and_is_none() {
        // "é" and "€" are multibyte in UTF-8; a byte-index slice on these
        // would previously panic with "byte index N is not a char boundary".
        assert_eq!(parse_color(Some("#aéaaa")), None);
        assert_eq!(parse_color(Some("#a€aa")), None);
    }

    #[test]
    fn hex_with_leading_plus_sign_is_none() {
        // u8::from_str_radix accepts a leading '+' (e.g. "+f" -> 15), which
        // would otherwise let "#+f+f+f" parse as a valid color.
        assert_eq!(parse_color(Some("#+f+f+f")), None);
    }

    #[test]
    fn parses_all_sixteen_css1_keywords_plus_orange_and_transparent() {
        let cases: &[(&str, [f32; 4])] = &[
            ("black", [0.0, 0.0, 0.0, 1.0]),
            ("silver", [0.75294125, 0.75294125, 0.75294125, 1.0]),
            ("gray", [0.5019608, 0.5019608, 0.5019608, 1.0]),
            ("white", [1.0, 1.0, 1.0, 1.0]),
            ("maroon", [0.5019608, 0.0, 0.0, 1.0]),
            ("red", [1.0, 0.0, 0.0, 1.0]),
            ("purple", [0.5019608, 0.0, 0.5019608, 1.0]),
            ("fuchsia", [1.0, 0.0, 1.0, 1.0]),
            ("green", [0.0, 0.5019608, 0.0, 1.0]),
            ("lime", [0.0, 1.0, 0.0, 1.0]),
            ("olive", [0.5019608, 0.5019608, 0.0, 1.0]),
            ("yellow", [1.0, 1.0, 0.0, 1.0]),
            ("navy", [0.0, 0.0, 0.5019608, 1.0]),
            ("blue", [0.0, 0.0, 1.0, 1.0]),
            ("teal", [0.0, 0.5019608, 0.5019608, 1.0]),
            ("aqua", [0.0, 1.0, 1.0, 1.0]),
            ("orange", [1.0, 0.64705884, 0.0, 1.0]),
            ("transparent", [0.0, 0.0, 0.0, 0.0]),
        ];
        for (name, expected) in cases {
            let got = parse_color(Some(name)).unwrap_or_else(|| panic!("{name} should parse"));
            for i in 0..4 {
                assert!(
                    (got[i] - expected[i]).abs() < 0.001,
                    "{name}: channel {i} got {got:?}, expected {expected:?}"
                );
            }
        }
    }

    #[test]
    fn keyword_matching_is_case_insensitive() {
        assert_eq!(parse_color(Some("RED")), parse_color(Some("red")));
        assert_eq!(parse_color(Some("Blue")), parse_color(Some("blue")));
        assert_eq!(parse_color(Some("TRANSPARENT")), parse_color(Some("transparent")));
    }

    #[test]
    fn unrecognized_keyword_is_none() {
        assert_eq!(parse_color(Some("rebeccapurple")), None);
        assert_eq!(parse_color(Some("banana")), None);
    }

    #[test]
    fn unsupported_function_syntax_is_none() {
        assert_eq!(parse_color(Some("rgb(255, 0, 0)")), None);
        assert_eq!(parse_color(Some("hsl(0, 100%, 50%)")), None);
    }
}
