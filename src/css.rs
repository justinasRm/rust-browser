//! A small CSS parser: stylesheet text in, a list of rules out.
//!
//! CSS is a list of **rules**. Each rule has one or more **selectors** (what it
//! targets) and a block of **declarations** (`property: value;`). For example:
//!
//! ```css
//! a.story, h1 { color: #ff6600; font-weight: bold; }
//! ```
//!
//! is one rule with two selectors (`a.story` and `h1`) and two declarations.
//!
//! We support **simple selectors** - an optional tag name, an optional `#id`,
//! and any number of `.class`es, plus the universal `*`. That is enough to drive
//! a built-in user-agent stylesheet and to match the common author rules on the
//! pages we render. Combinators (descendant, child, …) are intentionally left
//! out; adding them is a great exercise (see the chapter).
//!
//! Like the HTML parser, this one is *tolerant*: anything it can't understand
//! (an `@media` block, an exotic value, a malformed rule) is skipped rather than
//! aborting the parse.

/// A parsed stylesheet: just an ordered list of rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

/// We only model simple selectors. The enum leaves room to grow.
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Simple(SimpleSelector),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SimpleSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub name: String,
    pub value: Value,
}

/// A CSS value. We keep just the shapes layout and paint care about.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Keyword(String),
    Length(f32, Unit),
    ColorValue(Color),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Px,
    Em,
    Rem,
    Pt,
    Percent,
    /// Viewport width / height percentages (`1vw` = 1% of the viewport width).
    Vw,
    Vh,
}

/// An 8-bit-per-channel RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
}

/// Specificity is the tie-breaker in the cascade: (#id, #class, #tag). Higher
/// wins. `*` and bare declarations count for nothing.
pub type Specificity = (usize, usize, usize);

impl Selector {
    pub fn specificity(&self) -> Specificity {
        let Selector::Simple(s) = self;
        let a = s.id.iter().count();
        let b = s.classes.len();
        let c = s.tag_name.iter().count();
        (a, b, c)
    }
}

impl Value {
    /// Interpret a length as pixels. `em`/`rem` are resolved against a 16px
    /// base (we don't track inherited font-size in this simple model);
    /// percentages and keywords are not lengths, so they return 0.
    pub fn to_px(&self) -> f32 {
        match self {
            Value::Length(n, Unit::Px) => *n,
            Value::Length(n, Unit::Pt) => n * 96.0 / 72.0,
            Value::Length(n, Unit::Em) | Value::Length(n, Unit::Rem) => n * 16.0,
            _ => 0.0,
        }
    }

    /// The keyword text, if this value is a keyword.
    pub fn keyword(&self) -> Option<&str> {
        match self {
            Value::Keyword(k) => Some(k),
            _ => None,
        }
    }
}

// Parse a whole stylesheet.
pub fn parse(source: &str) -> Stylesheet {
    let mut parser = Parser {
        input: source,
        pos: 0,
    };
    Stylesheet {
        rules: parser.parse_rules(),
    }
}

