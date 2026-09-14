//! An interactive, scrollable window.
//!
//! Saving a PNG is great for tests and screenshots, but a browser is something
//! you *scroll*. This module opens a real OS window with [`minifb`] - a tiny
//! cross-platform framebuffer, not a GUI toolkit - and presents the rendered
//! page in it.
//!
//! The trick is simple: we render the whole page once into a tall [`Canvas`]
//! (it can be many screens high), then each frame we copy the slice of rows the
//! user has scrolled to into the window's buffer. Arrow keys, Page Up/Down,
//! Home/End and the mouse wheel move the scroll offset.

use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};

use crate::render::Canvas;

/// Open a window showing `canvas`, scrollable until the user closes it (or hits
/// Escape / `q`).
pub fn show(canvas: &Canvas, title: &str) -> Result<(), String> {
    let view_w = canvas.width;
    // The visible height is capped so the window fits on screen even for a very
    // tall page.
    let view_h = canvas.height.clamp(1, 800);

    let mut window = Window::new(
        title,
        view_w,
        view_h,
        WindowOptions {
            resize: false,
            scale: Scale::X1,
            scale_mode: ScaleMode::Stretch,
            ..WindowOptions::default()
        },
    )
    .map_err(|e| e.to_string())?;

    window.set_target_fps(60);

    // The full page as 0x00RGB pixels, and a reusable per-frame view buffer.
    let page = canvas.to_argb_u32();
    let max_scroll = canvas.height.saturating_sub(view_h);
    let mut scroll: f64 = 0.0;
    let mut target_scroll: f64 = 0.0;
    let mut last_frame = std::time::Instant::now();

    let mut view = vec![0u32; view_w * view_h];

    while window.is_open() && !window.is_key_down(Key::Escape) && !window.is_key_down(Key::Q) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f64();
        last_frame = now;

        target_scroll = apply_input(&window, target_scroll, max_scroll, view_h);

        let blend = 1.0 - (-dt / 0.05).exp();
        scroll += (target_scroll - scroll) * blend;

        blit_view(
            &page,
            &mut view,
            view_w,
            view_h,
            canvas.height,
            scroll.round() as usize,
        );
        window
            .update_with_buffer(&view, view_w, view_h)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Update the scroll offset from this frame's keyboard and wheel input.
fn apply_input(window: &Window, scroll: f64, max_scroll: usize, view_h: usize) -> f64 {
    let mut s = scroll;
    let line = 48.0;
    let page = view_h.saturating_sub(40).max(40) as f64;

    if window.is_key_down(Key::Down) || window.is_key_down(Key::J) {
        s += line;
    }
    if window.is_key_down(Key::Up) || window.is_key_down(Key::K) {
        s -= line;
    }
    if window.is_key_down(Key::PageDown) || window.is_key_down(Key::Space) {
        s += page;
    }
    if window.is_key_down(Key::PageUp) {
        s -= page;
    }
    if window.is_key_down(Key::Home) {
        s = 0.0;
    }
    if window.is_key_down(Key::End) {
        s = max_scroll as f64;
    }
    if let Some((_, wheel_y)) = window.get_scroll_wheel() {
        // Wheel up is positive; scrolling up should decrease the offset.
        s -= (wheel_y as f64) * 12.0;
    }

    s.clamp(0.0, max_scroll as f64)
}

/// Copy the visible rows (`scroll..scroll+view_h`) of the page into the window
/// buffer, padding with white if the page is shorter than the view.
fn blit_view(
    page: &[u32],
    view: &mut [u32],
    view_w: usize,
    view_h: usize,
    page_h: usize,
    scroll: usize,
) {
    for row in 0..view_h {
        let src_row = scroll + row;
        let dst = &mut view[row * view_w..(row + 1) * view_w];
        if src_row < page_h {
            dst.copy_from_slice(&page[src_row * view_w..(src_row + 1) * view_w]);
        } else {
            dst.fill(0x00ff_ffff); // white padding below the page
        }
    }
}
