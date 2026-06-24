# 🚧 Chapter 12 - Limitations & next steps

Robin renders Hacker News, Wikipedia and example.com well enough to read. It is
also, very deliberately, a *toy*. Knowing exactly **what it can't do** is part of
understanding what a real browser is - every limitation below is a feature a
production engine spends enormous effort on.

This chapter is a map of the edges, roughly easiest-to-hardest, so you can pick
your next project.

## Things Robin doesn't do (yet)

**No JavaScript.** Robin renders the HTML it's given and stops. Modern pages -
Google's real homepage, anything "single-page" - build themselves with JS after
load, so Robin sees only the initial markup. A JS engine is a whole second
universe (parsing, a bytecode VM, the DOM bindings that let scripts mutate the
tree). This is the single biggest gap.

**No images or form controls.** `<img>` contributes nothing, and `<input>` /
`<button>` / `<select>` don't render as widgets. That's why Google's page shows
its links but not its logo or search box. Decoding images (the `image` crate is
already a dependency) and drawing placeholder boxes is a very approachable
upgrade - see the exercises in [Chapter 6](06-painting.md).

**Only simple selectors.** No descendant (`.nav a`), child (`>`), sibling, or
pseudo-class (`:hover`) selectors - Robin [drops](11-rendering-real-pages.md)
what it can't model. Real stylesheets lean on these constantly, which is why
Robin falls back to its user-agent defaults more than a real browser would (and
why Wikipedia's table of contents renders with too much spacing).

**No floats, flexbox, grid, or positioning.** Layout is block-and-inline flow
only. `display: flex`/`grid` are treated as block, and `float` / `position:
absolute` are ignored. This is why Wikipedia's sidebar appears stacked at the top
of the page (in DOM order) instead of floated beside the article. Modern layout
is one of the deepest parts of a real engine.

**Simplified tables.** `<table>` is rendered as stacked blocks, not a real grid
with shared column widths. Content stays readable but not aligned into columns.

**Whitespace and typography shortcuts.** `white-space: pre` isn't honored in
wrapping, unitless `line-height` is ignored, `em` is resolved against a fixed
16px base rather than the inherited font-size, and there's no bidi or complex
text shaping. Good enough for Latin prose; not for the real world's scripts.

**Layout isn't incremental.** Every render lays out the whole page from scratch.
Real browsers do an enormous amount of work to *re-*layout only what changed when
you scroll, resize, or a script mutates the DOM.

## Good next projects

Roughly in order of effort:

1. **Image placeholders, then real images.** Reserve a box from `width`/`height`
   attributes and draw the `alt` text; then decode `<img src>` with the `image`
   crate and blit it. Big visual payoff.
2. **`text-align`.** You already compute each line's width in the inline layout -
   centering or right-aligning a line is mostly arithmetic. ([Chapter 8](08-inline-layout-and-text.md).)
3. **Descendant selectors.** Extend the CSS model to a list of simple selectors
   and match by walking ancestors. This alone makes real stylesheets behave far
   better. ([Chapter 3](03-css-parser.md).)
4. **Clickable links in the window.** Record each link's screen rectangle during
   paint, hit-test mouse clicks, and load the new URL - a real address bar.
   ([Chapter 10](10-interactive-window.md).)
5. **A glyph cache.** Rasterizing the same character over and over is wasteful;
   memoize `(char, size, face)` → bitmap. A clean intro to caching in Rust.
   ([Chapter 7](07-text-rasterization.md).)
6. **Floats.** The classic next layout feature, and a genuine challenge.
7. **A tiny JavaScript engine.** The deep end. Even a parser that can run
   `document.querySelector(...).textContent = "..."` teaches you a lot.

## Where to read more

- Matt Brubeck's [*Let's build a browser engine!*](https://limpet.net/mbrubeck/2014/08/08/toy-layout-engine-1.html):
  the **robinson** series that inspired Robin (and its name).
- [web.dev: *How browsers work*](https://web.dev/articles/howbrowserswork) - the
  canonical high-level tour of the real pipeline.
- The [HTML](https://html.spec.whatwg.org/multipage/parsing.html) and
  [CSS](https://www.w3.org/TR/css-cascade/) specs - dense, but the parsing and
  cascade sections map directly onto the code you've just read.
- [Servo](https://github.com/servo/servo) - a real, parallel browser engine in
  Rust. What Robin's modules look like at production scale.

You've built a browser. Most people never look inside one; you've written every
stage. Go break it, extend it, and make it yours.

---

Previous: [Rendering real pages](11-rendering-real-pages.md) · Back to [start](00-how-to-use-this-repo.md)
