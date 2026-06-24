# 🌐 Chapter 9 - Networking

So far our browser has rendered HTML we handed it directly. But a real browser
starts somewhere more humble: an address bar. You type something - maybe a full
`https://example.com`, maybe a `file://` URL, maybe just a path on disk - and the
browser's first job is to turn that location into **bytes**.

This chapter builds that step. We fetch the page's text, and - just as
importantly - we remember the **base URL** it came from. That base URL is what
lets us resolve relative links like `../style.css` into something fetchable. All
of this lives in [`src/net.rs`](../src/net.rs).

## The idea

The address bar is forgiving, so [`load`](../src/net.rs) is too. It accepts three
shapes of "location" and hides them behind one call:

- a real `http://` or `https://` URL → fetch it over the network,
- a `file://` URL → strip the prefix and read from disk,
- a bare filesystem path → read from disk directly.

Whatever you pass, `load` returns a `Resource`:

```rust
pub struct Resource {
    pub body: String,
    pub base_url: Option<String>,
}
```

The `base_url` is `Some(url)` for network pages and `None` for local files
(there's nothing meaningful to resolve relative links against on disk).

### Why fetch linked stylesheets?

Almost no real page keeps its CSS inline. Instead the HTML carries lines like:

```html
<link rel="stylesheet" href="/site.css">
```

To render such a page faithfully we have to follow those links, fetch the CSS,
and feed it to our parser alongside any inline `<style>` blocks. That's the job
of [`fetch_linked_css`](../src/net.rs): walk the DOM, collect every stylesheet
`href`, resolve each to an absolute URL, fetch it, and concatenate the results.

### Base URL + relative resolution

A stylesheet `href` is usually **relative** - `../style.css`, `/x.css`, or just
`theme.css`. The page's base URL plus the URL-joining rules tell us the real
address. We lean on the [`url`](https://crates.io/crates/url) crate for this; it
implements the same resolution rules browsers use.

### Tolerant fetching

Real networks fail. A stylesheet might 404, time out, or live on a server that's
down. We don't want one broken file to blank the whole page, so `fetch_linked_css`
**skips failures silently** and renders with whatever CSS it managed to get.

### Blocking, not async

We use [`ureq`](https://crates.io/crates/ureq), a small **blocking** HTTP client.
Each fetch runs top to bottom - no async, no executor, no `await`. That keeps the
code readable and easy to follow. A production browser fetches dozens of
resources concurrently; doing that here is a great later exercise (see below).

## Walking the code

**`load` - one door for three kinds of location:**

```rust
pub fn load(target: &str) -> Result<Resource, String> {
    if is_http(target) {
        let body = fetch_text(target)?;
        Ok(Resource { body, base_url: Some(target.to_string()) })
    } else {
        let path = target.strip_prefix("file://").unwrap_or(target);
        let body = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(Resource { body, base_url: None })
    }
}
```

The `strip_prefix("file://").unwrap_or(target)` is a tidy trick: if the prefix is
there, drop it; if not, use the string unchanged. Either way we end up with a
plain path for `read_to_string`.

**`fetch_text` - the actual HTTP call:**

```rust
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
```

We build an `Agent` with sane timeouts and a redirect cap, set a couple of
request headers (a polite `User-Agent` identifying Robin, and an `Accept` that
says we want HTML or CSS), then `into_string()` to read the body as text.

**`resolve` - relative href → absolute URL:**

```rust
pub fn resolve(base: &str, href: &str) -> Option<String> {
    let base = url::Url::parse(base).ok()?;
    base.join(href).ok().map(|u| u.to_string())
}
```

So `resolve("https://a.com/x/y.html", "../style.css")` gives
`Some("https://a.com/style.css")`. If the base won't parse or the join fails, we
get `None` and the caller skips that link.

**`fetch_linked_css` + `collect_stylesheet_hrefs` - gather the CSS:**

```rust
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
```

`collect_stylesheet_hrefs` is a plain recursive tree walk. At each element it
checks for `<link>` with a `rel` containing `stylesheet` (handling values like
`"preload stylesheet"` by splitting on whitespace), and if so records the `href`.
Then it recurses into the children. Note the two nested `if let`s in
`fetch_linked_css`: a failed `resolve` *or* a failed `fetch_text` quietly skips
that stylesheet - exactly the tolerant behaviour we wanted.

### How the CLI combines linked + inline CSS

The networking module only produces the *linked* CSS string. The main binary
([`src/main.rs`](../src/main.rs)) is what stitches everything together: it calls
`net::load`, parses the HTML, runs `net::fetch_linked_css` for the external
sheets, gathers the inline `<style>` blocks from the document, and feeds the
combined CSS to the style engine before laying the page out.

### The snapshot tool

[`examples/snapshot.rs`](../examples/snapshot.rs) reuses these same functions for
a different purpose: building a **reproducible offline page**. It fetches the HTML
and every linked stylesheet, then inlines all the CSS into a single `<style>`
block injected into `<head>`. The result renders identically forever, with no
network access - which is exactly what you want for the test fixtures in
`assets/snapshots/`.

## Rust notes

- **`Result` + `map_err` for error plumbing.** Both `std` and `ureq` hand back
  their own error types. `map_err(|e| e.to_string())` flattens them all into
  `Result<_, String>`, so `load` and `fetch_text` share one simple error type and
  the `?` operator just works.
- **The builder pattern.** `ureq::AgentBuilder::new().timeout_connect(..)..build()`
  is the classic Rust builder: chain configuration methods that each return
  `self`, then `build()` to finish. It reads almost like prose.
- **`Option` chaining in `resolve`.** `Url::parse(base).ok()?` converts a `Result`
  to an `Option` and bails early with `None` on failure; `.ok().map(..)` does the
  same for the join. No `if`/`else` ladders, just a short pipeline.
- **`&Node` tree walking.** `collect_stylesheet_hrefs` takes `&Node` and recurses
  over `&node.children`, pushing into a `&mut Vec<String>`. Borrowing the tree
  immutably while mutating a separate output buffer is a common, friendly pattern.
- **External crates.** We pull in two small crates - `ureq` (blocking HTTP) and
  `url` (parsing and joining) - rather than hand-rolling either. Both are listed
  in `Cargo.toml`.

## Try it

Run the networking tests (URL resolution, stylesheet discovery, local load):

```sh
cargo test net
```

Fetch and render a live page to a PNG:

```sh
cargo run -- https://example.com --png /tmp/ex.png
```

Build a self-contained offline snapshot with all CSS inlined:

```sh
cargo run --example snapshot -- https://example.com /tmp/ex.html
```

Open `/tmp/ex.html` in any browser, or feed it back to Robin with
`cargo run -- /tmp/ex.html --png /tmp/ex2.png` - it renders with no network at all.

## Exercises

1. **Show `<img>` dimensions (warm-up).** Write a function that walks the DOM,
   finds every `<img>`, resolves its `src` against the base URL with `resolve`,
   fetches the bytes, and prints the image's width and height. (A crate like
   `image` can decode the header for you.)
2. **Follow `@import` in CSS (medium).** Stylesheets can pull in *other*
   stylesheets with `@import url("...")`. After fetching a sheet in
   `fetch_linked_css`, scan it for `@import` rules, resolve each against the
   sheet's own URL, and fetch those too. Watch out for import cycles.
3. **Add a simple on-disk cache (medium).** Before fetching a URL in
   `fetch_text`, hash it and check for a cached copy under a temp directory; on a
   miss, fetch and write the result. Add a way to bypass the cache for fresh data.
4. **Fetch stylesheets concurrently (advanced).** `fetch_linked_css` fetches
   sheets one at a time. Spawn a `std::thread` per `href` (or use a small thread
   pool), collect the results, and concatenate them **in document order**. This is
   the first taste of the concurrency a real browser lives on.

---

Previous: [Inline layout & text flow](08-inline-layout-and-text.md) · Next: [The interactive window](10-interactive-window.md) · Source: [src/net.rs](../src/net.rs)
