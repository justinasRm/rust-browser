# ✍️ Chapter 7 — Text rasterization

Up to now every box has been a solid rectangle. That got us backgrounds, borders,
and list bullets — but look at any real web page and you'll notice the obvious: it's
mostly **text**. Headings, paragraphs, links, code. Drawing text is its own small
world, and this chapter is where we enter it.

The job breaks into three steps. **Load a font** (a file full of glyph outlines).
**Ask it for the shape** of each character at a given size. **Blend that shape**
onto the canvas in the right place and the right color. The shapes come out as
grayscale bitmaps, and those gray edges are exactly what makes text look smooth
instead of jagged.

All of this lives in [`src/text.rs`](../src/text.rs). We lean on the
[`fontdue`](https://crates.io/crates/fontdue) crate for the genuinely hard part —
decoding TrueType outlines and turning curves into pixels — and we focus on wiring
it into Robin's canvas.

## The idea

Two different jobs share the same font data, and it's worth separating them in your
head:

- **Measuring** answers "how wide is this string?" Layout needs this *before* it
  draws anything, so it can decide where lines wrap and how wide a paragraph is.
  Measuring touches no pixels — it just sums up advances.
- **Drawing** is the paint-time job: rasterize each glyph and blend it onto the
  canvas.

### Coverage bitmaps and anti-aliasing

When you ask fontdue to rasterize a character, you get back a flat array of bytes —
one per pixel — called a **coverage bitmap**. Each byte (0–255) says *how much* of
that pixel the glyph covers: `0` is "not touched", `255` is "fully inside the
letter", and the values in between live along the curved and diagonal edges where a
pixel is only partly covered.

We treat coverage as **alpha**. A half-covered edge pixel blends halfway between the
text color and the background, which is what gives smooth, **anti-aliased** edges
instead of the staircase you'd get from a hard on/off test. No special magic — the
gray values fall straight out of the rasterizer, and we just feed them to the same
alpha-blending math from Chapter 6.

### The baseline

Fonts aren't positioned by their top-left corner; they hang from a **baseline** —
the line the bottoms of most letters sit on. Two numbers describe how a line of text
relates to that baseline:

- **ascent** — how far the tallest glyphs rise *above* the baseline.
- **descent** — how far descenders (the tails of `g`, `p`, `y`) drop *below* it.

Each individual glyph also reports where its bitmap sits relative to the pen and the
baseline (`xmin`, `ymin`, `height`) plus an `advance_width` — how far to move the
pen before the next character. We'll use all of these in `draw_run`.

### Why bundle the fonts

Robin embeds the DejaVu fonts directly into the binary with `include_bytes!`. That
costs a few hundred kilobytes, but it buys two real things: the binary is **fully
self-contained** (no "font not found" failures on a fresh machine), and rendering is
**identical everywhere** because everyone uses the exact same font bytes. For a
teaching project where the output PNGs need to match, that determinism matters.

## Walking the code

### The `Fonts` struct

We parse five faces once and hold onto them. The bytes are baked in at compile time:

```rust
const SANS: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
const SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
const SANS_ITALIC: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Oblique.ttf");
const SANS_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-BoldOblique.ttf");
const MONO: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

pub struct Fonts {
    regular: Font,
    bold: Font,
    italic: Font,
    bold_italic: Font,
    mono: Font,
}
```

`bundled()` parses each one. Parsing a TTF isn't free, so we do it a single time at
startup and pass `&Fonts` around. Notice it returns a `Result` rather than panicking
— in practice the bytes always parse (they shipped with the binary), but surfacing
an error is tidier than an `unwrap` deep in the rendering path:

```rust
pub fn bundled() -> Result<Fonts, String> {
    let load = |bytes| Font::from_bytes(bytes, FontSettings::default()).map_err(String::from);
    Ok(Fonts {
        regular: load(SANS)?,
        bold: load(SANS_BOLD)?,
        // ...
    })
}
```

### Picking a face

A text run carries bold/italic/monospace flags; `face()` maps those to the right
parsed `Font`. Monospace wins outright (a bold mono run still uses the one mono
face we bundle):

```rust
fn face(&self, bold: bool, italic: bool, monospace: bool) -> &Font {
    match (monospace, bold, italic) {
        (true, _, _) => &self.mono,
        (false, true, true) => &self.bold_italic,
        (false, true, false) => &self.bold,
        (false, false, true) => &self.italic,
        (false, false, false) => &self.regular,
    }
}
```

### Measuring

`char_advance` asks the font for one character's `advance_width`; `measure` sums
those advances across a whole string. This is the measuring path — no rasterization,
no pixels:

```rust
pub fn measure(&self, text: &str, size: f32, bold: bool, italic: bool, mono: bool) -> f32 {
    let font = self.face(bold, italic, mono);
    text.chars().map(|c| font.metrics(c, size).advance_width).sum()
}
```

`line_metrics` reads the font's vertical metrics for a given size and returns a
`LineMetrics { ascent, descent, line_height }`. One gotcha: fontdue reports descent
as a *negative* number, so we flip its sign. If the font lacks the metrics table we
fall back to rough proportions of the font size:

```rust
match font.horizontal_line_metrics(size) {
    Some(m) => LineMetrics {
        ascent: m.ascent,
        descent: -m.descent, // fontdue reports descent as negative
        line_height: m.new_line_size,
    },
    None => LineMetrics { ascent: size * 0.8, descent: size * 0.2, line_height: size * 1.2 },
}
```

### Drawing a run

`draw_run` is where it all comes together. `run.y` is the **baseline**, and `pen_x`
walks left to right. For each character we rasterize, position its bitmap relative to
the baseline, blit it, then advance the pen:

```rust
pub fn draw_run(&self, canvas: &mut Canvas, run: &TextRun) {
    let font = self.face(run.bold, run.italic, run.monospace);
    let mut pen_x = run.x;
    for ch in run.text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, run.font_size);
        let glyph_left = pen_x + metrics.xmin as f32;
        let glyph_top = run.y - (metrics.ymin as f32 + metrics.height as f32);
        blit_coverage(canvas, &bitmap, metrics.width, metrics.height, glyph_left, glyph_top, run.color);
        pen_x += metrics.advance_width;
    }
}
```

The `glyph_top` line is the one to stare at. Screen space has `y` growing *downward*,
but `ymin` measures upward from the baseline. The bitmap's *top* edge sits above the
baseline by `ymin + height`, so we subtract that from the baseline `y` to find where
row 0 of the bitmap lands.

### Blitting coverage

Finally `blit_coverage` turns the coverage bitmap into pixels. It rounds the
floating-point position to whole pixels, then for each covered byte combines glyph
coverage with the run's own alpha and calls `Canvas::blend_pixel`:

```rust
fn blit_coverage(canvas, coverage, width, height, left, top, color) {
    let left = left.round() as i32;
    let top = top.round() as i32;
    for row in 0..height {
        for col in 0..width {
            let cov = coverage[row * width + col];
            if cov == 0 { continue; }
            let alpha = (cov as u32 * color.a as u32 / 255) as u8;
            let x = left + col as i32;
            let y = top + row as i32;
            if x >= 0 && y >= 0 {
                canvas.blend_pixel(x as usize, y as usize, Color { a: alpha, ..color });
            }
        }
    }
}
```

`blend_pixel` (back in `src/render.rs`) does straight alpha blending of that
semi-transparent color over whatever was already on the canvas — the same routine
that powers `fill_rect`. Skipping `cov == 0` pixels is a small but real speedup:
glyph bitmaps are mostly empty space.

## Rust notes

- **`include_bytes!`** embeds a file's contents into the binary at compile time as a
  `&'static [u8]`. It's the idiomatic way to ship assets (fonts, icons, default
  config) without depending on the filesystem at runtime.
