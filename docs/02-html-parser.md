# 📄 Chapter 2 - The HTML parser

In [Chapter 1](01-the-dom.md) we built the DOM - the tree every later stage walks.
But trees don't grow on their own. We feed the browser a string of HTML text, and
something has to turn that messy string into `dom::Node`s.

That something is the parser in [`src/html.rs`](../src/html.rs), and its single most
important property is this: **it never errors.** Real-world HTML is a disaster - tags
left unclosed, elements mis-nested, attributes with no quotes, `<script>` bodies full
of things that _look_ like tags but aren't. A parser that demanded well-formed XML would
choke on every page on the web. So, like every real browser, ours does its best and
keeps going. The public entry point is one function:

```rust
pub fn parse(source: &str) -> Node {
    let mut builder = TreeBuilder::new();
    Tokenizer::new(source, &mut builder).run();
    builder.finish()
}
```

Text in, a single root `Node` out. Everything else is private machinery.

## The idea

The design is the classic browser split: a **tokenizer** feeding a **tree builder**.

- The **`Tokenizer`** walks the input once, recognizing start tags, end tags, text,
  comments, and doctypes. It doesn't know anything about nesting.
- The **`TreeBuilder`** turns that stream of tokens into a tree. Its secret weapon is
  a **stack of open elements** - literally a `Vec<Node>`. Opening a tag pushes a node;
  closing one pops back to the matching element, auto-closing anything mis-nested in
  between.

Why all the tolerance? Because authors lean on it constantly:

- `<ul><li>one<li>two</ul>` - the `<li>`s are never closed; the parser must imply it.
- `<p><b><i>x</b></i></p>` - `</b>` arrives before `</i>`. Mis-nested, but it must render.
- `<script>if (a < b) {}</script>` - that `<` is _not_ a tag.
- `<a href=/about class=nav>` - unquoted attribute values.

A real browser handles every one of these without complaint, and so does Robin. This is
a _pragmatic subset_ of the HTML5 algorithm: enough to render Hacker News and Wikipedia,
small enough to read in one sitting.

## Walking the code

### The main loop

`Tokenizer::run()` is a flat dispatch - look at what's under the cursor and pick a handler:

```rust
fn run(&mut self) {
    while self.pos < self.input.len() {
        if self.starts_with("<!--") {
            self.parse_comment();
        } else if self.starts_with("<!") || self.starts_with("<?") {
            // Doctype or processing instruction - skip to the next '>'.
            self.skip_until_byte(b'>');
        } else if self.starts_with("</") {
            self.parse_end_tag();
        } else if self.peek_byte() == Some(b'<') && self.is_tag_start() {
            self.parse_start_tag();
        } else {
            self.parse_text();
        }
    }
}
```

Note `is_tag_start()`: a `<` only begins a tag if it's followed by an ASCII letter.
A lone `<` (as in `a < b`) falls through to `parse_text()` and is treated as literal text.

### Open and close, on the stack

The tree builder is where nesting lives. Opening a tag first applies any implied end
tags (more below), then either _appends_ a void element or _pushes_ a normal one:

```rust
fn open_tag(&mut self, tag: &str, attrs: AttrMap) {
    self.apply_implied_end_tags(tag);
    let node = dom::elem(tag, attrs, Vec::new());
    if is_void(tag) {
        self.append(node);
    } else {
        self.stack.push(node);
    }
}
```

Closing a tag is where tolerance shines. We find the _nearest_ matching open element and
pop everything above it - auto-closing whatever was mis-nested:

```rust
fn close_tag(&mut self, tag: &str) {
    if let Some(idx) = self.stack.iter().rposition(|n| n.tag_name() == Some(tag)) {
        if idx == 0 {
            return; // never pop the synthetic root
        }
        while self.stack.len() > idx {
            let node = self.stack.pop().unwrap();
            self.append(node);
        }
    }
    // An unmatched end tag (e.g. a stray `</div>`) is simply ignored.
}
```

That `rposition` is what fixes `<b><i>x</b>`: when `</b>` arrives, we pop `<i>` and `<b>`,
attaching each to its parent on the way out. A stray `</div>` with no matching open tag?
The `if let` simply doesn't fire, and we move on.

### Implied end tags

HTML lets you omit closing tags all over the place. When a new tag opens,
`apply_implied_end_tags` closes any open elements the spec says it implicitly ends:

```rust
fn apply_implied_end_tags(&mut self, opening: &str) {
    loop {
        let Some(current) = self.stack.last().and_then(Node::tag_name) else { return };
        let should_close = match opening {
            "li" => current == "li",
            "dt" | "dd" => current == "dt" || current == "dd",
            "tr" => matches!(current, "tr" | "td" | "th"),
            // ...block-level elements close an open paragraph...
            "p" | "div" | "ul" | "ol" | "table" /* … */ => current == "p",
            _ => false,
        };
        if should_close && self.stack.len() > 1 {
            let node = self.stack.pop().unwrap();
            self.append(node);
        } else {
            return;
        }
    }
}
```

This is the rule that makes `<li>one<li>two` produce two _siblings_ instead of nesting
the second `<li>` inside the first. The same machinery closes an open `<p>` when a block
element starts, and handles table rows and cells.

### Void and self-closing elements

Some elements never have children: `<br>`, `<img>`, `<input>`, and friends. They live in
`VOID_ELEMENTS`, and `is_void()` checks membership. In `parse_start_tag`, both a void tag
and an explicit `/>` are appended rather than pushed - so they can never swallow the
siblings that follow them:

