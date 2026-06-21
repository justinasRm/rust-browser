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

use crate::css::Color;
use crate::style::{Display, StyledNode};
use crate::text::Fonts;

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
    /// Positioned text produced when this box runs an inline formatting context.
    /// Only anonymous/inline boxes fill this in; paint reads it to draw glyphs.
    pub fragments: Vec<InlineFragment>,
}

/// A laid-out run of text on a single line: the word(s), where to draw them
/// (`x`, and `baseline` as the y of the text baseline), and how.
#[derive(Debug, Clone)]
pub struct InlineFragment {
    pub text: String,
    pub x: f32,
    pub baseline: f32,
    pub font_size: f32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub monospace: bool,
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
        LayoutBox {
            box_type,
            dimensions: Dimensions::default(),
            children: Vec::new(),
            fragments: Vec::new(),
        }
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
pub fn layout_tree<'a>(
    node: &'a StyledNode<'a>,
    mut viewport: Dimensions,
    fonts: &Fonts,
) -> LayoutBox<'a> {
    viewport.content.height = 0.0; // height is discovered from content
    let mut root = build_layout_tree(node);
    root.layout(viewport, fonts);
    root
}

/// Translate the styled tree into a tree of boxes, inserting anonymous boxes to
/// hold inline runs.
pub fn build_layout_tree<'a>(style_node: &'a StyledNode<'a>) -> LayoutBox<'a> {
    let mut root = LayoutBox::new(match effective_display(style_node) {
        Display::Inline => BoxType::Inline(style_node),
        _ => BoxType::Block(style_node), // Block, ListItem, None-as-root
    });

    for child in &style_node.children {
        match effective_display(child) {
            Display::None => {} // skip display:none entirely
            Display::Inline => root.inline_container().children.push(build_layout_tree(child)),
            _ => root.children.push(build_layout_tree(child)),
        }
    }
    root
}

/// The display value a box should *behave* as. An inline element that contains
/// block-level descendants (e.g. `<center><table>…` on Hacker News, or
/// `<a><div>…`) has to become a block itself — an inline box can't lay out block
/// children. This is the engine's version of the CSS rule that block-in-inline
/// forces anonymous block wrappers.
fn effective_display(node: &StyledNode) -> Display {
    match node.display() {
        Display::Inline if node.children.iter().any(is_block_level) => Display::Block,
        d => d,
    }
}

