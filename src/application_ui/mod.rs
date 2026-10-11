//! Desktop UI. The engine hands us a Canvas; egui only presents its pixels.

mod page_view;
use crate::ffis::screen;
use eframe::egui;
use robin::render::Canvas;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use page_view::PageView;

/// Open the browser, optionally displaying a completed page.
pub fn show(canvas: Option<Canvas>, width: f32, title: &str, address: &str) -> Result<(), String> {
    let (_, height) = screen::get_window_dimensions().unwrap_or((0, 700));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([width, height as f32])
            .with_resizable(false),
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    };
    let address = address.to_owned();
    eframe::run_native(
        title,
        options,
        Box::new(move |_| {
            Ok(Box::new(BrowserUi {
                page: canvas.map(PageView::new),
                address,
                width,
                pending: None,
                error: None,
            }))
        }),
    )
    .map_err(|error| error.to_string())
}

struct BrowserUi {
    page: Option<PageView>,
    address: String,
    width: f32,
    pending: Option<PendingPage>,
    error: Option<String>,
}

struct PendingPage {
    address: String,
    receiver: Receiver<Result<Canvas, String>>,
}

impl BrowserUi {
    fn navigate(&mut self, ctx: &egui::Context) {
        let address = self.address.trim().to_owned();
        if address.is_empty() {
            return;
        }
        self.address = address.clone();
        self.error = None;
        let (sender, receiver) = mpsc::channel();
        let width = self.width;
        let repaint = ctx.clone();
        let target = address.clone();
        // Fetching and rasterizing can take time. Keep both off the UI thread.
        let worker = std::thread::Builder::new()
            .name("page-loader".into())
            .spawn(move || {
                let result = crate::browser::load_page(&target, width);
                let _ = sender.send(result);
                repaint.request_repaint();
            });
        match worker {
            Ok(_) => {
                // Replacing the receiver discards results from older requests.
                self.pending = Some(PendingPage { address, receiver });
            }
            Err(error) => {
                self.pending = None;
                self.error = Some(format!("Could not start page load: {error}"));
            }
        }
    }

    fn finish_navigation(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending else {
            return;
        };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Page loading stopped unexpectedly".into()),
        };
        match result {
            Ok(canvas) => {
                self.page = Some(PageView::new(canvas));
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "robin - {}",
                    pending.address
                )));
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        self.pending = None;
    }
}

impl eframe::App for BrowserUi {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.finish_navigation(ctx);

        // Page shortcuts must not consume typing when the toolbar gains input.
        if !ctx.wants_keyboard_input()
            && ctx.input(|input| {
                input.key_pressed(egui::Key::Escape) || input.key_pressed(egui::Key::Q)
            })
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let mut submitted = false;
        // Keep some space between the address field and the window edges.
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::NONE
                    .fill(egui::Color32::BLACK)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                submitted = address_field(ui, &mut self.address);
            });

        if submitted {
            self.navigate(ctx);
        }
        if self.pending.is_some() || self.error.is_some() {
            egui::TopBottomPanel::bottom("navigation status").show(ctx, |ui| {
                if let Some(pending) = &self.pending {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Loading {}", pending.address));
                    });
                }
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
            });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::WHITE))
            .show(ctx, |ui| {
                if let Some(page) = &mut self.page {
                    page.show(ui);
                }
            });
    }
}

/// A single-line edit loses focus on Enter; unrelated Enter presses do nothing.
fn address_field(ui: &mut egui::Ui, address: &mut String) -> bool {
    let response = ui.add_sized(
        ui.available_size(),
        egui::TextEdit::singleline(address)
            .id(egui::Id::new("address"))
            .hint_text("Enter a URL or file path")
            .desired_width(f32::INFINITY),
    );
    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use robin::css::Color;

    fn browser() -> BrowserUi {
        BrowserUi {
            page: None,
            address: String::new(),
            width: 300.0,
            pending: None,
            error: None,
        }
    }

    fn enter() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn enter_submits_only_the_focused_address_field() {
        let ctx = egui::Context::default();
        let mut address = String::new();
        let mut draw = |events| {
            let mut submitted = false;
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        submitted |= address_field(ui, &mut address);
                    });
                },
            );
            submitted
        };
        assert!(!draw(vec![enter()]));
        draw(vec![]);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("address")));
        assert!(draw(vec![
            egui::Event::Text("https://example.com".into()),
            enter()
        ]));
        assert_eq!(address, "https://example.com");
    }

    #[test]
    fn submission_loads_local_file_and_failure_keeps_previous_page() {
        let ctx = egui::Context::default();
        let mut app = browser();
        app.address = "assets/snapshots/hackernews.html".into();
        app.navigate(&ctx);
        let result = app
            .pending
            .as_ref()
            .unwrap()
            .receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let canvas = result.unwrap();
        assert_eq!(canvas.width, 300);
        assert!(canvas.height > 0);
        app.page = Some(PageView::new(canvas));

        let (sender, receiver) = mpsc::channel();
        app.pending = Some(PendingPage {
            address: "missing.html".into(),
            receiver,
        });
        sender.send(Err("File not found".into())).unwrap();
        app.finish_navigation(&ctx);
        assert!(app.page.is_some());
        assert_eq!(app.error.as_deref(), Some("File not found"));
        assert!(app.pending.is_none());
    }

    #[test]
    fn old_load_result_cannot_replace_new_request() {
        let ctx = egui::Context::default();
        let mut app = browser();
        let (old_sender, old_receiver) = mpsc::channel();
        app.pending = Some(PendingPage {
            address: "old".into(),
            receiver: old_receiver,
        });
        let (new_sender, new_receiver) = mpsc::channel();
        app.pending = Some(PendingPage {
            address: "new".into(),
            receiver: new_receiver,
        });
        assert!(old_sender.send(Err("old failure".into())).is_err());
        new_sender
            .send(Ok(Canvas::new(2, 2, Color::rgb(255, 0, 0))))
            .unwrap();
        app.finish_navigation(&ctx);
        assert!(app.page.is_some());
        assert!(app.error.is_none());
        assert!(app.pending.is_none());
    }

    #[test]
    fn empty_address_does_not_start_loading() {
        let mut app = browser();
        app.address = "   ".into();
        app.navigate(&egui::Context::default());
        assert!(app.pending.is_none());
    }
}
