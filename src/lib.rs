//! # Robin — a tiny web browser engine, built from scratch in Rust
//!
//! Robin is a teaching browser engine. It is **not** a wrapper around Chromium
//! or WebKit: every stage of the rendering pipeline is implemented here in
//! plain Rust so you can read it, change it, and learn how a browser actually
//! turns bytes on the wire into pixels on the screen.
//!
//! The pipeline, stage by stage (each is one module and roughly one commit in
//! the project's history):
//!
//! ```text
//!   bytes ──► HTML parser ──► DOM tree
//!                               │
//!                CSS parser ──► stylesheet
//!                               │
//!                               ▼
//!                        style (the cascade) ──► styled tree
//!                               │
//!                               ▼
//!                          layout ──► box tree (positions + sizes)
//!                               │
//!                               ▼
//!                          paint ──► display list ──► pixels
//! ```
//!
//! Start reading at [`html`], then [`css`], [`style`], [`layout`] and
//! [`paint`]. The [`net`] module fetches pages, and [`window`] shows them in a
//! scrollable window.

// Each stage of the pipeline lives in its own module. They are added to the
// project one commit at a time; see `docs/` for the matching tutorial chapter.

pub mod css;
pub mod dom;
pub mod html;
pub mod layout;
pub mod style;

/// Crate version, surfaced in the CLI banner.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
