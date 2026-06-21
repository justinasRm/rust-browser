//! Networking — getting a page's bytes, wherever they live.
//!
//! A browser's address bar accepts a few different kinds of "location": a real
//! `https://` URL, a `file://` path, or just a path on disk. This module hides
//! those behind one [`load`] call that returns the HTML plus the **base URL**
//! we resolve relative links against.
//!
//! It also knows how to follow `<link rel="stylesheet">`: real pages keep most
//! of their CSS in separate files, so to render them we have to fetch those too
//! and feed them to the CSS parser alongside any inline `<style>` blocks.
//!
//! We use [`ureq`], a small blocking HTTP client, so the code reads like
//! ordinary top-to-bottom Rust — no async, no executor. A real browser fetches
//! many resources concurrently; that's a great thing to add later.

use std::time::Duration;

use crate::dom::Node;

/// A loaded page: its HTML text, and the URL it came from (used as the base for
/// resolving relative links). `base_url` is `None` for local files.
pub struct Resource {
    pub body: String,
    pub base_url: Option<String>,
}

const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; RobinBrowser/0.1; +https://github.com/addyosmani/rust-browser)";

/// Load a target that may be a URL, a `file://` URL, or a filesystem path.
pub fn load(target: &str) -> Result<Resource, String> {
    if is_http(target) {
        let body = fetch_text(target)?;
        Ok(Resource {
            body,
            base_url: Some(target.to_string()),
        })
    } else {
        let path = target.strip_prefix("file://").unwrap_or(target);
        let body = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(Resource {
            body,
            base_url: None,
        })
    }
}

fn is_http(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

/// Fetch a URL over HTTP(S) and return the body as text.
pub fn fetch_text(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .redirects(5)
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "text/html,text/css,*/*")
        .call()
        .map_err(|e| e.to_string())?;
    response.into_string().map_err(|e| e.to_string())
}

/// Resolve a possibly-relative `href` against a base URL.
///
/// `resolve("https://a.com/x/y.html", "../style.css")` →
/// `Some("https://a.com/style.css")`.
pub fn resolve(base: &str, href: &str) -> Option<String> {
    let base = url::Url::parse(base).ok()?;
    base.join(href).ok().map(|u| u.to_string())
}

/// Fetch every `<link rel="stylesheet">` the document references and return
/// their CSS concatenated. Individual failures (a 404, a timeout) are skipped so
/// one broken stylesheet doesn't sink the whole page.
pub fn fetch_linked_css(dom: &Node, base_url: &str) -> String {
    let mut hrefs = Vec::new();
    collect_stylesheet_hrefs(dom, &mut hrefs);

    let mut css = String::new();
    for href in hrefs {
        if let Some(abs) = resolve(base_url, &href) {
            if let Ok(text) = fetch_text(&abs) {
                css.push_str(&text);
                css.push('\n');
            }
        }
    }
    css
}

fn collect_stylesheet_hrefs(node: &Node, out: &mut Vec<String>) {
    if let Some(el) = node.element() {
        if el.tag_name == "link" {
            let is_stylesheet = el
                .get_attribute("rel")
                .map(|rel| {
                    rel.split_whitespace()
                        .any(|r| r.eq_ignore_ascii_case("stylesheet"))
                })
                .unwrap_or(false);
            if is_stylesheet {
                if let Some(href) = el.get_attribute("href") {
                    out.push(href.to_string());
                }
            }
        }
    }
    for child in &node.children {
        collect_stylesheet_hrefs(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html;

    #[test]
    fn resolves_relative_urls() {
        assert_eq!(
            resolve("https://example.com/a/b.html", "../style.css").as_deref(),
            Some("https://example.com/style.css")
        );
        assert_eq!(
            resolve("https://example.com/a/b.html", "/x.css").as_deref(),
            Some("https://example.com/x.css")
        );
        assert_eq!(
            resolve("https://example.com/", "https://cdn.com/y.css").as_deref(),
            Some("https://cdn.com/y.css")
        );
    }

    #[test]
    fn finds_stylesheet_links() {
        let dom = html::parse(
            r#"<head>
                 <link rel="stylesheet" href="/site.css">
                 <link rel="icon" href="/favicon.ico">
                 <link rel="preload stylesheet" href="late.css">
               </head>"#,
        );
        let mut hrefs = Vec::new();
        collect_stylesheet_hrefs(&dom, &mut hrefs);
        assert_eq!(hrefs, vec!["/site.css".to_string(), "late.css".to_string()]);
    }

    #[test]
    fn local_file_load_has_no_base_url() {
        // Loading a path that doesn't exist errors; loading this source file works.
        let res = load(file!());
        assert!(res.is_ok());
        assert!(res.unwrap().base_url.is_none());
    }
}
