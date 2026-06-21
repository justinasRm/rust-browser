# 🐦 Robin — build a web browser engine from scratch in Rust

> A tiny, readable browser **engine** — not a Chromium or WebKit wrapper. Every
> stage that turns bytes on the wire into pixels on the screen is implemented
> here in plain Rust, one commit at a time, so you can learn how browsers (and
> Rust) actually work.

Robin is a modern, from-scratch reimagining of the classic
[*Intro to Rust: building a browser engine*](https://steemit.com/utopianio/@tensor/intro-to-rust-building-a-browser-engine-dom-and-html-parser)
tutorial (itself inspired by Matt Brubeck's *Let's build a browser engine!* /
**robinson** — hence the name **Robin**). The original is wonderful but now
abandoned and outdated; this repo brings the same idea up to modern Rust, makes
it render *real* pages, and is built to be **agent-first** so you can point a
coding agent at it and learn by doing.

*(A demo GIF and screenshots are added in the final commit.)*

## What it can do

- Parse **real HTML** into a DOM (tolerant of the messy markup real sites ship)
- Parse **CSS** — selectors, the cascade, specificity, inheritance
- Build a **layout tree** — the box model, block + inline flow, text wrapping
- **Paint** to a pixel buffer and save a **PNG**, or open a **scrollable window**
- **Fetch** pages over HTTPS, from local files, or from bundled offline snapshots
- Render simplified-but-readable versions of **Hacker News**, **Wikipedia** and
  **example.com / Google (no-JS)**

It is deliberately small and has real limits (no JavaScript, no flexbox/grid, no
incremental layout). Those limits are the *point* — see
[`docs/15-limitations-and-next-steps.md`](docs/15-limitations-and-next-steps.md).

## Quick start

```bash
# Render a bundled snapshot of Hacker News to a PNG (no network needed)
cargo run --release -- assets/snapshots/hackernews.html --png out/hn.png

# Render a live page over HTTPS
cargo run --release -- https://example.com --png out/example.png

# Open an interactive, scrollable window
cargo run --release -- assets/snapshots/wikipedia.html --window
```

## Learn it commit by commit

This repo's **git history is the tutorial**. Each commit adds exactly one idea,
and each maps to a chapter in [`docs/`](docs/):

```bash
git log --oneline --reverse   # walk the whole pipeline, one concept at a time
```

| Stage | Module | Chapter |
| ----- | ------ | ------- |
| The DOM | `src/dom.rs` | [01](docs/01-the-dom.md) |
| HTML parser | `src/html.rs` | [02](docs/02-html-parser.md) |
| CSS parser | `src/css.rs` | [03](docs/03-css-parser.md) |
| Style (the cascade) | `src/style.rs` | [04](docs/04-style-and-the-cascade.md) |
| Block layout | `src/layout.rs` | [05](docs/05-block-layout.md) |
| Inline layout & text | `src/layout.rs` | [06](docs/06-inline-layout-and-text.md) |
| Painting | `src/paint.rs` | [07](docs/07-painting.md) |
| Text rasterization | `src/text.rs` | [08](docs/08-text-rasterization.md) |
| Networking | `src/net.rs` | [09](docs/09-networking.md) |
| Interactive window | `src/window.rs` | [10](docs/10-interactive-window.md) |

## Built to learn *with an agent*

Robin is **agent-first**. If you point a coding agent (Claude Code, etc.) at
this repo, it has everything it needs to teach you:

- [`AGENTS.md`](AGENTS.md) / [`CLAUDE.md`](CLAUDE.md) — how the code is organized,
  how to build/test, and how to coach rather than just hand over answers
- [`docs/`](docs/) — a chapter per stage, with exercises
- A clean module-per-stage layout and tests you can run one at a time

See [`docs/00-how-to-use-this-repo.md`](docs/00-how-to-use-this-repo.md).

## License

MIT for the code (see [`LICENSE`](LICENSE)); bundled DejaVu fonts under their own
permissive license (see [`assets/fonts/FONT-LICENSE.txt`](assets/fonts/FONT-LICENSE.txt)).
