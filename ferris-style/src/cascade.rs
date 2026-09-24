use std::collections::HashMap;

use ferris_css::stylesheet::Declaration;

pub struct MatchedDeclaration<'a> {
    pub specificity: (u32, u32, u32),
    pub source_order: usize,
    pub declaration: &'a Declaration,
}

pub fn resolve_cascade(matches: &[MatchedDeclaration]) -> HashMap<String, String> {
    let mut entries: Vec<(bool, (u32, u32, u32), usize, &str, String)> = matches
        .iter()
        .map(|m| {
            let (value, important) = split_important(&m.declaration.value);
            (important, m.specificity, m.source_order, m.declaration.property.as_str(), value)
        })
        .collect();

    entries.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));

    let mut style = HashMap::new();
    for (_, _, _, property, value) in entries {
        style.insert(property.to_string(), value);
    }
    style
}

fn split_important(value: &str) -> (String, bool) {
    let trimmed = value.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix("!important") {
        // Keep as many chars from the ORIGINAL `trimmed` string as `stripped` has —
        // to_ascii_lowercase() never changes character count, only ASCII letter case,
        // so this correctly preserves the original casing/content of everything before
        // "!important" without ever slicing at a byte offset that could land mid-character.
        let char_count_to_keep = stripped.chars().count();
        let kept: String = trimmed.chars().take(char_count_to_keep).collect();
        return (kept.trim_end().to_string(), true);
    }
    (value.to_string(), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(property: &str, value: &str) -> Declaration {
        Declaration { property: property.to_string(), value: value.to_string() }
    }

    #[test]
    fn higher_specificity_wins() {
        let low = decl("color", "blue");
        let high = decl("color", "red");
        let matches = vec![
            MatchedDeclaration { specificity: (0, 0, 1), source_order: 0, declaration: &low },
            MatchedDeclaration { specificity: (0, 1, 0), source_order: 1, declaration: &high },
        ];
        let style = resolve_cascade(&matches);
        assert_eq!(style.get("color"), Some(&"red".to_string()));
    }

    #[test]
    fn equal_specificity_later_source_order_wins() {
        let first = decl("color", "blue");
        let second = decl("color", "red");
        let matches = vec![
            MatchedDeclaration { specificity: (0, 1, 0), source_order: 0, declaration: &first },
            MatchedDeclaration { specificity: (0, 1, 0), source_order: 1, declaration: &second },
        ];
        let style = resolve_cascade(&matches);
        assert_eq!(style.get("color"), Some(&"red".to_string()));
    }

    #[test]
    fn important_wins_over_higher_specificity() {
        let important_low_spec = decl("color", "red !important");
        let normal_high_spec = decl("color", "blue");
        let matches = vec![
            MatchedDeclaration { specificity: (0, 0, 1), source_order: 0, declaration: &important_low_spec },
            MatchedDeclaration { specificity: (1, 1, 1), source_order: 1, declaration: &normal_high_spec },
        ];
        let style = resolve_cascade(&matches);
        assert_eq!(style.get("color"), Some(&"red".to_string()), "important should win despite lower specificity");
    }

    #[test]
    fn important_marker_is_stripped_from_stored_value() {
        let d = decl("color", "red !important");
        let matches = vec![MatchedDeclaration { specificity: (0, 0, 1), source_order: 0, declaration: &d }];
        let style = resolve_cascade(&matches);
        assert_eq!(style.get("color"), Some(&"red".to_string()));
    }

    #[test]
    fn important_detection_is_case_insensitive_and_whitespace_tolerant() {
        let d = decl("color", "red  !IMPORTANT");
        let matches = vec![MatchedDeclaration { specificity: (0, 0, 1), source_order: 0, declaration: &d }];
        let style = resolve_cascade(&matches);
        assert_eq!(style.get("color"), Some(&"red".to_string()));
    }

    #[test]
    fn different_properties_do_not_interfere() {
        let color = decl("color", "red");
        let width = decl("width", "10px");
        let matches = vec![
            MatchedDeclaration { specificity: (0, 0, 1), source_order: 0, declaration: &color },
            MatchedDeclaration { specificity: (0, 0, 1), source_order: 1, declaration: &width },
        ];
        let style = resolve_cascade(&matches);
        assert_eq!(style.get("color"), Some(&"red".to_string()));
        assert_eq!(style.get("width"), Some(&"10px".to_string()));
    }

    // --- Review Focus: multiple declarations for the same property in one rule ---
    #[test]
    fn same_property_declared_twice_at_identical_specificity_and_source_order_keeps_the_later_one() {
        // Simulates two declarations from the SAME rule (identical specificity AND
        // identical source_order) — the tie must be broken by their order in the
        // input slice itself (source order within the rule), not dropped or randomized.
        let first = decl("color", "blue");
        let second = decl("color", "red");
        let matches = vec![
            MatchedDeclaration { specificity: (0, 0, 1), source_order: 0, declaration: &first },
            MatchedDeclaration { specificity: (0, 0, 1), source_order: 0, declaration: &second },
        ];
        let style = resolve_cascade(&matches);
        assert_eq!(style.get("color"), Some(&"red".to_string()), "the later declaration within the same rule must win");
    }
}