- **`&[u8]` slices** let `from_bytes` read the font data without copying it. Robin
  hands fontdue a view into the embedded bytes and lets it borrow.
- **Build once, share `&Fonts`.** Parsing TTFs is expensive, so we construct `Fonts`
  a single time and pass shared references through painting. The type owns the parsed
  `Font`s; everyone else borrows.
- **`Result` over `panic`.** `bundled()` returns `Result<Fonts, String>` so a parse
  failure propagates cleanly with `?` instead of crashing — good habit even when the
  failure is "impossible".
- **Float vs. integer coordinates.** Layout works in `f32` (sub-pixel positions), but
  pixels are integers. `blit_coverage` rounds at the last moment with `.round() as
  i32`, keeping the math precise for as long as possible.

## Try it

Run the text tests:

```sh
cargo test text
```

You'll see the font load and measure, the line metrics come out sensible, and a
drawn run actually deposit dark pixels on a white canvas. More fun: text now shows up
in **any** rendered PNG from the earlier chapters — re-run an example and the boxes
finally have words in them.

## Exercises

1. **Underline (easy).** Add an `underline: bool` to `draw_run` (or `TextRun`) and,
   when set, fill a thin horizontal rectangle a pixel or two below the baseline
   spanning the run's measured width. Reuse `Canvas::fill_rect`.
2. **Glyph cache (medium).** Rasterizing the same `(char, size, face)` repeatedly is
   wasteful. Add a `HashMap` keyed on those fields that caches the returned
   `(metrics, bitmap)`. Measure the speedup on a text-heavy page.
3. **A serif face (medium).** Bundle a serif TTF with `include_bytes!`, add a
   `serif: Font` field and a flag, and extend `face()` to select it. Wire a CSS
   `font-family: serif` through to the run flags.
4. **Gamma / subpixel (hard).** Anti-aliased text can look too thin on light
   backgrounds. Apply a gamma curve to `cov` before using it as alpha and compare the
   PNGs. For extra credit, explore fontdue's subpixel rendering and what it would take
   to blend three coverage channels.

---

Previous: [Painting](06-painting.md) · Next: [Inline layout & text flow](08-inline-layout-and-text.md) · Source: [src/text.rs](../src/text.rs)
