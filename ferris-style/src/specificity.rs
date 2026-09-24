use ferris_css::stylesheet::{Selector, SelectorComponent};

pub fn specificity(selector: &Selector) -> (u32, u32, u32) {
    let mut ids = 0;
    let mut classes_and_attributes = 0;
    let mut types = 0;
    for component in &selector.components {
        if let SelectorComponent::Simple(simple) = component {
            if simple.id.is_some() {
                ids += 1;
            }
            classes_and_attributes += simple.classes.len() as u32;
            classes_and_attributes += simple.attributes.len() as u32;
            if simple.type_name.is_some() {
                types += 1;
            }
        }
    }
    (ids, classes_and_attributes, types)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferris_css::stylesheet::{AttributeSelector, Combinator, SimpleSelector};

    fn simple(component: SimpleSelector) -> SelectorComponent {
        SelectorComponent::Simple(component)
    }

    #[test]
    fn id_selector_has_specificity_1_0_0() {
        let selector = Selector { components: vec![simple(SimpleSelector { id: Some("x".to_string()), ..Default::default() })] };
        assert_eq!(specificity(&selector), (1, 0, 0));
    }

    #[test]
    fn class_selector_has_specificity_0_1_0() {
        let selector = Selector { components: vec![simple(SimpleSelector { classes: vec!["x".to_string()], ..Default::default() })] };
        assert_eq!(specificity(&selector), (0, 1, 0));
    }

    #[test]
    fn attribute_selector_counts_same_as_class() {
        let selector = Selector {
            components: vec![simple(SimpleSelector {
                attributes: vec![AttributeSelector { name: "x".to_string(), value: None }],
                ..Default::default()
            })],
        };
        assert_eq!(specificity(&selector), (0, 1, 0));
    }

    #[test]
    fn type_selector_has_specificity_0_0_1() {
        let selector = Selector { components: vec![simple(SimpleSelector { type_name: Some("div".to_string()), ..Default::default() })] };
        assert_eq!(specificity(&selector), (0, 0, 1));
    }

    #[test]
    fn composite_simple_selector_sums_all_parts() {
        let selector = Selector {
            components: vec![simple(SimpleSelector {
                type_name: Some("div".to_string()),
                id: Some("x".to_string()),
                classes: vec!["a".to_string(), "b".to_string()],
                attributes: vec![AttributeSelector { name: "c".to_string(), value: None }],
            })],
        };
        assert_eq!(specificity(&selector), (1, 3, 1));
    }

    #[test]
    fn combinator_selector_sums_both_sides() {
        let selector = Selector {
            components: vec![
                simple(SimpleSelector { type_name: Some("div".to_string()), ..Default::default() }),
                SelectorComponent::Combinator(Combinator::Descendant),
                simple(SimpleSelector { id: Some("x".to_string()), ..Default::default() }),
            ],
        };
        assert_eq!(specificity(&selector), (1, 0, 1));
    }

    #[test]
    fn id_outranks_class_outranks_type_lexicographically() {
        let id_spec = (1, 0, 0);
        let class_spec = (0, 99, 99);
        let type_spec = (0, 0, 99);
        assert!(id_spec > class_spec);
        assert!(class_spec > type_spec);
    }
}
