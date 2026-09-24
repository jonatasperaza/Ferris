use crate::stylesheet::{
    AttributeSelector, Combinator, Declaration, MediaRule, Rule, Selector, SelectorComponent,
    SimpleSelector, StyleRule, Stylesheet,
};
use crate::tokenizer::Token;

pub struct Parser;

impl Parser {
    pub fn parse(tokens: &[Token]) -> Stylesheet {
        let mut rules = Vec::new();
        let mut i = 0;
        loop {
            skip_whitespace(tokens, &mut i);
            if i >= tokens.len() {
                break;
            }
            let rule_start = i;

            if let Some(Token::AtKeyword(name)) = tokens.get(i) {
                if name == "media" {
                    i += 1;
                    skip_whitespace(tokens, &mut i);
                    let condition_start = i;
                    while i < tokens.len() && tokens[i] != Token::OpenBrace {
                        i += 1;
                    }
                    let condition = reconstruct_value(&tokens[condition_start..i]);
                    if i < tokens.len() {
                        i += 1; // consume {
                    }
                    let mut media_rules = Vec::new();
                    loop {
                        skip_whitespace(tokens, &mut i);
                        if i >= tokens.len() || tokens[i] == Token::CloseBrace {
                            break;
                        }
                        let nested_start = i;
                        match parse_style_rule(tokens, i) {
                            Some((rule, next)) => {
                                media_rules.push(rule);
                                i = next;
                            }
                            None => {
                                i = skip_to_next_boundary(tokens, nested_start);
                            }
                        }
                    }
                    if i < tokens.len() && tokens[i] == Token::CloseBrace {
                        i += 1; // consume outer }
                    }
                    rules.push(Rule::Media(MediaRule { condition, rules: media_rules }));
                    continue;
                } else {
                    // Unknown at-rule: out of scope, skip just the keyword and move on.
                    i += 1;
                    continue;
                }
            }

            match parse_style_rule(tokens, i) {
                Some((rule, next)) => {
                    rules.push(Rule::Style(rule));
                    i = next;
                }
                None => {
                    i = skip_to_next_boundary(tokens, rule_start);
                }
            }
        }
        Stylesheet { rules }
    }
}

fn skip_whitespace(tokens: &[Token], i: &mut usize) {
    while *i < tokens.len() && tokens[*i] == Token::Whitespace {
        *i += 1;
    }
}

fn is_simple_selector_start(tokens: &[Token], i: usize) -> bool {
    matches!(
        tokens.get(i),
        Some(Token::Ident(_)) | Some(Token::Hash(_)) | Some(Token::Delim('.')) | Some(Token::Delim('['))
    )
}

fn parse_style_rule(tokens: &[Token], mut i: usize) -> Option<(StyleRule, usize)> {
    let (selectors, next) = parse_selector_list(tokens, i);
    i = next;
    skip_whitespace(tokens, &mut i);
    if tokens.get(i) != Some(&Token::OpenBrace) {
        return None;
    }
    i += 1;
    let (declarations, next) = parse_declarations(tokens, i);
    i = next;
    if tokens.get(i) != Some(&Token::CloseBrace) {
        return None;
    }
    i += 1;
    Some((StyleRule { selectors, declarations }, i))
}

