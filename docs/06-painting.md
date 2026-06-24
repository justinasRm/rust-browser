# 🖌️ Chapter 6 - Painting

By now we have a box tree: every element knows its position and size on the
page. But a box tree isn't an image - you can't look at it. This chapter turns
geometry into pixels.

We do it in two steps. First we **flatten** the box tree into a *display list*:
a flat, ordered list of "draw this rectangle here, in this color" commands.
Then we **rasterize** that list onto a `Canvas` - a plain array of colors - and
finally hand the buffer to the `image` crate to write a PNG.

Two files do the work: `src/paint.rs` builds and runs the display list, and
`src/render.rs` is the pixel canvas underneath it.

## The idea

Why bother with a display list at all? Why not walk the box tree and draw
straight to the screen?

Because the indirection buys us a lot for almost no cost:

- **It's simple.** A command is just `SolidColor(color, rect)` or a run of
  text. No tree, no recursion, no style lookups - just a flat `Vec` you can
  iterate top to bottom.
- **It's testable.** You can build the list and assert on it ("there should be
  one orange rectangle here") without ever touching pixels. Our tests do exactly
  this.
- **It's replayable.** The same list can be drawn to a PNG, to a window, or (in
  a real browser) handed to the GPU or another thread. Separating *what to draw*
  from *how to draw it* is one of the oldest tricks in graphics.

The canvas itself is humble. A `Canvas` is just a flat array of `Color`s,
`width * height` of them, laid out row by row. "Drawing" means writing colors
into that array. When a color is semi-transparent we **alpha-blend** it over
what's already there, so layered colors mix correctly.

And it's all done on the CPU: this is a **software rasterizer**. No GPU, no
graphics API, just arithmetic on a byte array. That's all a browser's
compositor is underneath - we've just removed the hardware acceleration.

## Walking the code

### The display list

A drawing instruction is one of two things:

```rust
pub enum DisplayCommand {
    /// Fill a rectangle with a solid color.
    SolidColor(Color, Rect),
    /// Draw a string of text.
    Text(TextRun),
}
```

`SolidColor` covers backgrounds, borders, and list bullets - anything that's a
flat rectangle. `Text` carries a `TextRun` (the string, its position, font size,
color, and bold/italic/monospace flags). We *emit* text commands here, but
actually rasterizing glyphs is the next chapter's job - the `TextRun` just keeps
the display-list shape stable until then.

### Building it

`build_display_list` kicks off a recursive walk:

```rust
pub fn build_display_list(layout_root: &LayoutBox) -> Vec<DisplayCommand> {
    let mut list = Vec::new();
    render_layout_box(&mut list, layout_root);
    list
}
```

The heart of the walk is `render_layout_box`, and the **order matters** - later
commands paint over earlier ones:

```rust
fn render_layout_box(list: &mut Vec<DisplayCommand>, layout_box: &LayoutBox) {
    render_background(list, layout_box);
    render_borders(list, layout_box);
    render_list_marker(list, layout_box);
    render_text(list, layout_box);
    for child in &layout_box.children {
        render_layout_box(list, child);
    }
}
```

So for each box we paint: background first, then borders on top, then a list
bullet (for `display: list-item`), then text, and *then* we recurse into
children so they land on top of their parent. This is a simplified version of
CSS paint order, but it gets the common cases right.

`render_background` is a one-liner: if the box has a background color, push a
`SolidColor` covering its padding box. `render_list_marker` drops a small 5×5
filled square in the list's left padding - a bullet without any font math.

### The four border rects

There's no "draw a rectangle outline" primitive - we only have *filled*
rectangles. So a border is four thin filled rects, one per edge.
`render_borders` builds them from the box's `border_box()` and its per-edge
border widths:

```rust
// Left edge.
list.push(DisplayCommand::SolidColor(color, Rect {
    x: border_box.x,
    y: border_box.y,
    width: d.border.left,
    height: border_box.height,
}));
// Right edge.
list.push(DisplayCommand::SolidColor(color, Rect {
    x: border_box.x + border_box.width - d.border.right,
    y: border_box.y,
    width: d.border.right,
    height: border_box.height,
}));
// Top and bottom edges follow the same pattern...
```

The left and right edges run the full height; the top and bottom run the full
width. They overlap in the corners, which is fine for a solid color. (The
border color comes from `border-color`, falling back to the text `color`, then
to black.)

### The canvas

Over in `src/render.rs`, a canvas is created pre-filled with a background color:

```rust
pub fn new(width: usize, height: usize, background: Color) -> Canvas {
    Canvas { width, height, pixels: vec![background; width * height] }
}
```

`fill_rect` is where rectangles become pixels. It does two jobs - **clip** the
rect to the canvas, then **blend** each pixel:

```rust
pub fn fill_rect(&mut self, rect: Rect, color: Color) {
    if color.a == 0 {
        return; // fully transparent: nothing to draw
    }
    // Clip to the canvas bounds, rounding to whole pixels.
    let x0 = rect.x.max(0.0) as usize;
    let y0 = rect.y.max(0.0) as usize;
    let x1 = ((rect.x + rect.width).min(self.width as f32)).max(0.0) as usize;
    let y1 = ((rect.y + rect.height).min(self.height as f32)).max(0.0) as usize;

    for y in y0..y1 {
        for x in x0..x1 {
            let idx = y * self.width + x;
            self.pixels[idx] = blend(color, self.pixels[idx]);
        }
    }
}
```

