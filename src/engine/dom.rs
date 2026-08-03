use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub root: Node,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub tag_name: String,
    pub attributes: HashMap<String, String>,
}

impl Document {
    pub fn new(root: Node) -> Self {
        Self { root }
    }
}

impl Node {
    pub fn element(tag_name: impl Into<String>, children: Vec<Node>) -> Self {
        Self::element_with_attributes(tag_name, HashMap::new(), children)
    }

    pub fn element_with_attributes(
        tag_name: impl Into<String>,
        attributes: HashMap<String, String>,
        children: Vec<Node>,
    ) -> Self {
        Self {
            kind: NodeKind::Element(Element {
                tag_name: tag_name.into().to_ascii_lowercase(),
                attributes,
            }),
            children,
        }
    }

    pub fn text(contents: impl Into<String>) -> Self {
        Self {
            kind: NodeKind::Text(contents.into()),
            children: Vec::new(),
        }
    }
}
