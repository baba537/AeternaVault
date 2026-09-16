//! Backups: every backup at the destination, with checking, browsing, copying
//! out, moving and deleting, plus the rules for removing old backups.

use eframe::egui::{self, Align, CornerRadius, FontFamily, FontId, Layout, Sense, Ui};

use super::status_color;
use crate::config::BackupMode;
use crate::engine::snapshots::{Location, SnapshotInfo};
use crate::error::EngineError;
use crate::gui::theme::{SANS_STRONG, palette};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{AeternaApp, AfterUnlock};
use crate::platform;

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    list_card(app, ui);
    ui.add_space(14.0);
    if app.viewer.is_none() {
        retention_card(app, ui);
        ui.add_space(14.0);
    }
    if app.vault.header.is_some() {
        super::settings::encryption_info_card(app, ui);
        ui.add_space(12.0);
    }
}

/// "Incremental · 1,234 files · 2.3 GB · partly encrypted · LAPTOP"
pub fn snapshot_details(app: &AeternaApp, info: &SnapshotInfo) -> (String, String) {
    let t = app.lang.t();
    let lang = app.lang;
    match &info.header {
        Some(h) => {
            let mode = match h.mode {
                BackupMode::Incremental => t.mode_incremental,
                BackupMode::Full => t.mode_full,
            };
            let mut detail = mode.to_string();
            match info.total_stats() {
                Some(stats) => detail.push_str(&format!(
                    " · {} · {}",
                    lang.files(stats.files),
                    lang.bytes(stats.bytes)
                )),
                None => detail.push_str(&format!(" · {}", t.partly_locked)),
            }
            if info.is_split() {
                detail.push_str(&format!(" · {}", t.partly_encrypted_label));
            } else if h.encrypted {
                detail.push_str(&format!(" · {}", t.encrypted_label));
            }
            if !info.computer.eq_ignore_ascii_case(&app.computer) {
                detail.push_str(&format!(" · {}", lang.computer_label(&info.computer)));
            }
            (
                lang.weekday_date(h.started_at.with_timezone(&chrono::Local)),
                detail,
            )
        }
        None if info.is_locked() => (info.id.clone(), t.locked_backup.to_string()),
        None => (info.id.clone(), lang.computer_label(&info.computer)),
    }
}

