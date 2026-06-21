//! Layout — deciding where every box goes and how big it is.
//!
//! Style told us *what* each element looks like; layout decides *where* it sits
//! and *how large* it is. The unit of layout is the **box**, and every box has
//! four nested rectangles — the **box model**:
//!
//! ```text
//!   ┌─────────────── margin ───────────────┐
//!   │   ┌─────────── border ───────────┐   │
//!   │   │   ┌─────── padding ───────┐   │   │
//!   │   │   │       content         │   │   │
//!   │   │   └───────────────────────┘   │   │
//!   │   └───────────────────────────────┘   │
//!   └───────────────────────────────────────┘
//! ```
//!
//! This module builds a **layout tree** from the styled tree and runs **block
//! layout**: block boxes stack top to bottom, each as wide as its container,
//! growing tall enough to hold its children. (Inline boxes and text flow arrive
//! once we can measure glyphs — see the text chapter.)
//!
//! The algorithm follows Matt Brubeck's *robinson*: compute a block's width from
//! its container, then its position, then lay out its children to discover its
//! height.

use crate::style::{Display, StyledNode};

/// A rectangle in CSS pixels. The origin is the top-left of the page.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// Grow a rectangle outward on all sides by the given edge sizes.
    pub fn expanded_by(self, edge: EdgeSizes) -> Rect {
        Rect {
            x: self.x - edge.left,
            y: self.y - edge.top,
            width: self.width + edge.left + edge.right,
            height: self.height + edge.top + edge.bottom,
        }
    }
}

/// The thickness of one of the box-model layers on each side.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeSizes {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// A box's full geometry: the content rectangle plus the three surrounding rings.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Dimensions {
    /// Position and size of the content area (relative to the page origin).
    pub content: Rect,
    pub padding: EdgeSizes,
    pub border: EdgeSizes,
    pub margin: EdgeSizes,
}

impl Dimensions {
    /// The content area plus padding.
    pub fn padding_box(self) -> Rect {
        self.content.expanded_by(self.padding)
    }
    /// The padding box plus border — i.e. the visible box edge.
    pub fn border_box(self) -> Rect {
        self.padding_box().expanded_by(self.border)
    }
    /// The border box plus margin — the space the box reserves in flow.
    pub fn margin_box(self) -> Rect {
        self.border_box().expanded_by(self.margin)
    }
}

/// One node of the layout tree.
#[derive(Debug, Clone)]
pub struct LayoutBox<'a> {
    pub dimensions: Dimensions,
    pub box_type: BoxType<'a>,
    pub children: Vec<LayoutBox<'a>>,
}

#[derive(Debug, Clone)]
pub enum BoxType<'a> {
    Block(&'a StyledNode<'a>),
    Inline(&'a StyledNode<'a>),
    /// A wrapper a block box generates to hold a run of inline children, so that
    /// "blocks contain only blocks, or only inlines" stays true.
    Anonymous,
}

impl<'a> LayoutBox<'a> {
    fn new(box_type: BoxType<'a>) -> Self {
        LayoutBox { box_type, dimensions: Dimensions::default(), children: Vec::new() }
    }

    /// The styled node this box came from, if any (anonymous boxes have none).
    pub fn styled_node(&self) -> Option<&'a StyledNode<'a>> {
        match self.box_type {
            BoxType::Block(node) | BoxType::Inline(node) => Some(node),
            BoxType::Anonymous => None,
        }
    }
}

/// Build the layout tree and lay it out inside a viewport.
///
/// The viewport's width is fixed (the window/PNG width); its height grows to fit
/// the content, which is what lets a page be taller than the screen and scroll.
pub fn layout_tree<'a>(node: &'a StyledNode<'a>, mut viewport: Dimensions) -> LayoutBox<'a> {
    viewport.content.height = 0.0; // height is discovered from content
    let mut root = build_layout_tree(node);
    root.layout(viewport);
    root
}

/// Translate the styled tree into a tree of boxes, inserting anonymous boxes to
/// hold inline runs.
pub fn build_layout_tree<'a>(style_node: &'a StyledNode<'a>) -> LayoutBox<'a> {
    let mut root = LayoutBox::new(match style_node.display() {
        Display::Block | Display::ListItem => BoxType::Block(style_node),
        Display::Inline => BoxType::Inline(style_node),
        Display::None => BoxType::Block(style_node), // root is forced visible
    });

    for child in &style_node.children {
        match child.display() {
            Display::Block | Display::ListItem => root.children.push(build_layout_tree(child)),
            Display::Inline => root.inline_container().children.push(build_layout_tree(child)),
            Display::None => {} // skip display:none entirely
        }
    }
    root
}

