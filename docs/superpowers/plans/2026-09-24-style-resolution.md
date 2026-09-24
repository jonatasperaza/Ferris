# Style Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a new `ferris-style` crate (fourth member of the existing Cargo workspace) that matches `ferris-css` `Selector`s against the `ferris-dom` `Element` tree, computes CSS specificity, applies `!important`, resolves the cascade, and produces a `StyledNode` tree carrying each element's computed `HashMap<String, String>` style.

**Architecture:** A top-down recursive walk over the `Element` tree (no parent pointers needed in `ferris-dom` — the walk carries the ancestor chain and preceding-sibling list as recursion parameters). At each element, every top-level `StyleRule`'s selectors are tested via right-to-left combinator matching; matches are collected with their specificity and source order, then merged into a final style map by a stable sort that lets later, higher-priority entries overwrite earlier ones.

**Tech Stack:** Rust 2021, zero external dependencies — path dependencies on `ferris-dom` and `ferris-css` only, matching the established workspace pattern.

**Spec:** `docs/superpowers/specs/2026-09-24-style-resolution-design.md`

## Global Constraints

- New workspace crate `ferris-style`, zero external dependencies (path deps on `ferris-dom`/`ferris-css` only).
- Values stay opaque `String`s — no unit/color parsing, no property validation.
- Rules inside `Rule::Media` are ignored entirely in this sub-project.
- No property inheritance between elements — each `StyledNode`'s style contains only declarations that matched that element directly.
- Known, documented limitation (not fixed in this plan): selector chains of 3+ combinators mixing ancestor-type (`Descendant`/`Child`) and sibling-type (`NextSibling`/`SubsequentSibling`) combinators may fail to match correctly, because the walk only threads preceding-sibling context for the immediately-matched element, not for every ancestor level. Chains of up to 2 combinators work correctly.
- `id`/`class` matching reads the HTML `id`/`class` attributes off `Element.attributes` (there is no dedicated `id` field on `ferris-dom`'s `Element` — ids are ordinary attributes, matching how real HTML/DOM works).

## Review Focus

Five input classes the spec implies but no task's tests would catch by accident — each gets an explicit test in the task that owns the code:

- **Attribute-value selectors (`[attr=value]`) require exact string equality, not substring/token matching** (unlike `.class`, which matches if the class is one of several space-separated tokens) — e.g. `[data-x=card]` must NOT match `data-x="card featured"`. Owned by Task 2.
- **A class/id/attribute selector against an element with no such attribute at all** (not just a non-matching value) must not panic and must simply not match. Owned by Task 2.
- **Multiple declarations for the same property within a single matched rule** (e.g. `p { color: red; color: blue; }`) — the later one in source order must win, even though both declarations share identical specificity and rule source-order (the tie is broken by their position within the declarations list itself, which Rust's stable sort preserves). Owned by Task 4.
- **Empty stylesheet (zero rules) applied to a real element tree** must produce an all-empty style map for every element, not panic. Owned by Task 5.
- **Class/id/attribute-VALUE matching is case-sensitive**, relying on the cross-crate contract that `ferris-dom` lowercases tag/attribute *names* (not values) and `ferris-css` lowercases selector type/attribute *names* (not class names, ids, or values) — a selector `.Card` must match an element with `class="Card"` and NOT match `class="card"`. This only surfaces when real parsed output from both crates is combined, so it's owned by Task 5's integration tests.

---

### Task 1: Add `ferris-style` as the fourth workspace member

**Files:**
- Modify: `Cargo.toml` (root workspace manifest)
- Create: `ferris-style/Cargo.toml`
- Create: `ferris-style/src/lib.rs`

**Interfaces:**
- Consumes: nothing — pure scaffolding.
- Produces: a working four-member Cargo workspace (`ferris-compositor`, `ferris-dom`, `ferris-css`, `ferris-style`). Tasks 2-5 add modules inside `ferris-style/src/`.

- [ ] **Step 1: Update the root workspace manifest**

Read the current root `Cargo.toml` first to confirm its exact contents, then add `"ferris-style"` to `members`:

```toml
[workspace]
members = ["ferris-compositor", "ferris-dom", "ferris-css", "ferris-style"]
resolver = "2"
```

- [ ] **Step 2: Create the `ferris-style` crate**

Create `ferris-style/Cargo.toml`:

```toml
[package]
name = "ferris-style"
version = "0.1.0"
edition = "2021"

[dependencies]
ferris-dom = { path = "../ferris-dom" }
ferris-css = { path = "../ferris-css" }
```

Create `ferris-style/src/lib.rs`:

```rust
// Modules are added by later tasks in this plan (matcher, specificity, cascade).
```

- [ ] **Step 3: Build and verify no regressions**

Run: `cargo build --workspace`
Expected: compiles clean — `ferris-compositor`, `ferris-dom`, `ferris-css` (unchanged) and the new empty `ferris-style` (which now successfully resolves its path dependencies on `ferris-dom`/`ferris-css`) all build.

Run: `cargo test --workspace`
Expected: `ferris-compositor` (24), `ferris-dom` (22), `ferris-css` (39) all still pass unchanged; `ferris-style` reports 0 tests.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml ferris-style
git commit -m "feat: add ferris-style crate as fourth workspace member"
```

---

### Task 2: Selector matcher

**Files:**
- Create: `ferris-style/src/matcher.rs`
- Modify: `ferris-style/src/lib.rs` (add `pub mod matcher;`)

**Interfaces:**
- Consumes: `ferris_dom::dom::Element`, `ferris_css::stylesheet::{Selector, SelectorComponent, SimpleSelector, Combinator}`.
- Produces: `matcher::element_matches_simple(element: &Element, simple: &SimpleSelector) -> bool`, `matcher::selector_matches(selector: &Selector, element: &Element, ancestors: &[&Element], preceding_siblings: &[&Element]) -> bool`. Used by Task 5's `resolve_styles`.

This task covers 2 of the 5 Review Focus items — each gets its own test below.

- [ ] **Step 1: Write the failing tests**

Create `ferris-style/src/matcher.rs`:

```rust
use ferris_css::stylesheet::{Combinator, Selector, SelectorComponent, SimpleSelector};
use ferris_dom::dom::Element;

pub fn element_matches_simple(element: &Element, simple: &SimpleSelector) -> bool {
    todo!()
}

pub fn selector_matches(
    selector: &Selector,
    element: &Element,
    ancestors: &[&Element],
    preceding_siblings: &[&Element],
) -> bool {
    todo!()
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-style matcher::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement `element_matches_simple` and `selector_matches`**

Replace both `todo!()`s and add the private recursive helper:

```rust
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
```

Note on the `ancestors`/`preceding_siblings` reset to `&[]` when recursing via `Child`/`Descendant`: this is the plan's documented, deliberate scope limit (see Global Constraints) — a selector chain needing a preceding sibling of an *ancestor* (not of the originally-matched element) won't resolve correctly. Chains of up to 2 combinators are unaffected.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-style matcher::tests`
Expected: 13 passed.

- [ ] **Step 5: Register the module**

In `ferris-style/src/lib.rs`, replace the placeholder comment with:

```rust
pub mod matcher;
```

- [ ] **Step 6: Build and commit**

Run: `cargo build --workspace` — expected clean.

```bash
git add ferris-style/src/matcher.rs ferris-style/src/lib.rs
git commit -m "feat: add CSS selector matcher (simple selectors + all 4 combinators)"
```

---

### Task 3: Specificity calculation

**Files:**
- Create: `ferris-style/src/specificity.rs`
- Modify: `ferris-style/src/lib.rs` (add `pub mod specificity;`)

**Interfaces:**
- Consumes: `ferris_css::stylesheet::{Selector, SelectorComponent}`.
- Produces: `specificity::specificity(selector: &Selector) -> (u32, u32, u32)` (comparable via `Ord`/`PartialOrd` on the tuple — Rust compares tuples lexicographically, matching CSS's own a-b-c specificity model exactly). Used by Task 5's `resolve_styles`.

- [ ] **Step 1: Write the failing tests**

Create `ferris-style/src/specificity.rs`:

```rust
use ferris_css::stylesheet::{Selector, SelectorComponent};

pub fn specificity(selector: &Selector) -> (u32, u32, u32) {
    todo!()
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-style specificity::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement `specificity`**

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-style specificity::tests`
Expected: 7 passed.

- [ ] **Step 5: Register the module**

In `ferris-style/src/lib.rs`, add:

```rust
pub mod specificity;
```

- [ ] **Step 6: Build and commit**

Run: `cargo build --workspace` — expected clean.

```bash
git add ferris-style/src/specificity.rs ferris-style/src/lib.rs
git commit -m "feat: add CSS specificity calculation"
```

---

### Task 4: Cascade resolution

**Files:**
- Create: `ferris-style/src/cascade.rs`
- Modify: `ferris-style/src/lib.rs` (add `pub mod cascade;`)

**Interfaces:**
- Consumes: `ferris_css::stylesheet::Declaration`.
- Produces: `cascade::MatchedDeclaration<'a> { specificity: (u32, u32, u32), source_order: usize, declaration: &'a Declaration }`, `cascade::resolve_cascade(matches: &[MatchedDeclaration]) -> HashMap<String, String>`. Used by Task 5's `resolve_styles`.

This task covers 1 of the 5 Review Focus items.

- [ ] **Step 1: Write the failing tests**

Create `ferris-style/src/cascade.rs`:

```rust
use std::collections::HashMap;

use ferris_css::stylesheet::Declaration;

pub struct MatchedDeclaration<'a> {
    pub specificity: (u32, u32, u32),
    pub source_order: usize,
    pub declaration: &'a Declaration,
}

pub fn resolve_cascade(matches: &[MatchedDeclaration]) -> HashMap<String, String> {
    todo!()
}

fn split_important(value: &str) -> (String, bool) {
    todo!()
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-style cascade::tests`
Expected: FAIL (panics on `todo!()`).

- [ ] **Step 3: Implement `resolve_cascade` and `split_important`**

```rust
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
```

Note: `entries.sort_by` uses Rust's stable sort (`Vec::sort_by` is guaranteed stable), which is why the same-specificity-same-source-order test in Step 1 works correctly — entries that compare equal keep their original relative order from the input slice.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-style cascade::tests`
Expected: 7 passed.

- [ ] **Step 5: Register the module**

In `ferris-style/src/lib.rs`, add:

```rust
pub mod cascade;
```

- [ ] **Step 6: Build and commit**

Run: `cargo build --workspace` — expected clean.

```bash
git add ferris-style/src/cascade.rs ferris-style/src/lib.rs
git commit -m "feat: add cascade resolution with !important support"
```

---

### Task 5: `StyledNode`, `resolve_styles`, and integration tests

**Files:**
- Modify: `ferris-style/src/lib.rs` (add `StyledNode`, `resolve_styles`, and their tests)
- Create: `ferris-style/tests/integration.rs`

**Interfaces:**
- Consumes: `matcher::selector_matches` (Task 2), `specificity::specificity` (Task 3), `cascade::{resolve_cascade, MatchedDeclaration}` (Task 4), `ferris_dom::dom::{Node, Element}`, `ferris_css::stylesheet::{Stylesheet, Rule}`.
- Produces: `StyledNode<'a> { element: &'a Element, style: HashMap<String, String>, children: Vec<StyledNode<'a>> }`, `resolve_styles<'a>(root: &'a Element, stylesheet: &Stylesheet) -> StyledNode<'a>`. This is the last task in this plan — nothing downstream depends on further changes here.

This task covers 2 of the 5 Review Focus items.

- [ ] **Step 1: Write the failing tests**

Replace the entire contents of `ferris-style/src/lib.rs` with:

```rust
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
    todo!()
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package ferris-style`
Expected: FAIL (panics on `todo!()` in `resolve_styles`). The other modules' tests (matcher, specificity, cascade) still pass — only `lib.rs`'s new tests fail.

- [ ] **Step 3: Implement `resolve_styles`**

Replace the `todo!()`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package ferris-style`
Expected: all pass — 13 (matcher) + 7 (specificity) + 7 (cascade) + 7 (lib.rs, including this task's 7) = 34.

- [ ] **Step 5: Write the integration tests (spec's stated success criterion)**

Create `ferris-style/tests/integration.rs`:

```rust
use ferris_css::parser::Parser as CssParser;
use ferris_css::tokenizer::Tokenizer as CssTokenizer;
use ferris_dom::dom::Node;
use ferris_dom::parser::Parser as DomParser;
use ferris_dom::tokenizer::Tokenizer as DomTokenizer;
use ferris_style::resolve_styles;

fn parse_html(html: &str) -> Node {
    let tokens = DomTokenizer::tokenize(html);
    DomParser::parse(&tokens)
}

fn parse_css(css: &str) -> ferris_css::stylesheet::Stylesheet {
    let tokens = CssTokenizer::tokenize(css);
    CssParser::parse(&tokens)
}

#[test]
fn resolves_composite_selector_specificity_and_important_together() {
    let html = r#"<div class="card"><p id="lead">Hello</p><span>World</span></div>"#;
    let Node::Element(root) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(div) = &root.children[0] else { panic!("expected div") };

    let css = r#"
        p { color: blue; }
        #lead { color: green; }
        .card p { color: red !important; }
    "#;
    let stylesheet = parse_css(css);

    let styled_div = resolve_styles(div, &stylesheet);
    let styled_p = &styled_div.children[0];
    assert_eq!(styled_p.element.tag_name, "p");
    assert_eq!(
        styled_p.style.get("color"),
        Some(&"red".to_string()),
        "important descendant-combinator rule should win over higher-specificity #lead"
    );
}

#[test]
fn resolves_next_sibling_combinator_against_real_parsed_html() {
    let html = r#"<div><h1>Title</h1><p>First</p></div>"#;
    let Node::Element(root) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(div) = &root.children[0] else { panic!("expected div") };

    let css = "h1 + p { margin-top: 0; }";
    let stylesheet = parse_css(css);

    let styled_div = resolve_styles(div, &stylesheet);
    let styled_p = &styled_div.children[1];
    assert_eq!(styled_p.element.tag_name, "p");
    assert_eq!(styled_p.style.get("margin-top"), Some(&"0".to_string()));
}

// --- Review Focus: class/id/attribute-VALUE matching is case-sensitive across the
// ferris-dom/ferris-css crate boundary (only tag/attribute NAMES are lowercased by
// either crate — values and class/id content are preserved as-written) ---
#[test]
fn class_value_matching_is_case_sensitive_across_the_dom_css_boundary() {
    let html = r#"<div class="Card">content</div>"#;
    let Node::Element(root) = parse_html(html) else { panic!("expected document element") };
    let Node::Element(div) = &root.children[0] else { panic!("expected div") };

    let matching_css = parse_css(".Card { color: red; }");
    let styled = resolve_styles(div, &matching_css);
    assert_eq!(styled.style.get("color"), Some(&"red".to_string()), "exact-case class selector must match");

    let wrong_case_css = parse_css(".card { color: blue; }");
    let styled_wrong = resolve_styles(div, &wrong_case_css);
    assert!(styled_wrong.style.is_empty(), "class matching is case-sensitive — .card must not match class=\"Card\"");
}
```

- [ ] **Step 6: Run all tests to verify everything passes**

Run: `cargo test --package ferris-style`
Expected: 37 passed total (34 lib + 3 integration).

Run: `cargo test --workspace`
Expected: `ferris-compositor` (24) + `ferris-dom` (22) + `ferris-css` (39) + `ferris-style` (37) = 122 passing, nothing broken by adding the new crate.

- [ ] **Step 7: Commit**

```bash
git add ferris-style/src/lib.rs ferris-style/tests/integration.rs
git commit -m "feat: add StyledNode/resolve_styles and integration tests"
```
