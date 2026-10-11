//! Desktop UI. The engine hands us a Canvas; egui only presents its pixels.

mod page_view;
pub(crate) mod screen;

use eframe::egui;
use robin::render::Canvas;

use page_view::PageView;

const TOOLBAR_HEIGHT: f32 = 100.0;

/// Display a completed page. Navigation will later replace the owned canvas.
pub fn show(canvas: Canvas, title: &str) -> Result<(), String> {
    let height = screen::get_window_height().unwrap_or(700) as f32;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([canvas.width as f32, height.max(TOOLBAR_HEIGHT + 1.0)])
            .with_resizable(false),
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        title,
        options,
        Box::new(move |_| {
            Ok(Box::new(BrowserUi {
                page: PageView::new(canvas),
            }))
        }),
    )
    .map_err(|error| error.to_string())
}

struct BrowserUi {
    page: PageView,
}

impl eframe::App for BrowserUi {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Page shortcuts must not consume typing when the toolbar gains input.
        if !ctx.wants_keyboard_input()
            && ctx.input(|input| {
                input.key_pressed(egui::Key::Escape) || input.key_pressed(egui::Key::Q)
            })
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Preserve the existing space for the future address/search bar.
        egui::TopBottomPanel::top("toolbar")
            .exact_height(TOOLBAR_HEIGHT)
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |_ui| {});

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::WHITE))
            .show(ctx, |ui| self.page.show(ui));
    }
}
