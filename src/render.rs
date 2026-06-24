//! The pixel canvas - where boxes finally become pixels.
//!
//! A [`Canvas`] is just a flat array of [`Color`]s, `width * height` of them.
//! Painting fills rectangles into it (with alpha blending, so semi-transparent
//! colors layer correctly), and at the end we hand the buffer to the `image`
//! crate to write a PNG - or, later, to a window for display.
//!
//! This is a *software* rasterizer: no GPU, no graphics API, just arithmetic on
//! a byte array. That's all a browser's compositor is, underneath.

use crate::css::Color;
use crate::layout::Rect;

/// A rectangular grid of pixels we can draw into.
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Color>,
}

impl Canvas {
    /// A new canvas filled with a solid background color (usually white).
    pub fn new(width: usize, height: usize, background: Color) -> Canvas {
        Canvas {
            width,
            height,
            pixels: vec![background; width * height],
        }
    }

    /// Fill a rectangle with a color, clipped to the canvas and alpha-blended
    /// over whatever is already there.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if color.a == 0 {
            return; // fully transparent: nothing to draw
        }
        // Clip to the canvas bounds, rounding to whole pixels.
        let x0 = rect.x.max(0.0) as usize;
        let y0 = rect.y.max(0.0) as usize;
        let x1 = ((rect.x + rect.width).min(self.width as f32)).max(0.0) as usize;
        let y1 = ((rect.y + rect.height).min(self.height as f32)).max(0.0) as usize;

        for y in y0..y1 {
            for x in x0..x1 {
                let idx = y * self.width + x;
                self.pixels[idx] = blend(color, self.pixels[idx]);
            }
        }
    }

    /// Blend a single pixel - used by glyph rasterization, where each pixel has
    /// its own coverage/alpha.
    pub fn blend_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x < self.width && y < self.height && color.a > 0 {
            let idx = y * self.width + x;
            self.pixels[idx] = blend(color, self.pixels[idx]);
        }
    }

    /// The buffer as tightly-packed RGBA bytes (for PNG encoding).
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.width * self.height * 4);
        for p in &self.pixels {
            out.extend_from_slice(&[p.r, p.g, p.b, p.a]);
        }
        out
    }

    /// The buffer as 0xRRGGBB `u32`s (the format `minifb` wants for the window).
    pub fn to_argb_u32(&self) -> Vec<u32> {
        self.pixels
            .iter()
            .map(|p| (u32::from(p.r) << 16) | (u32::from(p.g) << 8) | u32::from(p.b))
            .collect()
    }

    /// A new canvas containing rows `[top, top + height)` of this one (clamped).
    /// Used to screenshot a specific band of a tall page.
    pub fn cropped(&self, top: usize, height: usize) -> Canvas {
        let top = top.min(self.height);
        let height = height.min(self.height - top).max(1);
        let start = top * self.width;
        let end = (top + height) * self.width;
        Canvas {
            width: self.width,
            height,
            pixels: self.pixels[start..end].to_vec(),
        }
    }

    /// Save the canvas as a PNG file.
    pub fn save_png(&self, path: &str) -> Result<(), String> {
        image::save_buffer(
            path,
            &self.to_rgba_bytes(),
            self.width as u32,
            self.height as u32,
            image::ColorType::Rgba8,
        )
        .map_err(|e| e.to_string())
    }
}

/// Alpha-blend `src` over `dst` (straight, non-premultiplied alpha).
fn blend(src: Color, dst: Color) -> Color {
    if src.a == 255 {
        return src;
    }
    let sa = src.a as f32 / 255.0;
    let inv = 1.0 - sa;
    let mix = |s: u8, d: u8| ((s as f32) * sa + (d as f32) * inv).round() as u8;
    Color {
        r: mix(src.r, dst.r),
        g: mix(src.g, dst.g),
        b: mix(src.b, dst.b),
        a: 255,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_is_clipped_to_bounds() {
        let mut c = Canvas::new(4, 4, Color::rgb(255, 255, 255));
        // Rect partly off the right/bottom edge shouldn't panic.
        c.fill_rect(
            Rect {
                x: 2.0,
                y: 2.0,
                width: 10.0,
                height: 10.0,
            },
            Color::rgb(0, 0, 0),
        );
        assert_eq!(c.pixels[0], Color::rgb(255, 255, 255)); // top-left untouched
        assert_eq!(c.pixels[3 * 4 + 3], Color::rgb(0, 0, 0)); // bottom-right filled
    }

    #[test]
    fn alpha_blends_halfway() {
        let mut c = Canvas::new(1, 1, Color::rgb(0, 0, 0));
        c.fill_rect(
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 128,
            },
        );
        let p = c.pixels[0];
        assert!((120..=135).contains(&p.r), "got {}", p.r);
    }
}
