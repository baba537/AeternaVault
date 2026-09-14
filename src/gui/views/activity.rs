//! Activity: the most recent log entries of this session.

use eframe::egui::{self, Align, Layout, Ui};
use tracing::Level;

use crate::gui::AeternaApp;
use crate::gui::theme::palette;
use crate::gui::widgets::{self, ButtonKind};
use crate::platform;

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    let t = app.lang.t();
    let p = *palette(ui);
    let lines = app.log.lines();

    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| widgets::section_title(ui, t.nav_activity));
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if widgets::button(ui, ButtonKind::Quiet, t.open_log_folder, true).clicked() {
                    platform::open_in_file_manager(&app.paths.log_dir);
                }
            });
        });

        if lines.is_empty() {
            widgets::secondary_text(ui, t.activity_empty);
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt("activity")
            .max_height((ui.ctx().content_rect().height() - 300.0).max(240.0))
            .auto_shrink([false, true])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &lines {
                    ui.horizontal_top(|ui| {
                        ui.label(
                            egui::RichText::new(line.time.format("%H:%M:%S").to_string())
                                .monospace()
                                .color(p.text_secondary),
                        );
                        let (label, color) = match line.level {
                            Level::ERROR => ("ERROR", p.error),
                            Level::WARN => ("WARN ", p.warning),
                            Level::INFO => ("INFO ", p.text_secondary),
                            _ => ("DEBUG", p.text_secondary),
                        };
                        ui.label(egui::RichText::new(label).monospace().color(color));
                        ui.add(
                            egui::Label::new(egui::RichText::new(&line.message).size(13.5)).wrap(),
                        );
                    });
                }
            });
    });
}
