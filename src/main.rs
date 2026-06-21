//! Robin's command-line front end.
//!
//! As the tutorial progresses this file grows from "print a banner" into a real
//! little browser CLI: fetch a URL, run it through the pipeline, and either save
//! a PNG or open an interactive window. For the very first commit it does just
//! enough to prove the project is wired together.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") || args.is_empty() {
        print_usage();
        return;
    }

    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("robin {}", robin::VERSION);
        return;
    }

    // Future commits replace this with the real pipeline. For now we simply
    // acknowledge the input so the scaffold is runnable end to end.
    let input = &args[0];
    println!("robin {}", robin::VERSION);
    println!("(the rendering pipeline is built up over the following commits)");
    println!("you asked me to open: {input}");
}

fn print_usage() {
    println!(
        "robin {} — a tiny browser engine you build from scratch\n\
         \n\
         USAGE:\n    \
             robin <URL|FILE> [options]\n\
         \n\
         OPTIONS:\n    \
             -h, --help       Show this help\n    \
             -V, --version    Show the version\n\
         \n\
         More options (--png, --window, ...) are added as the tutorial builds\n\
         out the rendering pipeline. See docs/ for the matching chapters.",
        robin::VERSION
    );
}
