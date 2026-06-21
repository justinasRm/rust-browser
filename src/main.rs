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
    mode: Mode,
}

enum Mode {
    DumpDom,
    DumpLayout,
    Png(String),
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
        content: Rect { x: 0.0, y: 0.0, width: opts.width, height: 0.0 },
        ..Default::default()
    };
    let layout_root = layout::layout_tree(&styled, viewport, &fonts);

    match opts.mode {
        Mode::DumpLayout => {
            print!("{}", layout::box_tree_to_string(&layout_root));
            ExitCode::SUCCESS
        }
        Mode::Png(path) => match render_png(&layout_root, &fonts, opts.width, &path) {
            Ok(()) => {
                println!("robin: wrote {path}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("robin: failed to write {path}: {e}");
                ExitCode::FAILURE
            }
        },
        Mode::DumpDom => unreachable!(),
    }
}

/// Paint the layout tree to a canvas and save it as a PNG.
fn render_png(
    layout_root: &layout::LayoutBox,
    fonts: &robin::text::Fonts,
    width: f32,
    path: &str,
) -> Result<(), String> {
    let height = layout_root.dimensions.margin_box().height.ceil().max(1.0);
    let mut canvas = render::Canvas::new(width as usize, height as usize, Color::rgb(255, 255, 255));
    let display_list = paint::build_display_list(layout_root);
    paint::paint_list(&mut canvas, &display_list, fonts);
    ensure_parent_dir(path);
    canvas.save_png(path)
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
    let mut mode = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dump-dom" => mode = Some(Mode::DumpDom),
            "--dump-layout" => mode = Some(Mode::DumpLayout),
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
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => target = Some(other.to_string()),
        }
        i += 1;
    }
    let target = target.ok_or("no URL or file given")?;
    // Default to a PNG next to the target if no mode was chosen.
    let mode = mode.unwrap_or_else(|| Mode::Png("out/page.png".to_string()));
    Ok(Options { target, width, mode })
}

fn print_usage() {
    println!(
        "robin {} — a tiny browser engine you build from scratch\n\
         \n\
         USAGE:\n    \
             robin <URL|FILE> [options]\n\
         \n\
         OPTIONS:\n    \
             --png <FILE>     Render the page to a PNG (default: out/page.png)\n    \
             --width <PX>     Viewport width in pixels (default: {})\n    \
             --dump-dom       Print the parsed DOM tree and exit\n    \
             --dump-layout    Print the layout (box) tree and exit\n    \
             -h, --help       Show this help\n    \
             -V, --version    Show the version\n\
         \n\
         <URL|FILE> may be an https:// URL, a file:// URL, or a local path.\n\
         An interactive --window is added in the next commit. See docs/.",
        robin::VERSION, DEFAULT_WIDTH as u32
    );
}
