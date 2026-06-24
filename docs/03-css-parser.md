# 🎨 Chapter 3 - The CSS parser

In Chapter 2 we turned HTML text into a tree of nodes. Now we do the same for
CSS. A stylesheet is, at heart, a flat **list of rules**, and each rule is two
things glued together: the **selectors** that say *what to target* and the
**declarations** that say *what to change*. Our job in `src/css.rs` is to walk a
string of CSS and hand back those structures so a later stage (the cascade) can
match them against the document.

We keep the model deliberately small. It is enough to drive a built-in
user-agent stylesheet and the common author rules on the pages we render - and
nothing more.

## The idea

Here is a single rule:

```css
a.story, h1 { color: #ff6600; font-weight: bold; }
```

It has **two selectors** (`a.story` and `h1`) separated by a comma, and a block
of **two declarations** (`color: #ff6600` and `font-weight: bold`). That shape
maps almost directly onto our types:

- `Stylesheet` - a `Vec<Rule>`.
- `Rule` - a `Vec<Selector>` plus a `Vec<Declaration>`.
- `Selector` - an enum with one variant, `Selector::Simple(SimpleSelector)`.
- `SimpleSelector` - an optional `tag_name`, an optional `id`, and a `Vec` of
  `classes`.
- `Declaration` - a `name` string and a `Value`.

A `Value` is one of three shapes the rest of the engine cares about:
`Value::Keyword(String)` (like `bold`), `Value::Length(f32, Unit)` (like
`10px`), or `Value::ColorValue(Color)`. `Unit` covers `Px`, `Em`, `Rem`, `Pt`,
`Percent`, `Vw`, and `Vh`. `Color` is plain 8-bit-per-channel RGBA.

**Specificity** is how the cascade breaks ties when two rules touch the same
property. We model it as the tuple `(usize, usize, usize)` - read it as
`(#id, #class, #tag)`. Higher wins. A `*` and a bare declaration count for
nothing. `Selector::specificity()` builds the tuple by counting: one for an id
if present, one per class, one for a tag name if present.

**Tolerant parsing.** Like the HTML parser, this one never aborts on bad input.
An `@media` block, an `@import`, a malformed rule, an exotic value - each is
skipped, and parsing continues with the next thing it understands.

**We drop combinators and pseudo-selectors on purpose.** We only model *simple*
selectors. When the parser sees something it can't represent - a descendant
combinator (`.nav a`), a child combinator (`article > p`), a pseudo
(`a:hover`), an attribute selector (`a[x]`) - it **throws the whole selector
away** rather than salvaging the simple pieces. Why? Because keeping the bare
`a` out of `.nav a` would mean styling *every* `<a>` on the page, not just the
ones inside `.nav`. Matching nothing is safe; mis-matching is a visible bug.

## Walking the code

The parser is a small recursive-descent struct holding the input and a byte
cursor:

```rust
struct Parser<'a> {
    input: &'a str,
    pos: usize,
}
```

`parse()` constructs one and asks for all the rules; `parse_declarations()` is
the same idea for a `style="..."` attribute (a bare declaration block, no
selectors).

### Rules, and skipping at-rules

`parse_rules` loops until end of input. The one special case is `@`, which
means an at-rule we don't model - so we skip it wholesale and move on:

```rust
if self.next_char() == '@' {
    self.skip_at_rule();
    continue;
}
if let Some(rule) = self.parse_rule() {
    rules.push(rule);
}
```

`parse_rule` reads the selector list, then the declaration block, and returns
`None` if no selectors survived (so a stray block contributes nothing).

### Selectors, and the "comma or brace" rule

`parse_selectors` reads a comma-separated list, stopping at `{`. The important
detail is the guard after parsing each simple selector:

```rust
let parsed = self.parse_simple_selector();
self.skip_ws_and_comments();
match (parsed, self.next_char()) {
    (Some(sel), ',' | '{' | '\0') => selectors.push(Selector::Simple(sel)),
    _ => self.skip_until_any(&['{', ',']),
}
```

A simple selector is only *kept* if the very next non-space character is a comma
or the opening brace. If anything else follows - a space (`a b`), a `>`, a `:`,
a `[` - then this was part of a complex selector we don't model, so we skip
ahead to the next `,` or `{` and discard it. This is exactly the "drop, don't
mis-apply" rule from above.

Finally the list is sorted highest-specificity first:

```rust
selectors.sort_by(|a, b| b.specificity().cmp(&a.specificity()));
```

### A single simple selector

`parse_simple_selector` loops over the selector's pieces - `#id`, `.class`, the
universal `*`, and a tag name - until it hits something that isn't a selector
character:

```rust
'#' => { self.bump(); selector.id = Some(self.parse_identifier()); matched_something = true; }
'.' => { self.bump(); selector.classes.push(self.parse_identifier()); matched_something = true; }
'*' => { self.bump(); matched_something = true; }
c if is_identifier_char(c) => {
    selector.tag_name = Some(self.parse_identifier().to_ascii_lowercase());
    matched_something = true;
}
_ => break,
```

