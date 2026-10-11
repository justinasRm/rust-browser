# Robin

A browser project to learn Rust and understand how web browsers
work, from parsing HTML and CSS to drawing pixels on screen.

The project is a fork of [Addy Osmani’s Robin](https://github.com/addyosmani/rust-browser).

## What works

- HTML parsing, a DOM, and CSS selectors, cascading, and inheritance.
- Block and inline layout, text wrapping, and software rendering with bundled fonts.
- Loading pages over HTTP/HTTPS or from local files.
- An egui desktop UI with an address bar, background loading, and scrolling.

The page engine is implemented in Rust without Chromium or WebKit. egui handles
the browser interface; the engine renders the page itself.

JavaScript, flexbox, and grid not supported. Many pages won’t look like they do in a production browser.

## Run it

Install [Rust](https://rustup.rs) (1.88 or newer), then:

```bash
# Open an empty browser window
cargo run --release

# Open a web page
cargo run --release -- https://example.com

# Open a bundled offline snapshot
cargo run --release -- assets/snapshots/hackernews.html

# Render a page to a PNG
cargo run --release -- assets/snapshots/hackernews.html --png out/hn.png
```

In the address bar, enter a full `http://` or `https://` URL, a `file://` URL,
or a local path, then press Enter to load it.

The window is the default output mode. Use `--dump-dom` or `--dump-layout` to
inspect a page, `--width <px>` to set the viewport width, and `--clip-top` /
`--clip-height` to crop PNG output. See `cargo run -- --help` for all options.

## Project structure

The engine and desktop interface are separate:

| Path                                                             | Responsibility                                     |
| ---------------------------------------------------------------- | -------------------------------------------------- |
| [`src/html.rs`](src/html.rs), [`src/dom.rs`](src/dom.rs)         | Parse HTML and represent the document tree.        |
| [`src/css.rs`](src/css.rs), [`src/style.rs`](src/style.rs)       | Parse CSS and determine each element’s styles.     |
| [`src/layout.rs`](src/layout.rs)                                 | Calculate boxes, positions, and text flow.         |
| [`src/paint.rs`](src/paint.rs), [`src/render.rs`](src/render.rs) | Turn the layout into draw commands and pixels.     |
| [`src/text.rs`](src/text.rs)                                     | Measure and rasterize text.                        |
| [`src/net.rs`](src/net.rs)                                       | Load documents and stylesheets.                    |
| [`src/browser.rs`](src/browser.rs)                               | Connect loading and rendering for the application. |
| [`src/ui/`](src/ui/)                                             | Desktop window, address bar, and page display.     |
| [`src/main.rs`](src/main.rs)                                     | CLI options and application startup.               |

The core DOM, HTML, CSS, style, and layout modules use the Rust standard library.
Other stages use `fontdue` for fonts, `image` for PNG output, `ureq` and `url` for
networking, and `eframe`/`egui` for the UI.

## Development

```bash
cargo test
cargo clippy
cargo fmt
```

The original tutorial chapters are kept in [`docs/`](docs/). They explain the
engine’s foundations, but some details describe the upstream implementation and
may differ from this fork. Offline sample pages live in
[`assets/snapshots/`](assets/snapshots/).

## Credits and license

Forked from [Addy Osmani’s Robin](https://github.com/addyosmani/rust-browser),
which draws on [tensor-programming’s browser tutorial](https://github.com/tensor-programming/rust_browser_part_1)
and Matt Brubeck’s [Let’s build a browser engine!](https://limpet.net/mbrubeck/2014/08/08/toy-layout-engine-1.html).

The code is licensed under [MIT](LICENSE). The bundled DejaVu fonts have their
[own license](assets/fonts/FONT-LICENSE.txt).