fn list_card(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| widgets::section_title(ui, t.backups_title));
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if widgets::button(ui, ButtonKind::Quiet, t.refresh, !app.is_busy()).clicked() {
                    app.refresh_vault();
                    app.refresh_snapshots(&ctx);
                }
            });
        });
        widgets::secondary_text(ui, app.config.destination.display().to_string());
        let (count, bytes) = app
            .all_snapshots()
            .iter()
            .fold((0usize, 0u64), |(n, b), s| {
                (n + 1, b + s.total_stats().map_or(0, |st| st.copied_bytes))
            });
        let mut summary = lang.backups_count(count);
        if bytes > 0 {
            summary.push_str(&format!(" · {}", lang.stored_bytes(bytes)));
        }
        if let Some(free) = app.destination_free {
            summary.push_str(&format!(" · {}", lang.free_space(free)));
        }
        widgets::secondary_text(ui, summary);
        ui.add_space(8.0);

        let locked = app.all_snapshots().iter().any(SnapshotInfo::needs_unlock);
        if locked {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(t.some_backups_locked).color(p.text_secondary));
                if widgets::button(ui, ButtonKind::Secondary, t.unlock, !app.is_busy()).clicked() {
                    app.open_unlock(AfterUnlock::RefreshSnapshots);
                }
            });
            ui.add_space(6.0);
        }

        match &app.snapshots {
            None => {
                widgets::secondary_text(ui, t.loading);
                return;
            }
            Some(Err(EngineError::NoDestination)) => {
                widgets::secondary_text(ui, t.destination_not_set);
                return;
            }
            Some(Err(err)) => {
                ui.label(egui::RichText::new(lang.error_message(err)).color(p.warning));
                return;
            }
            Some(Ok(list)) if list.is_empty() => {
                widgets::secondary_text(ui, t.restore_none);
                return;
            }
            Some(Ok(_)) => {}
        }

        let rows: Vec<(
            String,
            String,
            String,
            Option<crate::engine::manifest::SnapshotStatus>,
            bool,
        )> = app
            .all_snapshots()
            .iter()
            .map(|s| {
                let (title, detail) = snapshot_details(app, s);
                (
                    s.qualified_id(),
                    title,
                    detail,
                    s.header.as_ref().map(|h| h.status),
                    s.needs_unlock(),
                )
            })
            .collect();

        let mut toggled = None;
        egui::ScrollArea::vertical()
            .id_salt("backups-list")
            .max_height(420.0)
            .min_scrolled_height(300.0)
            .auto_shrink([false, true])
            .show_rows(ui, 52.0, rows.len(), |ui, range| {
                for (id, title, detail, status, locked) in &rows[range] {
                    let checked = app.manage.checked.contains(id);
                    let width = ui.available_width();
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(width, 52.0), Sense::click());
                    let painter = ui.painter();
                    if checked {
                        painter.rect_filled(rect, CornerRadius::same(4), p.raised);
                        painter.rect_filled(
                            egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                            CornerRadius::same(1),
                            p.accent,
                        );
                    } else if response.hovered() {
                        painter.rect_filled(
                            rect,
                            CornerRadius::same(4),
                            p.raised.gamma_multiply(0.6),
                        );
                    }
                    let mut row = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect.shrink2(egui::vec2(10.0, 0.0)))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    let mut on = checked;
                    let checkbox = row.add(egui::Checkbox::without_text(&mut on));
                    checkbox.widget_info(|| {
                        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, title)
                    });
                    if checkbox.changed() || response.clicked() {
                        toggled = Some(id.clone());
                    }
                    let left = row.cursor().left() + 8.0;
                    let dot = if *locked {
                        p.accent
                    } else {
                        status_color(&p, *status)
                    };
                    ui.painter()
                        .circle_filled(egui::pos2(left + 4.0, rect.center().y), 4.0, dot);
                    let text_left = left + 18.0;
                    widgets::paint_cell(
                        ui,
                        egui::Rect::from_min_max(
                            egui::pos2(text_left, rect.top() + 7.0),
                            egui::pos2(rect.right() - 170.0, rect.top() + 27.0),
                        ),
                        title,
                        FontId::new(15.0, FontFamily::Name(SANS_STRONG.into())),
                        p.text,
                        Align::Min,
                    );
                    widgets::paint_cell(
                        ui,
                        egui::Rect::from_min_max(
                            egui::pos2(text_left, rect.top() + 28.0),
                            egui::pos2(rect.right() - 170.0, rect.bottom() - 5.0),
                        ),
                        detail,
                        FontId::proportional(12.5),
                        p.text_secondary,
                        Align::Min,
                    );
                    let status_text = if *locked {
                        t.selected_backup_locked
                    } else {
                        lang.status(*status)
                    };
                    widgets::paint_cell(
                        ui,
                        egui::Rect::from_min_max(
                            egui::pos2(rect.right() - 165.0, rect.top()),
                            egui::pos2(rect.right() - 12.0, rect.bottom()),
                        ),
                        status_text,
                        FontId::proportional(13.0),
                        p.text_secondary,
                        Align::Max,
                    );
                }
            });
        if let Some(id) = toggled
            && !app.manage.checked.remove(&id)
        {
            app.manage.checked.insert(id);
        }
        // Forget ticks of backups that are gone.
        let existing: Vec<String> = app
            .all_snapshots()
            .iter()
            .map(SnapshotInfo::qualified_id)
            .collect();
        app.manage.checked.retain(|id| existing.contains(id));

        ui.add_space(10.0);
        let checked = app.checked_snapshots();
        let single = (checked.len() == 1).then(|| checked[0].clone());
        let ready = !app.is_busy();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let one = single.is_some() && ready;
            if widgets::button(ui, ButtonKind::Secondary, t.browse, one).clicked()
                && let Some(s) = single.clone()
            {
                app.browse_snapshot(&ctx, s);
            }
            if widgets::button(ui, ButtonKind::Secondary, t.check_backup, one).clicked()
                && let Some(s) = single.clone()
            {
                app.verify_snapshot(&ctx, s);
            }
            if widgets::button(ui, ButtonKind::Secondary, t.copy_files_to, one).clicked()
                && let Some(s) = single.clone()
            {
                app.ask_extract(s, None);
            }
            if app.viewer.is_none()
                && widgets::button(ui, ButtonKind::Secondary, t.move_to, one).clicked()
                && let Some(s) = single.clone()
            {
                app.ask_transfer(s);
            }
            if widgets::button(ui, ButtonKind::Quiet, t.open_folder, one).clicked()
                && let Some(s) = &single
            {
                let folder = match &s.location {
                    Location::Plain { dir, .. } | Location::Legacy { dir, .. } => dir.clone(),
                    Location::Encrypted { destination, .. } => {
                        crate::engine::vault::vault_dir(destination)
                    }
                };
                platform::open_in_file_manager(&folder);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if checked.len() > 1 {
                    lang.delete_n(checked.len())
                } else {
                    t.delete_backup.to_string()
                };
                if app.viewer.is_none()
                    && widgets::button(
                        ui,
                        ButtonKind::Secondary,
                        &label,
                        ready && !checked.is_empty(),
                    )
                    .clicked()
                {
                    app.ask_delete(checked.clone());
                }
            });
        });
        if checked.is_empty() {
            widgets::secondary_text(ui, t.backups_select_hint);
        }
    });
}

