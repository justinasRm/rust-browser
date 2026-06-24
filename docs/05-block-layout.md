# 📐 Chapter 5 - Block layout

In [the last chapter](04-style-and-the-cascade.md) the cascade told us *what*
each element looks like: its color, its font, its `width`, its `margin`. But
"`width: 400px`" is only a wish until something decides where those 400 pixels
actually land on the page. That something is **layout**.

Layout takes the styled tree and answers two questions for every element:
**where** does it sit, and **how big** is it? The answer is a tree of
**boxes**, each carrying a precise rectangle in CSS pixels. This chapter builds
that tree and runs **block layout** - the rule that block boxes stack top to
bottom, each as wide as its container, growing tall enough to hold its
children. (Inline boxes and the line-breaking that flows text into wrapped
lines arrive in a later chapter; here we focus on the blocks.)

All the code lives in [`src/layout.rs`](../src/layout.rs).

## The box model

Every box is really four nested rectangles. From the inside out: the
**content** holds the box's actual stuff; **padding** is breathing room inside
the visible edge; **border** is that edge; **margin** is the gap to neighbours.

```text
  ┌─────────────── margin ───────────────┐
  │   ┌─────────── border ───────────┐   │
  │   │   ┌─────── padding ───────┐   │   │
  │   │   │       content         │   │   │
  │   │   └───────────────────────┘   │   │
  │   └───────────────────────────────┘   │
  └───────────────────────────────────────┘
```

We model this with three small `Copy` structs. A `Rect` is a position and a
size; `EdgeSizes` is a thickness on each of the four sides; and `Dimensions`
bundles the content rectangle with the three rings around it:

```rust
pub struct Rect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }

pub struct EdgeSizes { pub left: f32, pub right: f32, pub top: f32, pub bottom: f32 }

pub struct Dimensions {
    pub content: Rect,
    pub padding: EdgeSizes,
    pub border: EdgeSizes,
    pub margin: EdgeSizes,
}
```

The content rectangle is the only one stored as an absolute position; the rings
are just thicknesses. To recover the outer rectangles we grow the content
outward one ring at a time. `Rect::expanded_by` does the growing, and three
helpers chain it together:

```rust
pub fn padding_box(self) -> Rect { self.content.expanded_by(self.padding) }
pub fn border_box(self)  -> Rect { self.padding_box().expanded_by(self.border) }
pub fn margin_box(self)  -> Rect { self.border_box().expanded_by(self.margin) }
```

The `margin_box()` is the important one for flow: it's the total space a box
reserves, so the *next* sibling knows how far down to start.

## Walking the code

### Building the layout tree

`layout_tree()` is the entry point. It zeroes the viewport's height (height is
*discovered*, never assumed), builds the box tree, lays it out, and returns the
root:

```rust
pub fn layout_tree<'a>(node: &'a StyledNode<'a>, mut viewport: Dimensions, fonts: &Fonts)
    -> LayoutBox<'a>
{
    viewport.content.height = 0.0; // height is discovered from content
    let mut root = build_layout_tree(node);
    root.layout(viewport, fonts);
    root
}
```

`build_layout_tree()` translates the styled tree into a tree of `LayoutBox`es.
Each box is one of three `BoxType`s:

```rust
pub enum BoxType<'a> {
    Block(&'a StyledNode<'a>),
    Inline(&'a StyledNode<'a>),
    Anonymous, // wraps a run of inline children
}
```

The subtle part is keeping a clean rule: *a block contains only blocks, or only
inlines.* When a block has some inline children, we slip them into an
**anonymous** box so the rule holds. `inline_container()` creates that wrapper
on demand:

```rust
for child in &style_node.children {
    match effective_display(child) {
        Display::None => {}                                // skip display:none
        Display::Inline => root.inline_container().children.push(build_layout_tree(child)),
        _ => root.children.push(build_layout_tree(child)),
    }
}
```

Why `effective_display()` and not just `child.display()`? Because the web is
messy. An inline element like `<a>` or `<center>` can wrap a `<div>` or a
`<table>` - block-level content - and an inline box has no machinery to lay out
block children. So we *promote* such an inline box to behave as a block:

```rust
fn effective_display(node: &StyledNode) -> Display {
    match node.display() {
        Display::Inline if node.children.iter().any(is_block_level) => Display::Block,
        d => d,
    }
}
```

`is_block_level` recurses through nested inlines to find any block-level
descendant. This is our version of the CSS rule that block-in-inline forces a
block context - it's exactly what keeps Hacker News from collapsing into a
single line.

### Width → position → children → height

Block layout follows the order Matt Brubeck's *robinson* uses, and the reason
is causal: a block's **width** comes from its container (top-down), but its
**height** comes from its children (bottom-up). So `layout_block` does width and
position first, then descends, then measures:

```rust
fn layout_block(&mut self, containing_block: Dimensions, fonts: &Fonts) {
    self.calculate_block_width(containing_block);
    self.calculate_block_position(containing_block);
    self.layout_block_children(fonts);
    self.calculate_block_height();
}
```

**Width and auto margins.** `calculate_block_width` adds up the horizontal
pieces - both margins, both borders, both paddings, and the width - then
compares the total to the container width. The leftover is the *underflow*, and
a single `match` decides who absorbs it. `Auto` width fills the gap; two `auto`
margins split it evenly (this is how `margin: 0 auto` centers a box):

