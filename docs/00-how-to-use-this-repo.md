# 🐦 Chapter 0 - How to use this repo

Welcome! **Robin** is a tiny web browser *engine* you build from scratch in
Rust. It is not a wrapper around Chromium or WebKit - every stage that turns
bytes on the wire into pixels on the screen is implemented here, in plain Rust,
small enough to read in an afternoon.

This repo is designed to be read three different ways. Pick whichever suits you.

## 1. Read the chapters

The `docs/` folder is a chapter per stage of the rendering pipeline. They build
on each other, so read them in order:

| #  | Chapter | What you'll build | Source |
| -- | ------- | ----------------- | ------ |
| 1  | [The DOM](01-the-dom.md) | the document tree | [dom.rs](../src/dom.rs) |
| 2  | [The HTML parser](02-html-parser.md) | text → DOM, tolerantly | [html.rs](../src/html.rs) |
| 3  | [The CSS parser](03-css-parser.md) | stylesheets → rules | [css.rs](../src/css.rs) |
| 4  | [Style & the cascade](04-style-and-the-cascade.md) | the styled tree | [style.rs](../src/style.rs) |
| 5  | [Block layout](05-block-layout.md) | the box model | [layout.rs](../src/layout.rs) |
| 6  | [Painting](06-painting.md) | display list → pixels → PNG | [paint.rs](../src/paint.rs), [render.rs](../src/render.rs) |
| 7  | [Text rasterization](07-text-rasterization.md) | glyphs → pixels | [text.rs](../src/text.rs) |
| 8  | [Inline layout & text flow](08-inline-layout-and-text.md) | line breaking | [layout.rs](../src/layout.rs) |
| 9  | [Networking](09-networking.md) | fetching pages & CSS | [net.rs](../src/net.rs) |
| 10 | [The interactive window](10-interactive-window.md) | a scrollable window | [window.rs](../src/window.rs) |
| 11 | [Rendering real pages](11-rendering-real-pages.md) | the messy-web fixes | - |
| 12 | [Limitations & next steps](12-limitations-and-next-steps.md) | where to go next | - |

Each chapter ends with **exercises** that extend that stage. Doing them is the
real learning - the reading is just the setup.

## 2. Walk the git history

The repo's commit history *is* the tutorial: each commit adds exactly one idea,
in the same order as the chapters. To watch the engine grow one concept at a
time:

```bash
git log --oneline --reverse          # the whole story, oldest first
git show <hash>                      # the diff that added one concept
```

Reading commit `feat: DOM tree`, then `feat: tolerant HTML parser`, and so on,
shows you not just the finished code but *the order it was built in* - which is
often the part tutorials leave out.

## 3. Learn it with a coding agent

Robin is **agent-first**. If you point a coding agent (Claude Code, Cursor, …)
at this repo and ask it to teach you, it has everything it needs:

- [`AGENTS.md`](../AGENTS.md) - a map of the codebase, how to build and test,
  and (importantly) guidance to **coach** you rather than just hand over answers.
- A clean module-per-stage layout, so "explain `src/layout.rs`" is a tractable
  request.
- Per-module tests you can run one at a time (`cargo test layout`), so an agent
  can propose a change and immediately check it.

A good first prompt: *"I'm learning Rust. Walk me through `src/dom.rs`, then give
me exercise 1 from `docs/01-the-dom.md` and review my attempt."*

## Never written Rust before?

That's fine - this repo is a good way to learn it. You don't need to understand
all of Rust to start; you'll pick it up one stage at a time. Two things will
help:

- Keep the free [*The Rust Programming Language* book](https://doc.rust-lang.org/book/)
  open in a tab. When a chapter's **"Rust notes"** mentions something new
  (`enum`, `match`, `Option`, ownership, lifetimes), the book has a short, clear
  section on it.
- Start with [Chapter 1: The DOM](01-the-dom.md). Its module
  ([`src/dom.rs`](../src/dom.rs)) is small and uses only basic Rust, so it's a
  gentle on-ramp before the parsers and layout.

The compiler is your friend here: it gives unusually helpful error messages, and
`cargo test` after every small change tells you immediately whether you broke
anything.

## Getting set up

You need a recent Rust toolchain ([rustup.rs](https://rustup.rs)). Then:

```bash
# Run the test suite (every chapter's module has tests)
cargo test

# Render a bundled page to a PNG - no network needed
cargo run --release -- assets/snapshots/hackernews.html --png out/hn.png

# Open the interactive, scrollable window
cargo run --release -- assets/snapshots/wikipedia.html --window

# Fetch and render a live page
cargo run --release -- https://example.com --png out/example.png
```

Useful CLI flags while you work: `--dump-dom` and `--dump-layout` print the tree
at that stage, `--width <px>` sets the viewport width, and `--clip-top` /
`--clip-height` crop the saved PNG to one band of a tall page.

Ready? Start with [Chapter 1 - The DOM](01-the-dom.md).

---

Next: [The DOM](01-the-dom.md)
