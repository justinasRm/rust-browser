//! A tolerant HTML parser: text in, [`dom::Node`] tree out.
//!
//! Real-world HTML is *messy*. Tags are left unclosed (`<li>` after `<li>`),
//! elements are mis-nested, attributes go unquoted, and `<script>` bodies
//! contain things that look like tags but aren't. A parser that only accepts
//! perfectly-formed XML would choke on every page on the web. So, like real
//! browsers, ours never errors out - it does its best and keeps going.
//!
//! The design is a **tokenizer** feeding a **tree builder**:
//!
//! * The tokenizer walks the input once, recognizing start tags, end tags,
//!   text, comments and doctypes.
//! * The tree builder keeps a *stack of open elements*. Opening a tag pushes;
//!   closing one pops back to the matching element (auto-closing anything
//!   mis-nested in between). A handful of "implied end tag" rules let optional
//!   closing tags work the way they do on real pages.
//!
//! This is a pragmatic subset of the HTML5 parsing algorithm - enough to render
//! Hacker News and Wikipedia, small enough to read in one sitting.

use crate::dom::{self, AttrMap, Node};

/// Parse a full HTML document into a single root [`Node`].
///
/// The returned node is the `<html>` element (real or synthesized) so the rest
/// of the pipeline always has one tree to walk.
pub fn parse(source: &str) -> Node {
    let mut builder = TreeBuilder::new();
    Tokenizer::new(source, &mut builder).run();
    builder.finish()
}

// Elements that never have children or a closing tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr", "path", "circle",
];

// Elements whose content is raw text, not markup. We read their body verbatim
// up to the matching close tag instead of parsing tags inside them.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "textarea", "title", "noscript"];

fn is_void(tag: &str) -> bool {
    VOID_ELEMENTS.contains(&tag)
}

// --- Tree builder -----------------------------------------------------------

/// Builds the DOM from a stream of tokens, maintaining the stack of open
/// elements so that nesting and auto-closing behave sensibly.
struct TreeBuilder {
    /// Open elements, outermost first. `stack[0]` is a synthetic document root.
    stack: Vec<Node>,
}

impl TreeBuilder {
    fn new() -> Self {
        // The synthetic root collects everything; finish() unwraps it.
        TreeBuilder {
            stack: vec![dom::elem("#document", AttrMap::new(), Vec::new())],
        }
    }

    fn open_tag(&mut self, tag: &str, attrs: AttrMap) {
        self.apply_implied_end_tags(tag);
        let node = dom::elem(tag, attrs, Vec::new());
        if is_void(tag) {
            self.append(node);
        } else {
            self.stack.push(node);
        }
    }

