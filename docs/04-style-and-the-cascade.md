# 🪄 Chapter 4 — Style & the cascade

By now we can turn HTML into a [DOM](01-the-dom.md) tree and turn CSS text into a list of rules ([Chapter 3](03-css-parser.md)). This chapter joins them. We walk the DOM, and for every node we work out the **final value** of each CSS property — the actual `color`, `display`, `margin`, and so on that layout and paint will read. The output is a **styled tree**: the same shape as the DOM, but every node now carries a flat map of resolved values.

The whole thing lives in [`src/style.rs`](../src/style.rs). The key type is `StyledNode<'a>`, and the entry point is `style_tree()`.

## The idea

Two different rules can both want to set an element's `color`. Which one wins? That decision is called **the cascade**, and it runs in a fixed order for every element:

1. **Inherit** — start with the properties handed down from the parent (`color`, `font-size`, …). A child with no rules of its own still gets its parent's text color.
2. **Presentational hints** — map a few legacy HTML attributes (`bgcolor`, `width`, `height`) onto CSS. These sit at the very bottom of the pile so author CSS can always beat them.
3. **Matched rules** — collect every rule whose selector matches the element, from the user-agent sheet *and* the page's author sheet, then sort by **specificity** (`#id` beats `.class` beats `tag`). Ties keep **source order**, with the UA sheet applied before author rules so author rules win.
4. **Inline `style="..."`** — applied last, so it beats every selector.

Each later layer simply overwrites earlier ones in a `HashMap`, so "later wins" falls out for free.

### The user-agent stylesheet

Why does plain, unstyled HTML still look like a document — paragraphs stacked, headings big and bold, links blue? Because every browser ships a built-in **user-agent stylesheet**. Robin's lives in `UA_CSS` and is loaded by `user_agent_stylesheet()`. It is the thing that says `div { display: block }` and `span` stays inline, that `body` has an `8px` margin, that `h1` is `32px` bold, and that `a { color: #0000ee }`. Keeping it as real CSS text (not hard-coded Rust) means the same parser is exercised — and you can read it like any stylesheet.

### Presentational attributes

