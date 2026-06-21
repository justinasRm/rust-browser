//! Text rasterization — turning characters into pixels.
//!
//! Up to now every box has been a solid rectangle. Real pages are mostly *text*,
//! and drawing text is its own small world: load a font, ask it for the shape of
//! each character at a given size, and blend that shape (a grayscale *coverage*
//! bitmap, which is what gives us smooth anti-aliased edges) onto the canvas.
//!
//! We lean on [`fontdue`] for the hard part — decoding TrueType outlines and
//! rasterizing them — and bundle the DejaVu fonts directly into the binary with
//! `include_bytes!`, so Robin renders identically on any machine with no system
//! fonts required.
//!
//! Two jobs live here:
//!
//! * **Measuring** — how wide is this string? Layout needs this to wrap lines.
//! * **Drawing** — blit each glyph at the right place in the right color.

use fontdue::{Font, FontSettings};

use crate::css::Color;
use crate::paint::TextRun;
use crate::render::Canvas;

// The fonts are embedded so the binary is self-contained.
const SANS: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
const SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
const SANS_ITALIC: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Oblique.ttf");
const SANS_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-BoldOblique.ttf");
const MONO: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

/// The set of font faces Robin can draw with. Built once and shared (parsing a
/// TTF isn't free), it picks the right face for a run's bold/italic/monospace
/// flags.
pub struct Fonts {
    regular: Font,
    bold: Font,
    italic: Font,
    bold_italic: Font,
    mono: Font,
}

impl Fonts {
    /// Parse the bundled fonts. Infallible in practice — the bytes ship with the
    /// binary — but we surface an error rather than panic, just in case.
    pub fn bundled() -> Result<Fonts, String> {
        let load = |bytes| Font::from_bytes(bytes, FontSettings::default()).map_err(String::from);
        Ok(Fonts {
            regular: load(SANS)?,
            bold: load(SANS_BOLD)?,
            italic: load(SANS_ITALIC)?,
            bold_italic: load(SANS_BOLD_ITALIC)?,
            mono: load(MONO)?,
        })
    }

    fn face(&self, bold: bool, italic: bool, monospace: bool) -> &Font {
        match (monospace, bold, italic) {
            (true, _, _) => &self.mono,
            (false, true, true) => &self.bold_italic,
            (false, true, false) => &self.bold,
            (false, false, true) => &self.italic,
            (false, false, false) => &self.regular,
        }
    }

    /// The horizontal advance of a single character at a given size.
    pub fn char_advance(&self, ch: char, size: f32, bold: bool, italic: bool, mono: bool) -> f32 {
        self.face(bold, italic, mono)
            .metrics(ch, size)
            .advance_width
    }

    /// The total width of a string laid out on one line.
    pub fn measure(&self, text: &str, size: f32, bold: bool, italic: bool, mono: bool) -> f32 {
        let font = self.face(bold, italic, mono);
        text.chars()
            .map(|c| font.metrics(c, size).advance_width)
            .sum()
    }

    /// Vertical metrics for a line of text at a given size: how far the tallest
    /// glyph rises above the baseline (ascent), drops below it (descent), and the
    /// natural line-to-line distance.
    pub fn line_metrics(&self, size: f32, bold: bool, italic: bool, mono: bool) -> LineMetrics {
        let font = self.face(bold, italic, mono);
        match font.horizontal_line_metrics(size) {
            Some(m) => LineMetrics {
                ascent: m.ascent,
                descent: -m.descent, // fontdue reports descent as negative
                line_height: m.new_line_size,
            },
            // Fallback proportions if the font lacks the table.
            None => LineMetrics {
                ascent: size * 0.8,
                descent: size * 0.2,
                line_height: size * 1.2,
            },
        }
    }

    /// Draw a positioned run of text onto the canvas. `run.y` is the baseline.
    pub fn draw_run(&self, canvas: &mut Canvas, run: &TextRun) {
        let font = self.face(run.bold, run.italic, run.monospace);
        let mut pen_x = run.x;
        for ch in run.text.chars() {
            let (metrics, bitmap) = font.rasterize(ch, run.font_size);
            // Place the glyph relative to the baseline. In screen space y grows
            // downward, so the glyph's top edge is above the baseline by
            // (ymin + height).
            let glyph_left = pen_x + metrics.xmin as f32;
            let glyph_top = run.y - (metrics.ymin as f32 + metrics.height as f32);

            blit_coverage(
                canvas,
                &bitmap,
                metrics.width,
                metrics.height,
                glyph_left,
                glyph_top,
                run.color,
            );
            pen_x += metrics.advance_width;
        }
    }
}

/// Ascent/descent/line-height for a line of text, in pixels.
#[derive(Debug, Clone, Copy)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
}

/// Blend a glyph's coverage bitmap onto the canvas in a solid color. Each byte
/// of `coverage` (0–255) becomes the glyph's alpha at that pixel — that's what
/// produces smooth, anti-aliased edges instead of jagged ones.
fn blit_coverage(
    canvas: &mut Canvas,
    coverage: &[u8],
    width: usize,
    height: usize,
    left: f32,
    top: f32,
    color: Color,
) {
    let left = left.round() as i32;
    let top = top.round() as i32;
    for row in 0..height {
        for col in 0..width {
            let cov = coverage[row * width + col];
            if cov == 0 {
                continue;
            }
            // Combine glyph coverage with the run's own alpha.
            let alpha = (cov as u32 * color.a as u32 / 255) as u8;
            let x = left + col as i32;
            let y = top + row as i32;
            if x >= 0 && y >= 0 {
                canvas.blend_pixel(x as usize, y as usize, Color { a: alpha, ..color });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_load_and_measure() {
        let fonts = Fonts::bundled().expect("bundled fonts parse");
        let w = fonts.measure("Hello", 16.0, false, false, false);
        assert!(w > 20.0 && w < 80.0, "unexpected width {w}");
        // Bold text is at least as wide as regular at the same size.
        let wb = fonts.measure("Hello", 16.0, true, false, false);
        assert!(wb >= w * 0.9);
    }

    #[test]
    fn line_metrics_are_sensible() {
        let fonts = Fonts::bundled().unwrap();
        let m = fonts.line_metrics(16.0, false, false, false);
        assert!(m.ascent > 0.0 && m.descent > 0.0);
        assert!(m.line_height >= m.ascent + m.descent);
    }

    #[test]
    fn drawing_puts_dark_pixels_on_the_canvas() {
        let fonts = Fonts::bundled().unwrap();
        let mut canvas = Canvas::new(120, 40, Color::rgb(255, 255, 255));
        fonts.draw_run(
            &mut canvas,
            &TextRun {
                text: "Robin".into(),
                x: 4.0,
                y: 28.0,
                font_size: 20.0,
                color: Color::rgb(0, 0, 0),
                bold: false,
                italic: false,
                monospace: false,
            },
        );
        // Some pixels should now be noticeably darker than the white background.
        let dark = canvas.pixels.iter().filter(|p| p.r < 128).count();
        assert!(dark > 20, "expected glyph pixels, got {dark}");
    }
}