```rust
let underflow = containing_block.content.width - total;
match (width == Auto, margin_left == Auto, margin_right == Auto) {
    (false, false, false) => margin_right = Length(margin_right.to_px() + underflow),
    (false, false, true)  => margin_right = Length(underflow),
    (false, true, false)  => margin_left  = Length(underflow),
    (false, true, true)   => { margin_left  = Length(underflow / 2.0);
                               margin_right = Length(underflow / 2.0); }
    (true, _, _) => { /* auto margins become 0 */
        width = if underflow >= 0.0 { Length(underflow) } else { Length(0.0) };
    }
}
```

**Position and stacking.** `calculate_block_position` reads the vertical
margins/borders/paddings, then places the box. The horizontal `x` is the
container's left edge plus the left rings. The vertical `y` is the clever bit:
it adds the container's *running content height* - how much its children have
already consumed - so each block lands just below the previous one:

```rust
d.content.x = containing_block.content.x + d.margin.left + d.border.left + d.padding.left;
d.content.y = containing_block.content.height   // ← running height of siblings so far
            + containing_block.content.y
            + d.margin.top + d.border.top + d.padding.top;
```

That running height is fed in by `layout_block_children`, which lays out each
child against a snapshot of `self` with the accumulated height, then grows the
accumulator by the child's `margin_box().height`:

```rust
for child in &mut self.children {
    child.layout(self_dims_with_height(self_dims, content_height), fonts);
    content_height += child.dimensions.margin_box().height;
}
self.dimensions.content.height = content_height;
```

**Height.** Finally `calculate_block_height` lets an explicit `height` override
the discovered one; otherwise the height the children produced stands.

**Percentages and `vw`.** Widths aren't always pixels. `resolve_width` turns
`%` and `vw` into a fraction of the containing block's width, takes absolute
units at face value, and treats everything else (including `auto` and a missing
value) as `Auto`:

```rust
match style.value("width") {
    Some(Value::Length(n, Unit::Percent)) | Some(Value::Length(n, Unit::Vw)) =>
        Length(n / 100.0 * cb_width),
    Some(v @ Value::Length(..)) => Length(v.to_px()),
    _ => Auto,
}
```

For a top-level box the containing block *is* the viewport, so `50%` and `50vw`
resolve identically and exactly.

## Rust notes

- **A tiny `LengthOrAuto` enum** (`Length(f32)` / `Auto`) makes the width
  algorithm readable: the `match (width == Auto, ...)` reads almost like the CSS
  spec, and `to_px()` collapses `Auto` to `0.0` when we just need a number.
- **`&mut self` recursion over the tree.** `layout()` borrows each box mutably
  and walks its children, mutating dimensions in place. Because Rust forbids two
  mutable borrows at once, `layout_block_children` snapshots `self.dimensions`
  into a local *before* the loop rather than borrowing `self` inside it.
- **`Copy` types pay off.** `Rect`, `EdgeSizes`, and `Dimensions` all derive
  `Copy`, so passing a `containing_block` by value or snapshotting dimensions is
  a cheap bitwise copy - no clones, no borrow gymnastics.
- **Lifetimes on `LayoutBox<'a>`.** A box holds `&'a StyledNode<'a>` rather than
  owning its style, so the styled tree must outlive the layout tree. That's why
  the tests build both in the same scope and never return a `LayoutBox` past the
  styled data it points into.

## Try it

Run the layout tests:

```sh
cargo test layout
```

Or dump the box geometry of a real file. `--dump-layout` prints the indented
tree from `box_tree_to_string`, one line per box with its content rectangle:

```sh
cargo run -- page.html --dump-layout
```

With a tiny `page.html` of two stacked, centered divs:

```html
<div style="width: 400px; height: 30px; margin: 0 auto"></div>
<div style="height: 30px"></div>
```

you'd see the first box centered (offset 200 in an 800-wide viewport) and the
second stacked directly below it:

```text
block <html> @ (0,0) 800x60
  block <div> @ (200,0) 400x30
  block <div> @ (0,30) 800x30
```

The first div's explicit 400px width leaves 400px of underflow, split evenly by
the two `auto` margins; the second div has `auto` width, so it fills the whole
800. Both heights are 30px, so the container discovers a total height of 60.

## Exercises

1. **`min-width` / `max-width`.** After `resolve_width` produces a width, clamp
   it so a `min-width` raises it and a `max-width` lowers it. Re-run the
   underflow `match` afterward - clamping changes the leftover space the auto
   margins divide.

2. **`box-sizing: border-box`.** Today `width` sets the *content* width. When a
   box opts into `border-box`, the declared width should *include* padding and
   border, so the content shrinks by their sum. Subtract them inside
   `calculate_block_width` when the property is set.

3. **Percentage margins and padding.** `lookup_len` only understands pixels and
   `auto`. Teach it (or a sibling helper) to resolve `%` against the containing
   block's content width, the way `resolve_width` already does for `width`.

4. **A `max-width` clamp on the root.** Real sites cap their content with
   something like `max-width: 960px; margin: 0 auto`. Confirm that exercise 1
   plus the existing auto-margin centering reproduce this for a top-level box -
   write a test asserting the box is 960 wide and horizontally centered in a
   1400px viewport.

---

Previous: [Style & the cascade](04-style-and-the-cascade.md) · Next: [Painting](06-painting.md) · Source: [src/layout.rs](../src/layout.rs)
