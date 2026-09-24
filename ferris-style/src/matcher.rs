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

/// `ancestors` carries, for each ancestor of `element` (ordered from the furthest,
/// i.e. the root, to the nearest, i.e. the immediate parent — same order the
/// original `&[&Element]` used), a pair of `(ancestor, ancestor_preceding_siblings)`
/// where `ancestor_preceding_siblings` is that ancestor's own list of preceding
/// `Element` siblings within ITS parent. This lets sibling combinators (`+`/`~`)
/// resolve correctly even when they apply to an ancestor rather than to `element`
/// itself (e.g. `.a + .b .f`, where `.b` — an ancestor of the target `.f` — must be
/// checked against its OWN preceding sibling, not the target's).
pub fn selector_matches(
    selector: &Selector,
    element: &Element,
    ancestors: &[(&Element, &[&Element])],
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
    ancestors: &[(&Element, &[&Element])],
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
            let Some(&(parent, parent_preceding_siblings)) = ancestors.last() else { return false };
            let parent_ancestors = &ancestors[..ancestors.len() - 1];
            matches_from(components, prev_idx, parent, parent_ancestors, parent_preceding_siblings)
        }
        Combinator::Descendant => {
            for i in (0..ancestors.len()).rev() {
                let (ancestor, ancestor_preceding_siblings) = ancestors[i];
                let ancestor_ancestors = &ancestors[..i];
                if matches_from(components, prev_idx, ancestor, ancestor_ancestors, ancestor_preceding_siblings) {
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
        assert!(selector_matches(&matching, &el, &[(&grandparent, &[]), (&parent, &[])], &[]));

        let non_matching = Selector {
            components: vec![simple_selector("article"), SelectorComponent::Combinator(Combinator::Descendant), simple_selector("p")],
        };
        assert!(!selector_matches(&non_matching, &el, &[(&grandparent, &[]), (&parent, &[])], &[]));
    }

    #[test]
    fn child_combinator_requires_immediate_parent() {
        let grandparent = element("section", &[]);
        let parent = element("div", &[]);
        let el = element("p", &[]);
        let matching = Selector {
            components: vec![simple_selector("div"), SelectorComponent::Combinator(Combinator::Child), simple_selector("p")],
        };
        assert!(selector_matches(&matching, &el, &[(&grandparent, &[]), (&parent, &[])], &[]));

        let non_matching = Selector {
            components: vec![simple_selector("section"), SelectorComponent::Combinator(Combinator::Child), simple_selector("p")],
        };
        assert!(
            !selector_matches(&non_matching, &el, &[(&grandparent, &[]), (&parent, &[])], &[]),
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

    // --- Review Focus (I-1 fix): sibling combinator to the LEFT of an ancestor
    // combinator must be checked against that ANCESTOR's own preceding siblings,
    // not the target element's. Reproduces `.a + .b .f` against:
    //   <div id="wrap">
    //     <section class="a" id="s1">...</section>
    //     <section class="b" id="s2"><div class="inner"><p class="f" id="p4"/></div></section>
    //   </div>
    // `.f` (p4) is a descendant of `.b` (section#s2), and `.b` is the next sibling
    // of `.a` (section#s1) within `#wrap`. Before the fix, the recursive call for
    // the `Descendant` combinator always passed `preceding_siblings: &[]` for the
    // ancestor being checked, so `.a + .b` could never match here.
    #[test]
    fn sibling_combinator_before_ancestor_combinator_carries_correct_per_ancestor_siblings() {
        let wrap = element("div", &[("id", "wrap")]);
        let section_a = element("section", &[("class", "a"), ("id", "s1")]);
        let section_b = element("section", &[("class", "b"), ("id", "s2")]);
        let target = element("p", &[("class", "f"), ("id", "p4")]);

        let selector = Selector {
            components: vec![
                SelectorComponent::Simple(SimpleSelector { classes: vec!["a".to_string()], ..Default::default() }),
                SelectorComponent::Combinator(Combinator::NextSibling),
                SelectorComponent::Simple(SimpleSelector { classes: vec!["b".to_string()], ..Default::default() }),
                SelectorComponent::Combinator(Combinator::Descendant),
                SelectorComponent::Simple(SimpleSelector { classes: vec!["f".to_string()], ..Default::default() }),
            ],
        };

        // `target` (p.f)'s immediate parent is section.b (its "inner" div is elided
        // here; the fix cares about ancestors carrying per-level sibling context,
        // regardless of chain depth). Ancestors are ordered furthest (root) to
        // nearest (immediate parent), same as before the fix.
        let section_b_preceding_siblings = [&section_a];
        let ancestors: Vec<(&Element, &[&Element])> =
            vec![(&wrap, &[] as &[&Element]), (&section_b, &section_b_preceding_siblings)];
        assert!(
            selector_matches(&selector, &target, &ancestors, &[]),
            ".a + .b .f must match: section.b is the next sibling of section.a, and p.f is a descendant of section.b"
        );

        // Sanity check: without section.a recorded as section.b's preceding sibling,
        // the sibling combinator correctly fails to match.
        let ancestors_no_sibling: Vec<(&Element, &[&Element])> = vec![(&wrap, &[]), (&section_b, &[])];
        assert!(!selector_matches(&selector, &target, &ancestors_no_sibling, &[]));
    }
}
