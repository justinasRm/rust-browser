# 🌳 Chapter 1 - The DOM

The **DOM** (Document Object Model) is the tree a browser builds out of HTML. It is the foundation everything else walks: CSS matching, layout, and painting all read this tree, so before we can parse HTML or draw a pixel, we need a good way to hold a document in memory.

## The idea

A document is a tree of **nodes**, and in Robin every node is one of exactly three things:

- an **element** - a tag like `<p>`, `<a>`, or `<div>`, which carries a tag name and a bag of attributes;
- a run of **text** - the actual words, like `Hello`;
- a **comment** - `<!-- like this -->`, kept so the tree can round-trip but ignored by style and layout.

Any node can have **children**, and that recursion is what makes it a tree. A real browser's DOM has dozens of node types and an enormous API; ours has exactly what layout and painting need to read.

Take this snippet of HTML:

```html
<p>Hello <b>world</b></p>
```

It becomes this tree:

```text
<p>                  ← element
├── #text "Hello "   ← text
└── <b>              ← element
    └── #text "world"← text
```

The `<p>` element owns two children: the text `Hello ` and the `<b>` element. The `<b>` element in turn owns one child, the text `world`. Walk the tree top to bottom and you can reconstruct the page.

## Walking the code

The whole model lives in [`src/dom.rs`](../src/dom.rs). At the center is `Node`:

```rust
pub struct Node {
    pub children: Vec<Node>,
    pub node_type: NodeType,
}
```

A node is just its children plus a `node_type` tag telling us *what* it is. Children are stored **inline** (`Vec<Node>`) rather than behind pointers, so a parent simply owns its children - clean, Rust-friendly ownership.

What a node *is* lives in the `NodeType` enum:

```rust
pub enum NodeType {
    Text(String),
    Element(ElementData),
    Comment(String),
}
```

Each variant carries exactly the data that kind of node needs: text and comments carry a `String`, and elements carry an `ElementData`:

```rust
pub struct ElementData {
    pub tag_name: String,   // lower-cased, e.g. "div"
    pub attributes: AttrMap,
}
```

`AttrMap` is just a `type` alias for `HashMap<String, String>` - an element's attributes, e.g. `{"href": "/about", "class": "nav link"}`.

Rather than building these structs by hand everywhere, the module gives us three small **constructor functions** so the HTML parser and tests can build nodes readably:

```rust
pub fn text(data: impl Into<String>) -> Node { /* ... */ }
pub fn comment(data: impl Into<String>) -> Node { /* ... */ }
pub fn elem(tag_name: impl Into<String>, attributes: AttrMap, children: Vec<Node>) -> Node { /* ... */ }
```

So our `<p>Hello <b>world</b></p>` tree is built like this:

```rust
elem("p", AttrMap::new(), vec![
    text("Hello "),
    elem("b", AttrMap::new(), vec![text("world")]),
])
```

On the element side, `ElementData` exposes the lookups CSS will need. `id()` and `classes()` are thin wrappers over `get_attribute`:

```rust
pub fn id(&self) -> Option<&str> {
    self.get_attribute("id")
}

pub fn classes(&self) -> HashSet<&str> {
    match self.get_attribute("class") {
        Some(classlist) => classlist.split_whitespace().collect(),
        None => HashSet::new(),
    }
}
```

`classes()` splits the `class` attribute on whitespace into a `HashSet`, which is exactly the shape a CSS class selector wants to test against.

To pull the text out of a subtree, `inner_text()` walks every descendant and concatenates the text it finds - handy for tests and for reading a page's `<title>`:

```rust
pub fn inner_text(&self) -> String {
    let mut out = String::new();
    self.collect_text(&mut out);
    out
}
```

The recursion lives in the private helper `collect_text`, which pushes any `Text` it sees, then recurses into each child. For our example tree, `inner_text()` returns `"Hello world"`.

Finally, so you can actually *see* what the parser produced, `pretty_print()` renders the tree as indented text:

```rust
pub fn pretty_print(node: &Node) -> String {
    let mut out = String::new();
    print_node(node, 0, &mut out);
    out
}
```

`print_node` prints `#text "..."` for text, `<!-- ... -->` for comments, and `<tag attr="...">` for elements - sorting attributes so the output is stable for tests - then recurses one level deeper for each child. The CLI exposes this via `--dump-dom`.

## Rust notes

A few techniques here are worth pausing on:

- **Enums + `match` model "one of N things."** `NodeType` is a *sum type*: a node is text **or** an element **or** a comment, never two at once, and each variant carries its own payload. Functions like `element()` and `collect_text` use `match` (or `if let`) to handle each case, and the compiler makes sure you don't forget one.
- **Owning children via `Vec<Node>`.** Storing children inline gives every node a single, obvious owner - its parent. No reference counting, no lifetimes to thread through the tree. (The trade-off, noted in the source, is no parent pointers; a JS-driving browser would need them, a render-only engine doesn't.)
- **`impl Into<String>` for ergonomic constructors.** `text("hi")` accepts a `&str` *and* a `String` because the parameter is `impl Into<String>`. The constructor calls `.into()` once and stores an owned `String`, so callers don't have to sprinkle `.to_string()` everywhere.
- **`HashSet<&str>` for classes.** Membership tests (`classes().contains("active")`) are O(1), and borrowing `&str` slices out of the existing attribute string means `classes()` allocates the set but not the strings inside it.

## Try it

Run the DOM unit tests:

```sh
cargo test dom
```

Build a tiny HTML file and dump its tree with the CLI:

```sh
printf '<p>Hi <b>there</b></p>' > /tmp/x.html && cargo run -- /tmp/x.html --dump-dom
```

You should see an indented tree with a `<p>` element, a `#text "Hi"` node, and a nested `<b>` element holding `#text "there"`.

## Exercises

1. **Easy - count the nodes.** Add a method `fn node_count(&self) -> usize` on `Node` that returns the total number of nodes in the subtree (this node plus all descendants). Follow the recursive shape of `collect_text`. Write a test asserting the `<p>Hello <b>world</b></p>` tree has 4 nodes.

2. **Medium - find by tag.** Add `fn find_by_tag<'a>(&'a self, tag: &str, out: &mut Vec<&'a Node>)` that pushes every element whose `tag_name()` matches `tag` into `out`. Reuse the existing `tag_name()` helper, and test it against a tree with two `<b>` elements.

3. **Hard - a `descendants()` iterator.** Write an iterator that yields every descendant node in document order (depth-first, pre-order), so callers can write `for node in root.descendants() { ... }` instead of recursing by hand. Implement a struct holding a stack of `&Node`, push children in reverse so they pop left-to-right, and `impl Iterator for` it. Rewrite `inner_text()` on top of it and confirm the existing tests still pass.

---

Previous: [How to use this repo](00-how-to-use-this-repo.md) · Next: [The HTML parser](02-html-parser.md) · Source: [src/dom.rs](../src/dom.rs)
