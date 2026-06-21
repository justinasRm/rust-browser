# 🪟 Chapter 10 — The interactive window

So far Robin has been a one-shot machine: feed it HTML, get a PNG back. That's
perfect for tests and screenshots, but it's not how anyone *uses* a browser. A
browser is something you **scroll**. You point it at a page that's taller than
your screen, and you drag, swipe, or arrow-key your way down it.

In this chapter we open a real operating-system window and present the rendered
page inside it — scrollable, until you close it. The whole thing lives in
[`src/window.rs`](../src/window.rs) and is barely a hundred lines, because we
lean on a clever shortcut.

## The idea

We use a crate called [`minifb`](https://crates.io/crates/minifb). The name
stands for "mini framebuffer", and that's exactly what it is: a tiny
cross-platform library that opens a window and lets you hand it a flat array of
pixels to display. It is **not** a GUI toolkit — there are no buttons, no
layout, no widgets. Just "here is a rectangle of pixels, draw it." That suits us
perfectly, because Robin already *has* a rectangle of pixels: the `Canvas` from
the rendering chapters.

The key trick is this:

1. Render the **whole page once** into a tall `Canvas`. The page might be many
   screens high — that's fine, the canvas can be any height.
2. Each frame, **copy the slice of rows** the user has scrolled to into a
   smaller window-sized buffer, and show that.
3. Keyboard and mouse-wheel input just nudge a single number — the **scroll
   offset** — up or down.

Rendering is the expensive part, and we do it exactly once. Scrolling is then
just memory-copying rows, which is cheap enough to do at 60 frames per second
without breaking a sweat.

## Walking the code

### `show()` — open the window and run the loop

```rust
pub fn show(canvas: &Canvas, title: &str) -> Result<(), String> {
    let view_w = canvas.width;
    // The visible height is capped so the window fits on screen even for a very
    // tall page.
    let view_h = canvas.height.min(800).max(1);

    let mut window = Window::new(
        title,
        view_w,
        view_h,
        WindowOptions {
            resize: false,
            scale: Scale::X1,
            scale_mode: ScaleMode::Stretch,
            ..WindowOptions::default()
        },
    )
    .map_err(|e| e.to_string())?;

    window.set_target_fps(60);
```

The window is as wide as the page and at most 800px tall (so a 5000px-tall
Wikipedia article doesn't open a window taller than your monitor). `view_h` is
the *visible* height; the full page height stays in `canvas.height`.

Next we prepare the pixels:

```rust
    // The full page as 0x00RGB pixels, and a reusable per-frame view buffer.
    let page = canvas.to_argb_u32();
    let max_scroll = canvas.height.saturating_sub(view_h);
    let mut scroll: usize = 0;
    let mut view = vec![0u32; view_w * view_h];
```

`canvas.to_argb_u32()` flattens the canvas into a `Vec<u32>`, one pixel per
`u32`, packed as `0x00RRGGBB` — minifb's expected format. The top byte (alpha)
is unused, so it's left at zero. Here's that packing, from `render.rs`:

```rust
pub fn to_argb_u32(&self) -> Vec<u32> {
    self.pixels
        .iter()
        .map(|p| (u32::from(p.r) << 16) | (u32::from(p.g) << 8) | u32::from(p.b))
        .collect()
}
```

`max_scroll` is the furthest down we can go: any further and we'd be scrolling
past the bottom of the page. `view` is the window-sized buffer we'll re-fill
every frame — allocated once, reused forever.

Then the event loop:

```rust
    while window.is_open() && !window.is_key_down(Key::Escape) && !window.is_key_down(Key::Q) {
        scroll = apply_input(&window, scroll, max_scroll, view_h);
        blit_view(&page, &mut view, view_w, view_h, canvas.height, scroll);
        window
            .update_with_buffer(&view, view_w, view_h)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

Every iteration is one frame: read input to update `scroll`, copy the visible
slice into `view`, and hand `view` to the window. The loop runs until the window
is closed, or the user presses **Escape** or **Q**.

### `apply_input()` — turn key/wheel presses into a new scroll offset

```rust
fn apply_input(window: &Window, scroll: usize, max_scroll: usize, view_h: usize) -> usize {
    let mut s = scroll as i64;
    let line = 48;
    let page = (view_h as i64 - 40).max(40);

    if window.is_key_down(Key::Down) || window.is_key_down(Key::J) {
        s += line;
    }
    if window.is_key_down(Key::Up) || window.is_key_down(Key::K) {
        s -= line;
    }
    if window.is_key_down(Key::PageDown) || window.is_key_down(Key::Space) {
        s += page;
    }
    if window.is_key_down(Key::PageUp) {
        s -= page;
    }
    if window.is_key_down(Key::Home) {
        s = 0;
    }
    if window.is_key_down(Key::End) {
        s = max_scroll as i64;
    }
    if let Some((_, wheel_y)) = window.get_scroll_wheel() {
        // Wheel up is positive; scrolling up should decrease the offset.
        s -= (wheel_y * 12.0) as i64;
    }

    s.clamp(0, max_scroll as i64) as usize
}
```

A small but complete set of controls:

- **Down / `j`** scroll one line (48px); **Up / `k`** scroll back up.
- **PageDown / Space** jump a near-full screen; **PageUp** jumps back.
- **Home** snaps to the top; **End** to the bottom.
- The **mouse wheel** via `get_scroll_wheel()` — wheel-up reads positive, and
  scrolling up should *decrease* the offset, so we subtract.

Everything happens in `i64` (signed) so an "Up" past the top can go negative
mid-calculation without underflowing. The final `s.clamp(0, max_scroll)` snaps
it back into the valid range before we cast home to `usize`.

### `blit_view()` — copy the visible rows

```rust
fn blit_view(page: &[u32], view: &mut [u32], view_w: usize, view_h: usize, page_h: usize, scroll: usize) {
    for row in 0..view_h {
        let src_row = scroll + row;
        let dst = &mut view[row * view_w..(row + 1) * view_w];
        if src_row < page_h {
            dst.copy_from_slice(&page[src_row * view_w..(src_row + 1) * view_w]);
        } else {
            dst.fill(0x00ff_ffff); // white padding below the page
        }
    }
}
```

For each row of the window, we figure out which page row it maps to
(`scroll + row`) and copy that whole row in one `copy_from_slice`. If we've run
off the end of the page (a short page in a tall window), we paint the rest white
instead of reading out of bounds.

## Rust notes

- **The event loop is just a `while`.** `window.is_open()` and
  `is_key_down(..)` are polled each pass — there's no callback machinery, no
  async runtime. minifb's `set_target_fps(60)` paces the loop for us.
- **One buffer, reused.** `view` is allocated once before the loop with
  `vec![0u32; view_w * view_h]` and refilled in place every frame. No
  per-frame allocation means no GC-style churn and steady performance.
- **Signed math for scrolling.** `scroll` lives as `usize`, but we lift it to
  `i64` inside `apply_input` so subtractions can dip below zero safely, then
  `.clamp(0, max_scroll as i64)` brings it home before casting back.
- **Slices and `copy_from_slice`.** `&page[a..b]` and `&mut view[a..b]` borrow
  sub-ranges with no copy; `copy_from_slice` then does a fast bulk memcpy. Rust
  checks at runtime that both slices are the same length.
- **`Result` + `map_err`.** Window creation and `update_with_buffer` return
  minifb errors; `.map_err(|e| e.to_string())?` converts them to our
  `String` error type and bubbles them up with `?`.

## Try it

This needs an actual display (it opens a window), so run it on your own machine
rather than over a headless SSH session:

```sh
cargo run -- assets/snapshots/wikipedia.html --window
```

A window opens with the rendered page. Controls:

| Key | Action |
| --- | --- |
| **Down / `j`**, **Up / `k`** | Scroll a line |
| **Space / PageDown**, **PageUp** | Scroll a page |
| **Home / End** | Jump to top / bottom |
| **Mouse wheel** | Scroll |
| **`q` / Esc** | Quit |

## Exercises

1. **Scrollbar indicator** *(easy)*. Draw a thin vertical bar on the right edge
   of `view` whose position and height reflect `scroll` and `max_scroll`. It's
   just a few columns of grey pixels filled in after `blit_view`.

2. **Horizontal scroll** *(medium)*. Right now the window is exactly as wide as
   the page. Cap `view_w` like we cap `view_h`, add a second `scroll_x` offset,
   and shift each copied row by `scroll_x` columns. Wire it to Left/Right keys.

3. **Click a link to navigate** *(hard)*. Have the layout step record link
   rectangles and their URLs. In the loop, read `window.get_mouse_pos(..)` and
   `get_mouse_down(..)`; on a click, find which rectangle (offset by `scroll`)
   was hit, fetch that URL, re-render into a fresh `Canvas`, and swap `page`.

4. **Live reload** *(hard)*. When the source is a local file, watch its
   modification time (or use the `notify` crate). On change, re-parse,
   re-layout, re-render, and rebuild `page` — so editing the HTML updates the
   window instantly.

---

Previous: [Networking](09-networking.md) · Next: [Rendering real pages](11-rendering-real-pages.md) · Source: [src/window.rs](../src/window.rs)
