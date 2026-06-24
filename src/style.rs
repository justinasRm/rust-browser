//! Style - turning the DOM + CSS into a **styled tree**.
//!
//! This is "the cascade". For every element we figure out the final value of
//! each CSS property by:
//!
//! 1. Collecting every rule whose selector matches the element.
//! 2. Sorting those matches by **specificity** (and source order) so more
//!    specific rules win.
//! 3. Applying them in order, then layering the element's inline `style="..."`
//!    on top (it beats everything).
//! 4. **Inheriting** properties like `color` and `font-size` from the parent
//!    when the element didn't set them itself.
//!
//! The result is a [`StyledNode`]: the same tree shape as the DOM, but each node
//! now carries a flat map of resolved property values that layout and paint can
//! read without thinking about selectors ever again.
//!
//! A built-in **user-agent stylesheet** ([`user_agent_stylesheet`]) supplies the
//! defaults every browser ships - which elements are blocks, default margins,
//! heading sizes, link colors - so even completely unstyled HTML lays out in a
//! recognizable way.

use std::collections::HashMap;

use crate::css::{self, Rule, Selector, SimpleSelector, Specificity, Stylesheet, Value};
use crate::dom::{ElementData, Node, NodeType};

/// A node's fully-resolved property values, keyed by lower-case property name.
pub type PropertyMap = HashMap<String, Value>;

/// The DOM, annotated with computed style.
#[derive(Debug, Clone)]
pub struct StyledNode<'a> {
    pub node: &'a Node,
    pub specified_values: PropertyMap,
    pub children: Vec<StyledNode<'a>>,
}

/// How an element participates in layout. Driven by the `display` property
/// (mostly from the user-agent stylesheet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Inline,
    Block,
    ListItem,
    None,
}

impl<'a> StyledNode<'a> {
    /// Look up a property's computed value.
    pub fn value(&self, name: &str) -> Option<Value> {
        self.specified_values.get(name).cloned()
    }

    /// A property's value, falling back to a default if unset.
    pub fn lookup(&self, name: &str, fallback: &Value) -> Value {
        self.value(name).unwrap_or_else(|| fallback.clone())
    }

    /// The element's `display`, defaulting to `inline` (the CSS default).
    pub fn display(&self) -> Display {
        match self.value("display").as_ref().and_then(Value::keyword) {
            Some("block") => Display::Block,
            Some("list-item") => Display::ListItem,
            Some("none") => Display::None,
            Some("inline") => Display::Inline,
            // Anything we don't model (flex, grid, table, inline-block, …) is
            // treated as block so the content still stacks and stays readable.
            Some(_) => Display::Block,
            None => Display::Inline,
        }
    }
}

/// Build the styled tree for a document, combining the user-agent stylesheet
/// with the page's author stylesheet.
pub fn style_tree<'a>(root: &'a Node, author: &Stylesheet) -> StyledNode<'a> {
    let ua = user_agent_stylesheet();
    style_node(root, &ua, author, &PropertyMap::new())
}

fn style_node<'a>(
    node: &'a Node,
    ua: &Stylesheet,
    author: &Stylesheet,
    inherited: &PropertyMap,
) -> StyledNode<'a> {
    let specified = match &node.node_type {
        NodeType::Element(elem) => specified_values(elem, ua, author, inherited),
        // Text nodes inherit everything (color, font, …) from their parent.
        NodeType::Text(_) | NodeType::Comment(_) => inherited.clone(),
    };

    // Compute what *this* node passes down to its children.
    let to_inherit = inheritable_subset(&specified);

    let children = node
        .children
        .iter()
        .map(|child| style_node(child, ua, author, &to_inherit))
        .collect();

    StyledNode {
        node,
        specified_values: specified,
        children,
    }
}

/// Resolve one element's properties: inherited values first, then matched rules
/// in cascade order, then the inline `style` attribute.
fn specified_values(
    elem: &ElementData,
    ua: &Stylesheet,
    author: &Stylesheet,
    inherited: &PropertyMap,
) -> PropertyMap {
    let mut values: PropertyMap = inherited.clone();

    // Presentational attributes (bgcolor, width, height) are old HTML's way of
    // carrying style. Browsers map them to low-priority CSS so author rules can
    // still override them. Hacker News's orange bar is a `bgcolor` attribute.
    apply_presentational_hints(elem, &mut values);

    // Gather (specificity, rule) for every matching rule, UA before author so
    // author rules win ties. A stable sort then keeps that source order.
    let mut matches: Vec<(Specificity, &Rule)> = Vec::new();
    for rule in ua.rules.iter().chain(author.rules.iter()) {
        if let Some(spec) = match_rule(elem, rule) {
            matches.push((spec, rule));
        }
    }
    matches.sort_by(|a, b| a.0.cmp(&b.0)); // ascending: low specificity applied first

    for (_, rule) in matches {
        for decl in &rule.declarations {
            values.insert(decl.name.clone(), decl.value.clone());
        }
    }

    // Inline styles beat any selector.
    if let Some(style_attr) = elem.get_attribute("style") {
        for decl in css::parse_declarations(style_attr) {
            values.insert(decl.name, decl.value);
        }
    }

    values
}

