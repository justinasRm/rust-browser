# 🔤 Chapter 8 — Inline layout & text flow

Block layout (Chapter 6) gave us a stack of rectangles: each box as wide as its
container, stacked top to bottom. That is the skeleton of a page. But a page is
not really made of rectangles — it is made of *words*. This chapter is where the
skeleton gets its text, where boxes stop being grey slabs and start being
**readable**.

The job sounds simple: put text inside a box. The reality is the single most
important loop in a browser's layout engine. Text does not fit on one line, so we
have to break it into lines. Words have different widths, so we have to *measure*
them. A sentence can mix a bold word, a link, and plain text — and they all share
the same line, aligned along a common baseline. That whole machine is the
**inline formatting context**, and Robin builds it in `src/layout.rs`.

## The idea

An inline formatting context flows content **left to right**, then **top to
bottom**, like a typewriter:

1. Walk the inline boxes inside the strip, in document order.
2. Split their text into **words** (runs separated by whitespace).
3. **Measure** each word at its font/size/weight to get a pixel width.
4. Place words along a horizontal cursor, advancing as you go.
5. When the next word would **overflow** the available width, start a new line.
6. **Collapse whitespace**: any run of spaces, tabs, and newlines in the source
   becomes a single space between words.
7. A `<br>` forces a line break wherever it appears.
8. **Mixed styles share a line** — bold, italic, and plain words sit side by
   side, and each line is aligned to the **baseline** of its tallest glyph.

That last point is the subtle one. A 16px word and a 24px word on the same line
do not line up by their tops or their bottoms — they line up by the invisible
line their letters *sit on*, the baseline. So we cannot assign a word its final
vertical position until we have seen the whole line and know who the tallest
glyph is. Robin handles this by buffering a line, then committing it.

## Walking the code

### Setting up the strip

A block that contains inline children wraps them in an anonymous box. When that
box lays itself out, it runs `layout_inline`. It first borrows a full-width strip
(same width logic as a block, but with no box-model rings), then hands that strip
to an `InlineFlow`:

```rust
fn layout_inline(&mut self, containing_block: Dimensions, fonts: &Fonts) {
    self.calculate_block_width(containing_block);
    self.calculate_block_position(containing_block);

    let origin = self.dimensions.content;
    let mut flow = InlineFlow::new(origin.x, origin.y, origin.width, fonts);
    for child in &self.children {
        flow.add_box(child);
    }
    flow.finish();
    self.fragments = flow.fragments;
    self.dimensions.content.height = flow.height;
}
```

Notice that `fonts` is threaded in. The whole reason the `Fonts` set travels all
the way down the layout pass — `layout_tree` → `layout` → `layout_inline` — is
that **you cannot lay out text without measuring it**, and measuring needs the
actual glyph metrics from `src/text.rs`. Layout and font metrics are inseparable.

`InlineFlow` keeps the line-breaker's running state:

- `line_start_x` / `origin_y` — the top-left corner of the strip.
- `avail` — how wide a line may be.
- `cursor_x` — the horizontal pen position on the current line.
- `line_top` — the y of the top of the current line.
- `current_line: Vec<PendingFragment>` — words placed but not yet vertically
  positioned (we are still discovering the tallest glyph).
- `fragments: Vec<InlineFragment>` — finished, baseline-positioned text.
- `pending_space` — whether a space should precede the next word.
- `height` — total vertical space consumed so far.

### Walking the inline boxes

`add_box` recurses through the inline subtree. A text node emits its words; a
`<br>` forces a break; anything else (an `<a>`, a `<b>`, a nested span) flows its
children into the *same* context, which is exactly what keeps mixed styles on one
line:

```rust
fn add_box(&mut self, b: &LayoutBox) {
    if let Some(node) = b.styled_node() {
        if let Some(text) = node.node.text_content() {
            self.add_text(text, InlineStyle::from_node(node));
            return;
        }
        if node.node.tag_name() == Some("br") {
            self.break_line();
            return;
        }
    }
    for child in &b.children {
        self.add_box(child);
    }
}
```