fn is_block_level(node: &StyledNode) -> bool {
    match node.display() {
        Display::Block | Display::ListItem => true,
        Display::None => false,
        Display::Inline => node.children.iter().any(is_block_level),
    }
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
    pub fn layout(&mut self, containing_block: Dimensions, fonts: &Fonts) {
        match self.box_type {
            BoxType::Block(_) => self.layout_block(containing_block, fonts),
            // An anonymous box holds a run of inline content: it runs an inline
            // formatting context, flowing text into lines. A bare inline box
            // reaching layout() (an inline directly under the root) is treated
            // the same way.
            BoxType::Anonymous | BoxType::Inline(_) => self.layout_inline(containing_block, fonts),
        }
    }

    fn layout_block(&mut self, containing_block: Dimensions, fonts: &Fonts) {
        // Width depends on the container, so compute it top-down first...
        self.calculate_block_width(containing_block);
        self.calculate_block_position(containing_block);
        // ...then lay out children, whose total height feeds our height.
        self.layout_block_children(fonts);
        self.calculate_block_height();
    }

    /// Lay out an inline formatting context: take a full-width strip and flow the
    /// inline descendants into wrapped lines of text.
    fn layout_inline(&mut self, containing_block: Dimensions, fonts: &Fonts) {
        // An inline strip is as wide as its container, with no box-model rings.
        self.calculate_block_width(containing_block);
        self.calculate_block_position(containing_block);

        let origin = self.dimensions.content;
        let mut flow = InlineFlow::new(origin.x, origin.y, origin.width, fonts);
        for child in &self.children {
            flow.add_box(child);
        }
        flow.finish();
        self.fragments = flow.fragments;
        self.dimensions.content.height = flow.height;
    }

    /// Resolve width and the horizontal margins/border/padding.
    fn calculate_block_width(&mut self, containing_block: Dimensions) {
        let style = self.style();
        let zero = Length(0.0);

        let mut width = style
            .map(|s| resolve_width(s, containing_block.content.width))
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

    fn layout_block_children(&mut self, fonts: &Fonts) {
        let mut content_height: f32 = 0.0;
        let self_dims = self.dimensions;
        for child in &mut self.children {
            child.layout(self_dims_with_height(self_dims, content_height), fonts);
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

// --- Inline formatting context ----------------------------------------------
// This is the line-breaker: it walks the inline boxes, splits their text into
// words, and flows the words left-to-right, wrapping to a new line whenever the
// next word would overshoot the available width. Mixed styles (a bold word next
// to a link next to plain text) all share the same line.

/// The visual style of a run of inline text, pulled off a styled node.
#[derive(Clone, Copy)]
struct InlineStyle {
    size: f32,
    bold: bool,
    italic: bool,
    monospace: bool,
    color: Color,
}

impl InlineStyle {
    fn from_node(node: &StyledNode) -> InlineStyle {
        use crate::css::Value;
        let size = node
            .value("font-size")
            .map(|v| v.to_px())
            .filter(|&px| px > 0.0)
            .unwrap_or(16.0);

        let bold = match node.value("font-weight") {
            Some(Value::Keyword(k)) => k == "bold" || k == "bolder",
            // Numeric weights parse as lengths; 600+ is bold.
            Some(v @ Value::Length(..)) => v.to_px() >= 600.0,
            _ => false,
        };
        let italic = matches!(
            node.value("font-style").as_ref().and_then(crate::css::Value::keyword),
            Some("italic") | Some("oblique")
        );
        let monospace = node
            .value("font-family")
            .and_then(|v| v.keyword().map(str::to_string))
            .map(|f| f.contains("mono"))
            .unwrap_or(false);
        let color = match node.value("color") {
            Some(Value::ColorValue(c)) => c,
            _ => Color::rgb(0, 0, 0),
        };
        InlineStyle { size, bold, italic, monospace, color }
    }
}

/// Flows inline content into wrapped lines, accumulating [`InlineFragment`]s.
struct InlineFlow<'f> {
    fonts: &'f Fonts,
    line_start_x: f32,
    origin_y: f32,
    avail: f32,
    cursor_x: f32,
    line_top: f32,
    /// Fragments on the line currently being built (their baseline is assigned
    /// once we know the line's tallest glyph).
    current_line: Vec<PendingFragment>,
    /// Finished, baseline-positioned fragments.
    fragments: Vec<InlineFragment>,
    /// Whether a space should precede the next word.
    pending_space: bool,
    /// Total height consumed so far (`line_top - origin_y`).
    height: f32,
}

struct PendingFragment {
    frag: InlineFragment,
    ascent: f32,
    line_height: f32,
}

impl<'f> InlineFlow<'f> {
    fn new(x: f32, y: f32, avail: f32, fonts: &'f Fonts) -> Self {
        InlineFlow {
            fonts,
            line_start_x: x,
            origin_y: y,
            avail,
            cursor_x: x,
            line_top: y,
            current_line: Vec::new(),
            fragments: Vec::new(),
            pending_space: false,
            height: 0.0,
        }
    }

    /// Walk one inline box, emitting its text (and recursing into nested inline
    /// elements like `<a><b>…</b></a>`).
    fn add_box(&mut self, b: &LayoutBox) {
        if let Some(node) = b.styled_node() {
            if let Some(text) = node.node.text_content() {
                self.add_text(text, InlineStyle::from_node(node));
                return;
            }
            if node.node.tag_name() == Some("br") {
                self.break_line();
                return;
            }
        }
        // An inline element (or a stray block in inline context): flow its
        // children into the same context.
        for child in &b.children {
            self.add_box(child);
        }
    }

    fn add_text(&mut self, text: &str, style: InlineStyle) {
        let leading_ws = text.starts_with(char::is_whitespace);
        let trailing_ws = text.ends_with(char::is_whitespace);
        let words: Vec<&str> = text.split_whitespace().collect();

        if words.is_empty() {
            // Whitespace-only text still separates its neighbours.
            self.pending_space |= leading_ws || trailing_ws;
            return;
        }
        if leading_ws {
            self.pending_space = true;
        }
        let last = words.len() - 1;
        for (i, word) in words.into_iter().enumerate() {
            self.place_word(word, style);
            if i < last {
                self.pending_space = true; // space between words in this run
            }
        }
        if trailing_ws {
            self.pending_space = true;
        }
    }

    fn place_word(&mut self, word: &str, style: InlineStyle) {
        let word_w = self.fonts.measure(word, style.size, style.bold, style.italic, style.monospace);
        let space_w = if self.pending_space {
            self.fonts.measure(" ", style.size, style.bold, style.italic, style.monospace)
        } else {
            0.0
        };

        let at_line_start = self.cursor_x <= self.line_start_x + 0.01;
        let overflows = self.cursor_x + space_w + word_w > self.line_start_x + self.avail;
        if overflows && !at_line_start {
            self.break_line();
        } else {
            self.cursor_x += space_w;
        }
        self.pending_space = false;

        let m = self.fonts.line_metrics(style.size, style.bold, style.italic, style.monospace);
        self.current_line.push(PendingFragment {
            frag: InlineFragment {
                text: word.to_string(),
                x: self.cursor_x,
                baseline: 0.0, // filled in by break_line
                font_size: style.size,
                color: style.color,
                bold: style.bold,
                italic: style.italic,
                monospace: style.monospace,
            },
            ascent: m.ascent,
            line_height: m.line_height,
        });
        self.cursor_x += word_w;
    }

    /// End the current line: align everyone to the tallest baseline, commit the
    /// fragments, and drop down to the next line.
    fn break_line(&mut self) {
        let (max_ascent, line_height) = self
            .current_line
            .iter()
            .fold((0.0_f32, 0.0_f32), |(a, h), pf| (a.max(pf.ascent), h.max(pf.line_height)));
        // An empty line (e.g. a leading <br>) still advances by a default height.
        let line_height = if self.current_line.is_empty() {
            self.fonts.line_metrics(16.0, false, false, false).line_height
        } else {
            line_height
        };
        let baseline = self.line_top + max_ascent;

        for mut pf in self.current_line.drain(..) {
            pf.frag.baseline = baseline;
            self.fragments.push(pf.frag);
        }
        self.line_top += line_height;
        self.cursor_x = self.line_start_x;
        self.pending_space = false;
        self.height = self.line_top - self.origin_y;
    }

    fn finish(&mut self) {
        if !self.current_line.is_empty() {
            self.break_line();
        }
    }
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

/// Resolve the `width` property against the containing block.
///
/// Absolute units become pixels; `%` and `vw` are taken as a fraction of the
/// container's width (for top-level boxes the container *is* the viewport, so
/// `vw` is exact; nested `vw` is approximated, which is fine for our pages).
/// Everything else — `auto`, `vh`, a missing value — means "auto".
fn resolve_width(style: &StyledNode, cb_width: f32) -> LengthOrAuto {
    use crate::css::{Unit, Value};
    match style.value("width") {
        Some(Value::Length(n, Unit::Percent)) | Some(Value::Length(n, Unit::Vw)) => {
            Length(n / 100.0 * cb_width)
        }
        Some(v @ Value::Length(..)) => Length(v.to_px()),
        _ => Auto,
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
        let fonts = crate::text::Fonts::bundled().unwrap();
        let dom = html::parse(html_src);
        let sheet = css::parse(css_src);
        let styled = style::style_tree(&dom, &sheet);
        let viewport = Dimensions {
            content: Rect { x: 0.0, y: 0.0, width, height: 0.0 },
            ..Default::default()
        };
        // We have to keep the styled tree alive for the borrow; build inline.
        let lb = layout_tree(&styled, viewport, &fonts);
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
    fn percentage_and_viewport_widths_resolve() {
        let half = layout("<div></div>", "div { width: 50%; height: 10px; }", 800.0);
        assert!(half.contains("400x10"), "got: {half}");
        let vw = layout("<div></div>", "div { width: 60vw; height: 10px; }", 1000.0);
        assert!(vw.contains("600x10"), "got: {vw}");
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

    fn layout_box_of<'a>(
        styled: &'a crate::style::StyledNode<'a>,
        fonts: &Fonts,
        width: f32,
    ) -> LayoutBox<'a> {
        let viewport = Dimensions {
            content: Rect { x: 0.0, y: 0.0, width, height: 0.0 },
            ..Default::default()
        };
        layout_tree(styled, viewport, fonts)
    }

    // Collect every inline fragment in the tree.
    fn all_fragments<'a>(b: &LayoutBox<'a>, out: &mut Vec<InlineFragment>) {
        out.extend(b.fragments.iter().cloned());
        for c in &b.children {
            all_fragments(c, out);
        }
    }

    #[test]
    fn long_text_wraps_onto_multiple_lines() {
        let fonts = crate::text::Fonts::bundled().unwrap();
        let dom = html::parse("<p>one two three four five six seven eight nine ten</p>");
        let sheet = css::parse("p { margin: 0; }");
        let styled = style::style_tree(&dom, &sheet);
        // Narrow viewport forces several lines.
        let lb = layout_box_of(&styled, &fonts, 80.0);
        let mut frags = Vec::new();
        all_fragments(&lb, &mut frags);
        let distinct_baselines: std::collections::BTreeSet<i32> =
            frags.iter().map(|f| f.baseline as i32).collect();
        assert!(distinct_baselines.len() >= 3, "expected wrapping onto >=3 lines");
        assert!(frags.iter().any(|f| f.text == "seven"));
    }

    #[test]
    fn bold_and_italic_flags_follow_markup() {
        let fonts = crate::text::Fonts::bundled().unwrap();
        let dom = html::parse("<p>plain <b>strong</b> <i>slanted</i></p>");
        let styled = style::style_tree(&dom, &css::parse(""));
        let lb = layout_box_of(&styled, &fonts, 400.0);
        let mut frags = Vec::new();
        all_fragments(&lb, &mut frags);
        assert!(frags.iter().any(|f| f.text == "strong" && f.bold));
        assert!(frags.iter().any(|f| f.text == "slanted" && f.italic));
        assert!(frags.iter().any(|f| f.text == "plain" && !f.bold && !f.italic));
    }

    #[test]
    fn br_forces_a_line_break() {
        let fonts = crate::text::Fonts::bundled().unwrap();
        let dom = html::parse("<p>line one<br>line two</p>");
        let styled = style::style_tree(&dom, &css::parse(""));
        let lb = layout_box_of(&styled, &fonts, 400.0);
        let mut frags = Vec::new();
        all_fragments(&lb, &mut frags);
        let one = frags.iter().find(|f| f.text == "one").unwrap().baseline;
        let two = frags.iter().find(|f| f.text == "two").unwrap().baseline;
        assert!(two > one, "second line should be below the first");
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
