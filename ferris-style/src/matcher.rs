use ferris_css::stylesheet::{Combinator, Selector, SelectorComponent, SimpleSelector};
use ferris_dom::dom::Element;

pub fn element_matches_simple(element: &Element, simple: &SimpleSelector) -> bool {
    if let Some(type_name) = &simple.type_name {
        if &element.tag_name != type_name {
            return false;
        }
    }
    if let Some(id) = &simple.id {
        if element.attributes.get("id") != Some(id) {
            return false;
        }
    }
    for class in &simple.classes {
        let has_class = element
            .attributes
            .get("class")
            .map(|value| value.split_whitespace().any(|token| token == class))
            .unwrap_or(false);
        if !has_class {
            return false;
        }
    }
    for attr in &simple.attributes {
        match &attr.value {
            None => {
                if !element.attributes.contains_key(&attr.name) {
                    return false;
                }
            }
            Some(expected) => {
                if element.attributes.get(&attr.name) != Some(expected) {
                    return false;
                }
            }
        }
    }
    true
}

pub fn selector_matches(
    selector: &Selector,
    element: &Element,
    ancestors: &[&Element],
    preceding_siblings: &[&Element],
) -> bool {
    let components = &selector.components;
    if components.is_empty() {
        return false;
    }
    matches_from(components, components.len() - 1, element, ancestors, preceding_siblings)
}

