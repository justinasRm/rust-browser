//! Robin's command-line front end.
//!
//! It now runs the whole pipeline: load HTML, parse it, apply CSS, lay it out,
//! and paint. Depending on the flags it can dump any intermediate stage (the
//! DOM, the layout tree) or render the page to a PNG.

use std::process::ExitCode;

mod browser;
mod ui;

use robin::{dom, html, layout, net, render};

const DEFAULT_WIDTH: f64 = 1000.0;

struct Options {
    target: Option<String>,
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

    if args.iter().any(|a| a == "-h" || a == "--help") {
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

    let Some(target) = opts.target.as_deref() else {
        return show_window(None, opts.width, "robin", "");
    };

    let page = match net::load(target) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("robin: could not load {}: {e}", target);
            return ExitCode::FAILURE;
        }
    };

    if let Mode::DumpDom = opts.mode {
        let dom = html::parse(&page.body);
        print!("{}", dom::pretty_print(&dom));
        return ExitCode::SUCCESS;
    }

    let result = browser::with_layout(&page, opts.width, |layout_root, fonts| match opts.mode {
        Mode::DumpLayout => {
            print!("{}", layout::box_tree_to_string(layout_root));
            ExitCode::SUCCESS
        }
        Mode::Png(path) => {
            let full = browser::paint_page(layout_root, fonts, opts.width);
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
            let canvas = browser::paint_page(layout_root, fonts, opts.width);
            let title = format!("robin - {target}");
            show_window(Some(canvas), opts.width, &title, target)
        }
        Mode::DumpDom => unreachable!(),
    });
    result.unwrap_or_else(|error| {
        eprintln!("robin: {error}");
        ExitCode::FAILURE
    })
}

fn show_window(canvas: Option<render::Canvas>, width: f32, title: &str, address: &str) -> ExitCode {
    match ui::show(canvas, width, title, address) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("robin: window error: {error}");
            ExitCode::FAILURE
        }
    }
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
    let mut width = None;
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
                width = Some(
                    args.get(i)
                        .and_then(|w| w.parse().ok())
                        .ok_or("--width needs a number")?,
                );
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
    let mode = mode.unwrap_or(Mode::Window);
    if target.is_none() && !matches!(mode, Mode::Window) {
        return Err("a URL or file is required for --png, --dump-dom, and --dump-layout".into());
    }
    Ok(Options {
        target,
        width: width
            .unwrap_or_else(|| ui::screen::get_screen_width().unwrap_or(DEFAULT_WIDTH) as f32),
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
             robin [URL|FILE] [options]\n\
         \n\
         OPTIONS:\n    \
             --window         Open an interactive window (default)\n    \
             --png <FILE>     Render the page to a PNG\n    \
             --width <PX>     Viewport width (default: screen width, fallback {})\n    \
             --clip-top <PX>  Skip the top PX pixels when saving (scroll offset)\n    \
             --clip-height <PX>  Cap the rendered height (for thumbnails)\n    \
             --dump-dom       Print the parsed DOM tree and exit\n    \
             --dump-layout    Print the layout (box) tree and exit\n    \
             -h, --help       Show this help\n    \
             -V, --version    Show the version\n\
         \n\
         Without a URL or file, opens an empty window.\n\
         <URL|FILE> may be an http(s):// URL, a file:// URL, or a local path.\n\
         In the window: arrows/j/k scroll, Space/PageDn page, Home/End jump,\n\
         q or Esc quits. See docs/ for the matching chapters.",
        robin::VERSION,
        DEFAULT_WIDTH as u32
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Options, String> {
        parse_args(
            &arguments
                .iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn empty_launch_defaults_to_window() {
        let options = parse(&[]).unwrap();
        assert!(options.target.is_none());
        assert!(matches!(options.mode, Mode::Window));
        let options = parse(&["--window", "--width", "640"]).unwrap();
        assert!(options.target.is_none());
        assert_eq!(options.width, 640.0);
    }

    #[test]
    fn target_alone_opens_window() {
        for target in ["https://example.com", "http://example.com", "page.html"] {
            let options = parse(&[target]).unwrap();
            assert_eq!(options.target.as_deref(), Some(target));
            assert!(matches!(options.mode, Mode::Window));
        }
    }

    #[test]
    fn non_window_modes_require_target_and_remain_explicit() {
        for arguments in [
            vec!["--png", "out.png"],
            vec!["--dump-dom"],
            vec!["--dump-layout"],
        ] {
            assert!(parse(&arguments).is_err());
        }
        let options = parse(&["page.html", "--png", "out.png"]).unwrap();
        assert!(matches!(options.mode, Mode::Png(path) if path == "out.png"));
        assert!(matches!(
            parse(&["page.html", "--dump-dom"]).unwrap().mode,
            Mode::DumpDom
        ));
        assert!(matches!(
            parse(&["page.html", "--dump-layout"]).unwrap().mode,
            Mode::DumpLayout
        ));
    }
}