`InlineStyle::from_node` reads the visual properties that affect both
measurement and painting: `font-size` (defaulting to 16px), `font-weight` (≥600
or the `bold`/`bolder` keyword counts as bold), `font-style` (`italic`/`oblique`),
a `mono`-containing `font-family` (monospace), and `color`. These flags pick the
font face when we measure, so a bold word is measured with the bold face.

### Splitting and collapsing text

`add_text` does the whitespace collapsing. `split_whitespace()` already drops
runs of internal whitespace, so the only thing we track by hand is whether the
text *started* or *ended* with whitespace — those edges become a `pending_space`
that separates this run from its neighbours:

```rust
fn add_text(&mut self, text: &str, style: InlineStyle) {
    let leading_ws = text.starts_with(char::is_whitespace);
    let trailing_ws = text.ends_with(char::is_whitespace);
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        // Whitespace-only text still separates its neighbours.
        self.pending_space |= leading_ws || trailing_ws;
        return;
    }
    if leading_ws {
        self.pending_space = true;
    }
    let last = words.len() - 1;
    for (i, word) in words.into_iter().enumerate() {
        self.place_word(word, style);
        if i < last {
            self.pending_space = true; // space between words in this run
        }
    }
    if trailing_ws {
        self.pending_space = true;
    }
}
```

A `pending_space` is *deferred*: we never emit a trailing space at the end of a
line. The space only becomes real width if another word follows it on the same
line.

### Placing a word and deciding to wrap

`place_word` is the heart of the line-breaker. It measures the word (and the
pending space, if any), then asks the one question that makes text wrap:

```rust
fn place_word(&mut self, word: &str, style: InlineStyle) {
    let word_w = self.fonts.measure(word, style.size, style.bold, style.italic, style.monospace);
    let space_w = if self.pending_space {
        self.fonts.measure(" ", style.size, style.bold, style.italic, style.monospace)
    } else {
        0.0
    };

    let at_line_start = self.cursor_x <= self.line_start_x + 0.01;
    let overflows = self.cursor_x + space_w + word_w > self.line_start_x + self.avail;
    if overflows && !at_line_start {
        self.break_line();
    } else {
        self.cursor_x += space_w;
    }
    self.pending_space = false;
    // ... push a PendingFragment with x = cursor_x, baseline filled later ...
    self.cursor_x += word_w;
}
```

The wrap test is `cursor_x + space_w + word_w > line_start_x + avail`: *would the
pen, plus the space, plus this word, run past the right edge?* If so — and we are
not already at the start of a line — we break. The `!at_line_start` guard matters:
a single word wider than the whole strip still has to go *somewhere*, so we place
it rather than loop forever breaking onto empty lines.

When the word is placed, its `baseline` is left at `0.0`. We record the glyph's
`ascent` and `line_height` (from `line_metrics`) in a `PendingFragment` and move
on — the real baseline comes at the end of the line.

### Ending a line

`break_line` is where the buffered line gets committed. It folds over the
pending fragments to find the **tallest ascent** and the largest line height,
puts the baseline that far below the line's top, then stamps that baseline onto
every fragment and advances:

```rust
fn break_line(&mut self) {
    let (max_ascent, line_height) = self
        .current_line
        .iter()
        .fold((0.0_f32, 0.0_f32), |(a, h), pf| (a.max(pf.ascent), h.max(pf.line_height)));
    let line_height = if self.current_line.is_empty() {
        self.fonts.line_metrics(16.0, false, false, false).line_height
    } else {
        line_height
    };
    let baseline = self.line_top + max_ascent;

    for mut pf in self.current_line.drain(..) {
        pf.frag.baseline = baseline;
        self.fragments.push(pf.frag);
    }
    self.line_top += line_height;
    self.cursor_x = self.line_start_x;
    self.pending_space = false;
    self.height = self.line_top - self.origin_y;
}
```