Tag names are lower-cased so `DIV` and `div` match. If nothing matched, it
returns `None`.

### Values: hex, `rgb()`, named colors, lengths

`parse_value` branches on the first character. A `#` is a hex color; a digit,
sign, or dot is a length; anything else is a "word" that might be a functional
color, a named color, or a plain keyword:

```rust
match self.next_char() {
    '#' => self.parse_hex_color(),
    c if c.is_ascii_digit() || c == '-' || c == '.' || c == '+' => self.parse_length(),
    _ => {
        let word = self.parse_value_word();
        if let Some(color) = parse_color_function(&word, self) { return Some(Value::ColorValue(color)); }
        if let Some(color) = named_color(&word) { Some(Value::ColorValue(color)) }
        else { Some(Value::Keyword(word.to_ascii_lowercase())) }
    }
}
```

`parse_hex` handles `#rgb`, `#rrggbb`, and `#rrggbbaa`. `parse_color_function`
handles `rgb(...)` and `rgba(...)`, splitting the arguments on commas, spaces,
and `/` and tolerating `%` suffixes. `named_color` is a small lookup table -
`black`, `white`, `red`, `navy`, `orange`, `transparent`, and a couple dozen
more that our pages actually use.

`parse_length` reads the number, then the unit, mapping `px`/empty → `Px`, `em`
→ `Em`, `%` → `Percent`, and so on. An unknown unit degrades to `Px` rather than
failing.

### Lengths in pixels

`to_px()` resolves a length to device pixels. Note that `pt` is converted at
96/72, and `em`/`rem` are resolved against a fixed 16px base - this simple model
doesn't track inherited font-size - while percentages and keywords return 0
because they aren't absolute lengths:

```rust
match self {
    Value::Length(n, Unit::Px) => *n,
    Value::Length(n, Unit::Pt) => n * 96.0 / 72.0,
    Value::Length(n, Unit::Em) | Value::Length(n, Unit::Rem) => n * 16.0,
    _ => 0.0,
}
```

### `bump()` and UTF-8 safety

The cursor never does `self.pos += 1`. Instead it advances by the *full width*
of the current character:

```rust
fn bump(&mut self) {
    if self.pos < self.input.len() {
        self.pos += self.next_char().len_utf8();
    }
}
```

`input` is a `&str`, which is UTF-8. A character like `-` or `·` takes more than
one byte. If we advanced one byte at a time we could land in the middle of a
character, and the next slice (`self.input[self.pos..]`) would panic on a
non-boundary. `len_utf8()` keeps the cursor on a clean boundary no matter what
the stylesheet contains.

## Rust notes

- **Enums model "one of a few shapes."** `Value` and `Unit` are sum types - a
  value *is* exactly one of keyword/length/color, and the compiler forces every
  `match` to handle each case. That's safer than a struct full of `Option`s.
- **`#[derive(PartialEq)]`** on every type is what lets the tests write
  `assert_eq!(value, Value::Length(10.0, Unit::Px))`. Deriving equality is free
  and pays off immediately in readable tests.
- **`Option` for optional pieces.** `SimpleSelector.tag_name` and `.id` are
  `Option<String>` because a selector may or may not have them; `specificity()`
  leans on `Option::iter().count()` to count "0 or 1" without a branch.
- **Recursive descent** is the whole shape of the parser: small methods that
  each consume one grammar production (`parse_rules` → `parse_rule` →
  `parse_selectors` → `parse_simple_selector`) and share one cursor.

## Try it

```sh
cargo test css
```

The test module exercises multi-selector rules, specificity ordering, units,
all three color forms, skipped at-rules and comments, recovery from malformed
rules, and - importantly - that complex selectors are dropped rather than
misread.

## Exercises

1. **Add an `hsl()`/`hsla()` color form.** Follow `parse_color_function`: detect
   the function name, read three or four arguments, then convert HSL → RGB and
   return a `Color`. Add a test alongside `colors_in_many_forms`.

2. **Honor `!important` in the cascade.** Right now `parse_declaration` skips
   everything after the value (including `!important`) with
   `skip_until_any(&[';', '}'])`. Detect the `!important` flag, store it on
   `Declaration`, and decide how it should outrank ordinary specificity. (You'll
   touch the cascade in the next chapter too.)

3. **Add a `Unit`.** Pick a real unit we don't handle - say `ch` or `ex` - add
   it to the `Unit` enum, wire it into `parse_length`, and give it a sensible
   `to_px()` conversion. Notice how the compiler's exhaustive `match` checks
   point you at every place that needs updating.

4. **Support a comma- or space-separated shorthand.** A declaration like
   `margin: 10px 20px` currently keeps only the first length. Extend `Value`
   with a list variant (e.g. `Value::List(Vec<Value>)`) and teach `parse_value`
   to collect multiple tokens until the `;` or `}`. Add a test that round-trips
   `padding: 1px 2px 3px 4px`.

---

Previous: [The HTML parser](02-html-parser.md) · Next: [Style & the cascade](04-style-and-the-cascade.md) · Source: [src/css.rs](../src/css.rs)