fn retention_card(app: &mut AeternaApp, ui: &mut Ui) {
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.retention_title);
        let before = app.config.retention.clone();
        ui.horizontal(|ui| {
            let mut on = app.config.retention.enabled;
            if widgets::toggle(ui, &mut on, true, t.retention_auto) {
                app.config.retention.enabled = on;
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                widgets::strong_text(ui, t.retention_auto);
                widgets::secondary_text(ui, t.retention_hint);
            });
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            let retention = &mut app.config.retention;
            ui.label(t.retention_keep);
            for (value, label) in [
                (&mut retention.keep_last, t.retention_newest),
                (&mut retention.keep_daily, t.retention_days),
                (&mut retention.keep_weekly, t.retention_weeks),
                (&mut retention.keep_monthly, t.retention_months),
            ] {
                ui.add(egui::DragValue::new(value).range(0..=999).speed(0.2));
                ui.label(egui::RichText::new(label).color(p.text_secondary));
                ui.add_space(6.0);
            }
        });
        if app.config.retention != before {
            app.mark_dirty();
        }

        if app.config.retention.enabled {
            ui.add_space(8.0);
            forecast_list(app, ui);
        }

        ui.add_space(6.0);
        let candidates = app.retention_candidates();
        ui.horizontal_wrapped(|ui| {
            widgets::secondary_text(ui, lang.retention_preview(candidates.len()));
            if !candidates.is_empty()
                && widgets::button(ui, ButtonKind::Quiet, t.clean_up_now, !app.is_busy()).clicked()
            {
                app.ask_clean_up();
            }
        });
    });
}

/// A table of the backups with the day each one is expected to be removed.
fn forecast_list(app: &mut AeternaApp, ui: &mut Ui) {
    use crate::engine::retention::Removal;
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);
    let forecast = app.retention_forecast();
    let rows: Vec<(String, String, bool)> = forecast
        .removals
        .iter()
        .filter(|(_, removal)| *removal != Removal::NotAffected)
        .filter_map(|(id, removal)| {
            let snapshot = app
                .all_snapshots()
                .iter()
                .find(|s| &s.qualified_id() == id)?;
            let (title, _) = snapshot_details(app, snapshot);
            let (text, soon) = match removal {
                Removal::NextBackup => (t.removed_next_backup.to_string(), true),
                Removal::After(at) => {
                    (lang.removed_around(at.with_timezone(&chrono::Local)), false)
                }
                Removal::NotWithin => (t.kept_long.to_string(), false),
                Removal::NotAffected => return None,
            };
            Some((title, text, soon))
        })
        .collect();

    egui::CollapsingHeader::new(egui::RichText::new(t.forecast_title).color(p.text))
        .id_salt("retention-forecast")
        .default_open(true)
        .show(ui, |ui| {
            widgets::secondary_text(
                ui,
                if forecast.assumed_daily {
                    t.forecast_assumes_daily
                } else {
                    t.forecast_assumes_jobs
                },
            );
            if rows.is_empty() {
                widgets::secondary_text(ui, t.forecast_empty);
                return;
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("retention-forecast-rows")
                .max_height(260.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("retention-forecast-grid")
                        .num_columns(2)
                        .spacing([28.0, 4.0])
                        .striped(false)
                        .show(ui, |ui| {
                            for (title, text, soon) in &rows {
                                ui.label(egui::RichText::new(title).size(13.5));
                                ui.label(egui::RichText::new(text).size(13.5).color(if *soon {
                                    p.warning
                                } else {
                                    p.text_secondary
                                }));
                                ui.end_row();
                            }
                        });
                });
        });
}