/// Map a few legacy presentational attributes onto CSS properties.
fn apply_presentational_hints(elem: &ElementData, values: &mut PropertyMap) {
    let mut set = |prop: &str, raw: &str| {
        for decl in css::parse_declarations(&format!("{prop}: {raw}")) {
            values.insert(decl.name, decl.value);
        }
    };
    if let Some(bg) = elem.get_attribute("bgcolor") {
        set("background-color", &normalize_color(bg));
    }
    if let Some(w) = elem.get_attribute("width") {
        set("width", w);
    }
    if let Some(h) = elem.get_attribute("height") {
        set("height", h);
    }
}

/// `bgcolor="ff6600"` (no `#`) is valid in old HTML; CSS needs the hash.
fn normalize_color(raw: &str) -> String {
    let t = raw.trim();
    let is_bare_hex = !t.is_empty()
        && !t.starts_with('#')
        && t.len() <= 6
        && t.chars().all(|c| c.is_ascii_hexdigit());
    if is_bare_hex {
        format!("#{t}")
    } else {
        t.to_string()
    }
}

/// If any selector in the rule matches the element, return the best specificity.
fn match_rule(elem: &ElementData, rule: &Rule) -> Option<Specificity> {
    rule.selectors
        .iter()
        .filter(|sel| matches_selector(elem, sel))
        .map(Selector::specificity)
        .max()
}

fn matches_selector(elem: &ElementData, selector: &Selector) -> bool {
    let Selector::Simple(simple) = selector;
    matches_simple(elem, simple)
}

fn matches_simple(elem: &ElementData, selector: &SimpleSelector) -> bool {
    // Tag name must match if the selector names one.
    if let Some(tag) = &selector.tag_name {
        if *tag != elem.tag_name {
            return false;
        }
    }
    // Id must match if present.
    if selector.id.is_some() && selector.id.as_deref() != elem.id() {
        return false;
    }
    // Every class in the selector must be on the element.
    let elem_classes = elem.classes();
    if selector
        .classes
        .iter()
        .any(|c| !elem_classes.contains(c.as_str()))
    {
        return false;
    }
    true
}

/// The properties that pass from parent to child unless overridden.
const INHERITED_PROPERTIES: &[&str] = &[
    "color",
    "font-size",
    "font-weight",
    "font-style",
    "font-family",
    "line-height",
    "text-align",
    "white-space",
    "list-style-type",
    "visibility",
];

fn inheritable_subset(values: &PropertyMap) -> PropertyMap {
    INHERITED_PROPERTIES
        .iter()
        .filter_map(|&prop| values.get(prop).map(|v| (prop.to_string(), v.clone())))
        .collect()
}

/// Collect a page's author CSS by concatenating the text of every `<style>`
/// element and parsing it. (Linked `<link rel=stylesheet>` files are fetched in
/// the networking chapter; this handles inline `<style>` blocks.)
pub fn document_stylesheet(dom: &Node) -> Stylesheet {
    css::parse(&inline_css(dom))
}

/// The concatenated text of every inline `<style>` block in the document.
pub fn inline_css(dom: &Node) -> String {
    let mut text = String::new();
    collect_style_text(dom, &mut text);
    text
}

fn collect_style_text(node: &Node, out: &mut String) {
    if node.tag_name() == Some("style") {
        out.push_str(&node.inner_text());
        out.push('\n');
    }
    for child in &node.children {
        collect_style_text(child, out);
    }
}

/// The built-in defaults every browser ships. Parsed once into a [`Stylesheet`].
///
/// Keeping it as real CSS (rather than hard-coded Rust) means the same parser is
/// exercised, and you can read the defaults the way you'd read any stylesheet.
pub fn user_agent_stylesheet() -> Stylesheet {
    css::parse(UA_CSS)
}

const UA_CSS: &str = r#"
/* Robin's user-agent stylesheet: the defaults that make plain HTML readable. */

html, body, div, section, article, header, footer, nav, aside, main, figure,
h1, h2, h3, h4, h5, h6, p, ul, ol, li, dl, dt, dd, blockquote, pre, form,
table, thead, tbody, tfoot, tr, hr, address, fieldset, figcaption, center,
main, details, summary, dir, menu, caption {
    display: block;
}

/* Things that should never render. */
head, script, style, meta, link, title, noscript, template, base, param {
    display: none;
}

li { display: list-item; }

