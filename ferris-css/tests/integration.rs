use ferris_css::parser::Parser;
use ferris_css::stylesheet::{Combinator, Rule, SelectorComponent, Stylesheet};
use ferris_css::tokenizer::Tokenizer;

fn parse_css(css: &str) -> Stylesheet {
    let tokens = Tokenizer::tokenize(css);
    Parser::parse(&tokens)
}

#[test]
fn parses_realistic_stylesheet_with_composite_selector_and_combinator() {
    let css = r#".card > h2, #hero { color: navy; font-family: Arial, sans-serif; }"#;
    let sheet = parse_css(css);
    assert_eq!(sheet.rules.len(), 1);
    let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
    assert_eq!(rule.selectors.len(), 2);
    assert_eq!(rule.selectors[0].components.len(), 3); // .card, >, h2
    assert!(matches!(
        rule.selectors[0].components[1],
        SelectorComponent::Combinator(Combinator::Child)
    ));
    assert_eq!(rule.declarations.len(), 2);
    assert_eq!(rule.declarations[1].value, "Arial, sans-serif");
}

#[test]
fn parses_stylesheet_with_media_query_and_malformed_rule_recovery() {
    let css = r#"
        div { color red; }
        p { color: blue; }
        @media (max-width: 480px) {
            .nav { display: none; }
        }
    "#;
    let sheet = parse_css(css);
    let style_rules: Vec<_> = sheet
        .rules
        .iter()
        .filter(|r| matches!(r, Rule::Style(_)))
        .collect();
    let media_rules: Vec<_> = sheet
        .rules
        .iter()
        .filter(|r| matches!(r, Rule::Media(_)))
        .collect();
    // The malformed "div { color red; }" (missing ':') is dropped; p and @media survive.
    assert_eq!(style_rules.len(), 1);
    assert_eq!(media_rules.len(), 1);
    let Rule::Media(media) = media_rules[0] else { unreachable!() };
    assert_eq!(media.condition, "(max-width: 480px)");
    assert_eq!(media.rules.len(), 1);
    assert_eq!(media.rules[0].declarations[0].value, "none");
}
