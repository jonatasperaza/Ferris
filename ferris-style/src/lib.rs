pub mod cascade;
pub mod matcher;
pub mod specificity;

use std::collections::HashMap;

use ferris_css::stylesheet::{Rule, Stylesheet};
use ferris_dom::dom::{Element, Node};

use cascade::{resolve_cascade, MatchedDeclaration};
use matcher::selector_matches;
use specificity::specificity;

pub struct StyledNode<'a> {
    pub element: &'a Element,
    pub style: HashMap<String, String>,
    pub children: Vec<StyledNode<'a>>,
}

pub fn resolve_styles<'a>(root: &'a Element, stylesheet: &Stylesheet) -> StyledNode<'a> {
    resolve_node(root, stylesheet, &[], &[])
}

fn resolve_node<'a>(
    element: &'a Element,
    stylesheet: &Stylesheet,
    ancestors: &[&'a Element],
    preceding_siblings: &[&'a Element],
) -> StyledNode<'a> {
    let mut matched: Vec<MatchedDeclaration> = Vec::new();

    for (source_order, rule) in stylesheet.rules.iter().enumerate() {
        let Rule::Style(style_rule) = rule else {
            continue; // Rule::Media ignored in this sub-project, per spec
        };

        let matched_specificities: Vec<(u32, u32, u32)> = style_rule
            .selectors
            .iter()
            .filter(|selector| selector_matches(selector, element, ancestors, preceding_siblings))
            .map(specificity)
            .collect();

        if matched_specificities.is_empty() {
            continue;
        }

        // A rule's declarations apply with the specificity of whichever of its
        // comma-separated selectors matched — use the highest if more than one did.
        let best_specificity = *matched_specificities.iter().max().unwrap();

        for declaration in &style_rule.declarations {
            matched.push(MatchedDeclaration { specificity: best_specificity, source_order, declaration });
        }
    }

    let style = resolve_cascade(&matched);

    let child_elements: Vec<&'a Element> = element
        .children
        .iter()
        .filter_map(|node| match node {
            Node::Element(el) => Some(el),
            Node::Text(_) | Node::Comment(_) => None,
        })
        .collect();

    let mut new_ancestors: Vec<&'a Element> = ancestors.to_vec();
    new_ancestors.push(element);

    let mut children = Vec::new();
    for (i, &child) in child_elements.iter().enumerate() {
        let siblings_before: Vec<&'a Element> = child_elements[..i].to_vec();
        children.push(resolve_node(child, stylesheet, &new_ancestors, &siblings_before));
    }

    StyledNode { element, style, children }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferris_css::stylesheet::{Declaration, MediaRule, Selector, SelectorComponent, SimpleSelector, StyleRule};

    fn type_selector(name: &str) -> Selector {
        Selector {
            components: vec![SelectorComponent::Simple(SimpleSelector {
                type_name: Some(name.to_string()),
                ..Default::default()
            })],
        }
    }

    #[test]
    fn single_element_gets_matching_declaration() {
        let root = Element::new("div");
        let stylesheet = Stylesheet {
            rules: vec![Rule::Style(StyleRule {
                selectors: vec![type_selector("div")],
                declarations: vec![Declaration { property: "color".to_string(), value: "red".to_string() }],
            })],
        };
        let styled = resolve_styles(&root, &stylesheet);
        assert_eq!(styled.style.get("color"), Some(&"red".to_string()));
    }

    #[test]
    fn non_matching_rule_does_not_apply() {
        let root = Element::new("div");
        let stylesheet = Stylesheet {
            rules: vec![Rule::Style(StyleRule {
                selectors: vec![type_selector("span")],
                declarations: vec![Declaration { property: "color".to_string(), value: "red".to_string() }],
            })],
        };
        let styled = resolve_styles(&root, &stylesheet);
        assert!(styled.style.is_empty());
    }

    #[test]
    fn media_rules_are_ignored() {
        let root = Element::new("div");
        let stylesheet = Stylesheet {
            rules: vec![Rule::Media(MediaRule {
                condition: "(min-width: 600px)".to_string(),
                rules: vec![StyleRule {
                    selectors: vec![type_selector("div")],
                    declarations: vec![Declaration { property: "color".to_string(), value: "red".to_string() }],
                }],
            })],
        };
        let styled = resolve_styles(&root, &stylesheet);
        assert!(styled.style.is_empty(), "rules inside @media must be ignored per spec scope");
    }

    #[test]
    fn child_elements_are_styled_and_appear_in_output_tree() {
        let mut root = Element::new("div");
        root.children.push(Node::Element(Element::new("p")));
        let stylesheet = Stylesheet {
            rules: vec![Rule::Style(StyleRule {
                selectors: vec![type_selector("p")],
                declarations: vec![Declaration { property: "margin".to_string(), value: "0".to_string() }],
            })],
        };
        let styled = resolve_styles(&root, &stylesheet);
        assert_eq!(styled.children.len(), 1);
        assert_eq!(styled.children[0].element.tag_name, "p");
        assert_eq!(styled.children[0].style.get("margin"), Some(&"0".to_string()));
    }

    #[test]
    fn text_and_comment_children_are_excluded_from_output_tree() {
        let mut root = Element::new("div");
        root.children.push(Node::Text("hello".to_string()));
        root.children.push(Node::Comment("note".to_string()));
        let stylesheet = Stylesheet::default();
        let styled = resolve_styles(&root, &stylesheet);
        assert!(styled.children.is_empty());
    }

    #[test]
    fn rule_with_multiple_selectors_uses_the_specificity_of_the_matching_one() {
        let mut root = Element::new("div");
        root.attributes.insert("class".to_string(), "a".to_string());
        let class_selector = Selector {
            components: vec![SelectorComponent::Simple(SimpleSelector { classes: vec!["a".to_string()], ..Default::default() })],
        };
        let id_selector = Selector {
            components: vec![SelectorComponent::Simple(SimpleSelector { id: Some("b".to_string()), ..Default::default() })],
        };
        let stylesheet = Stylesheet {
            rules: vec![Rule::Style(StyleRule {
                selectors: vec![class_selector, id_selector],
                declarations: vec![Declaration { property: "color".to_string(), value: "red".to_string() }],
            })],
        };
        let styled = resolve_styles(&root, &stylesheet);
        assert_eq!(styled.style.get("color"), Some(&"red".to_string()));
    }

    // --- Review Focus: empty stylesheet applied to a real element tree ---
    #[test]
    fn empty_stylesheet_yields_empty_style_for_every_element_no_panic() {
        let mut root = Element::new("div");
        root.children.push(Node::Element(Element::new("p")));
        let stylesheet = Stylesheet::default();
        let styled = resolve_styles(&root, &stylesheet);
        assert!(styled.style.is_empty());
        assert!(styled.children[0].style.is_empty());
    }
}