body { margin: 8px; color: #000000; font-size: 16px; line-height: 1.3; }

h1 { font-size: 32px; font-weight: bold; margin: 16px; }
h2 { font-size: 24px; font-weight: bold; margin: 14px; }
h3 { font-size: 20px; font-weight: bold; margin: 12px; }
h4 { font-size: 16px; font-weight: bold; margin: 12px; }
h5 { font-size: 14px; font-weight: bold; margin: 12px; }
h6 { font-size: 12px; font-weight: bold; margin: 12px; }

p, ul, ol, dl, blockquote, pre { margin: 12px; }
ul, ol { padding: 24px; }
li { margin: 2px; }
blockquote { padding: 8px; }

b, strong, th { font-weight: bold; }
i, em { font-style: italic; }
small { font-size: 13px; }
pre, code, tt { font-family: monospace; }

a { color: #0000ee; }

hr { margin: 8px; padding: 1px; background: #cccccc; }

td, th { padding: 2px; }

/* Make headers/cells in table-based layouts (Hacker News!) behave like blocks
   so their content still stacks readably even without real table layout. */
table, tbody, thead, tr { display: block; }
td, th { display: block; }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html;

    fn style(html_src: &str, css_src: &str) -> String {
        // Helper that returns a debug dump of (tag -> color) for inspection.
        let dom = html::parse(html_src);
        let sheet = css::parse(css_src);
        let styled = style_tree(&dom, &sheet);
        format!("{:?}", styled.value("color"))
    }

    #[test]
    fn author_rules_override_user_agent() {
        let dom = html::parse("<a href=x>link</a>");
        let sheet = css::parse("a { color: red; }");
        let styled = style_tree(&dom, &sheet);
        // Walk to the <a>.
        let body_or_a = find_tag(&styled, "a").expect("an <a> in the tree");
        assert_eq!(
            body_or_a.value("color"),
            Some(Value::ColorValue(css::Color::rgb(255, 0, 0)))
        );
    }

    #[test]
    fn specificity_decides_between_author_rules() {
        let dom = html::parse("<p id=lead class=intro>hi</p>");
        let sheet = css::parse("p { color: red; } .intro { color: green; } #lead { color: blue; }");
        let styled = style_tree(&dom, &sheet);
        let p = find_tag(&styled, "p").unwrap();
        assert_eq!(
            p.value("color"),
            Some(Value::ColorValue(css::Color::rgb(0, 0, 255)))
        );
    }

    #[test]
    fn inline_style_wins() {
        let dom = html::parse("<p id=lead style='color: orange'>hi</p>");
        let sheet = css::parse("#lead { color: blue; }");
        let styled = style_tree(&dom, &sheet);
        let p = find_tag(&styled, "p").unwrap();
        assert_eq!(
            p.value("color"),
            Some(Value::ColorValue(css::Color::rgb(255, 165, 0)))
        );
    }

    #[test]
    fn color_is_inherited() {
        let dom = html::parse("<div style='color: green'><span>child</span></div>");
        let sheet = css::parse("");
        let styled = style_tree(&dom, &sheet);
        let span = find_tag(&styled, "span").unwrap();
        assert_eq!(
            span.value("color"),
            Some(Value::ColorValue(css::Color::rgb(0, 128, 0)))
        );
    }

    #[test]
    fn ua_stylesheet_sets_display() {
        let dom = html::parse("<div>a</div><span>b</span><script>x</script>");
        let sheet = css::parse("");
        let styled = style_tree(&dom, &sheet);
        assert_eq!(find_tag(&styled, "div").unwrap().display(), Display::Block);
        assert_eq!(
            find_tag(&styled, "span").unwrap().display(),
            Display::Inline
        );
        assert_eq!(
            find_tag(&styled, "script").unwrap().display(),
            Display::None
        );
    }

    #[test]
    fn presentational_attributes_become_styles() {
        let dom = html::parse("<table bgcolor=#ff6600 width=85%><tr><td>x</td></tr></table>");
        let styled = style_tree(&dom, &css::parse(""));
        let table = find_tag(&styled, "table").unwrap();
        assert_eq!(
            table.value("background-color"),
            Some(Value::ColorValue(css::Color::rgb(255, 102, 0)))
        );
        assert_eq!(
            table.value("width"),
            Some(Value::Length(85.0, css::Unit::Percent))
        );
    }

    #[test]
    fn author_css_overrides_presentational_attribute() {
        let dom = html::parse("<td bgcolor=red>x</td>");
        let styled = style_tree(&dom, &css::parse("td { background-color: green; }"));
        let td = find_tag(&styled, "td").unwrap();
        assert_eq!(
            td.value("background-color"),
            Some(Value::ColorValue(css::Color::rgb(0, 128, 0)))
        );
    }

    #[test]
    fn unmatched_color_is_none_at_root() {
        assert_eq!(style("<html></html>", ""), "None");
    }

    // Depth-first search for the first element with the given tag.
    fn find_tag<'a, 'b>(node: &'b StyledNode<'a>, tag: &str) -> Option<&'b StyledNode<'a>> {
        if node.node.tag_name() == Some(tag) {
            return Some(node);
        }
        node.children.iter().find_map(|c| find_tag(c, tag))
    }
}
