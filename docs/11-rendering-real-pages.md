# 🌍 Chapter 11 — Rendering real pages

By the end of [Chapter 10](10-interactive-window.md) Robin is a complete little
browser: it parses, styles, lays out, paints, fetches, and scrolls. Point it at
a page *you* wrote and it looks great.

Then you point it at Hacker News and everything collapses into a pile of
overlapping text. Point it at Wikipedia and it **panics**.

The real web is not the clean HTML in a tutorial. This chapter is about the
handful of fixes that take Robin from "works on my test files" to "renders Hacker
News, Wikipedia and example.com okay". Each one is a real commit in the history,
and each teaches something about how messy the web actually is.

## 1. Don't *mis*-apply selectors you can't understand

Robin only models **simple selectors** (`tag`, `#id`, `.class`). Real
stylesheets are full of `.nav a`, `article > p`, `a:hover`. The first version of
the parser saw `.nav a` and quietly read it as the *list* `.nav, a` — so a rule
meant for links inside the nav restyled **every link on the page**.

The fix is a one-line rule with a big payoff: a selector is only kept if the very
next thing after it is a comma or the opening `{`. Anything else means a
combinator or pseudo-class we don't model, so we **drop the whole selector**:

```rust
match (parsed, self.next_char()) {
    (Some(sel), ',' | '{' | '\0') => selectors.push(Selector::Simple(sel)),
    _ => self.skip_until_any(&['{', ',']),
}
```

The lesson: **matching nothing is safer than matching the wrong thing.** A
dropped rule leaves an element looking a little plain; a mis-applied rule makes
the whole page wrong.

## 2. Percentages and viewport units

example.com sets `body { width: 60vw }`. Robin understood `px` but treated `vw`
as pixels, so the body became 60 *pixels* wide and the text wrapped one word per
line. Adding the `Vw`/`Vh` units to the parser is easy; the interesting part is
that a width can no longer be resolved in isolation — it depends on the
container:

```rust
fn resolve_width(style: &StyledNode, cb_width: f32) -> LengthOrAuto {
    match style.value("width") {
        Some(Value::Length(n, Unit::Percent)) | Some(Value::Length(n, Unit::Vw)) => {
            Length(n / 100.0 * cb_width)
        }
        Some(v @ Value::Length(..)) => Length(v.to_px()),
        _ => Auto,
    }
}
```

For a top-level box the containing block *is* the viewport, so `vw` comes out
exact; nested `vw` is approximated. That's a fine trade for a teaching engine —
and noticing where an approximation lives is itself a useful skill.

## 3. A block inside an inline: the Hacker News collapse

Hacker News wraps its entire page in `<center>`. By default `<center>` is an
**inline** element — but it contains a giant `<table>`, which is **block**. An
inline box can't lay out block children, so the whole table tree never got
positioned and everything piled up at the origin.

Browsers handle "block-in-inline" by forcing the inline element into a block
context. Robin does the same with a small recursive helper:

```rust
fn effective_display(node: &StyledNode) -> Display {
    match node.display() {
        Display::Inline if node.children.iter().any(is_block_level) => Display::Block,
        d => d,
    }
}
```

One function, and Hacker News goes from a pile of overlapping characters to a
clean, readable list. The web is full of structures the spec calls "unusual"
that are, in practice, everywhere.

## 4. Surviving UTF-8

Wikipedia didn't render wrong — it *crashed*. Two scanners (the HTML raw-text
reader and the CSS tokenizer) advanced one **byte** at a time and then sliced the
string. The moment the cursor landed in the middle of a multibyte character like
`·` or `—`, the slice panicked.

Rust makes this failure loud (a clean panic, not silent corruption), which is a
feature. The fixes are about respecting character boundaries: compare raw bytes
where we only look for ASCII delimiters, and advance by a character's full width
where we consume arbitrary text:

```rust
fn bump(&mut self) {
    if self.pos < self.input.len() {
        self.pos += self.next_char().len_utf8(); // not just += 1
    }
}
```

Text encoding is one of those things that's invisible until it isn't. The whole
English-language test suite passed; one accented character on a real page found
the bug instantly.

## 5. Old HTML kept its style in attributes

Hacker News's famous orange bar isn't CSS — it's `<td bgcolor="#ff6600">`. Before
CSS, HTML carried presentation in attributes: `bgcolor`, `width`, `height`,
`align`. Browsers still honor these by mapping them to low-priority CSS so author
rules can override them. Robin does the same:

```rust
if let Some(bg) = elem.get_attribute("bgcolor") {
    set("background-color", &normalize_color(bg));   // ff6600 -> #ff6600
}
```

That one mapping restores HN's orange header and cream background, and makes
`width="85%"` table sizing work.

## The pattern

None of these fixes are big. What they share is a mindset: **the input is
adversarial, and graceful degradation beats correctness-or-crash.** A real
browser is mostly this — thousands of small accommodations for a web that was
never as tidy as the spec. Robin makes five of them; the rest of the iceberg is
[the next chapter](12-limitations-and-next-steps.md).

---

Previous: [The interactive window](10-interactive-window.md) · Next: [Limitations & next steps](12-limitations-and-next-steps.md)