    fn close_tag(&mut self, tag: &str) {
        // Find the nearest matching open element. Everything above it was
        // mis-nested and gets auto-closed (popped) along the way.
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

    fn text(&mut self, data: String) {
        if data.is_empty() {
            return;
        }
        self.append(dom::text(data));
    }

    fn comment(&mut self, data: String) {
        self.append(dom::comment(data));
    }

    /// Attach a finished node to the current open element.
    fn append(&mut self, node: Node) {
        self.stack
            .last_mut()
            .expect("root is always present")
            .children
            .push(node);
    }

    /// HTML lets you omit many closing tags. When a new tag opens, close any
    /// open elements that the spec says it implicitly ends. This is what makes
    /// `<li>a<li>b` and table markup without `</td>` parse correctly.
    fn apply_implied_end_tags(&mut self, opening: &str) {
        loop {
            let Some(current) = self.stack.last().and_then(Node::tag_name) else {
                return;
            };
            let should_close = match opening {
                "li" => current == "li",
                "dt" | "dd" => current == "dt" || current == "dd",
                "option" => current == "option",
                "tr" => matches!(current, "tr" | "td" | "th"),
                "td" | "th" => matches!(current, "td" | "th"),
                "thead" | "tbody" | "tfoot" => {
                    matches!(current, "td" | "th" | "tr" | "thead" | "tbody" | "tfoot")
                }
                // Block-level elements close an open paragraph.
                "p" | "div" | "ul" | "ol" | "table" | "blockquote" | "pre" | "section"
                | "article" | "header" | "footer" | "nav" | "aside" | "form" | "hr" | "h1"
                | "h2" | "h3" | "h4" | "h5" | "h6" => current == "p",
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

    /// Pop everything still open and return the document's root element.
    fn finish(mut self) -> Node {
        while self.stack.len() > 1 {
            let node = self.stack.pop().unwrap();
            self.append(node);
        }
        let mut root = self.stack.pop().unwrap();
        // If the document already has a single <html> root, use it directly;
        // otherwise wrap the top-level nodes in a synthetic <html>.
        let html_children: Vec<usize> = root
            .children
            .iter()
            .enumerate()
            .filter(|(_, n)| n.tag_name() == Some("html"))
            .map(|(i, _)| i)
            .collect();
        if html_children.len() == 1 {
            return root.children.remove(html_children[0]);
        }
        dom::elem("html", AttrMap::new(), root.children)
    }
}

// --- Tokenizer --------------------------------------------------------------

struct Tokenizer<'a, 'b> {
    input: &'a [u8],
    chars: &'a str,
    pos: usize,
    builder: &'b mut TreeBuilder,
}

impl<'a, 'b> Tokenizer<'a, 'b> {
    fn new(source: &'a str, builder: &'b mut TreeBuilder) -> Self {
        Tokenizer {
            input: source.as_bytes(),
            chars: source,
            pos: 0,
            builder,
        }
    }

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

    // -- Text --

    fn parse_text(&mut self) {
        let start = self.pos;
        while self.pos < self.input.len() && self.peek_byte() != Some(b'<') {
            self.pos += 1;
        }
        // A lone '<' that doesn't begin a tag is literal text; pull it in so we
        // don't spin forever.
        if start == self.pos && self.peek_byte() == Some(b'<') {
            self.pos += 1;
        }
        let raw = &self.chars[start..self.pos];
        self.builder.text(decode_entities(raw));
    }

    // -- Comments --

    fn parse_comment(&mut self) {
        self.pos += 4; // consume "<!--"
        let start = self.pos;
        while self.pos < self.input.len() && !self.starts_with_bytes(b"-->") {
            self.pos += 1;
        }
        // `start` and `self.pos` both land on char boundaries (the bytes we look
        // for are ASCII), so slicing the &str here is safe.
        let data = self.chars[start..self.pos].to_string();
        if self.starts_with_bytes(b"-->") {
            self.pos += 3;
        }
        self.builder.comment(data);
    }

    // -- Tags --

    fn is_tag_start(&self) -> bool {
        // '<' must be followed by an ASCII letter to begin a start tag.
        matches!(self.input.get(self.pos + 1), Some(b) if b.is_ascii_alphabetic())
    }

    fn parse_start_tag(&mut self) {
        self.pos += 1; // consume '<'
        let tag = self.read_tag_name();
        let (attrs, self_closing) = self.read_attributes();

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
    }

    fn parse_end_tag(&mut self) {
        self.pos += 2; // consume "</"
        let tag = self.read_tag_name();
        self.skip_until_byte(b'>');
        self.builder.close_tag(&tag);
    }

    fn read_tag_name(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b':' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.chars[start..self.pos].to_ascii_lowercase()
    }

    /// Read attributes until `>` or `/>`. Returns the attribute map and whether
    /// the tag was self-closing.
    fn read_attributes(&mut self) -> (AttrMap, bool) {
        let mut attrs = AttrMap::new();
        let mut self_closing = false;
        loop {
            self.skip_whitespace();
            match self.peek_byte() {
                None => break,
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(b'/') => {
                    self.pos += 1;
                    if self.peek_byte() == Some(b'>') {
                        self.pos += 1;
                        self_closing = true;
                        break;
                    }
                }
                _ => {
                    let (name, value) = self.read_attribute();
                    if !name.is_empty() {
                        attrs.entry(name).or_insert(value);
                    }
                }
            }
        }
        (attrs, self_closing)
    }

    fn read_attribute(&mut self) -> (String, String) {
        let name = self.read_attr_name();
        self.skip_whitespace();
        let value = if self.peek_byte() == Some(b'=') {
            self.pos += 1;
            self.skip_whitespace();
            self.read_attr_value()
        } else {
            String::new()
        };
        (name, value)
    }

    fn read_attr_name(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b == b'=' || b == b'>' || b == b'/' || b.is_ascii_whitespace() {
                break;
            }
            self.pos += 1;
        }
        self.chars[start..self.pos].to_ascii_lowercase()
    }

    fn read_attr_value(&mut self) -> String {
        match self.peek_byte() {
            Some(q @ (b'"' | b'\'')) => {
                self.pos += 1; // opening quote
                let start = self.pos;
                while let Some(b) = self.peek_byte() {
                    if b == q {
                        break;
                    }
                    self.pos += 1;
                }
                let raw = &self.chars[start..self.pos];
                if self.peek_byte() == Some(q) {
                    self.pos += 1; // closing quote
                }
                decode_entities(raw)
            }
            _ => {
                // Unquoted value: read until whitespace or '>'.
                let start = self.pos;
                while let Some(b) = self.peek_byte() {
                    if b.is_ascii_whitespace() || b == b'>' {
                        break;
                    }
                    self.pos += 1;
                }
                decode_entities(&self.chars[start..self.pos])
            }
        }
    }

    /// Read the verbatim body of a raw-text element (`<script>`, `<style>`, …)
    /// up to its matching close tag, which we leave for the caller to consume.
    fn consume_raw_text(&mut self, tag: &str) {
        let close = format!("</{tag}");
        let close_bytes = close.as_bytes();
        let start = self.pos;
        // Byte-wise scan: the body can contain multibyte UTF-8, but the end tag
        // we stop on is pure ASCII, so we only ever slice at a char boundary.
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
        // Consume the "</tag" we stopped on plus its '>'.
        if self.starts_with_ci(close_bytes) {
            self.pos += close.len();
            self.skip_until_byte(b'>');
        }
    }

    // -- Low-level helpers --

    fn peek_byte(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.chars[self.pos..].starts_with(s)
    }

    /// Byte-wise prefix check. Safe at *any* position, including the middle of a
    /// multibyte UTF-8 character - unlike slicing `self.chars`, which would
    /// panic. Used by the scans that walk one byte at a time.
    fn starts_with_bytes(&self, needle: &[u8]) -> bool {
        self.input[self.pos..].starts_with(needle)
    }

    /// Like [`starts_with_bytes`] but ASCII-case-insensitive (for end tags such
    /// as `</SCRIPT>`).
    fn starts_with_ci(&self, needle: &[u8]) -> bool {
        let hay = &self.input[self.pos..];
        hay.len() >= needle.len() && hay[..needle.len()].eq_ignore_ascii_case(needle)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek_byte(), Some(b) if b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn skip_until_byte(&mut self, target: u8) {
        while let Some(b) = self.peek_byte() {
            self.pos += 1;
            if b == target {
                break;
            }
        }
    }
}

// --- Entities ---------------------------------------------------------------

/// Decode the HTML character references browsers see most often. A real browser
/// knows ~2,000 named entities; this covers the handful that actually show up in
/// body text on the pages we render, plus all numeric references.
fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            // Copy one UTF-8 char (advance by its byte length).
            let ch = input[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        // Find the ';' that should terminate the reference (cap the scan).
        let rest = &input[i..];
        if let Some(semi) = rest[1..].find(';').filter(|&n| n < 32) {
            let entity = &rest[1..1 + semi];
            if let Some(decoded) = decode_one_entity(entity) {
                out.push_str(&decoded);
                i += semi + 2; // '&' + entity + ';'
                continue;
            }
        }
        out.push('&');
        i += 1;
    }
    out
}

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
    // A small named table - the entities that actually appear in page text.
    let s = match entity {
        "hearts" => "♥",
        "dagger" => "†",
        "spades" => "♠",
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{00a0}",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "mdash" => "—",
        "ndash" => "–",
        "hellip" => "…",
        "ldquo" => "“",
        "rdquo" => "”",
        "lsquo" => "‘",
        "rsquo" => "’",
        "middot" => "·",
        "bull" => "•",
        "deg" => "°",
        "times" => "×",
        "divide" => "÷",
        "frac12" => "½",
        "rarr" => "→",
        "larr" => "←",
        "euro" => "€",
        "pound" => "£",
        "cent" => "¢",
        "sect" => "§",
        "para" => "¶",
        _ => return None,
    };
    Some(s.to_string())
}

#[cfg(test)]
mod tests {
    use std::assert_eq;

    use super::*;
    use crate::dom::pretty_print;

    #[test]
    fn parses_basic_nesting() {
        let dom = parse("<p>Hello <b>world</b>!</p>");
        let dump = pretty_print(&dom);
        assert!(dump.contains("<p>"));
        assert!(dump.contains("<b>"));
        assert_eq!(dom.inner_text(), "Hello world!");
    }

    #[test]
    fn handles_void_and_self_closing() {
        let dom = parse("<div>a<br>b<img src=x />c</div>");
        // br and img must not swallow following siblings as children.
        assert_eq!(dom.inner_text(), "abc");
        let dump = pretty_print(&dom);
        assert!(dump.contains("<br>"));
        assert!(dump.contains("<img src=\"x\">"));
    }

    #[test]
    fn handles_void_svgs() {
        let dom = parse("<div>a<br>b<svg><circle/><path/>_after_</svg>c</div>");
        assert_eq!(dom.inner_text(), "ab_after_c");
        let dump = pretty_print(&dom);
        assert!(dump.contains("<svg>"));
        assert!(dump.contains("<circle>"));

        for node in dom.descendants() {
            if let Some("svg") = node.tag_name() {
                assert_eq!(node.children.len(), 3);
                assert_eq!(node.children[0].tag_name(), Some("circle"));
                assert_eq!(node.children[1].tag_name(), Some("path"));
                break;
            }
        }
    }

    #[test]
    fn implied_end_tags_for_list_items() {
        let dom = parse("<ul><li>one<li>two<li>three</ul>");
        let dump = pretty_print(&dom);
        // Three siblings, not nested inside each other.
        assert_eq!(dump.matches("<li>").count(), 3);
        assert_eq!(dom.inner_text(), "onetwothree");
    }

    #[test]
    fn recovers_from_misnesting() {
        // Classic mis-nest: </b> appears after the </i> that should close first.
        let dom = parse("<p><b><i>x</b></i></p>");
        assert_eq!(dom.inner_text(), "x");
    }

    #[test]
    fn script_body_is_not_parsed_as_html() {
        let dom = parse("<script>if (a < b && c > d) {}</script><p>after</p>");
        // The '<' inside the script must not start a tag.
        assert!(dom.inner_text().contains("after"));
        let dump = pretty_print(&dom);
        assert!(dump.contains("<script>"));
        assert!(dump.contains("<p>"));
    }

    #[test]
    fn decodes_entities() {
        let dom = parse("<p>Fish &amp; chips &mdash; &#163;5 &#x263A;</p>");
        assert_eq!(dom.inner_text(), "Fish & chips — £5 ☺");

        let dom2 = parse("<p>&hearts;</p");
        assert_eq!(dom2.inner_text(), "♥");
    }

    #[test]
    fn skips_doctype_and_comments_render_clean() {
        let dom = parse("<!DOCTYPE html><!-- hi --><html><body>x</body></html>");
        assert_eq!(dom.tag_name(), Some("html"));
        assert_eq!(dom.inner_text(), "x");
    }

    #[test]
    fn multibyte_in_script_and_comments_does_not_panic() {
        // Bytes of '·' and '-' must never be sliced mid-character.
        let dom = parse("<style>/* π · ½ — */ a{}</style><!-- café · résumé --><p>after·text</p>");
        assert!(dom.inner_text().contains("after·text"));
    }

    #[test]
    fn unquoted_attributes() {
        let dom = parse("<a href=/about class=nav>about</a>");
        let dump = pretty_print(&dom);
        assert!(dump.contains("class=\"nav\""));
        assert!(dump.contains("href=\"/about\""));
    }
}
