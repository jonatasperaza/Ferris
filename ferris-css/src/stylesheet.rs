#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    Style(StyleRule),
    Media(MediaRule),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaRule {
    pub condition: String,
    pub rules: Vec<StyleRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub property: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Selector {
    pub components: Vec<SelectorComponent>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectorComponent {
    Simple(SimpleSelector),
    Combinator(Combinator),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimpleSelector {
    pub type_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<AttributeSelector>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSelector {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Combinator {
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stylesheet_has_no_rules() {
        assert_eq!(Stylesheet::default().rules, Vec::new());
    }

    #[test]
    fn default_style_rule_has_no_selectors_or_declarations() {
        let rule = StyleRule::default();
        assert!(rule.selectors.is_empty());
        assert!(rule.declarations.is_empty());
    }

    #[test]
    fn default_simple_selector_has_no_type_id_classes_or_attributes() {
        let selector = SimpleSelector::default();
        assert_eq!(selector.type_name, None);
        assert_eq!(selector.id, None);
        assert!(selector.classes.is_empty());
        assert!(selector.attributes.is_empty());
    }
}
