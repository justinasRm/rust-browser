//! Build a self-contained offline snapshot of a web page.
//!
//! Real pages keep their CSS in separate files. To render a page offline (and
//! reproducibly — live sites change), we fetch the page plus every linked
//! stylesheet and inline the CSS into a single `<style>` block. The resulting
//! HTML renders the same way forever, with no network access.
//!
//! Usage:
//!     cargo run --example snapshot -- <URL> <OUTPUT.html>
//!
//! This is the tool used to produce the files in `assets/snapshots/`.

use robin::{html, net};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: cargo run --example snapshot -- <URL> <OUTPUT.html>");
        std::process::exit(2);
    }
    let (url, out) = (&args[0], &args[1]);

    let page = net::load(url).unwrap_or_else(|e| {
        eprintln!("snapshot: could not fetch {url}: {e}");
        std::process::exit(1);
    });
    let base = page.base_url.clone().unwrap_or_else(|| url.clone());

    let dom = html::parse(&page.body);
    let css = net::fetch_linked_css(&dom, &base);

    // Inject the collected CSS as one <style> block. A banner comment records
    // where and (roughly) when the snapshot came from.
    let style_block =
        format!("<style>\n/* Inlined by robin's snapshot tool from {url} */\n{css}\n</style>\n");
    let snapshot = inject_into_head(&page.body, &style_block);

    std::fs::write(out, snapshot.as_bytes()).unwrap_or_else(|e| {
        eprintln!("snapshot: could not write {out}: {e}");
        std::process::exit(1);
    });
    println!(
        "snapshot: wrote {out} ({} KB, {} KB of inlined CSS)",
        snapshot.len() / 1024,
        css.len() / 1024
    );
}

/// Insert `block` just after the opening `<head>` tag, or at the very top if the
/// document has no head.
fn inject_into_head(html: &str, block: &str) -> String {
    let lower = html.to_ascii_lowercase();
    if let Some(head) = lower.find("<head") {
        if let Some(gt) = lower[head..].find('>') {
            let pos = head + gt + 1;
            let mut out = String::with_capacity(html.len() + block.len());
            out.push_str(&html[..pos]);
            out.push('\n');
            out.push_str(block);
            out.push_str(&html[pos..]);
            return out;
        }
    }
    format!("{block}{html}")
}
