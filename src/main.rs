//! Robin's command-line front end.
//!
//! As the tutorial progresses this file grows from "print a banner" into a real
//! little browser CLI: fetch a URL, run it through the pipeline, and either save
//! a PNG or open an interactive window. Right now it can load a local HTML file
//! and dump the DOM the parser builds.

use std::process::ExitCode;

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

    let target = &args[0];
    let dump_dom = args.iter().any(|a| a == "--dump-dom");

    let source = match load_source(target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("robin: could not load {target}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let dom = robin::html::parse(&source);

    if dump_dom {
        print!("{}", robin::dom::pretty_print(&dom));
    } else {
        // Until layout and painting land, show that parsing worked.
        println!("robin {}: parsed {} ({} bytes of HTML)", robin::VERSION, target, source.len());
        println!("root element: <{}>", dom.tag_name().unwrap_or("?"));
        println!("try `--dump-dom` to see the tree the parser built");
    }
    ExitCode::SUCCESS
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
             --dump-dom       Parse the HTML and print the DOM tree\n    \
             -h, --help       Show this help\n    \
             -V, --version    Show the version\n\
         \n\
         More options (--png, --window, URLs, ...) are added as the tutorial\n\
         builds out the rendering pipeline. See docs/ for the matching chapters.",
        robin::VERSION
    );
}
