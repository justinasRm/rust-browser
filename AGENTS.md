# AGENTS.md — guide for coding agents

This file orients an AI coding agent (Claude Code, Cursor, etc.) working in this
repo. **Robin** is a teaching web-browser engine in Rust. The primary goal of
this repo is *learning* — so your job is usually to **coach a human through it**,
not just to produce code.

## What this project is

Robin turns HTML+CSS into pixels, entirely from scratch (no Chromium/WebKit). It
is small, readable, and structured so each module is one stage of the rendering
pipeline:

```
bytes ──▶ html.rs ──▶ dom.rs ──▶ style.rs ──▶ layout.rs ──▶ paint.rs ──▶ render.rs ──▶ PNG / window
            (parse)    (tree)    (cascade)    (boxes)     (display list)  (pixels)
                          ▲                       ▲             ▲
                       css.rs                  text.rs       net.rs / window.rs
```

The **git history is the curriculum**: each commit adds one concept, matching one
chapter in `docs/`. `git log --oneline --reverse` is the table of contents.

## Repository map

| Path | What it is |
| ---- | ---------- |
| `src/dom.rs` | The document tree: `Node`, `NodeType`, `ElementData`. |
| `src/html.rs` | Tolerant HTML parser (tokenizer + tree builder). |
| `src/css.rs` | CSS parser: selectors, declarations, values, specificity. |
| `src/style.rs` | The cascade → `StyledNode`; the user-agent stylesheet. |
| `src/layout.rs` | Box model, block layout, and the inline/line-breaking engine. |
| `src/paint.rs` | Box tree → display list of draw commands. |
| `src/render.rs` | `Canvas`: software rasterizer + PNG output. |
| `src/text.rs` | Font loading (`fontdue`) + glyph rasterization; bundled fonts. |
| `src/net.rs` | Fetch pages and linked stylesheets (`ureq`, `url`). |
| `src/window.rs` | Scrollable interactive window (`minifb`). |
| `src/main.rs` | The CLI that wires the pipeline together. |
| `docs/` | One tutorial chapter per stage, with exercises. |
| `examples/snapshot.rs` | Build a self-contained offline page (inlines linked CSS). |
| `examples/make_gif.rs` | Render a page to a scrolling animated GIF. |
| `assets/snapshots/` | Offline copies of Hacker News / Wikipedia / Google. |
| `assets/fonts/` | Bundled DejaVu fonts (embedded via `include_bytes!`). |
| `scripts/render-demos.sh` | Regenerate `docs/images/` screenshots from snapshots. |

## Build, run, test

```bash
cargo build                 # debug build
cargo test                  # run ALL tests (every module has unit tests)
cargo test layout           # run one module's tests (dom/html/css/style/layout/paint/render/text/net)
cargo run -- <URL|FILE> --png out/page.png      # render to PNG
cargo run -- <FILE> --dump-dom                  # print the parsed DOM
cargo run -- <FILE> --dump-layout               # print the box tree
cargo run -- <FILE> --window                    # interactive window (needs a display)
cargo clippy                # lints — keep it clean
cargo fmt                   # formatting — run before finishing
```

Fast iteration tip: the `--dump-dom` / `--dump-layout` flags let you inspect an
intermediate stage without rendering, which is the quickest way to debug a layout
or parsing question.

## Conventions to follow

- **Match the house style.** The code favors small functions, clear names, and
  doc-comments that explain *why*, not just *what*. New code should read like the
  surrounding code. Keep comment density similar.
- **Pure-std where it already is.** `dom`, `html`, `css`, `style`, `layout` use
  no external crates by design (it's part of the lesson). Don't add dependencies
  to those stages without a very good reason.
- **Tolerance over correctness-or-crash.** Parsers and layout must never panic on
  real-world input. Prefer skipping/defaulting to erroring. (See
  `docs/11-rendering-real-pages.md`.)
- **Tests live next to the code** in `#[cfg(test)] mod tests`. Any behavior
  change needs a test. Run `cargo test` before claiming something works.
- **Keep modules single-stage.** Don't blur the pipeline boundaries; the clean
  separation is what makes the repo teachable.

## When the human is learning (the common case)

Default to **coaching**, not solving:

- Explain the relevant module in plain terms and point at the matching
  `docs/NN-*.md` chapter and the exact `src/<file>.rs:line`.
- When they're stuck on an exercise, give the **next hint**, not the full answer.
  Ask what they've tried. Offer to review their attempt.
- Prefer small, runnable steps: "add this test, watch it fail, now make it pass."
- Use the dump flags to *show* them what the engine currently sees.
- Only write a complete solution if they explicitly ask for it.

A good interaction: *"That belongs in `src/css.rs`. Look at `parse_value` around
the color handling — see how `#rgb` is dispatched? Exercise 1 in
`docs/03-css-parser.md` asks you to add `rgba()`. Want a hint, or want to try and
have me review it?"*

## When asked to extend the engine

Good, well-scoped features (most have exercises in `docs/`): image placeholder
boxes, `text-align`, descendant selectors, a glyph cache, clickable links in the
window. See `docs/12-limitations-and-next-steps.md` for the full list and
relative difficulty. For anything touching layout, add a `--dump-layout`-based
test and verify against a bundled snapshot.

## Things to be careful about

- Don't turn Robin into a Chromium/WebKit wrapper or pull in a real HTML/CSS
  engine crate (`html5ever`, `servo`-anything) — that defeats the entire purpose.
- Don't "fix" a documented limitation silently; if you implement floats or JS,
  it's a real feature with real tests, and `docs/12-*` should be updated.
- The bundled fonts and snapshots are intentionally committed binaries; don't
  delete them.
- Regenerate demo images with `scripts/render-demos.sh` (don't hand-edit PNGs).
