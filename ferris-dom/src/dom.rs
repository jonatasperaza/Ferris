use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub tag_name: String,
    pub attributes: HashMap<String, String>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn new(tag_name: impl Into<String>) -> Self {
        Self { tag_name: tag_name.into(), attributes: HashMap::new(), children: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_element_has_given_tag_name_and_empty_attributes_and_children() {
        let el = Element::new("div");
        assert_eq!(el.tag_name, "div");
        assert!(el.attributes.is_empty());
        assert!(el.children.is_empty());
    }

    #[test]
    fn new_accepts_both_str_and_string() {
        let from_str = Element::new("p");
        let from_string = Element::new(String::from("p"));
        assert_eq!(from_str.tag_name, from_string.tag_name);
    }
}
