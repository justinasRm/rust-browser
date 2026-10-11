//! Application orchestration shared by the CLI and address-bar navigation.
//! The UI asks for pixels; all rendering stages still belong to `robin`.

use robin::css::Color;
use robin::layout::{Dimensions, Rect};
use robin::{css, html, layout, net, paint, render, style, text};

pub fn load_page(target: &str, width: f32) -> Result<render::Canvas, String> {
    let page = net::load(target)?;
    with_layout(&page, width, |layout, fonts| {
        paint_page(layout, fonts, width)
    })
}

/// Keep borrowed DOM/style/layout trees alive while the caller uses the layout.
pub fn with_layout<T>(
    page: &net::Resource,
    width: f32,
    use_layout: impl FnOnce(&layout::LayoutBox<'_>, &text::Fonts) -> T,
) -> Result<T, String> {
    let dom = html::parse(&page.body);
    let fonts = text::Fonts::bundled()?;
    let mut css_text = String::new();
    if let Some(base) = &page.base_url {
        css_text.push_str(&net::fetch_linked_css(&dom, base));
    }
    css_text.push_str(&style::inline_css(&dom));
    let author = css::parse(&css_text);
    let styled = style::style_tree(&dom, &author);
    let viewport = Dimensions {
        content: Rect {
            width,
            ..Default::default()
        },
        ..Default::default()
    };
    let layout_root = layout::layout_tree(&styled, viewport, &fonts);
    Ok(use_layout(&layout_root, &fonts))
}

/// Paint the laid-out page into a (possibly very tall) full-height canvas.
pub fn paint_page(
    layout_root: &layout::LayoutBox,
    fonts: &robin::text::Fonts,
    width: f32,
) -> render::Canvas {
    let height = layout_root.dimensions.margin_box().height.ceil().max(1.0);
    let mut canvas =
        render::Canvas::new(width as usize, height as usize, Color::rgb(255, 255, 255));
    let display_list = paint::build_display_list(layout_root);
    paint::paint_list(&mut canvas, &display_list, fonts);
    canvas
}