impl<'a> LayoutBox<'a> {
    /// Where new inline children go: the last anonymous box, created on demand.
    fn inline_container(&mut self) -> &mut LayoutBox<'a> {
        let needs_new = !matches!(
            self.children.last().map(|c| &c.box_type),
            Some(BoxType::Anonymous) | Some(BoxType::Inline(_))
        );
        if needs_new {
            self.children.push(LayoutBox::new(BoxType::Anonymous));
        }
        self.children.last_mut().unwrap()
    }

    /// Lay this box (and its subtree) out inside its containing block.
    pub fn layout(&mut self, containing_block: Dimensions) {
        match self.box_type {
            BoxType::Block(_) => self.layout_block(containing_block),
            // Anonymous boxes participate in block flow as full-width strips.
            // Real inline layout (text wrapping) is added in the text chapter;
            // for now an inline/anonymous box is a zero-height placeholder.
            BoxType::Anonymous | BoxType::Inline(_) => self.layout_block(containing_block),
        }
    }

    fn layout_block(&mut self, containing_block: Dimensions) {
        // Width depends on the container, so compute it top-down first...
        self.calculate_block_width(containing_block);
        self.calculate_block_position(containing_block);
        // ...then lay out children, whose total height feeds our height.
        self.layout_block_children();
        self.calculate_block_height();
    }

    /// Resolve width and the horizontal margins/border/padding.
    fn calculate_block_width(&mut self, containing_block: Dimensions) {
        let style = self.style();
        let zero = Length(0.0);

        let mut width = style
            .and_then(|s| length_px(s, "width"))
            .map(Length)
            .unwrap_or(Auto);

        let margin_left = lookup_len(style, &["margin-left", "margin"], &zero);
        let margin_right = lookup_len(style, &["margin-right", "margin"], &zero);
        let border_left = lookup_len(style, &["border-left-width", "border-width", "border"], &zero);
        let border_right =
            lookup_len(style, &["border-right-width", "border-width", "border"], &zero);
        let padding_left = lookup_len(style, &["padding-left", "padding"], &zero);
        let padding_right = lookup_len(style, &["padding-right", "padding"], &zero);

        let total: f32 = [
            &margin_left, &margin_right, &border_left, &border_right, &padding_left, &padding_right,
            &width,
        ]
        .iter()
        .map(|v| v.to_px())
        .sum();

        let mut margin_left = margin_left;
        let mut margin_right = margin_right;

        // If the box is wider than its container and margins are auto, clamp.
        if width != Auto && total > containing_block.content.width {
            if margin_left == Auto {
                margin_left = Length(0.0);
            }
            if margin_right == Auto {
                margin_right = Length(0.0);
            }
        }

        let underflow = containing_block.content.width - total;
        match (width == Auto, margin_left == Auto, margin_right == Auto) {
            // Over-constrained: adjust right margin to soak up the difference.
            (false, false, false) => {
                margin_right = Length(margin_right.to_px() + underflow);
            }
            (false, false, true) => margin_right = Length(underflow),
            (false, true, false) => margin_left = Length(underflow),
            (false, true, true) => {
                margin_left = Length(underflow / 2.0);
                margin_right = Length(underflow / 2.0);
            }
            // Auto width fills the remaining space.
            (true, _, _) => {
                if margin_left == Auto {
                    margin_left = Length(0.0);
                }
                if margin_right == Auto {
                    margin_right = Length(0.0);
                }
                width = if underflow >= 0.0 { Length(underflow) } else { Length(0.0) };
            }
        }

        let d = &mut self.dimensions;
        d.content.width = width.to_px();
        d.padding.left = padding_left.to_px();
        d.padding.right = padding_right.to_px();
        d.border.left = border_left.to_px();
        d.border.right = border_right.to_px();
        d.margin.left = margin_left.to_px();
        d.margin.right = margin_right.to_px();
    }

    /// Place the box just below the container's current content cursor.
    fn calculate_block_position(&mut self, containing_block: Dimensions) {
        let style = self.style();
        let zero = Length(0.0);
        let d = &mut self.dimensions;

        d.margin.top = lookup_len(style, &["margin-top", "margin"], &zero).to_px();
        d.margin.bottom = lookup_len(style, &["margin-bottom", "margin"], &zero).to_px();
        d.border.top = lookup_len(style, &["border-top-width", "border-width", "border"], &zero).to_px();
        d.border.bottom =
            lookup_len(style, &["border-bottom-width", "border-width", "border"], &zero).to_px();
        d.padding.top = lookup_len(style, &["padding-top", "padding"], &zero).to_px();
        d.padding.bottom = lookup_len(style, &["padding-bottom", "padding"], &zero).to_px();

        d.content.x =
            containing_block.content.x + d.margin.left + d.border.left + d.padding.left;
        // Stack below everything already placed in the container.
        d.content.y = containing_block.content.height
            + containing_block.content.y
            + d.margin.top
            + d.border.top
            + d.padding.top;
    }

    fn layout_block_children(&mut self) {
        let mut content_height: f32 = 0.0;
        let self_dims = self.dimensions;
        for child in &mut self.children {
            child.layout(self_dims_with_height(self_dims, content_height));
            // Track how much vertical space children have consumed.
            content_height += child.dimensions.margin_box().height;
        }
        self.dimensions.content.height = content_height;
    }

    fn calculate_block_height(&mut self) {
        // An explicit height wins; otherwise we keep the height children gave us.
        if let Some(h) = self.style().and_then(|s| length_px(s, "height")) {
            self.dimensions.content.height = h;
        }
    }

    fn style(&self) -> Option<&'a StyledNode<'a>> {
        self.styled_node()
    }
}