The `.max(0.0)` and `.min(self.width as f32)` clamps mean a rectangle hanging
off any edge just gets trimmed instead of panicking with an out-of-bounds index.
The index math `y * self.width + x` is how a 2-D position becomes a 1-D array
offset.

Blending uses **straight (non-premultiplied) alpha**:

```rust
fn blend(src: Color, dst: Color) -> Color {
    if src.a == 255 {
        return src; // opaque: just overwrite
    }
    let sa = src.a as f32 / 255.0;
    let inv = 1.0 - sa;
    let mix = |s: u8, d: u8| ((s as f32) * sa + (d as f32) * inv).round() as u8;
    Color { r: mix(src.r, dst.r), g: mix(src.g, dst.g), b: mix(src.b, dst.b), a: 255 }
}
```

A fully opaque source short-circuits to a plain overwrite. Otherwise each
channel is a weighted average of source and destination. There's also
`blend_pixel`, the single-pixel version that glyph rasterization uses when every
pixel has its own coverage value.

Running the whole list is just a `match` over commands:

```rust
pub fn paint_list(canvas: &mut Canvas, list: &[DisplayCommand], fonts: &crate::text::Fonts) {
    for cmd in list {
        match cmd {
            DisplayCommand::SolidColor(color, rect) => canvas.fill_rect(*rect, *color),
            DisplayCommand::Text(run) => fonts.draw_run(canvas, run),
        }
    }
}
```

### Getting it out

A few helpers turn the canvas into something the outside world wants.
`to_rgba_bytes` packs the pixels into tightly-packed RGBA bytes; `to_argb_u32`
packs them into `0xRRGGBB` `u32`s (what the `minifb` window wants); `cropped`
slices out a horizontal band of rows for screenshotting a tall page. And
`save_png` ties it together:

```rust
pub fn save_png(&self, path: &str) -> Result<(), String> {
    image::save_buffer(path, &self.to_rgba_bytes(), self.width as u32,
        self.height as u32, image::ColorType::Rgba8)
        .map_err(|e| e.to_string())
}
```

The `image` crate handles all the PNG encoding - we just hand it the raw bytes
and the dimensions.

## Rust notes

- **A `Vec<Color>` is a framebuffer.** No fancy 2-D type - a flat, contiguous
  `Vec` is exactly what hardware uses, and it's cache-friendly to scan in row
  order.
- **`y * width + x` is the row-major index.** Every 2-D pixel access flattens
  to this. Getting it wrong is the classic graphics bug, so it lives in one
  place (`fill_rect` / `blend_pixel`) and nowhere else.
- **Enums make commands self-describing.** `DisplayCommand` is a closed set, so
  `paint_list`'s `match` is exhaustive - add a new command variant and the
  compiler forces you to handle it.
- **Clamping with `.min()` / `.max()`** on `f32` is how we clip without
  branches: `rect.x.max(0.0)` and `(...).min(self.width as f32)` keep every
  index in range before it ever touches the `Vec`.
- **The `image` crate is the only PNG code we need.** `save_buffer` does the
  encoding; our job is just to produce correct bytes.

## Try it

Make a tiny page with a colored box:

```bash
cat > /tmp/x.html <<'EOF'
<html><body>
  <div style="width: 120px; height: 80px; background: #ff6600;
              border-width: 4px; border-color: #003366;"></div>
</body></html>
EOF

cargo run -- /tmp/x.html --png /tmp/x.png
```

Open `/tmp/x.png` and you should see an orange rectangle with a dark blue
border on a white page - backgrounds, borders, and the canvas all working
together.

Run the painting and canvas tests:

```bash
cargo test paint    # display-list shape: background → one rect, border → four rects
cargo test render   # canvas: clipping doesn't panic, alpha blends halfway
```

## Exercises

1. **Image placeholder box (easy).** Real browsers draw a gray box with a
   broken-image icon when an `<img>` fails to load. Add a
   `DisplayCommand::Placeholder(Rect)` variant, emit it from a new
   `render_layout_box` step, and have `paint_list` draw a gray fill with a black
   outline (reuse the four-border-rects trick).

2. **Checkerboard debug mode (easy/medium).** Add a function that fills the
   whole canvas with a light/dark checkerboard *before* painting the page. It
   makes transparent and unpainted regions obvious. Wire it to a `--debug-bg`
   flag.

3. **Rounded corners (medium).** Approximate `border-radius` by skipping the
   corner pixels of a `SolidColor` fill. In `fill_rect`, given a radius `r`,
   leave a pixel unfilled when it falls outside a quarter-circle at the nearest
   corner. Start with a single radius for all four corners.

4. **Clip to a parent (medium/hard).** Right now a child can paint outside its
   parent (think `overflow: hidden`). Add an optional clip `Rect` threaded
   through `render_layout_box`, intersect it with each child's box, and pass the
   final clip into `fill_rect` so pixels outside it are skipped. Watch the index
   math - clipping is just tighter `x0/y0/x1/y1` bounds.

---

Previous: [Block layout](05-block-layout.md) · Next: [Text rasterization](07-text-rasterization.md) · Source: [src/paint.rs](../src/paint.rs), [src/render.rs](../src/render.rs)
