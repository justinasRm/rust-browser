# CLAUDE.md

This repo is a teaching web-browser engine in Rust. All guidance for AI coding
agents - the codebase map, build/test commands, conventions, and (importantly)
how to **coach a human learner rather than just hand over answers** - lives in
[`AGENTS.md`](AGENTS.md). Please read it first.

Quick reference:

- `cargo test` runs everything; `cargo test <module>` runs one stage's tests.
- `cargo run -- <URL|FILE> --png out/page.png` renders a page; `--dump-dom` and
  `--dump-layout` show intermediate stages.
- The pipeline is one module per stage (`dom → html → css → style → layout →
  paint/render`, with `text`, `net`, `window` alongside). The git history and
  `docs/` chapters follow the same order.
- Default to **coaching**: explain, point at the relevant `docs/NN-*.md` chapter
  and `src/<file>.rs`, and give the next hint - write full solutions only when
  asked.
