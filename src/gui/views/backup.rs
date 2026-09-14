//! Main view: what is kept safe, where, how, and when it last happened.

use eframe::egui::{self, Align, FontFamily, FontId, Layout, Sense, Stroke, Ui};

use super::status_color;
use crate::config::BackupMode;
use crate::gui::theme::{SANS_STRONG, palette};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{AeternaApp, View};

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    // --- sources -----------------------------------------------------------------
    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.sources_title);

        if app.config.sources.is_empty() {
            widgets::secondary_text(ui, t.sources_empty);
        }

        let mut remove = None;
        let mut changed = false;
        let row_count = app.config.sources.len();
        for index in 0..row_count {
            let size = app.size_of(&app.config.sources[index].path).cloned();
            let source = &mut app.config.sources[index];
            let width = ui.available_width();
            let (row_rect, _) = ui.allocate_exact_size(egui::vec2(width, 48.0), Sense::hover());
            let mut row = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );

            if row.checkbox(&mut source.enabled, "").changed() {
                changed = true;
            }

            let text_width = (width - 230.0).max(120.0);
            let text_left = row.cursor().left();
            let name_rect = egui::Rect::from_min_size(
                egui::pos2(text_left, row_rect.top() + 5.0),
                egui::vec2(text_width, 20.0),
            );
            let path_rect = egui::Rect::from_min_size(
                egui::pos2(text_left, row_rect.top() + 25.0),
                egui::vec2(text_width, 18.0),
            );
            let name_color = if source.enabled {
                p.text
            } else {
                p.text_secondary
            };
            widgets::paint_cell(
                &row,
                name_rect,
                &source.display_name(),
                FontId::new(15.0, FontFamily::Name(SANS_STRONG.into())),
                name_color,
                Align::Min,
            );
            widgets::paint_cell(
                &row,
                path_rect,
                &source.path.display().to_string(),
                FontId::proportional(12.5),
                p.text_secondary,
                Align::Min,
            );

            row.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, ButtonKind::Quiet, "×", true)
                    .on_hover_text(t.remove_source)
                    .clicked()
                {
                    remove = Some(index);
                }
                ui.add_space(6.0);
                let size_text = match size {
                    Some(Some((bytes, _))) => lang.bytes(bytes),
                    Some(None) => t.source_missing.to_string(),
                    None => t.calculating.to_string(),
                };
                let color = if matches!(size, Some(None)) {
                    p.warning
                } else {
                    p.text_secondary
                };
                ui.label(egui::RichText::new(size_text).color(color).size(13.5));
            });

            if index + 1 < row_count {
                ui.painter().line_segment(
                    [
                        egui::pos2(row_rect.left(), row_rect.bottom() + 3.0),
                        egui::pos2(row_rect.right(), row_rect.bottom() + 3.0),
                    ],
                    Stroke::new(1.0, p.border),
                );
                ui.add_space(6.0);
            }
        }
        if let Some(index) = remove {
            let removed = app.config.sources.remove(index);
            tracing::info!("source removed from list: {}", removed.path.display());
            changed = true;
        }
        if changed {
            app.mark_dirty();
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if widgets::button(ui, ButtonKind::Secondary, t.add_folder, !app.is_busy()).clicked()
                && let Some(folder) = rfd::FileDialog::new().pick_folder()
            {
                app.add_folder(&ctx, folder);
            }
            if widgets::button(ui, ButtonKind::Quiet, t.suggestions, true).clicked() {
                app.view = View::Apps;
            }
        });
        widgets::secondary_text(ui, t.drop_hint);
    });

    ui.add_space(14.0);

    // --- destination and mode ------------------------------------------------------------
    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.destination_title);
        ui.horizontal(|ui| {
            let destination = app.config.destination.display().to_string();
            if destination.is_empty() {
                widgets::secondary_text(ui, t.destination_not_set);
            } else {
                widgets::strong_text(ui, destination);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, ButtonKind::Secondary, t.choose, !app.is_busy()).clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    app.config.destination = folder;
                    app.mark_dirty();
                    app.refresh_snapshots(&ctx);
                }
            });
        });
        match app.destination_reachable() {
            Some(false) => {
                ui.label(
                    egui::RichText::new(t.destination_unreachable)
                        .color(p.warning)
                        .size(13.0),
                );
            }
            Some(true) => {
                if let Some(free) = app.destination_free {
                    widgets::secondary_text(ui, lang.free_space(free));
                }
            }
            None => {}
        }

        ui.add_space(14.0);
        widgets::section_title(ui, t.mode_title);
        let before = app.config.mode;
        ui.radio_value(
            &mut app.config.mode,
            BackupMode::Incremental,
            t.mode_incremental,
        );
        indented_hint(ui, t.mode_incremental_hint);
        ui.radio_value(&mut app.config.mode, BackupMode::Full, t.mode_full);
        indented_hint(ui, t.mode_full_hint);
        if app.config.mode != before {
            app.mark_dirty();
        }
    });

    ui.add_space(12.0);
}

/// Always-visible bar at the bottom: last backup on the left, actions on the right.
pub fn action_bar(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            widgets::secondary_text(ui, t.last_backup_title);
            ui.horizontal(|ui| match app.last_backup() {
                None => {
                    ui.label(t.last_backup_none);
                }
                Some(info) => {
                    let status = info.header.as_ref().map(|h| h.status);
                    widgets::status_dot(ui, status_color(&p, status));
                    match &info.header {
                        Some(header) => {
                            let when =
                                lang.relative_time(header.started_at.with_timezone(&chrono::Local));
                            ui.label(format!("{when} — {}", lang.status(status)));
                        }
                        None => {
                            ui.label(format!("{} — {}", info.id, lang.status(None)));
                        }
                    }
                }
            });
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let ready = !app.is_busy() && app.config.enabled_sources().next().is_some();
            if widgets::button(ui, ButtonKind::Primary, t.back_up_now, ready).clicked() {
                app.plan_backup(&ctx, true);
            }
            if widgets::button(ui, ButtonKind::Secondary, t.preview, ready).clicked() {
                app.plan_backup(&ctx, false);
            }
            if widgets::button(ui, ButtonKind::Quiet, t.restore_ellipsis, true).clicked() {
                app.view = View::Restore;
                app.refresh_snapshots(&ctx);
            }
        });
    });
}

fn indented_hint(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(26.0);
        widgets::secondary_text(ui, text);
    });
    ui.add_space(2.0);
}