/// Parse a single declaration block (the contents of a `style="..."` attribute).
pub fn parse_declarations(source: &str) -> Vec<Declaration> {
    let mut parser = Parser {
        input: source,
        pos: 0,
    };
    parser.parse_declarations_until_end()
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn parse_rules(&mut self) -> Vec<Rule> {
        let mut rules = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.eof() {
                break;
            }
            // Skip at-rules wholesale: @media { ... }, @import ...;, @font-face { }.
            if self.next_char() == '@' {
                self.skip_at_rule();
                continue;
            }
            if let Some(rule) = self.parse_rule() {
                rules.push(rule);
            }
        }
        rules
    }

    fn parse_rule(&mut self) -> Option<Rule> {
        let selectors = self.parse_selectors();
        // parse_selectors consumes up to (and including) the '{'.
        let declarations = self.parse_declarations_block();
        if selectors.is_empty() {
            None
        } else {
            Some(Rule {
                selectors,
                declarations,
            })
        }
    }

    /// Parse a comma-separated selector list, stopping at `{`.
    fn parse_selectors(&mut self) -> Vec<Selector> {
        let mut selectors = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.next_char() {
                '{' => {
                    self.bump();
                    break;
                }
                ',' => {
                    self.bump();
                }
                '\0' => break,
                _ => {
                    let parsed = self.parse_simple_selector();
                    self.skip_ws_and_comments();
                    // A simple selector is only valid here if the next thing is a
                    // comma or the start of the declaration block. Anything else
                    // means a combinator/compound/pseudo we don't model
                    // (`a b`, `a > b`, `a:hover`, `a[x]`) - drop the whole
                    // selector so we never *mis*-apply it. (Matching nothing is
                    // safer than matching the wrong elements.)
                    match (parsed, self.next_char()) {
                        (Some(sel), ',' | '{' | '\0') => selectors.push(Selector::Simple(sel)),
                        _ => self.skip_until_any(&['{', ',']),
                    }
                }
            }
        }
        // Sort highest-specificity first so style matching can stop at the first
        // hit per property if it wants to.
        selectors.sort_by_key(|s| std::cmp::Reverse(s.specificity()));
        selectors
    }

    fn parse_simple_selector(&mut self) -> Option<SimpleSelector> {
        let mut selector = SimpleSelector::default();
        let mut matched_something = false;
        loop {
            match self.next_char() {
                '#' => {
                    self.bump();
                    selector.id = Some(self.parse_identifier());
                    matched_something = true;
                }
                '.' => {
                    self.bump();
                    selector.classes.push(self.parse_identifier());
                    matched_something = true;
                }
                '*' => {
                    self.bump();
                    matched_something = true;
                }
                c if is_identifier_char(c) => {
                    selector.tag_name = Some(self.parse_identifier().to_ascii_lowercase());
                    matched_something = true;
                }
                _ => break,
            }
        }
        if matched_something {
            Some(selector)
        } else {
            None
        }
    }

    fn parse_declarations_block(&mut self) -> Vec<Declaration> {
        let decls = self.parse_declarations_until_end();
        // Consume the closing '}' if present.
        if self.next_char() == '}' {
            self.bump();
        }
        decls
    }

    fn parse_declarations_until_end(&mut self) -> Vec<Declaration> {
        let mut declarations = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.next_char() {
                '}' | '\0' => break,
                ';' => {
                    self.bump();
                }
                _ => {
                    if let Some(decl) = self.parse_declaration() {
                        declarations.push(decl);
                    } else {
                        self.skip_until_any(&[';', '}']);
                    }
                }
            }
        }
        declarations
    }

    fn parse_declaration(&mut self) -> Option<Declaration> {
        let name = self.parse_identifier().to_ascii_lowercase();
        self.skip_ws_and_comments();
        if self.next_char() != ':' {
            return None;
        }
        self.bump();
        self.skip_ws_and_comments();
        let value = self.parse_value()?;
        // Skip anything up to the terminating ';' or '}' (e.g. "!important").
        self.skip_until_any(&[';', '}']);
        if name.is_empty() {
            None
        } else {
            Some(Declaration { name, value })
        }
    }

    fn parse_value(&mut self) -> Option<Value> {
        match self.next_char() {
            '#' => self.parse_hex_color(),
            c if c.is_ascii_digit() || c == '-' || c == '.' || c == '+' => self.parse_length(),
            _ => {
                let word = self.parse_value_word();
                if word.is_empty() {
                    return None;
                }
                // `rgb(...)`/`rgba(...)` functional colors.
                if let Some(color) = parse_color_function(&word, self) {
                    return Some(Value::ColorValue(color));
                }
                // A named color, otherwise a plain keyword.
                if let Some(color) = named_color(&word) {
                    Some(Value::ColorValue(color))
                } else {
                    Some(Value::Keyword(word.to_ascii_lowercase()))
                }
            }
        }
    }

    fn parse_length(&mut self) -> Option<Value> {
        let number = self.parse_number()?;
        let unit = match self
            .parse_identifier_or_percent()
            .to_ascii_lowercase()
            .as_str()
        {
            "px" | "" => Unit::Px,
            "em" => Unit::Em,
            "rem" => Unit::Rem,
            "pt" => Unit::Pt,
            "%" => Unit::Percent,
            "vw" => Unit::Vw,
            "vh" => Unit::Vh,
            // Unknown unit: treat as px so we degrade gracefully.
            _ => Unit::Px,
        };
        Some(Value::Length(number, unit))
    }

    fn parse_hex_color(&mut self) -> Option<Value> {
        self.bump(); // consume '#'
        let start = self.pos;
        while self.pos < self.input.len() && self.next_char().is_ascii_hexdigit() {
            self.bump();
        }
        let hex = &self.input[start..self.pos];
        parse_hex(hex).map(Value::ColorValue)
    }

    // -- token helpers --

    fn parse_identifier(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.input.len() && is_identifier_char(self.next_char()) {
            self.bump();
        }
        self.input[start..self.pos].to_string()
    }

    fn parse_identifier_or_percent(&mut self) -> String {
        if self.next_char() == '%' {
            self.bump();
            return "%".to_string();
        }
        self.parse_identifier()
    }

    /// A value token: a run that isn't whitespace, a separator, or a delimiter.
    fn parse_value_word(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.input.len() {
            let c = self.next_char();
            if c.is_whitespace() || c == ';' || c == '}' || c == ',' || c == '(' {
                break;
            }
            self.bump();
        }
        self.input[start..self.pos].to_string()
    }

    fn parse_number(&mut self) -> Option<f32> {
        let start = self.pos;
        while self.pos < self.input.len() {
            let c = self.next_char();
            if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
                self.bump();
            } else {
                break;
            }
        }
        self.input[start..self.pos].parse().ok()
    }

    fn skip_at_rule(&mut self) {
        // Either `@name ... ;` or `@name ... { ... }` (with nested braces).
        while !self.eof() {
            match self.next_char() {
                ';' => {
                    self.bump();
                    return;
                }
                '{' => {
                    self.skip_balanced_braces();
                    return;
                }
                _ => self.pos += 1,
            }
        }
    }

    fn skip_balanced_braces(&mut self) {
        let mut depth = 0;
        while !self.eof() {
            match self.next_char() {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    self.bump();
                    if depth == 0 {
                        return;
                    }
                    continue;
                }
                _ => {}
            }
            self.bump();
        }
    }

    fn skip_until_any(&mut self, stops: &[char]) {
        while !self.eof() && !stops.contains(&self.next_char()) {
            self.bump();
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            let before = self.pos;
            while !self.eof() && self.next_char().is_whitespace() {
                self.bump();
            }
            if self.input[self.pos..].starts_with("/*") {
                if let Some(end) = self.input[self.pos..].find("*/") {
                    self.pos += end + 2;
                } else {
                    self.pos = self.input.len();
                }
            }
            if self.pos == before {
                break;
            }
        }
    }

    fn next_char(&self) -> char {
        self.input[self.pos..].chars().next().unwrap_or('\0')
    }

    /// Advance past the current character by its full UTF-8 width. Using this
    /// instead of a bare `self.pos += 1` is what keeps the cursor on a character
    /// boundary even when the stylesheet contains multibyte text (`·`, `-`, …) -
    /// land mid-character and the next `next_char()` slice would panic.
    fn bump(&mut self) {
        if self.pos < self.input.len() {
            self.pos += self.next_char().len_utf8();
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Color {
    let h = h.rem_euclid(360.0);
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    Color::rgb(
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

fn parse_color_function(word: &str, parser: &mut Parser) -> Option<Color> {
    let fname = word.to_ascii_lowercase();
    if fname != "rgb" && fname != "rgba" && fname != "hsl" && fname != "hsla" {
        return None;
    }
    // We're positioned at '(' (parse_value_word stops before it).
    if parser.next_char() != '(' {
        return None;
    }
    let start = parser.pos + 1;
    let end = parser.input[start..].find(')').map(|i| start + i)?;
    let args: Vec<f32> = parser.input[start..end]
        .split([',', ' ', '/'])
        .filter(|s| !s.trim().is_empty())
        .filter_map(|s| s.trim().trim_end_matches('%').parse::<f32>().ok())
        .collect();
    parser.pos = end + 1; // consume past ')'
    if args.len() < 3 {
        return None;
    }
    let a = if args.len() >= 4 {
        (args[3] * 255.0).round() as u8
    } else {
        255
    };

    if fname == "hsl" || fname == "hsla" {
        Some(Color {
            a,
            ..hsl_to_rgb(args[0], args[1], args[2])
        })
    } else {
        Some(Color {
            r: args[0] as u8,
            g: args[1] as u8,
            b: args[2] as u8,
            a,
        })
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    let parse2 = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        3 => {
            // #rgb -> #rrggbb
            let r = parse2(&hex[0..1].repeat(2))?;
            let g = parse2(&hex[1..2].repeat(2))?;
            let b = parse2(&hex[2..3].repeat(2))?;
            Some(Color::rgb(r, g, b))
        }
        6 => Some(Color::rgb(
            parse2(&hex[0..2])?,
            parse2(&hex[2..4])?,
            parse2(&hex[4..6])?,
        )),
        8 => Some(Color {
            r: parse2(&hex[0..2])?,
            g: parse2(&hex[2..4])?,
            b: parse2(&hex[4..6])?,
            a: parse2(&hex[6..8])?,
        }),
        _ => None,
    }
}

/// A handful of CSS named colors - the ones our pages and UA stylesheet use.
fn named_color(name: &str) -> Option<Color> {
    let c = match name.to_ascii_lowercase().as_str() {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        "navy" => (0, 0, 128),
        "gray" | "grey" => (128, 128, 128),
        "silver" => (192, 192, 192),
        "lightgray" | "lightgrey" => (211, 211, 211),
        "darkgray" | "darkgrey" => (169, 169, 169),
        "whitesmoke" => (245, 245, 245),
        "gainsboro" => (220, 220, 220),
        "yellow" => (255, 255, 0),
        "orange" => (255, 165, 0),
        "purple" => (128, 0, 128),
        "teal" => (0, 128, 128),
        "maroon" => (128, 0, 0),
        "olive" => (128, 128, 0),
        "lime" => (0, 255, 0),
        "aqua" | "cyan" => (0, 255, 255),
        "fuchsia" | "magenta" => (255, 0, 255),
        "transparent" => return Some(Color::TRANSPARENT),
        _ => return None,
    };
    Some(Color::rgb(c.0, c.1, c.2))
}

#[cfg(test)]
mod tests {
    use std::println;

    use super::*;

    #[test]
    fn parses_a_rule_with_multiple_selectors() {
        let ss = parse("a.story, h1 { color: #ff6600; font-weight: bold; }");
        assert_eq!(ss.rules.len(), 1);
        let rule = &ss.rules[0];
        assert_eq!(rule.selectors.len(), 2);
        assert_eq!(rule.declarations.len(), 2);
        assert_eq!(rule.declarations[0].name, "color");
        assert_eq!(
            rule.declarations[0].value,
            Value::ColorValue(Color::rgb(255, 102, 0))
        );
    }

    #[test]
    fn specificity_ordering() {
        let id: Specificity = Selector::Simple(SimpleSelector {
            id: Some("x".into()),
            ..Default::default()
        })
        .specificity();
        let class: Specificity = Selector::Simple(SimpleSelector {
            classes: vec!["c".into()],
            ..Default::default()
        })
        .specificity();
        assert!(id > class);
    }

    #[test]
    fn lengths_and_units() {
        let ss = parse("p { margin: 10px; padding: 1.5em; width: 12pt; }");
        let d = &ss.rules[0].declarations;
        assert_eq!(d[0].value, Value::Length(10.0, Unit::Px));
        assert_eq!(d[1].value, Value::Length(1.5, Unit::Em));
        assert_eq!(d[1].value.to_px(), 24.0);
        assert!((d[2].value.to_px() - 16.0).abs() < 0.01); // 12pt == 16px
    }

    #[test]
    fn hsl() {
        let ss = parse("a { color: hsl(120, 100%, 50%) }");
        assert_eq!(
            ss.rules[0].declarations[0].value,
            Value::ColorValue(Color {
                a: 255,
                ..Color::rgb(0, 255, 0)
            })
        )
    }

    #[test]
    fn hsla() {
        let ss = parse("a { color: hsla(120, 100%, 50%, 0.2) } b { color: rgb(255, 255, 255) }");
        assert_eq!(
            ss.rules[0].declarations[0].value,
            Value::ColorValue(Color {
                a: 51,
                ..Color::rgb(0, 255, 0)
            })
        )
    }

    #[test]
    fn colors_in_many_forms() {
        let ss = parse("a { color: #f60; } b { color: rgb(10, 20, 30); } i { color: navy; }");
        assert_eq!(
            ss.rules[0].declarations[0].value,
            Value::ColorValue(Color::rgb(255, 102, 0))
        );
        assert_eq!(
            ss.rules[1].declarations[0].value,
            Value::ColorValue(Color::rgb(10, 20, 30))
        );
        assert_eq!(
            ss.rules[2].declarations[0].value,
            Value::ColorValue(Color::rgb(0, 0, 128))
        );
    }

    #[test]
    fn skips_at_rules_and_comments() {
        let ss = parse(
            "@media screen { p { color: red } } /* c */ @import url(x); h1 { color: black; }",
        );
        // Only the h1 rule survives.
        assert_eq!(ss.rules.len(), 1);
        assert_eq!(
            ss.rules[0].selectors[0],
            Selector::Simple(SimpleSelector {
                tag_name: Some("h1".into()),
                ..Default::default()
            })
        );
    }

    #[test]
    fn recovers_from_a_malformed_rule() {
        let ss = parse("p { color: ; bogus } valid { color: red; }");
        // The valid rule still parses.
        assert!(ss.rules.iter().any(|r| r
            .declarations
            .iter()
            .any(|d| d.name == "color" && d.value == Value::ColorValue(Color::rgb(255, 0, 0)))));
    }

    #[test]
    fn unsupported_selectors_are_dropped_not_misread() {
        // Descendant, child and pseudo selectors must NOT turn into a list of
        // simple selectors (which would mis-style unrelated elements).
        let ss =
            parse(".nav a { color: red } article > p { color: green } a:hover { color: blue }");
        for rule in &ss.rules {
            for sel in &rule.selectors {
                let Selector::Simple(s) = sel;
                // None of these should be a bare `a`, `p`, `.nav`, etc. that
                // leaked out of a complex selector.
                assert!(
                    s.tag_name.is_none() || rule.selectors.len() == 1,
                    "complex selector leaked into a list: {sel:?}"
                );
            }
        }
        // The bare `a` from `.nav a` must not match every <a>.
        let a_only = Selector::Simple(SimpleSelector {
            tag_name: Some("a".into()),
            ..Default::default()
        });
        assert!(!ss.rules.iter().any(|r| r.selectors.contains(&a_only)));
    }

    #[test]
    fn compound_without_combinator_still_parses() {
        // `a.story` (no space) is a single simple selector and must survive.
        let ss = parse("a.story { color: red; } li, .item { color: blue; }");
        assert_eq!(ss.rules.len(), 2);
        let first = &ss.rules[0].selectors[0];
        assert_eq!(
            *first,
            Selector::Simple(SimpleSelector {
                tag_name: Some("a".into()),
                classes: vec!["story".into()],
                ..Default::default()
            })
        );
        assert_eq!(ss.rules[1].selectors.len(), 2); // li and .item
    }

    #[test]
    fn inline_declarations() {
        let decls = parse_declarations("color: red; font-size: 14px");
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[1].value, Value::Length(14.0, Unit::Px));
    }
}