Old HTML carried style in attributes: `<table bgcolor="ff6600">`, `<td width="85%">`. `apply_presentational_hints()` maps these onto CSS (`bgcolor` → `background-color`, etc.). It applies them *before* matched rules, so they act as the weakest possible source — exactly how real browsers treat them. (Hacker News's orange bar is a `bgcolor` attribute.)

## Walking the code

A `StyledNode` borrows its DOM node and adds the resolved values:

```rust
pub type PropertyMap = HashMap<String, Value>;

pub struct StyledNode<'a> {
    pub node: &'a Node,
    pub specified_values: PropertyMap,
    pub children: Vec<StyledNode<'a>>,
}
```

Two small helpers read that map. `value()` looks a property up, and `display()` turns the `display` keyword into a typed enum (defaulting to `Inline`, the CSS default):

```rust
pub fn value(&self, name: &str) -> Option<Value> {
    self.specified_values.get(name).cloned()
}

pub fn display(&self) -> Display {
    match self.value("display").as_ref().and_then(Value::keyword) {
        Some("block") => Display::Block,
        Some("list-item") => Display::ListItem,
        Some("none") => Display::None,
        Some("inline") => Display::Inline,
        // flex, grid, table, … fall back to block so content still stacks.
        Some(_) => Display::Block,
        None => Display::Inline,
    }
}
```

`Display` has exactly four variants — `Inline`, `Block`, `ListItem`, `None` — which is all layout needs to model.

Building the tree is a recursion. `style_tree()` parses the UA sheet once, then `style_node()` styles a node and hands its **inheritable subset** down to its children:

```rust
pub fn style_tree<'a>(root: &'a Node, author: &Stylesheet) -> StyledNode<'a> {
    let ua = user_agent_stylesheet();
    style_node(root, &ua, author, &PropertyMap::new())
}
```

The heart of the cascade is `specified_values()`. Read it top to bottom and you see the four layers in order:

```rust
fn specified_values(elem, ua, author, inherited) -> PropertyMap {
    let mut values = inherited.clone();           // 1. inherited

    apply_presentational_hints(elem, &mut values); // 2. bgcolor/width/height

    // 3. matched rules: UA before author, sorted by specificity.
    let mut matches: Vec<(Specificity, &Rule)> = Vec::new();
    for rule in ua.rules.iter().chain(author.rules.iter()) {
        if let Some(spec) = match_rule(elem, rule) {
            matches.push((spec, rule));
        }
    }
    matches.sort_by(|a, b| a.0.cmp(&b.0)); // ascending — low specificity first
    for (_, rule) in matches {
        for decl in &rule.declarations {
            values.insert(decl.name.clone(), decl.value.clone());
        }
    }

    // 4. inline style="" beats any selector.
    if let Some(style_attr) = elem.get_attribute("style") {
        for decl in css::parse_declarations(style_attr) {
            values.insert(decl.name, decl.value);
        }
    }
    values
}
```

A rule matches if *any* of its selectors matches; `match_rule()` returns the best specificity among them:

```rust
fn match_rule(elem: &ElementData, rule: &Rule) -> Option<Specificity> {
    rule.selectors
        .iter()
        .filter(|sel| matches_selector(elem, sel))
        .map(Selector::specificity)
        .max()
}
```

The actual matching is `matches_simple()`: a selector matches when its tag name (if any) equals the element's, its `#id` (if any) equals the element's, and *every* class it names is on the element.

```rust
fn matches_simple(elem: &ElementData, selector: &SimpleSelector) -> bool {
    if let Some(tag) = &selector.tag_name {
        if *tag != elem.tag_name { return false; }
    }
    if selector.id.is_some() && selector.id.as_deref() != elem.id() {
        return false;
    }
    let elem_classes = elem.classes();
    if selector.classes.iter().any(|c| !elem_classes.contains(c.as_str())) {
        return false;
    }
    true
}
```

Inheritance is just a fixed list. `INHERITED_PROPERTIES` names the properties that flow from parent to child, and `inheritable_subset()` plucks exactly those out of a node's resolved values to pass down:

```rust
const INHERITED_PROPERTIES: &[&str] = &[
    "color", "font-size", "font-weight", "font-style", "font-family",
    "line-height", "text-align", "white-space", "list-style-type", "visibility",
];

fn inheritable_subset(values: &PropertyMap) -> PropertyMap {
    INHERITED_PROPERTIES
        .iter()
        .filter_map(|&prop| values.get(prop).map(|v| (prop.to_string(), v.clone())))
        .collect()
}
```

Notice what is *not* there: `margin`, `background-color`, `width`. Those are deliberately non-inherited, so a box's background never leaks onto its children.

Finally, a peek at `UA_CSS` — the defaults, written as ordinary CSS:

```css
html, body, div, section, /* … */ p, ul, ol, li { display: block; }
head, script, style, meta, link, title { display: none; }
li { display: list-item; }
body { margin: 8px; color: #000000; font-size: 16px; line-height: 1.3; }
h1 { font-size: 32px; font-weight: bold; margin: 16px; }
a  { color: #0000ee; }
```

`document_stylesheet()` (via `inline_css()`) gathers a page's author CSS by concatenating the text of every `<style>` block and parsing it — that's the sheet `style_tree()` cascades on top of the UA defaults.

## Rust notes

- **Lifetimes.** `StyledNode<'a>` holds `node: &'a Node` — it *borrows* the DOM rather than copying it. The `'a` says "this styled node can't outlive the DOM it points at," which the compiler enforces for free. No duplicated tree, no dangling pointers.
- **`HashMap` as `PropertyMap`.** Resolved values live in a `HashMap<String, Value>`. Cascading is then just repeated `insert()`s where "later wins" is the map's natural overwrite behavior.
- **Closures.** `apply_presentational_hints()` defines a local `let mut set = |prop, raw| { … };` closure to avoid repeating the parse-and-insert dance for `bgcolor`, `width`, and `height`. It captures `values` mutably, so calling it edits the map in place.
- **Stable sort for source order.** `matches.sort_by(...)` is a *stable* sort, so rules with equal specificity stay in the order they were collected — UA before author. That's why a UA `a { color: #0000ee }` and an author `a { color: red }` resolve to red without any extra tie-break code.

## Try it

Run the style tests:

```sh
cargo test style
```

You'll see the cascade in action: `author_rules_override_user_agent`, `specificity_decides_between_author_rules`, `inline_style_wins`, `color_is_inherited`, and `ua_stylesheet_sets_display`.

To *see* the user-agent sheet's effect, render a plain HTML file with no `<style>` at all. Paragraphs still stack as blocks, the `<head>` and `<script>` vanish, headings come out big and bold, and links are blue — all of that comes from `UA_CSS`, not the page.

## Exercises

1. **Add an inherited property.** Add `"text-decoration"` to `INHERITED_PROPERTIES`, set it on a parent, and confirm a child picks it up. Then try `"background-color"` and convince yourself why it is *not* in the list (a parent's background should not paint over its children).

2. **A new presentational hint.** The old `<body text="...">` attribute set the document text color. Extend `apply_presentational_hints()` to map `text` → `color`. Remember to run the value through something like `normalize_color()` so bare hex (`text=ff0000`) still works.

3. **Support a `font` shorthand.** Right now `font-size` and `font-weight` are separate. In `specified_values()` (or a small helper), expand a `font: bold 14px ...` declaration into the individual longhand properties so the shorthand actually takes effect.

4. **Rank hints below the UA sheet, properly.** Presentational hints currently load before matched rules, so even a UA rule beats them — but trace the cascade and prove it. Write a test where a `<td bgcolor=red>` is overridden by a UA `td { background-color: ... }` rule you add, and watch `author_css_overrides_presentational_attribute` still pass.

---

Previous: [The CSS parser](03-css-parser.md) · Next: [Block layout](05-block-layout.md) · Source: [src/style.rs](../src/style.rs)
