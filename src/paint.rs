//! Painting — turning the laid-out box tree into drawing commands, then pixels.
//!
//! Browsers don't draw straight from the layout tree. They first flatten it into
//! a **display list**: a simple, ordered list of "draw this rectangle here, in
//! this color" commands. That indirection is useful — the list is easy to
//! reason about, easy to test, and (in a real browser) easy to replay on the GPU
//! or hand to a different thread.
//!
//! For now a command is either a solid-color rectangle (backgrounds, borders) or
//! a run of text (added in the text chapter). We build the list by walking the
//! box tree, then [`paint_list`] executes it into a [`Canvas`].

use crate::css::{Color, Value};
use crate::layout::{LayoutBox, Rect};
use crate::render::Canvas;
use crate::style::StyledNode;

/// One drawing instruction.
#[derive(Debug, Clone)]
pub enum DisplayCommand {
    /// Fill a rectangle with a solid color.
    SolidColor(Color, Rect),
    /// Draw a string of text. Rasterized in the text chapter; carried here so
    /// the display-list shape is stable.
    Text(TextRun),
}

/// A positioned run of text to draw. `origin` is the baseline's left end.
#[derive(Debug, Clone)]
pub struct TextRun {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub monospace: bool,
}

/// Flatten a laid-out box tree into a display list.
pub fn build_display_list(layout_root: &LayoutBox) -> Vec<DisplayCommand> {
    let mut list = Vec::new();
    render_layout_box(&mut list, layout_root);
    list
}

fn render_layout_box(list: &mut Vec<DisplayCommand>, layout_box: &LayoutBox) {
    render_background(list, layout_box);
    render_borders(list, layout_box);
    // Text runs are emitted by the text chapter's code; see paint_inline there.
    for child in &layout_box.children {
        render_layout_box(list, child);
    }
}

fn render_background(list: &mut Vec<DisplayCommand>, layout_box: &LayoutBox) {
    if let Some(color) = background_color(layout_box) {
        list.push(DisplayCommand::SolidColor(color, layout_box.dimensions.padding_box()));
    }
}

fn render_borders(list: &mut Vec<DisplayCommand>, layout_box: &LayoutBox) {
    let Some(color) = border_color(layout_box) else { return };
    let d = &layout_box.dimensions;
    let border_box = d.border_box();

    // Left edge.
    list.push(DisplayCommand::SolidColor(color, Rect {
        x: border_box.x,
        y: border_box.y,
        width: d.border.left,
        height: border_box.height,
    }));
    // Right edge.
    list.push(DisplayCommand::SolidColor(color, Rect {
        x: border_box.x + border_box.width - d.border.right,
        y: border_box.y,
        width: d.border.right,
        height: border_box.height,
    }));
    // Top edge.
    list.push(DisplayCommand::SolidColor(color, Rect {
        x: border_box.x,
        y: border_box.y,
        width: border_box.width,
        height: d.border.top,
    }));
    // Bottom edge.
    list.push(DisplayCommand::SolidColor(color, Rect {
        x: border_box.x,
        y: border_box.y + border_box.height - d.border.bottom,
        width: border_box.width,
        height: d.border.bottom,
    }));
}

/// The element's background color, if it sets one.
fn background_color(layout_box: &LayoutBox) -> Option<Color> {
    let style = layout_box.styled_node()?;
    color_property(style, &["background-color", "background"])
}

/// The element's border color — explicit, else the text color, if it has a
/// visible border at all.
fn border_color(layout_box: &LayoutBox) -> Option<Color> {
    let b = &layout_box.dimensions.border;
    if b.left == 0.0 && b.right == 0.0 && b.top == 0.0 && b.bottom == 0.0 {
        return None;
    }
    let style = layout_box.styled_node()?;
    color_property(style, &["border-color", "border-top-color", "color"])
        .or(Some(Color::rgb(0, 0, 0)))
}

/// First of the named properties that resolves to a color.
fn color_property(style: &StyledNode, names: &[&str]) -> Option<Color> {
    for name in names {
        if let Some(Value::ColorValue(c)) = style.value(name) {
            return Some(c);
        }
    }
    None
}

/// Execute a display list into a canvas. (Text commands are handled by the text
/// chapter; here we draw the solid rectangles.)
pub fn paint_list(canvas: &mut Canvas, list: &[DisplayCommand]) {
    for cmd in list {
        if let DisplayCommand::SolidColor(color, rect) = cmd {
            canvas.fill_rect(*rect, *color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Dimensions, Rect as LRect};
    use crate::{css, html, layout, style};

    fn display_list(html_src: &str, css_src: &str) -> Vec<DisplayCommand> {
        let dom = html::parse(html_src);
        let sheet = css::parse(css_src);
        let styled = style::style_tree(&dom, &sheet);
        let viewport = Dimensions {
            content: LRect { x: 0.0, y: 0.0, width: 200.0, height: 0.0 },
            ..Default::default()
        };
        let root = layout::layout_tree(&styled, viewport);
        build_display_list(&root)
    }

    #[test]
    fn background_becomes_a_solid_rect() {
        let list = display_list("<div></div>", "div { height: 20px; background: #ff6600; }");
        let has_orange = list.iter().any(|c| matches!(c,
            DisplayCommand::SolidColor(col, _) if *col == css::Color::rgb(255, 102, 0)));
        assert!(has_orange, "expected an orange background rect");
    }

    #[test]
    fn borders_emit_four_rects() {
        let list = display_list(
            "<div></div>",
            "div { height: 20px; border-width: 2px; border-color: black; }",
        );
        let black_rects = list
            .iter()
            .filter(|c| matches!(c, DisplayCommand::SolidColor(col, _) if *col == css::Color::rgb(0, 0, 0)))
            .count();
        assert_eq!(black_rects, 4);
    }
}
