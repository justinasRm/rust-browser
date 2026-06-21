//! Robin's command-line front end.
//!
//! It now runs the whole pipeline: load HTML, parse it, apply CSS, lay it out,
//! and paint. Depending on the flags it can dump any intermediate stage (the
//! DOM, the layout tree) or render the page to a PNG.

use std::process::ExitCode;

use robin::css::Color;
use robin::layout::{Dimensions, Rect};
use robin::{dom, html, layout, paint, render, style};

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

    let source = match load_source(&opts.target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("robin: could not load {}: {e}", opts.target);
            return ExitCode::FAILURE;
        }
    };

    // The pipeline, stage by stage.
    let dom = html::parse(&source);
    if let Mode::DumpDom = opts.mode {
        print!("{}", dom::pretty_print(&dom));
        return ExitCode::SUCCESS;
    }

    let author = style::document_stylesheet(&dom);
    let styled = style::style_tree(&dom, &author);
    let viewport = Dimensions {
        content: Rect { x: 0.0, y: 0.0, width: opts.width, height: 0.0 },
        ..Default::default()
    };
    let layout_root = layout::layout_tree(&styled, viewport);

    match opts.mode {
        Mode::DumpLayout => {
            print!("{}", layout::box_tree_to_string(&layout_root));
            ExitCode::SUCCESS
        }
        Mode::Png(path) => match render_png(&layout_root, opts.width, &path) {
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
fn render_png(layout_root: &layout::LayoutBox, width: f32, path: &str) -> Result<(), String> {
    let fonts = robin::text::Fonts::bundled()?;
    let height = layout_root.dimensions.margin_box().height.ceil().max(1.0);
    let mut canvas = render::Canvas::new(width as usize, height as usize, Color::rgb(255, 255, 255));
    let display_list = paint::build_display_list(layout_root);
    paint::paint_list(&mut canvas, &display_list, &fonts);
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

/// Load a page's HTML. For now only local files are supported; the networking
/// commit teaches this to fetch `https://` URLs and bundled snapshots.
fn load_source(target: &str) -> std::io::Result<String> {
    if target.starts_with("http://") || target.starts_with("https://") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "networking is added in a later commit — pass a local .html file for now",
        ));
    }
    let target = target.strip_prefix("file://").unwrap_or(target);
    std::fs::read_to_string(target)
}

fn print_usage() {
    println!(
        "robin {} — a tiny browser engine you build from scratch\n\
         \n\
         USAGE:\n    \
             robin <FILE> [options]\n\
         \n\
         OPTIONS:\n    \
             --png <FILE>     Render the page to a PNG (default: out/page.png)\n    \
             --width <PX>     Viewport width in pixels (default: {})\n    \
             --dump-dom       Print the parsed DOM tree and exit\n    \
             --dump-layout    Print the layout (box) tree and exit\n    \
             -h, --help       Show this help\n    \
             -V, --version    Show the version\n\
         \n\
         Networking (URLs) and an interactive --window are added in later\n\
         commits. See docs/ for the matching chapters.",
        robin::VERSION, DEFAULT_WIDTH as u32
    );
}