```rust
if self_closing || is_void(&tag) {
    self.builder.open_tag(&tag, attrs);
    // (void/self-closing: open_tag appends without pushing)
} else if RAW_TEXT_ELEMENTS.contains(&tag.as_str()) {
    self.builder.open_tag(&tag, attrs);
    self.consume_raw_text(&tag);
    self.builder.close_tag(&tag);
} else {
    self.builder.open_tag(&tag, attrs);
}
```

### Raw-text elements

`<script>`, `<style>`, `<textarea>`, `<title>`, and `<noscript>` (the `RAW_TEXT_ELEMENTS`)
hold _text_, not markup. Inside them, `<` is just a character. `consume_raw_text` reads
the body verbatim until it finds the matching close tag:

```rust
fn consume_raw_text(&mut self, tag: &str) {
    let close = format!("</{tag}");
    let close_bytes = close.as_bytes();
    let start = self.pos;
    while self.pos < self.input.len() && !self.starts_with_ci(close_bytes) {
        self.pos += 1;
    }
    let raw = self.chars[start..self.pos].to_string();
    // RCDATA elements (title, textarea) decode entities; script/style don't.
    let decoded = if matches!(tag, "title" | "textarea") {
        decode_entities(&raw)
    } else {
        raw
    };
    self.builder.text(decoded);
    // ...consume the "</tag" we stopped on plus its '>'...
}
```

`starts_with_ci` is case-insensitive so `</SCRIPT>` closes a `<script>`. And note the
distinction: `<title>`/`<textarea>` are _RCDATA_ - their entities are decoded - while
`<script>`/`<style>` are left completely raw.

### Decoding entities

Text and attribute values pass through `decode_entities`, which turns `&amp;` into `&`,
`&#163;` into `£`, and so on. It fast-paths the common case (no `&` at all), then scans
for references:

```rust
fn decode_one_entity(entity: &str) -> Option<String> {
    // Numeric: &#123; or &#xAB;
    if let Some(num) = entity.strip_prefix('#') {
        let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            num.parse::<u32>().ok()?
        };
        return char::from_u32(code).map(String::from);
    }
    let s = match entity {
        "amp" => "&",
        "lt" => "<",
        "mdash" => "-",
        "nbsp" => "\u{00a0}",
        // ...a small named table of what actually shows up in page text...
        _ => return None,
    };
    Some(s.to_string())
}
```

All numeric references are supported; named entities are a curated handful (the ~30 that
actually appear in body text). Anything unrecognized is left as a literal `&` - tolerant,
as always.

## Rust notes

- **A `Vec` is the stack.** Instead of recursive descent, the tree builder uses an explicit
  `Vec<Node>` and pushes/pops it. This sidesteps deep recursion on pathological input and
  makes "pop back to the matching element" a plain `while self.stack.len() > idx` loop.
- **Bytes vs. chars, carefully.** The tokenizer holds both `input: &[u8]` and `chars: &str`.
  Scanning advances `pos` one _byte_ at a time (fast, simple), but the helpers know the
  difference: `starts_with_bytes` and `starts_with_ci` compare raw bytes and are safe even
  mid-character, while slicing `self.chars[..]` only happens at known ASCII boundaries -
  so a multibyte `-` or `café` inside a comment or `<script>` never panics.
- **`matches!` for tidy membership.** Tests like `matches!(current, "td" | "th" | "tr")`
  read cleanly as "is `current` one of these?" without spelling out `==` chains.
- **`let … else` for early exit.** `let Some(current) = … else { return };` grabs the
  current element or bails - a clean way to handle "nothing left on the stack."

## Try it

Run the parser's tests:

```sh
cargo test html
```

You'll see the cases from this chapter exercised by name: `implied_end_tags_for_list_items`,
`recovers_from_misnesting`, `script_body_is_not_parsed_as_html`, `decodes_entities`, and more.

Then watch implied end tags in action with the CLI's DOM dumper:

```sh
printf '<ul><li>one<li>two</ul>' > /tmp/x.html && cargo run -- /tmp/x.html --dump-dom
```

The two `<li>` elements come out as _siblings_ inside `<ul>`, not nested - even though the
source never closed the first one.

## Exercises

1. **Add named entities (easy).** Extend the `match` in `decode_one_entity` with a few more
   references you've seen on the web - `&hearts;` (♥), `&dagger;` (†), `&spades;` (♠). Add a
   case to the `decodes_entities` test to confirm.

2. **Teach it a new void element (easy).** SVG's `<circle>` and `<path>` are effectively void
   in HTML contexts. Add an element to `VOID_ELEMENTS` and write a test proving it doesn't
   swallow its following siblings as children (mirror `handles_void_and_self_closing`).

3. **Handle CDATA sections (medium).** `<![CDATA[ … ]]>` currently gets swallowed by the
   `<!` branch in `run()`, which skips to the next `>` - wrong, since the content may contain
   `>`. Add a branch that detects `<![CDATA[`, reads verbatim to `]]>`, and emits the inside
   as a text node.

4. **Spot the `<title>` attribute gap (medium).** `consume_raw_text` is entered _after_
   `read_attributes`, so `<title lang="en">Hi</title>` already keeps its attributes - verify
   this with a test. Then make the harder case work: ensure a `</title >` with trailing
   whitespace before `>` still closes correctly (look at how `consume_raw_text` finishes by
   calling `skip_until_byte(b'>')`).

---

Previous: [The DOM](01-the-dom.md) · Next: [The CSS parser](03-css-parser.md) · Source: [src/html.rs](../src/html.rs)
