#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    Percent(f32),
    Auto,
}

pub fn parse_length(value: Option<&str>) -> Length {
    let Some(raw) = value else {
        return Length::Auto;
    };
    let trimmed = raw.trim();

    if trimmed.eq_ignore_ascii_case("auto") {
        return Length::Auto;
    }

    if let Some(number_part) = trimmed.strip_suffix("px") {
        if let Ok(n) = number_part.trim().parse::<f32>() {
            return Length::Px(n);
        }
        return Length::Auto;
    }

    if let Some(number_part) = trimmed.strip_suffix('%') {
        if let Ok(n) = number_part.trim().parse::<f32>() {
            return Length::Percent(n);
        }
        return Length::Auto;
    }

    Length::Auto
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_px_value() {
        assert_eq!(parse_length(Some("10px")), Length::Px(10.0));
    }

    #[test]
    fn parses_px_value_with_decimal() {
        assert_eq!(parse_length(Some("10.5px")), Length::Px(10.5));
    }

    #[test]
    fn parses_px_value_with_surrounding_whitespace() {
        assert_eq!(parse_length(Some("  10px  ")), Length::Px(10.0));
    }

    #[test]
    fn parses_percent_value() {
        assert_eq!(parse_length(Some("50%")), Length::Percent(50.0));
    }

    #[test]
    fn parses_percent_value_with_decimal() {
        assert_eq!(parse_length(Some("33.33%")), Length::Percent(33.33));
    }

    #[test]
    fn parses_auto_keyword_case_insensitive() {
        assert_eq!(parse_length(Some("auto")), Length::Auto);
        assert_eq!(parse_length(Some("AUTO")), Length::Auto);
        assert_eq!(parse_length(Some("Auto")), Length::Auto);
    }

    #[test]
    fn absent_value_is_auto() {
        assert_eq!(parse_length(None), Length::Auto);
    }

    #[test]
    fn empty_string_is_auto() {
        assert_eq!(parse_length(Some("")), Length::Auto);
    }

    #[test]
    fn unsupported_unit_is_auto() {
        assert_eq!(parse_length(Some("10vh")), Length::Auto);
        assert_eq!(parse_length(Some("2rem")), Length::Auto);
        assert_eq!(parse_length(Some("1em")), Length::Auto);
    }

    #[test]
    fn garbage_string_is_auto() {
        assert_eq!(parse_length(Some("banana")), Length::Auto);
    }

    #[test]
    fn bare_number_with_no_unit_is_auto() {
        assert_eq!(parse_length(Some("10")), Length::Auto);
    }

    #[test]
    fn negative_px_value_parses_as_negative() {
        assert_eq!(parse_length(Some("-5px")), Length::Px(-5.0));
    }
}