/// A copy of the container's dimensions with the running content height set, so
/// the next child knows where to stack.
fn self_dims_with_height(mut d: Dimensions, height: f32) -> Dimensions {
    d.content.height = height;
    d
}

// --- Length resolution ------------------------------------------------------
// A tiny `auto`-or-pixels helper type keeps the width algorithm readable.

#[derive(Debug, Clone, Copy, PartialEq)]
enum LengthOrAuto {
    Length(f32),
    Auto,
}
use LengthOrAuto::{Auto, Length};

impl LengthOrAuto {
    fn to_px(self) -> f32 {
        match self {
            Length(px) => px,
            Auto => 0.0,
        }
    }
}

/// Read a px length property directly off a styled node.
fn length_px(style: &StyledNode, name: &str) -> Option<f32> {
    match style.value(name) {
        Some(v @ crate::css::Value::Length(..)) => Some(v.to_px()),
        _ => None,
    }
}

/// Look up the first present property among `names` (e.g. margin-left then the
/// margin shorthand), honoring the `auto` keyword. Falls back to `default`.
fn lookup_len(
    style: Option<&StyledNode>,
    names: &[&str],
    default: &LengthOrAuto,
) -> LengthOrAuto {
    let Some(style) = style else { return *default };
    for name in names {
        match style.value(name) {
            Some(crate::css::Value::Keyword(k)) if k == "auto" => return Auto,
            Some(v @ crate::css::Value::Length(..)) => return Length(v.to_px()),
            _ => {}
        }
    }
    *default
}

/// Render the layout tree as indented text — handy for tests and `--dump-layout`.
pub fn box_tree_to_string(root: &LayoutBox) -> String {
    let mut out = String::new();
    print_box(root, 0, &mut out);
    out
}

fn print_box(b: &LayoutBox, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let label = match b.box_type {
        BoxType::Block(n) => format!("block <{}>", n.node.tag_name().unwrap_or("?")),
        BoxType::Inline(n) => format!("inline <{}>", n.node.tag_name().unwrap_or("#text")),
        BoxType::Anonymous => "anonymous".to_string(),
    };
    let c = b.dimensions.content;
    out.push_str(&format!(
        "{indent}{label} @ ({:.0},{:.0}) {:.0}x{:.0}\n",
        c.x, c.y, c.width, c.height
    ));
    for child in &b.children {
        print_box(child, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{css, html, style};

    fn layout(html_src: &str, css_src: &str, width: f32) -> String {
        let dom = html::parse(html_src);
        let sheet = css::parse(css_src);
        let styled = style::style_tree(&dom, &sheet);
        let viewport = Dimensions {
            content: Rect { x: 0.0, y: 0.0, width, height: 0.0 },
            ..Default::default()
        };
        // We have to keep the styled tree alive for the borrow; build inline.
        let lb = layout_tree(&styled, viewport);
        box_tree_to_string(&lb)
    }

    #[test]
    fn block_fills_container_width() {
        let dump = layout("<div></div>", "div { height: 50px; }", 800.0);
        assert!(dump.contains("800x50"), "got: {dump}");
    }

    #[test]
    fn explicit_width_and_auto_margins_center() {
        let dump = layout(
            "<div></div>",
            "div { width: 400px; height: 10px; margin-left: auto; margin-right: auto; }",
            800.0,
        );
        // (800-400)/2 == 200 left offset.
        assert!(dump.contains("@ (200,0) 400x10"), "got: {dump}");
    }

    #[test]
    fn blocks_stack_vertically() {
        let dump = layout(
            "<div></div><div></div>",
            "div { height: 30px; }",
            600.0,
        );
        // Second div sits at y=30, below the first.
        assert!(dump.contains("@ (0,0) 600x30"), "got: {dump}");
        assert!(dump.contains("@ (0,30) 600x30"), "got: {dump}");
    }

    #[test]
    fn padding_and_margin_offset_content() {
        let dump = layout(
            "<div></div>",
            "div { height: 10px; margin: 5px; padding: 10px; }",
            100.0,
        );
        // content x = margin(5) + padding(10) = 15; width = 100 - 2*5 - 2*10 = 70
        assert!(dump.contains("@ (15,15) 70x10"), "got: {dump}");
    }
}
