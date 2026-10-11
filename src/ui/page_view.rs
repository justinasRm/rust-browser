//! Scroll an engine canvas without uploading a whole, potentially huge page.

use eframe::egui::{self, Color32, ColorImage, Key, Rect, TextureHandle};
use robin::render::Canvas;

pub(super) struct PageView {
    canvas: Canvas,
    texture: Option<TextureHandle>,
    uploaded: Option<PixelRegion>,
    scroll_y: f32,
}

impl PageView {
    pub fn new(canvas: Canvas) -> Self {
        Self {
            canvas,
            texture: None,
            uploaded: None,
            scroll_y: 0.0,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let viewport_height = ui.available_height();
        let max_scroll = (self.canvas.height as f32 - viewport_height).max(0.0);
        let mut area = egui::ScrollArea::both().auto_shrink([false, false]);
        if !ui.ctx().wants_keyboard_input() {
            if let Some(offset) = keyboard_scroll(ui, self.scroll_y, max_scroll, viewport_height) {
                area = area.vertical_scroll_offset(offset);
            }
        }

        let output = area.show_viewport(ui, |ui, viewport| {
            let size = egui::vec2(self.canvas.width as f32, self.canvas.height as f32);
            let (page, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let region = PixelRegion::visible(&self.canvas, viewport);
            if region.width == 0 || region.height == 0 {
                return;
            }
            if self.uploaded != Some(region) {
                let image = region.image(&self.canvas);
                if let Some(texture) = &mut self.texture {
                    texture.set(image, egui::TextureOptions::NEAREST);
                } else {
                    self.texture = Some(ui.ctx().load_texture(
                        "page viewport",
                        image,
                        egui::TextureOptions::NEAREST,
                    ));
                }
                self.uploaded = Some(region);
            }
            let destination = Rect::from_min_size(
                page.min + egui::vec2(region.x as f32, region.y as f32),
                egui::vec2(region.width as f32, region.height as f32),
            );
            if let Some(texture) = &self.texture {
                ui.painter().image(
                    texture.id(),
                    destination,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        });
        self.scroll_y = output.state.offset.y;
    }
}

/// egui handles wheel/trackpad input and scrollbars; retain the old page keys.
fn keyboard_scroll(ui: &egui::Ui, current: f32, max: f32, height: f32) -> Option<f32> {
    ui.input(|input| {
        let page = (height - 40.0).max(40.0);
        let mut next = current;
        if input.key_pressed(Key::ArrowDown) || input.key_pressed(Key::J) {
            next += 48.0;
        }
        if input.key_pressed(Key::ArrowUp) || input.key_pressed(Key::K) {
            next -= 48.0;
        }
        if input.key_pressed(Key::PageDown) || input.key_pressed(Key::Space) {
            next += page;
        }
        if input.key_pressed(Key::PageUp) {
            next -= page;
        }
        if input.key_pressed(Key::Home) {
            next = 0.0;
        }
        if input.key_pressed(Key::End) {
            next = max;
        }
        (next != current).then_some(next.clamp(0.0, max))
    })
}

/// Round outward so fractional scrolling never leaves a missing edge row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRegion {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl PixelRegion {
    fn visible(canvas: &Canvas, viewport: Rect) -> Self {
        let x = (viewport.min.x.max(0.0).floor() as usize).min(canvas.width);
        let y = (viewport.min.y.max(0.0).floor() as usize).min(canvas.height);
        let right = (viewport.max.x.max(0.0).ceil() as usize)
            .min(canvas.width)
            .max(x);
        let bottom = (viewport.max.y.max(0.0).ceil() as usize)
            .min(canvas.height)
            .max(y);
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    fn image(self, canvas: &Canvas) -> ColorImage {
        let mut bytes = Vec::with_capacity(self.width * self.height * 4);
        for y in self.y..self.y + self.height {
            let start = y * canvas.width + self.x;
            for pixel in &canvas.pixels[start..start + self.width] {
                bytes.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
            }
        }
        ColorImage::from_rgba_unmultiplied([self.width, self.height], &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use robin::css::Color;

    #[test]
    fn keyboard_can_reach_bottom_and_return_home() {
        let ctx = egui::Context::default();
        let mut page = PageView::new(Canvas::new(200, 20_000, Color::rgb(255, 255, 255)));
        let mut frame = |key: Option<Key>| {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 400.0),
                )),
                ..Default::default()
            };
            if let Some(key) = key {
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| page.show(ui));
            });
            page.scroll_y
        };
        frame(None);
        assert!(frame(Some(Key::End)) > 19_000.0);
        assert_eq!(frame(Some(Key::Home)), 0.0);
    }

    #[test]
    fn tall_page_uploads_only_visible_rows_and_correct_colors() {
        let mut canvas = Canvas::new(3, 20_000, Color::rgb(255, 255, 255));
        canvas.pixels[10_000 * 3 + 1] = Color::rgb(255, 0, 0);
        let region = PixelRegion::visible(
            &canvas,
            Rect::from_min_max(egui::pos2(1.0, 10_000.5), egui::pos2(3.0, 10_002.5)),
        );
        assert_eq!(
            region,
            PixelRegion {
                x: 1,
                y: 10_000,
                width: 2,
                height: 3
            }
        );
        let image = region.image(&canvas);
        assert_eq!(image.size, [2, 3]);
        assert_eq!(image.pixels[0], Color32::RED);
        assert_eq!(image.pixels[1], Color32::WHITE);
    }

    #[test]
    fn short_page_and_bottom_edge_are_clipped_to_canvas() {
        let canvas = Canvas::new(3, 2, Color::rgb(255, 255, 255));
        let region = PixelRegion::visible(
            &canvas,
            Rect::from_min_max(egui::pos2(0.0, 1.0), egui::pos2(100.0, 800.0)),
        );
        assert_eq!(
            region,
            PixelRegion {
                x: 0,
                y: 1,
                width: 3,
                height: 1
            }
        );
        assert_eq!(region.image(&canvas).pixels.len(), 3);
    }
}