fn matches_from(
    components: &[SelectorComponent],
    idx: usize,
    element: &Element,
    ancestors: &[&Element],
    preceding_siblings: &[&Element],
) -> bool {
    let SelectorComponent::Simple(simple) = &components[idx] else {
        return false;
    };
    if !element_matches_simple(element, simple) {
        return false;
    }
    if idx == 0 {
        return true;
    }
    if idx < 2 {
        // A Simple selector at idx > 0 must be preceded by (Combinator, Simple). idx < 2
        // here would mean the component list doesn't start with a Simple selector, which
        // ferris-css's parser never produces — treat defensively as no match, not a panic.
        return false;
    }

    let SelectorComponent::Combinator(combinator) = &components[idx - 1] else {
        return false;
    };
    let prev_idx = idx - 2;

    match combinator {
        Combinator::Child => {
            let Some(&parent) = ancestors.last() else { return false };
            let parent_ancestors = &ancestors[..ancestors.len() - 1];
            matches_from(components, prev_idx, parent, parent_ancestors, &[])
        }
        Combinator::Descendant => {
            for i in (0..ancestors.len()).rev() {
                let ancestor = ancestors[i];
                let ancestor_ancestors = &ancestors[..i];
                if matches_from(components, prev_idx, ancestor, ancestor_ancestors, &[]) {
                    return true;
                }
            }
            false
        }
        Combinator::NextSibling => {
            let Some(&sibling) = preceding_siblings.last() else { return false };
            let sibling_preceding = &preceding_siblings[..preceding_siblings.len() - 1];
            matches_from(components, prev_idx, sibling, ancestors, sibling_preceding)
        }
        Combinator::SubsequentSibling => {
            for i in (0..preceding_siblings.len()).rev() {
                let sibling = preceding_siblings[i];
                let sibling_preceding = &preceding_siblings[..i];
                if matches_from(components, prev_idx, sibling, ancestors, sibling_preceding) {
                    return true;
                }
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(tag: &str, attrs: &[(&str, &str)]) -> Element {
        let mut el = Element::new(tag);
        for (k, v) in attrs {
            el.attributes.insert(k.to_string(), v.to_string());
        }
        el
    }

    fn simple_selector(type_name: &str) -> SelectorComponent {
        SelectorComponent::Simple(SimpleSelector {
            type_name: Some(type_name.to_string()),
            ..Default::default()
        })
    }

    #[test]
    fn matches_by_type_name() {
        let el = element("div", &[]);
        let matching = SimpleSelector { type_name: Some("div".to_string()), ..Default::default() };
        assert!(element_matches_simple(&el, &matching));
        let wrong = SimpleSelector { type_name: Some("span".to_string()), ..Default::default() };
        assert!(!element_matches_simple(&el, &wrong));
    }

    #[test]
    fn matches_by_id() {
        let el = element("div", &[("id", "hero")]);
        let matching = SimpleSelector { id: Some("hero".to_string()), ..Default::default() };
        assert!(element_matches_simple(&el, &matching));
        let wrong = SimpleSelector { id: Some("other".to_string()), ..Default::default() };
        assert!(!element_matches_simple(&el, &wrong));
    }

    #[test]
    fn matches_class_among_several() {
        let el = element("div", &[("class", "a b c")]);
        let matching = SimpleSelector { classes: vec!["b".to_string()], ..Default::default() };
        assert!(element_matches_simple(&el, &matching));
        let wrong = SimpleSelector { classes: vec!["z".to_string()], ..Default::default() };
        assert!(!element_matches_simple(&el, &wrong));
    }

    #[test]
    fn matches_attribute_presence_and_value() {
        use ferris_css::stylesheet::AttributeSelector;
        let el = element("input", &[("type", "text"), ("disabled", "")]);
        let presence = SimpleSelector {
            attributes: vec![AttributeSelector { name: "disabled".to_string(), value: None }],
            ..Default::default()
        };
        assert!(element_matches_simple(&el, &presence));

        let value_match = SimpleSelector {
            attributes: vec![AttributeSelector { name: "type".to_string(), value: Some("text".to_string()) }],
            ..Default::default()
        };
        assert!(element_matches_simple(&el, &value_match));

        let value_mismatch = SimpleSelector {
            attributes: vec![AttributeSelector { name: "type".to_string(), value: Some("password".to_string()) }],
            ..Default::default()
        };
        assert!(!element_matches_simple(&el, &value_mismatch));
    }

    #[test]
    fn composite_simple_selector_requires_all_parts_to_match() {
        let el = element("div", &[("id", "hero"), ("class", "card")]);
        let all_match = SimpleSelector {
            type_name: Some("div".to_string()),
            id: Some("hero".to_string()),
            classes: vec!["card".to_string()],
            attributes: vec![],
        };
        assert!(element_matches_simple(&el, &all_match));

        let one_mismatch = SimpleSelector {
            type_name: Some("div".to_string()),
            id: Some("wrong".to_string()),
            classes: vec!["card".to_string()],
            attributes: vec![],
        };
        assert!(!element_matches_simple(&el, &one_mismatch));
    }

    // --- Review Focus: [attr=value] requires exact match, not substring/token match ---
    #[test]
    fn attribute_value_selector_requires_exact_match_not_substring() {
        use ferris_css::stylesheet::AttributeSelector;
        let el = element("div", &[("data-x", "card featured")]);
        let exact = SimpleSelector {
            attributes: vec![AttributeSelector { name: "data-x".to_string(), value: Some("card featured".to_string()) }],
            ..Default::default()
        };
        assert!(element_matches_simple(&el, &exact));

        let substring_attempt = SimpleSelector {
            attributes: vec![AttributeSelector { name: "data-x".to_string(), value: Some("card".to_string()) }],
            ..Default::default()
        };
        assert!(
            !element_matches_simple(&el, &substring_attempt),
            "[data-x=card] must not match data-x=\"card featured\" — attribute values require exact equality, unlike classes"
        );
    }

    // --- Review Focus: selector against an element missing the relevant attribute entirely ---
    #[test]
    fn selector_against_element_missing_the_attribute_entirely_does_not_match_or_panic() {
        let el = element("div", &[]); // no class, no id, no attributes at all
        let class_selector = SimpleSelector { classes: vec!["x".to_string()], ..Default::default() };
        assert!(!element_matches_simple(&el, &class_selector));
        let id_selector = SimpleSelector { id: Some("x".to_string()), ..Default::default() };
        assert!(!element_matches_simple(&el, &id_selector));
        use ferris_css::stylesheet::AttributeSelector;
        let attr_selector = SimpleSelector {
            attributes: vec![AttributeSelector { name: "data-x".to_string(), value: None }],
            ..Default::default()
        };
        assert!(!element_matches_simple(&el, &attr_selector));
    }

    #[test]
    fn single_component_selector_matches_element_directly() {
        let el = element("p", &[]);
        let matching = Selector { components: vec![simple_selector("p")] };
        assert!(selector_matches(&matching, &el, &[], &[]));
        let non_matching = Selector { components: vec![simple_selector("span")] };
        assert!(!selector_matches(&non_matching, &el, &[], &[]));
    }

    #[test]
    fn descendant_combinator_matches_any_ancestor() {
        let grandparent = element("section", &[]);
        let parent = element("div", &[]);
        let el = element("p", &[]);
        let matching = Selector {
            components: vec![simple_selector("section"), SelectorComponent::Combinator(Combinator::Descendant), simple_selector("p")],
        };
        assert!(selector_matches(&matching, &el, &[&grandparent, &parent], &[]));

        let non_matching = Selector {
            components: vec![simple_selector("article"), SelectorComponent::Combinator(Combinator::Descendant), simple_selector("p")],
        };
        assert!(!selector_matches(&non_matching, &el, &[&grandparent, &parent], &[]));
    }

    #[test]
    fn child_combinator_requires_immediate_parent() {
        let grandparent = element("section", &[]);
        let parent = element("div", &[]);
        let el = element("p", &[]);
        let matching = Selector {
            components: vec![simple_selector("div"), SelectorComponent::Combinator(Combinator::Child), simple_selector("p")],
        };
        assert!(selector_matches(&matching, &el, &[&grandparent, &parent], &[]));

        let non_matching = Selector {
            components: vec![simple_selector("section"), SelectorComponent::Combinator(Combinator::Child), simple_selector("p")],
        };
        assert!(
            !selector_matches(&non_matching, &el, &[&grandparent, &parent], &[]),
            "section is a grandparent, not the immediate parent"
        );
    }

    #[test]
    fn next_sibling_combinator_requires_immediately_preceding_sibling() {
        let first = element("h1", &[]);
        let second = element("p", &[]);
        let el = element("span", &[]);
        let matching = Selector {
            components: vec![simple_selector("p"), SelectorComponent::Combinator(Combinator::NextSibling), simple_selector("span")],
        };
        assert!(selector_matches(&matching, &el, &[], &[&first, &second]));

        let non_matching = Selector {
            components: vec![simple_selector("h1"), SelectorComponent::Combinator(Combinator::NextSibling), simple_selector("span")],
        };
        assert!(
            !selector_matches(&non_matching, &el, &[], &[&first, &second]),
            "h1 is not the immediately preceding sibling"
        );
    }

    #[test]
    fn subsequent_sibling_combinator_matches_any_preceding_sibling() {
        let first = element("h1", &[]);
        let second = element("p", &[]);
        let el = element("span", &[]);
        let matching = Selector {
            components: vec![simple_selector("h1"), SelectorComponent::Combinator(Combinator::SubsequentSibling), simple_selector("span")],
        };
        assert!(selector_matches(&matching, &el, &[], &[&first, &second]));

        let non_matching = Selector {
            components: vec![simple_selector("h2"), SelectorComponent::Combinator(Combinator::SubsequentSibling), simple_selector("span")],
        };
        assert!(!selector_matches(&non_matching, &el, &[], &[&first, &second]));
    }

    #[test]
    fn no_ancestor_or_sibling_means_combinator_selector_never_matches() {
        let el = element("p", &[]);
        let descendant_selector = Selector {
            components: vec![simple_selector("div"), SelectorComponent::Combinator(Combinator::Descendant), simple_selector("p")],
        };
        assert!(!selector_matches(&descendant_selector, &el, &[], &[]));

        let sibling_selector = Selector {
            components: vec![simple_selector("h1"), SelectorComponent::Combinator(Combinator::NextSibling), simple_selector("p")],
        };
        assert!(!selector_matches(&sibling_selector, &el, &[], &[]));
    }
}
