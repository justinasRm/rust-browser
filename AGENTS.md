# AGENTS.md - guide for coding agents

This file provides project guidance for Codex and other coding agents working in this
repo. **Robin** is a teaching web-browser engine in Rust. The primary goal of
this repo is _learning_ - so your job is usually to **coach a human through it**,
not just to produce code.

## What this project is

Robin turns HTML+CSS into pixels, entirely from scratch (no Chromium/WebKit). It
is small, readable, and structured so each module is one stage of the rendering
pipeline:

```
bytes ──▶ html.rs ──▶ dom.rs ──▶ style.rs ──▶ layout.rs ──▶ paint.rs ──▶ render.rs ──▶ PNG / window
            (parse)    (tree)    (cascade)    (boxes)     (display list)  (pixels)
                          ▲                       ▲             ▲
                       css.rs                  text.rs       net.rs / application_ui/
```

Robin is a fork of Addy Osmani’s original project. The inherited tutorial
chapters in `docs/` and early git history explain the engine’s foundations;
current development does not have to follow that curriculum. Check the current
code before relying on a chapter’s paths or implementation details.

## Repository map

| Path                      | What it is                                                     |
| ------------------------- | -------------------------------------------------------------- |
| `src/engine/dom.rs`       | The document tree: `Node`, `NodeType`, `ElementData`.          |
| `src/engine/html.rs`      | Tolerant HTML parser (tokenizer + tree builder).               |
| `src/engine/css.rs`       | CSS parser: selectors, declarations, values, specificity.      |
| `src/engine/style.rs`     | The cascade → `StyledNode`; the user-agent stylesheet.         |
| `src/engine/layout.rs`    | Box model, block layout, and the inline/line-breaking engine.  |
| `src/engine/paint.rs`     | Box tree → display list of draw commands.                      |
| `src/engine/render.rs`    | `Canvas`: software rasterizer + PNG output.                    |
| `src/engine/text.rs`      | Font loading (`fontdue`) + glyph rasterization; bundled fonts. |
| `src/engine/net.rs`       | Fetch pages and linked stylesheets (`ureq`, `url`).            |
| `src/engine/mod.rs`       | Root of the `robin` engine library, configured in Cargo.toml.  |
| `src/browser.rs`          | Application orchestration connecting loading and rendering.    |
| `src/application_ui/`     | egui desktop interface, navigation, and page display.          |
| `src/ffis/`               | Native platform bindings for available window dimensions.      |
| `src/main.rs`             | The CLI that wires the pipeline together.                      |
| `docs/`                   | One tutorial chapter per stage, with exercises.                |
| `examples/snapshot.rs`    | Build a self-contained offline page (inlines linked CSS).      |
| `examples/make_gif.rs`    | Render a page to a scrolling animated GIF.                     |
| `assets/snapshots/`       | Offline copies of Hacker News / Wikipedia / Google.            |
| `assets/fonts/`           | Bundled DejaVu fonts (embedded via `include_bytes!`).          |
| `scripts/render-demos.sh` | Regenerate `docs/images/` screenshots from snapshots.          |

## Build, run, test

```bash
cargo build                 # debug build
cargo test                  # run ALL tests (every module has unit tests)
cargo test layout           # run one module's tests (dom/html/css/style/layout/paint/render/text/net)
cargo run -- <URL|FILE> --png out/page.png      # render to PNG
cargo run -- <FILE> --dump-dom                  # print the parsed DOM
cargo run -- <FILE> --dump-layout               # print the box tree
cargo run                                     # empty browser window (needs a display)
cargo run -- <FILE>                            # open a page in the window
cargo clippy                # lints - keep it clean
cargo fmt                   # formatting - run before finishing
```

Fast iteration tip: the `--dump-dom` / `--dump-layout` flags let you inspect an
intermediate stage without rendering, which is the quickest way to debug a layout
or parsing question.

## Conventions to follow

- **Match the house style.** The code favors small functions, clear names, and
  doc-comments that explain _why_, not just _what_. New code should read like the
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

## Learning goals and working approach

The owner comes from frontend TypeScript and is learning Rust, browser internals,
and general computing concepts. Robin is an ongoing learning project. Measure progress by what they can independently implement and explain, not by the number of features.

- Keep Robin as the main project, but keep each learning step small. Do not assume
  familiarity with threads, channels, native APIs, or Rust ownership just because
  the repository already uses them.
- Consolidate existing code before piling on features. Trace one execution path,
  explain one unfamiliar concept at a time, and connect it back to the project.
- Use tiny standalone experiments for difficult concepts. For concurrency, start
  with one worker and one channel; explore `move`, dropping the receiver, and
  `join()` separately before returning to the browser’s navigation code.
- Let the owner write the first implementation. Help scope the task, give hints,
  explain compiler errors, and review their attempt. Do not implement a whole
  feature unless explicitly asked. Explicit requests for changes should still
  be carried out without unnecessary permission questions.
- Avoid introducing several unfamiliar abstractions in one step. Background
  loading combined threads, channels, closures, ownership transfers, and UI
  coordination; unpack those rather than assuming the working code is understood.
- Finish one bounded feature with meaningful tests, clear commits, and a short
  account of the design and tradeoffs before expanding scope. Navigation history
  is a possible next feature, not a commitment or a reason to start it unasked.
- Preserve upstream attribution and make the owner’s
  contributions clear. Aim for features they can explain, including failure
  cases, tests, and limitations; they need not rewrite the inherited engine.
- Later, consider a smaller project built from an empty directory to practise
  independent design and completion.

## When the human is learning (the common case)

Default to **coaching**, not solving:

- Explain the relevant module in plain terms and point at the matching
  `docs/NN-*.md` chapter and the exact `src/<file>.rs:line`.
- When they're stuck on an exercise, give the **next hint**, not the full answer.
  Ask what they've tried. Offer to review their attempt.
- Prefer small, runnable steps: "add this test, watch it fail, now make it pass."
- Use the dump flags to _show_ them what the engine currently sees.
- Only write a complete solution if they explicitly ask for it.

A good interaction: _"That belongs in `src/engine/css.rs`. Look at `parse_value` around
the color handling - see how `#rgb` is dispatched? Exercise 1 in
`docs/03-css-parser.md` asks you to add `rgba()`. Want a hint, or want to try and
have me review it?"_

## When asked to extend the engine

Good, well-scoped features (most have exercises in `docs/`): image placeholder
boxes, `text-align`, descendant selectors, a glyph cache, clickable links in the
window. See `docs/12-limitations-and-next-steps.md` for the full list and
relative difficulty. For anything touching layout, add a `--dump-layout`-based
test and verify against a bundled snapshot.

## Things to be careful about

- Don't turn Robin into a Chromium/WebKit wrapper or pull in a real HTML/CSS
  engine crate (`html5ever`, `servo`-anything) - that defeats the entire purpose.
- Don't "fix" a documented limitation silently; if you implement floats or JS,
  it's a real feature with real tests, and `docs/12-*` should be updated.
- The bundled fonts and snapshots are intentionally committed binaries; don't
  delete them.
- Regenerate demo images with `scripts/render-demos.sh` (don't hand-edit PNGs).
