//! Render a page and emit an animated GIF that scrolls down it.
//!
//! This is how `docs/images/demo.gif` is produced. It renders a (local) page to
//! a tall canvas with the normal pipeline, then writes a series of GIF frames,
//! each showing the visible window at a slightly larger scroll offset — exactly
//! what the interactive window does, captured to a file.
//!
//! Usage:
//!     cargo run --release --example make_gif -- <FILE.html> <OUT.gif> [width] [view_h]

use std::fs::File;

use robin::css::Color;
use robin::layout::{Dimensions, Rect};
use robin::render::Canvas;
use robin::{html, layout, net, paint, style, text};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: make_gif <FILE.html> <OUT.gif> [width] [view_h]");
        std::process::exit(2);
    }
    let input = &args[0];
    let out = &args[1];
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(820);
    let view_h: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(520);

    let canvas = render_page(input, width as f32);
    write_scrolling_gif(&canvas, out, view_h);
    println!(
        "make_gif: wrote {out} ({}x{} page)",
        canvas.width, canvas.height
    );
}

/// Run the normal pipeline on a local page and return the full painted canvas.
fn render_page(input: &str, width: f32) -> Canvas {
    let page = net::load(input).expect("load page");
    let dom = html::parse(&page.body);
    let author = style::document_stylesheet(&dom); // snapshots inline their CSS
    let styled = style::style_tree(&dom, &author);
    let fonts = text::Fonts::bundled().expect("fonts");

    let viewport = Dimensions {
        content: Rect {
            x: 0.0,
            y: 0.0,
            width,
            height: 0.0,
        },
        ..Default::default()
    };
    let layout_root = layout::layout_tree(&styled, viewport, &fonts);

    let height = layout_root.dimensions.margin_box().height.ceil().max(1.0);
    let mut canvas = Canvas::new(width as usize, height as usize, Color::rgb(255, 255, 255));
    paint::paint_list(
        &mut canvas,
        &paint::build_display_list(&layout_root),
        &fonts,
    );
    canvas
}

/// Write a looping GIF that holds at the top, scrolls down, and holds at the
/// bottom of the visible journey.
fn write_scrolling_gif(canvas: &Canvas, out: &str, view_h: usize) {
    let w = canvas.width;
    let view_h = view_h.min(canvas.height);
    let max_scroll = canvas.height.saturating_sub(view_h);
    // Keep the animation snappy and the file small: cap how far we scroll and
    // take big steps (every frame is stored in full, so fewer frames = smaller).
    let travel = max_scroll.min(1700);
    let step = 34;

    let mut file = File::create(out).expect("create gif");
    let mut encoder =
        gif::Encoder::new(&mut file, w as u16, view_h as u16, &[]).expect("gif encoder");
    encoder.set_repeat(gif::Repeat::Infinite).expect("repeat");

    let holds_top = 5;
    let holds_bottom = 7;
    let mut offsets: Vec<usize> = Vec::new();
    offsets.extend(std::iter::repeat(0).take(holds_top));
    let mut s = 0;
    while s < travel {
        offsets.push(s);
        s += step;
    }
    offsets.extend(std::iter::repeat(travel).take(holds_bottom));

    for (i, &scroll) in offsets.iter().enumerate() {
        let mut rgba = slice_rgba(canvas, scroll, view_h);
        let mut frame = gif::Frame::from_rgba_speed(w as u16, view_h as u16, &mut rgba, 10);
        // Longer pause on the hold frames at each end.
        frame.delay = if i < holds_top || i >= offsets.len() - holds_bottom {
            13
        } else {
            7
        };
        encoder.write_frame(&frame).expect("write frame");
    }
}

/// Extract the visible window of the page as RGBA bytes.
fn slice_rgba(canvas: &Canvas, scroll: usize, view_h: usize) -> Vec<u8> {
    let w = canvas.width;
    let mut out = Vec::with_capacity(w * view_h * 4);
    for row in 0..view_h {
        let src = scroll + row;
        for col in 0..w {
            let p = if src < canvas.height {
                canvas.pixels[src * w + col]
            } else {
                Color::rgb(255, 255, 255)
            };
            out.extend_from_slice(&[p.r, p.g, p.b, 255]);
        }
    }
    out
}
