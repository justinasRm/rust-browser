//! The DOM — the tree a browser builds out of HTML.
//!
//! Everything downstream (CSS matching, layout, painting) walks this tree, so it
//! is the foundation of the whole engine. A document is just a [`Node`], and
//! every node is one of three things: an **element** (`<p>`, `<a>`, …), a run of
//! **text**, or a **comment**. Elements carry a tag name and a bag of
//! attributes, and any node can have children — that recursion is what makes it
//! a tree.
//!
//! We keep the data model deliberately small. A real browser's DOM has dozens of
//! node types and a huge API surface; ours has exactly what layout and painting
//! need to read.

use std::collections::{HashMap, HashSet};

/// An element's attributes, e.g. `{"href": "/about", "class": "nav link"}`.
pub type AttrMap = HashMap<String, String>;

/// A single node in the document tree.
///
/// Children are stored inline (`Vec<Node>`) rather than behind pointers. That
/// keeps ownership simple — a parent owns its children — which is exactly the
/// kind of clear ownership Rust makes pleasant to work with. A production
/// browser needs parent pointers and shared mutability (so JavaScript can poke
/// at nodes), but for a render-only engine a plain tree is all we need.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub children: Vec<Node>,
    pub node_type: NodeType,
}

/// What kind of node this is, and the data specific to it.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeType {
    /// A run of text, e.g. the `Hello` in `<p>Hello</p>`.
    Text(String),
    /// An element, e.g. `<p class="lead">`.
    Element(ElementData),
    /// An HTML comment, `<!-- like this -->`. Kept so the tree round-trips, but
    /// ignored by style and layout.
    Comment(String),
}

/// The data attached to an [`NodeType::Element`].
#[derive(Debug, Clone, PartialEq)]
pub struct ElementData {
    /// Lower-cased tag name, e.g. `"div"`.
    pub tag_name: String,
    pub attributes: AttrMap,
}

// --- Constructors -----------------------------------------------------------
// Small helpers so the HTML parser (and tests) can build nodes readably.

/// Create a text node.
pub fn text(data: impl Into<String>) -> Node {
    Node { children: Vec::new(), node_type: NodeType::Text(data.into()) }
}

/// Create a comment node.
pub fn comment(data: impl Into<String>) -> Node {
    Node { children: Vec::new(), node_type: NodeType::Comment(data.into()) }
}

/// Create an element node with the given tag, attributes and children.
pub fn elem(tag_name: impl Into<String>, attributes: AttrMap, children: Vec<Node>) -> Node {
    Node {
        children,
        node_type: NodeType::Element(ElementData { tag_name: tag_name.into(), attributes }),
    }
}

impl Node {
    /// If this node is an element, return its data.
    pub fn element(&self) -> Option<&ElementData> {
        match &self.node_type {
            NodeType::Element(e) => Some(e),
            _ => None,
        }
    }

    /// The lower-cased tag name, if this is an element.
    pub fn tag_name(&self) -> Option<&str> {
        self.element().map(|e| e.tag_name.as_str())
    }

    /// The text contained directly in this node, if it is a text node.
    pub fn text_content(&self) -> Option<&str> {
        match &self.node_type {
            NodeType::Text(s) => Some(s),
            _ => None,
        }
    }

    /// All text under this node, concatenated. Handy for tests and for `<title>`.
    pub fn inner_text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        if let NodeType::Text(s) = &self.node_type {
            out.push_str(s);
        }
        for child in &self.children {
            child.collect_text(out);
        }
    }
}

impl ElementData {
    /// Look up an attribute by (lower-case) name.
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    /// The element's `id`, if any.
    pub fn id(&self) -> Option<&str> {
        self.get_attribute("id")
    }

    /// The set of `class` names on the element. CSS class selectors match
    /// against this, so it lives here on the element rather than being
    /// re-parsed every time.
    pub fn classes(&self) -> HashSet<&str> {
        match self.get_attribute("class") {
            Some(classlist) => classlist.split_whitespace().collect(),
            None => HashSet::new(),
        }
    }
}

// --- Pretty-printing --------------------------------------------------------
// A tiny tree dumper so you can *see* what the HTML parser produced. The CLI
// exposes this via `--dump-dom`.

/// Render the tree as indented, readable text.
pub fn pretty_print(node: &Node) -> String {
    let mut out = String::new();
    print_node(node, 0, &mut out);
    out
}

fn print_node(node: &Node, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    match &node.node_type {
        NodeType::Text(s) => {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                out.push_str(&format!("{indent}#text {:?}\n", truncate(trimmed, 60)));
            }
        }
        NodeType::Comment(s) => out.push_str(&format!("{indent}<!-- {} -->\n", truncate(s.trim(), 40))),
        NodeType::Element(e) => {
            let mut attrs: Vec<_> = e.attributes.iter().collect();
            attrs.sort_by(|a, b| a.0.cmp(b.0)); // stable output for tests
            let attr_str: String =
                attrs.iter().map(|(k, v)| format!(" {k}=\"{v}\"")).collect();
            out.push_str(&format!("{indent}<{}{}>\n", e.tag_name, attr_str));
        }
    }
    for child in &node.children {
        print_node(child, depth + 1, out);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let taken: String = s.chars().take(max).collect();
        format!("{taken}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> AttrMap {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn elements_expose_id_and_classes() {
        let node = elem("p", attrs(&[("id", "lead"), ("class", "a b c")]), vec![]);
        let e = node.element().unwrap();
        assert_eq!(e.id(), Some("lead"));
        assert!(e.classes().contains("b"));
        assert_eq!(e.classes().len(), 3);
    }

    #[test]
    fn inner_text_concatenates_descendants() {
        let tree = elem(
            "p",
            AttrMap::new(),
            vec![text("Hello, "), elem("b", AttrMap::new(), vec![text("world")]), text("!")],
        );
        assert_eq!(tree.inner_text(), "Hello, world!");
    }

    #[test]
    fn pretty_print_is_stable_and_indented() {
        let tree = elem("div", attrs(&[("class", "box")]), vec![text("hi")]);
        let dump = pretty_print(&tree);
        assert_eq!(dump, "<div class=\"box\">\n  #text \"hi\"\n");
    }
}
