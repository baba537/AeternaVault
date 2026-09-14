//! Restore view: pick a backup, pick a target, preview, restore.

use eframe::egui::{self, Align, CornerRadius, FontFamily, FontId, Layout, Sense, Ui};

use super::status_color;
use crate::config::{BackupMode, ConflictPolicy};
use crate::error::EngineError;
use crate::gui::AeternaApp;
use crate::gui::theme::{SANS_STRONG, palette};
use crate::gui::widgets::{self, ButtonKind};

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| widgets::section_title(ui, t.restore_choose_title));
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if widgets::button(ui, ButtonKind::Quiet, t.refresh, true).clicked() {
                    app.refresh_snapshots(&ctx);
                }
            });
        });

        let mut clicked = None;
        match &app.snapshots {
            None => {
                widgets::secondary_text(ui, t.loading);
            }
            Some(Err(EngineError::NoDestination)) => {
                widgets::secondary_text(ui, t.destination_not_set);
            }
            Some(Err(err)) => {
                ui.label(egui::RichText::new(lang.error_message(err)).color(p.warning));
            }
            Some(Ok(list)) if list.is_empty() => {
                widgets::secondary_text(ui, t.restore_none);
            }
            Some(Ok(list)) => {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for info in list {
                            let id = info.qualified_id();
                            let selected = app.restore.selected.as_deref() == Some(id.as_str());
                            let enabled = info.header.is_some();
                            let width = ui.available_width();
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(width, 50.0),
                                if enabled {
                                    Sense::click()
                                } else {
                                    Sense::hover()
                                },
                            );
                            let painter = ui.painter();
                            if selected {
                                painter.rect_filled(rect, CornerRadius::same(4), p.raised);
                                painter.rect_filled(
                                    egui::Rect::from_min_size(
                                        rect.min,
                                        egui::vec2(3.0, rect.height()),
                                    ),
                                    CornerRadius::same(1),
                                    p.accent,
                                );
                            } else if response.hovered() && enabled {
                                painter.rect_filled(
                                    rect,
                                    CornerRadius::same(4),
                                    p.raised.gamma_multiply(0.6),
                                );
                            }

                            let status = info.header.as_ref().map(|h| h.status);
                            painter.circle_filled(
                                egui::pos2(rect.left() + 18.0, rect.center().y),
                                4.0,
                                status_color(&p, status),
                            );

                            let text_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.left() + 34.0, rect.top() + 6.0),
                                egui::pos2(rect.right() - 190.0, rect.top() + 26.0),
                            );
                            let detail_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.left() + 34.0, rect.top() + 27.0),
                                egui::pos2(rect.right() - 190.0, rect.bottom() - 5.0),
                            );
                            let status_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.right() - 185.0, rect.top()),
                                egui::pos2(rect.right() - 12.0, rect.bottom()),
                            );

                            let (title, detail) = match &info.header {
                                Some(h) => {
                                    let mode = match h.mode {
                                        BackupMode::Incremental => t.mode_incremental,
                                        BackupMode::Full => t.mode_full,
                                    };
                                    let mut detail = format!(
                                        "{mode} · {} · {}",
                                        lang.files(h.stats.files),
                                        lang.bytes(h.stats.bytes)
                                    );
                                    if !info.computer.eq_ignore_ascii_case(&app.computer) {
                                        detail.push_str(&format!(
                                            " · {}",
                                            lang.computer_label(&info.computer)
                                        ));
                                    }
                                    (
                                        lang.weekday_date(
                                            h.started_at.with_timezone(&chrono::Local),
                                        ),
                                        detail,
                                    )
                                }
                                None => (info.id.clone(), lang.computer_label(&info.computer)),
                            };
                            widgets::paint_cell(
                                ui,
                                text_rect,
                                &title,
                                FontId::new(15.0, FontFamily::Name(SANS_STRONG.into())),
                                if enabled { p.text } else { p.text_secondary },
                                Align::Min,
                            );
                            widgets::paint_cell(
                                ui,
                                detail_rect,
                                &detail,
                                FontId::proportional(12.5),
                                p.text_secondary,
                                Align::Min,
                            );
                            widgets::paint_cell(
                                ui,
                                status_rect,
                                lang.status(status),
                                FontId::proportional(13.0),
                                p.text_secondary,
                                Align::Max,
                            );

                            if response.clicked() {
                                clicked = Some(id);
                            }
                        }
                    });
            }
        }
        if let Some(id) = clicked {
            app.restore.selected = Some(id);
        }
    });

    ui.add_space(14.0);

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.restore_target_title);
        ui.radio_value(&mut app.restore.to_folder, false, t.restore_original);
        ui.horizontal(|ui| {
            ui.radio_value(&mut app.restore.to_folder, true, t.restore_folder);
            if app.restore.to_folder {
                ui.add_space(8.0);
                if widgets::button(ui, ButtonKind::Secondary, t.choose, true).clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    app.restore.folder = Some(folder);
                }
            }
        });
        if app.restore.to_folder {
            ui.horizontal(|ui| {
                ui.add_space(26.0);
                match &app.restore.folder {
                    Some(folder) => widgets::strong_text(ui, folder.display().to_string()),
                    None => widgets::secondary_text(ui, t.choose_folder_first),
                };
            });
        }

        ui.add_space(8.0);
        egui::CollapsingHeader::new(egui::RichText::new(t.advanced).color(p.text_secondary))
            .id_salt("restore-advanced")
            .show(ui, |ui| {
                let before = (
                    app.config.advanced.restore_conflict,
                    app.config.advanced.verify_on_restore,
                );
                widgets::secondary_text(ui, t.conflict_title);
                let conflict = &mut app.config.advanced.restore_conflict;
                ui.radio_value(conflict, ConflictPolicy::ReplaceChanged, t.conflict_replace);
                ui.radio_value(conflict, ConflictPolicy::KeepNewer, t.conflict_newer);
                ui.radio_value(conflict, ConflictPolicy::KeepExisting, t.conflict_keep);
                ui.add_space(6.0);
                ui.checkbox(
                    &mut app.config.advanced.verify_on_restore,
                    t.verify_checksums,
                );
                if before
                    != (
                        app.config.advanced.restore_conflict,
                        app.config.advanced.verify_on_restore,
                    )
                {
                    app.mark_dirty();
                }
            });
    });

    ui.add_space(12.0);
}

/// Always-visible bar at the bottom: selected backup on the left, actions on the right.
pub fn action_bar(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            widgets::secondary_text(ui, t.restore_selected);
            let selected = app
                .selected_snapshot()
                .and_then(|s| s.header.as_ref())
                .map(|h| lang.weekday_date(h.started_at.with_timezone(&chrono::Local)));
            ui.label(selected.unwrap_or_else(|| "—".to_string()));
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let ready = !app.is_busy()
                && app.selected_snapshot().is_some_and(|s| s.header.is_some())
                && (!app.restore.to_folder || app.restore.folder.is_some());
            if widgets::button(ui, ButtonKind::Primary, t.restore, ready).clicked() {
                app.plan_restore(&ctx, true);
            }
            if widgets::button(ui, ButtonKind::Secondary, t.preview, ready).clicked() {
                app.plan_restore(&ctx, false);
            }
        });
    });
}