/// Scans forward from `start` (the beginning of a rule that failed to parse), tracking
/// brace depth, and returns the index just past the `}` that closes the first `{` opened
/// at this level — or `tokens.len()` if no such closing brace exists before the end of
/// input. This is the spec's "malformed rule dropped whole, resume at the next boundary"
/// error-recovery rule.
fn skip_to_next_boundary(tokens: &[Token], start: usize) -> usize {
    let mut i = start;
    let mut depth: i32 = 0;
    while i < tokens.len() {
        match tokens[i] {
            Token::OpenBrace => depth += 1,
            Token::CloseBrace => {
                depth -= 1;
                i += 1;
                if depth <= 0 {
                    return i;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    i
}

fn parse_selector_list(tokens: &[Token], mut i: usize) -> (Vec<Selector>, usize) {
    let mut selectors = Vec::new();
    loop {
        let (selector, next) = parse_selector(tokens, i);
        selectors.push(selector);
        i = next;
        skip_whitespace(tokens, &mut i);
        if i < tokens.len() && tokens[i] == Token::Comma {
            i += 1;
            skip_whitespace(tokens, &mut i);
        } else {
            break;
        }
    }
    (selectors, i)
}

fn parse_selector(tokens: &[Token], mut i: usize) -> (Selector, usize) {
    let mut components = Vec::new();
    skip_whitespace(tokens, &mut i);
    let (simple, next) = parse_simple_selector(tokens, i);
    components.push(SelectorComponent::Simple(simple));
    i = next;

    loop {
        let had_whitespace = i < tokens.len() && tokens[i] == Token::Whitespace;
        let mut j = i;
        if had_whitespace {
            skip_whitespace(tokens, &mut j);
        }

        let combinator = match tokens.get(j) {
            Some(Token::Delim('>')) => Some(Combinator::Child),
            Some(Token::Delim('+')) => Some(Combinator::NextSibling),
            Some(Token::Delim('~')) => Some(Combinator::SubsequentSibling),
            _ => None,
        };

        if let Some(combinator) = combinator {
            j += 1;
            skip_whitespace(tokens, &mut j);
            if !is_simple_selector_start(tokens, j) {
                break;
            }
            components.push(SelectorComponent::Combinator(combinator));
            let (simple, next) = parse_simple_selector(tokens, j);
            components.push(SelectorComponent::Simple(simple));
            i = next;
            continue;
        }

        if had_whitespace && is_simple_selector_start(tokens, j) {
            components.push(SelectorComponent::Combinator(Combinator::Descendant));
            let (simple, next) = parse_simple_selector(tokens, j);
            components.push(SelectorComponent::Simple(simple));
            i = next;
            continue;
        }

        break;
    }

    (Selector { components }, i)
}

fn parse_simple_selector(tokens: &[Token], mut i: usize) -> (SimpleSelector, usize) {
    let mut selector = SimpleSelector::default();

    if let Some(Token::Ident(name)) = tokens.get(i) {
        selector.type_name = Some(name.clone());
        i += 1;
    }

    loop {
        match tokens.get(i) {
            Some(Token::Hash(id)) => {
                selector.id = Some(id.clone());
                i += 1;
            }
            Some(Token::Delim('.')) => {
                if let Some(Token::Ident(class)) = tokens.get(i + 1) {
                    selector.classes.push(class.clone());
                    i += 2;
                } else {
                    break;
                }
            }
            Some(Token::Delim('[')) => {
                if let Some(Token::Ident(attr_name)) = tokens.get(i + 1) {
                    let mut j = i + 2;
                    let mut value = None;
                    if tokens.get(j) == Some(&Token::Delim('=')) {
                        j += 1;
                        match tokens.get(j) {
                            Some(Token::Ident(v)) => {
                                value = Some(v.clone());
                                j += 1;
                            }
                            Some(Token::Str(v)) => {
                                value = Some(v.clone());
                                j += 1;
                            }
                            _ => {}
                        }
                    }
                    if tokens.get(j) == Some(&Token::Delim(']')) {
                        selector.attributes.push(AttributeSelector { name: attr_name.clone(), value });
                        i = j + 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    (selector, i)
}

fn parse_declarations(tokens: &[Token], mut i: usize) -> (Vec<Declaration>, usize) {
    let mut declarations = Vec::new();
    loop {
        skip_whitespace(tokens, &mut i);
        if i >= tokens.len() || tokens[i] == Token::CloseBrace {
            break;
        }
        let property = if let Some(Token::Ident(name)) = tokens.get(i) {
            i += 1;
            name.clone()
        } else {
            break;
        };
        skip_whitespace(tokens, &mut i);
        if tokens.get(i) != Some(&Token::Colon) {
            break;
        }
        i += 1;
        skip_whitespace(tokens, &mut i);

        let value_start = i;
        while i < tokens.len() && tokens[i] != Token::Semicolon && tokens[i] != Token::CloseBrace {
            i += 1;
        }
        let value = reconstruct_value(&tokens[value_start..i]);
        declarations.push(Declaration { property, value });

        if i < tokens.len() && tokens[i] == Token::Semicolon {
            i += 1;
        }
    }
    (declarations, i)
}

fn reconstruct_value(tokens: &[Token]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for token in tokens {
        match token {
            Token::Whitespace => parts.push(" ".to_string()),
            Token::Ident(s) => parts.push(s.clone()),
            Token::Str(s) => parts.push(format!("\"{s}\"")),
            Token::Hash(s) => parts.push(format!("#{s}")),
            Token::Comma => parts.push(",".to_string()),
            Token::Delim(c) => parts.push(c.to_string()),
            Token::Colon => parts.push(":".to_string()),
            Token::AtKeyword(s) => parts.push(format!("@{s}")),
            Token::OpenBrace => parts.push("{".to_string()),
            Token::CloseBrace => parts.push("}".to_string()),
            Token::Semicolon => parts.push(";".to_string()),
        }
    }
    parts.concat().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    fn tokens_for(css: &str) -> Vec<Token> {
        Tokenizer::tokenize(css)
    }

    #[test]
    fn parses_type_selector_with_one_declaration() {
        let tokens = tokens_for("div { color: red; }");
        let sheet = Parser::parse(&tokens);
        assert_eq!(sheet.rules.len(), 1);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.selectors.len(), 1);
        assert_eq!(
            rule.selectors[0].components,
            vec![SelectorComponent::Simple(SimpleSelector {
                type_name: Some("div".to_string()),
                ..Default::default()
            })]
        );
        assert_eq!(
            rule.declarations,
            vec![Declaration { property: "color".to_string(), value: "red".to_string() }]
        );
    }

    #[test]
    fn parses_composite_simple_selector_type_class_id_attribute() {
        let tokens = tokens_for(r#"div.foo#bar[data-x=1] { width: 10px; }"#);
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        let SelectorComponent::Simple(simple) = &rule.selectors[0].components[0] else {
            panic!("expected simple selector")
        };
        assert_eq!(simple.type_name, Some("div".to_string()));
        assert_eq!(simple.id, Some("bar".to_string()));
        assert_eq!(simple.classes, vec!["foo".to_string()]);
        assert_eq!(
            simple.attributes,
            vec![AttributeSelector { name: "data-x".to_string(), value: Some("1".to_string()) }]
        );
    }

    #[test]
    fn parses_selector_list_separated_by_comma() {
        let tokens = tokens_for("h1, h2 { margin: 0; }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.selectors.len(), 2);
    }

    #[test]
    fn parses_each_combinator() {
        let descendant = Parser::parse(&tokens_for("div p { color: red; }"));
        let Rule::Style(r) = &descendant.rules[0] else { panic!("expected style rule") };
        assert!(matches!(r.selectors[0].components[1], SelectorComponent::Combinator(Combinator::Descendant)));

        let child = Parser::parse(&tokens_for("div > p { color: red; }"));
        let Rule::Style(r) = &child.rules[0] else { panic!("expected style rule") };
        assert!(matches!(r.selectors[0].components[1], SelectorComponent::Combinator(Combinator::Child)));

        let next_sibling = Parser::parse(&tokens_for("div + p { color: red; }"));
        let Rule::Style(r) = &next_sibling.rules[0] else { panic!("expected style rule") };
        assert!(matches!(r.selectors[0].components[1], SelectorComponent::Combinator(Combinator::NextSibling)));

        let subsequent = Parser::parse(&tokens_for("div ~ p { color: red; }"));
        let Rule::Style(r) = &subsequent.rules[0] else { panic!("expected style rule") };
        assert!(matches!(r.selectors[0].components[1], SelectorComponent::Combinator(Combinator::SubsequentSibling)));
    }

    #[test]
    fn reconstructs_multi_word_comma_value() {
        let tokens = tokens_for("p { font-family: Arial, sans-serif; }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.declarations[0].value, "Arial, sans-serif");
    }

    #[test]
    fn parses_declaration_without_trailing_semicolon_before_close_brace() {
        let tokens = tokens_for("p { color: red }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.declarations[0].value, "red");
    }

    #[test]
    fn parses_multiple_declarations() {
        let tokens = tokens_for("p { color: red; width: 10px; }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.declarations.len(), 2);
    }

    // --- Review Focus: quoted attribute-selector value with structural-looking characters ---
    #[test]
    fn attribute_selector_quoted_value_with_equals_and_bracket_like_chars() {
        let tokens = tokens_for(r#"[data-x="a=b]c"] { color: red; }"#);
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        let SelectorComponent::Simple(simple) = &rule.selectors[0].components[0] else {
            panic!("expected simple selector")
        };
        assert_eq!(
            simple.attributes,
            vec![AttributeSelector { name: "data-x".to_string(), value: Some("a=b]c".to_string()) }]
        );
    }

    // --- Review Focus: whitespace around the selector-list comma isn't a descendant combinator ---
    #[test]
    fn whitespace_around_selector_list_comma_is_not_a_descendant_combinator() {
        let tokens = tokens_for("h1 , h2 { margin: 0; }");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.selectors.len(), 2);
        assert_eq!(rule.selectors[0].components.len(), 1);
        assert_eq!(rule.selectors[1].components.len(), 1);
    }

    // --- Review Focus: empty declaration block ---
    #[test]
    fn empty_declaration_block_yields_empty_declarations() {
        let tokens = tokens_for("div {}");
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert!(rule.declarations.is_empty());
    }

    // --- Review Focus: semicolon inside a quoted value doesn't end the declaration early ---
    #[test]
    fn semicolon_inside_quoted_value_does_not_end_declaration_early() {
        let tokens = tokens_for(r#"p { content: "a;b"; color: red; }"#);
        let sheet = Parser::parse(&tokens);
        let Rule::Style(rule) = &sheet.rules[0] else { panic!("expected style rule") };
        assert_eq!(rule.declarations.len(), 2);
        assert_eq!(rule.declarations[0].value, "\"a;b\"");
        assert_eq!(rule.declarations[1].value, "red");
    }

    // --- Review Focus: empty/whitespace-only input ---
    #[test]
    fn empty_input_yields_empty_stylesheet() {
        assert_eq!(Parser::parse(&tokens_for("")), Stylesheet::default());
        assert_eq!(Parser::parse(&tokens_for("   \n  ")), Stylesheet::default());
    }

    #[test]
    fn parses_media_rule_with_nested_style_rules() {
        let tokens = tokens_for("@media (min-width: 600px) { p { color: red; } }");
        let sheet = Parser::parse(&tokens);
        assert_eq!(sheet.rules.len(), 1);
        let Rule::Media(media) = &sheet.rules[0] else { panic!("expected media rule") };
        assert_eq!(media.condition, "(min-width: 600px)");
        assert_eq!(media.rules.len(), 1);
        assert_eq!(media.rules[0].declarations[0].value, "red");
    }

    #[test]
    fn malformed_rule_is_skipped_without_breaking_the_rest_of_the_sheet() {
        // "div { color red; }" is missing the ':' after "color" — malformed.
        // The parser must drop it whole and still parse the well-formed "p" rule that follows.
        let tokens = tokens_for("div { color red; } p { color: blue; }");
        let sheet = Parser::parse(&tokens);
        let style_rules: Vec<&StyleRule> = sheet.rules.iter().filter_map(|r| match r {
            Rule::Style(s) => Some(s),
            Rule::Media(_) => None,
        }).collect();
        assert_eq!(style_rules.len(), 1, "expected only the well-formed p rule to survive");
        assert_eq!(style_rules[0].declarations, vec![Declaration { property: "color".to_string(), value: "blue".to_string() }]);
    }
}