Aligning to `line_top + max_ascent` is the baseline rule: a tall word and a short
word on the same line share the y where their letters sit, so a 24px heading word
next to 16px body text reads correctly. An empty line (a leading `<br>`) still
advances by a default 16px line height, so blank lines take up space.

Finally, `finish` flushes whatever words are still buffered when the input runs
out — the last line never ends with a `<br>`, so it needs an explicit commit.

## Rust notes

- **`&Fonts` rides the whole recursion.** Because measuring is part of layout,
  every layout method takes `fonts: &Fonts`. It is a shared reference, so the one
  parsed font set is borrowed immutably all the way down — no cloning, no global.
- **Two vectors, two phases.** `current_line: Vec<PendingFragment>` is the
  *in-progress* line; `fragments: Vec<InlineFragment>` is the *finished* output.
  A `PendingFragment` is just an `InlineFragment` plus the `ascent` and
  `line_height` we need to position it. Buffering in one vec and draining into the
  other is what lets us decide the baseline only once the line is complete.
- **`fold` finds the tallest glyph.** `current_line.iter().fold((0.0, 0.0), …)`
  reduces the line to its `(max_ascent, max_line_height)` in a single pass — a
  clean functional way to ask "who is the tallest word here?"
- **`char::is_whitespace` and `split_whitespace`.** The standard library already
  knows what counts as whitespace across Unicode, so collapsing runs of spaces,
  tabs, and newlines is `text.split_whitespace()` plus two edge checks with
  `starts_with(char::is_whitespace)` / `ends_with(...)`.

## Try it

Make a paragraph long enough to wrap and force it into a narrow column:

```html
<!-- /tmp/x.html -->
<p>The quick brown fox jumps over the lazy dog while a
curious cat watches the whole affair from a sunny windowsill,
entirely unimpressed by the commotion below.</p>
```

```sh
cargo run -- /tmp/x.html --png /tmp/x.png --width 300
```

Open `/tmp/x.png` and you should see the sentence broken across several lines,
each filling roughly 300px before wrapping. Try `--width 600` and watch the same
text reflow onto fewer lines — that is `place_word`'s wrap test responding to a
wider `avail`.

The wrapping, `<br>`, and mixed-style logic all have tests:

```sh
cargo test layout
```

Look in particular at `long_text_wraps_onto_multiple_lines` (counts distinct
baselines to prove the text wrapped), `br_forces_a_line_break` (the second line's
baseline is below the first), and `bold_and_italic_flags_follow_markup` (a `<b>`
word carries `bold`, an `<i>` word carries `italic`, plain text carries neither).

## Exercises

1. **Honor `white-space: pre`.** Right now all whitespace collapses. Add a flag
   to `InlineStyle` for `white-space`, and when it is `pre`, stop splitting on
   whitespace — preserve the runs of spaces and break the line on every literal
   `\n`. (Easy: just newlines. Harder: preserve internal spaces too.)
2. **Implement `text-align`.** After `break_line` knows the line's final width
   (`cursor_x - line_start_x`), shift every fragment on that line right by the
   leftover space for `right`, or half of it for `center`. You will need to pass
   the alignment down from the styled node into `InlineFlow`.
3. **Break very long words.** A URL or a 200-character token wider than `avail`
   currently overflows the strip. Detect this in `place_word` and split the word
   character by character (measuring as you go) so it wraps mid-word like a
   browser's `overflow-wrap: anywhere`.
4. **Respect a unitless `line-height`.** CSS `line-height: 1.5` means 1.5× the
   font size, not 1.5px. Read it in `InlineStyle::from_node`, and in `break_line`
   use `font_size * factor` for the advance instead of the font's natural
   `line_height`. Watch out for mixed sizes on one line.

---

Previous: [Text rasterization](07-text-rasterization.md) · Next: [Networking](09-networking.md) · Source: [src/layout.rs](../src/layout.rs)
