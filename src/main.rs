//! Robin's command-line front end.
//!
//! It now runs the whole pipeline: load HTML, parse it, apply CSS, lay it out,
//! and paint. Depending on the flags it can dump any intermediate stage (the
//! DOM, the layout tree) or render the page to a PNG.

use std::process::ExitCode;

use robin::css::Color;
use robin::layout::{Dimensions, Rect};
use robin::{css, dom, html, layout, net, paint, render, style};

const DEFAULT_WIDTH: f32 = 1000.0;

struct Options {
    target: String,
    width: f32,
    clip_top: Option<f32>,
    clip_height: Option<f32>,
    mode: Mode,
}

enum Mode {
    DumpDom,
    DumpLayout,
    Png(String),
    Window,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("robin {}", robin::VERSION);
        return ExitCode::SUCCESS;
    }

    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("robin: {e}");
            return ExitCode::FAILURE;
        }
    };

    let page = match net::load(&opts.target) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("robin: could not load {}: {e}", opts.target);
            return ExitCode::FAILURE;
        }
    };

    // The pipeline, stage by stage.
    let dom = html::parse(&page.body);
    if let Mode::DumpDom = opts.mode {
        print!("{}", dom::pretty_print(&dom));
        return ExitCode::SUCCESS;
    }

    // Layout needs to measure text, so it needs the fonts too.
    let fonts = match robin::text::Fonts::bundled() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("robin: could not load bundled fonts: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Author CSS = linked stylesheets (fetched for http(s) pages) + inline
    // <style> blocks. Later source wins ties, so inline goes last.
    let mut css_text = String::new();
    if let Some(base) = &page.base_url {
        css_text.push_str(&net::fetch_linked_css(&dom, base));
    }
    css_text.push_str(&style::inline_css(&dom));
    let author = css::parse(&css_text);
    let styled = style::style_tree(&dom, &author);
    let viewport = Dimensions {
        content: Rect {
            x: 0.0,
            y: 0.0,
            width: opts.width,
            height: 0.0,
        },
        ..Default::default()
    };
    let layout_root = layout::layout_tree(&styled, viewport, &fonts);

    match opts.mode {
        Mode::DumpLayout => {
            print!("{}", layout::box_tree_to_string(&layout_root));
            ExitCode::SUCCESS
        }
        Mode::Png(path) => {
            let full = paint_page(&layout_root, &fonts, opts.width);
            let canvas = clip(full, opts.clip_top, opts.clip_height);
            ensure_parent_dir(&path);
            match canvas.save_png(&path) {
                Ok(()) => {
                    println!("robin: wrote {path}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("robin: failed to write {path}: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Mode::Window => {
            // The window scrolls, so it always shows the full page.
            let canvas = paint_page(&layout_root, &fonts, opts.width);
            let title = format!("robin - {}", opts.target);
            match robin::window::show(&canvas, &title) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("robin: window error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Mode::DumpDom => unreachable!(),
    }
}

/// Paint the laid-out page into a (possibly very tall) full-height canvas.
fn paint_page(
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

/// Crop the canvas to the `--clip-top` / `--clip-height` window, if given. Lets
/// you screenshot just one band of a tall page.
fn clip(canvas: render::Canvas, top: Option<f32>, height: Option<f32>) -> render::Canvas {
    if top.is_none() && height.is_none() {
        return canvas;
    }
    let top = top.unwrap_or(0.0).max(0.0) as usize;
    let height = height
        .map(|h| h.max(1.0) as usize)
        .unwrap_or_else(|| canvas.height.saturating_sub(top));
    canvas.cropped(top, height)
}

fn ensure_parent_dir(path: &str) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut target = None;
    let mut width = DEFAULT_WIDTH;
    let mut clip_top = None;
    let mut clip_height = None;
    let mut mode = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dump-dom" => mode = Some(Mode::DumpDom),
            "--dump-layout" => mode = Some(Mode::DumpLayout),
            "--window" => mode = Some(Mode::Window),
            "--png" => {
                i += 1;
                let path = args.get(i).ok_or("--png needs an output file path")?;
                mode = Some(Mode::Png(path.clone()));
            }
            "--width" => {
                i += 1;
                width = args
                    .get(i)
                    .and_then(|w| w.parse().ok())
                    .ok_or("--width needs a number")?;
            }
            "--clip-height" => {
                i += 1;
                clip_height = Some(
                    args.get(i)
                        .and_then(|h| h.parse().ok())
                        .ok_or("--clip-height needs a number")?,
                );
            }
            "--clip-top" => {
                i += 1;
                clip_top = Some(
                    args.get(i)
                        .and_then(|t| t.parse().ok())
                        .ok_or("--clip-top needs a number")?,
                );
            }
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => target = Some(other.to_string()),
        }
        i += 1;
    }
    let target = target.ok_or("no URL or file given")?;
    // Default to a PNG next to the target if no mode was chosen.
    let mode = mode.unwrap_or_else(|| Mode::Png("out/page.png".to_string()));
    Ok(Options {
        target,
        width,
        clip_top,
        clip_height,
        mode,
    })
}

fn print_usage() {
    println!(
        "robin {} - a tiny browser engine you build from scratch\n\
         \n\
         USAGE:\n    \
             robin <URL|FILE> [options]\n\
         \n\
         OPTIONS:\n    \
             --window         Open an interactive, scrollable window\n    \
             --png <FILE>     Render the page to a PNG (default: out/page.png)\n    \
             --width <PX>     Viewport width in pixels (default: {})\n    \
             --clip-top <PX>  Skip the top PX pixels when saving (scroll offset)\n    \
             --clip-height <PX>  Cap the rendered height (for thumbnails)\n    \
             --dump-dom       Print the parsed DOM tree and exit\n    \
             --dump-layout    Print the layout (box) tree and exit\n    \
             -h, --help       Show this help\n    \
             -V, --version    Show the version\n\
         \n\
         <URL|FILE> may be an https:// URL, a file:// URL, or a local path.\n\
         In the window: arrows/j/k scroll, Space/PageDn page, Home/End jump,\n\
         q or Esc quits. See docs/ for the matching chapters.",
        robin::VERSION,
        DEFAULT_WIDTH as u32
    );
}
