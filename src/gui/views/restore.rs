//! Restore view: pick a backup (unlocking encrypted ones), choose what to
//! restore and where, preview, restore.

use eframe::egui::{self, Align, CornerRadius, FontFamily, FontId, Layout, Sense, Ui};

use super::status_color;
use crate::config::{BackupMode, ConflictPolicy};
use crate::engine::manifest::{APPS_DIR, PROGRAM_LIST_FILE};
use crate::engine::snapshots::Location;
use crate::error::EngineError;
use crate::gui::theme::{SANS_STRONG, palette};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{AeternaApp, AfterUnlock};
use crate::platform;

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
                    app.refresh_vault();
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
                    .max_height(320.0)
                    .min_scrolled_height(260.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for info in list {
                            let id = info.qualified_id();
                            let selected = app.restore.selected.as_deref() == Some(id.as_str());
                            let selectable = info.header.is_some() || info.is_locked();
                            let width = ui.available_width();
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(width, 50.0),
                                if selectable {
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
                            } else if response.hovered() && selectable {
                                painter.rect_filled(
                                    rect,
                                    CornerRadius::same(4),
                                    p.raised.gamma_multiply(0.6),
                                );
                            }

                            let status = info.header.as_ref().map(|h| h.status);
                            let dot = if info.is_locked() {
                                p.accent
                            } else {
                                status_color(&p, status)
                            };
                            painter.circle_filled(
                                egui::pos2(rect.left() + 18.0, rect.center().y),
                                4.0,
                                dot,
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
                                    if h.encrypted {
                                        detail.push_str(&format!(" · {}", t.encrypted_label));
                                    }
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
                                None if info.is_locked() => {
                                    (info.id.clone(), t.locked_backup.to_string())
                                }
                                None => (info.id.clone(), lang.computer_label(&info.computer)),
                            };
                            widgets::paint_cell(
                                ui,
                                text_rect,
                                &title,
                                FontId::new(15.0, FontFamily::Name(SANS_STRONG.into())),
                                if selectable { p.text } else { p.text_secondary },
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
                            let status_text = if info.is_locked() {
                                t.selected_backup_locked
                            } else {
                                lang.status(status)
                            };
                            widgets::paint_cell(
                                ui,
                                status_rect,
                                status_text,
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
            if app.restore.selected.as_deref() != Some(id.as_str()) {
                app.restore.skip.clear();
            }
            app.restore.selected = Some(id);
        }
    });

    ui.add_space(14.0);
    what_to_restore(app, ui);

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

/// Checkboxes for the folders and applications contained in the selected backup.
fn what_to_restore(app: &mut AeternaApp, ui: &mut Ui) {
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);
    let Some(snapshot) = app.selected_snapshot().cloned() else {
        return;
    };
    let Some(header) = snapshot.header.clone() else {
        return;
    };

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.restore_what_title);

        let folders: Vec<_> = header
            .sources
            .iter()
            .filter(|s| s.app.is_none() && !s.extra)
            .collect();
        if !folders.is_empty() {
            widgets::secondary_text(ui, t.restore_folders);
            ui.horizontal_wrapped(|ui| {
                for source in folders {
                    let mut on = !app.restore.skip.contains(&source.key);
                    if ui.checkbox(&mut on, &source.name).changed() {
                        if on {
                            app.restore.skip.remove(&source.key);
                        } else {
                            app.restore.skip.insert(source.key.clone());
                        }
                    }
                }
            });
            ui.add_space(6.0);
        }

        let mut app_ids: Vec<(String, String)> = header
            .sources
            .iter()
            .filter_map(|s| s.app.clone())
            .chain(header.registry.iter().map(|r| r.app.clone()))
            .map(|id| {
                let name = app
                    .catalog
                    .get(&id)
                    .map(|a| a.display_name(lang).to_string())
                    .or_else(|| {
                        header
                            .registry
                            .iter()
                            .find(|r| r.app == id)
                            .map(|r| r.app_name.clone())
                    })
                    .unwrap_or_else(|| id.clone());
                (id, name)
            })
            .collect();
        app_ids.sort_by_key(|a| a.1.to_lowercase());
        app_ids.dedup_by(|a, b| a.0 == b.0);

        if !app_ids.is_empty() {
            widgets::secondary_text(ui, t.apps_card_title);
            ui.horizontal_wrapped(|ui| {
                for (id, name) in &app_ids {
                    let key = format!("app:{id}");
                    let mut on = !app.restore.skip.contains(&key);
                    let running = app
                        .catalog
                        .get(id)
                        .is_some_and(|a| a.is_running(&app.running));
                    let label = if running {
                        egui::RichText::new(format!("{name} ({})", t.app_open)).color(p.warning)
                    } else {
                        egui::RichText::new(name.as_str())
                    };
                    if ui.checkbox(&mut on, label).changed() {
                        if on {
                            app.restore.skip.remove(&key);
                        } else {
                            app.restore.skip.insert(key);
                        }
                    }
                }
            });
            if !header.registry.is_empty() && !app.restore.to_folder {
                widgets::secondary_text(ui, t.restore_registry_note);
            }
        }

        // The program list helps reinstalling on a new computer.
        let has_list = header.sources.iter().any(|s| s.extra);
        if has_list && let Location::Plain { dir, .. } = &snapshot.location {
            let list = dir.join(APPS_DIR).join(PROGRAM_LIST_FILE);
            if list.is_file() {
                ui.add_space(6.0);
                if widgets::button(ui, ButtonKind::Quiet, t.open_program_list, true).clicked() {
                    platform::open_in_editor(&list);
                }
            }
        }
    });
    ui.add_space(14.0);
}

/// Always-visible bar at the bottom: selected backup on the left, actions on the right.
pub fn action_bar(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;

    ui.horizontal(|ui| {
        let selected = app.selected_snapshot().cloned();
        ui.vertical(|ui| {
            widgets::secondary_text(ui, t.restore_selected);
            let text = match &selected {
                Some(s) => match &s.header {
                    Some(h) => lang.weekday_date(h.started_at.with_timezone(&chrono::Local)),
                    None => s.id.clone(),
                },
                None => "—".to_string(),
            };
            ui.label(text);
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let locked = selected.as_ref().is_some_and(|s| s.is_locked());
            if locked {
                if widgets::button(ui, ButtonKind::Primary, t.unlock, !app.is_busy()).clicked() {
                    app.open_unlock(AfterUnlock::RefreshSnapshots);
                }
                return;
            }
            let ready = !app.is_busy()
                && selected.as_ref().is_some_and(|s| s.header.is_some())
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
