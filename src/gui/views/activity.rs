//! Activity: the history of everything AeternaVault did (kept across
//! sessions), and the technical log of this session.

use eframe::egui::{self, Align, Layout, Ui};
use tracing::Level;

use crate::gui::AeternaApp;
use crate::gui::theme::palette;
use crate::gui::widgets::{self, ButtonKind};
use crate::history::Severity;
use crate::platform;

/// Entries drawn at most; older ones are still in the file.
const SHOWN: usize = 1000;

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);
    let technical_id = egui::Id::new("activity-technical");
    let filter_id = egui::Id::new("activity-filter");
    let mut technical = ui
        .data(|d| d.get_temp::<bool>(technical_id))
        .unwrap_or(false);
    let mut filter = ui
        .data(|d| d.get_temp::<String>(filter_id))
        .unwrap_or_default();

    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| widgets::section_title(ui, t.nav_activity));
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if widgets::button(ui, ButtonKind::Quiet, t.open_log_folder, true).clicked() {
                    platform::open_in_file_manager(&app.paths.log_dir);
                }
                let label = if technical {
                    t.show_history
                } else {
                    t.show_technical_log
                };
                if widgets::button(ui, ButtonKind::Quiet, label, true).clicked() {
                    technical = !technical;
                }
            });
        });
        widgets::secondary_text(
            ui,
            if technical {
                t.technical_log_hint
            } else {
                t.history_hint
            },
        );
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::singleline(&mut filter)
                .hint_text(t.filter_activity)
                .desired_width(280.0),
        );
        ui.add_space(8.0);
        let needle = filter.trim().to_lowercase();
        let height = (ui.available_height() - 20.0).max(240.0);

        if technical {
            let lines = app.log.lines();
            if lines.is_empty() {
                widgets::secondary_text(ui, t.activity_empty);
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("activity-log")
                .max_height(height)
                .auto_shrink([false, true])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in lines
                        .iter()
                        .filter(|l| needle.is_empty() || l.message.to_lowercase().contains(&needle))
                    {
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
                                egui::Label::new(egui::RichText::new(&line.message).size(13.5))
                                    .wrap(),
                            );
                        });
                    }
                });
            return;
        }

        if app.history.is_empty() {
            widgets::secondary_text(ui, t.history_empty);
            return;
        }
        let rows: Vec<(chrono::DateTime<chrono::Local>, String, String, Severity)> = app
            .history
            .iter()
            .rev()
            .map(|entry| {
                let (title, detail, severity) = lang.history_entry(&entry.event);
                (
                    entry.at.with_timezone(&chrono::Local),
                    title,
                    detail,
                    severity,
                )
            })
            .filter(|(_, title, detail, _)| {
                needle.is_empty()
                    || title.to_lowercase().contains(&needle)
                    || detail.to_lowercase().contains(&needle)
            })
            .take(SHOWN)
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("activity-history")
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let mut day = None;
                for (at, title, detail, severity) in &rows {
                    if day != Some(at.date_naive()) {
                        day = Some(at.date_naive());
                        ui.add_space(6.0);
                        widgets::strong_text(ui, lang.weekday_date_only(*at));
                        ui.add_space(2.0);
                    }
                    ui.horizontal_top(|ui| {
                        ui.label(
                            egui::RichText::new(at.format("%H:%M").to_string())
                                .monospace()
                                .color(p.text_secondary),
                        );
                        let color = match severity {
                            Severity::Good => p.success,
                            Severity::Neutral => p.text_secondary,
                            Severity::Warning => p.warning,
                            Severity::Problem => p.error,
                        };
                        ui.add_space(2.0);
                        ui.vertical(|ui| {
                            ui.add_space(6.0);
                            widgets::status_dot(ui, color);
                        });
                        ui.vertical(|ui| {
                            ui.add(egui::Label::new(egui::RichText::new(title).size(14.0)).wrap());
                            if !detail.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(detail)
                                            .size(12.5)
                                            .color(p.text_secondary),
                                    )
                                    .wrap(),
                                );
                            }
                        });
                    });
                    ui.add_space(2.0);
                }
            });
    });

    ui.data_mut(|d| {
        d.insert_temp(technical_id, technical);
        d.insert_temp(filter_id, filter);
    });
}
