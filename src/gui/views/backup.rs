//! Main view: what is kept safe (folders and application settings), where,
//! how, and when it happens automatically.

use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, Pos2, Sense, Stroke, Ui,
};

use super::status_color;
use crate::config::{BackupMode, Frequency, Weekday};
use crate::engine::selection::{self, CheckState};
use crate::gui::theme::{SANS_STRONG, palette};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{AeternaApp, View};
use crate::platform::scheduler;

const TREE_ROW: f32 = 26.0;
const TREE_LIMIT: usize = 400;

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    folders_card(app, ui);
    ui.add_space(14.0);
    apps_card(app, ui);
    ui.add_space(14.0);
    destination_card(app, ui);
    ui.add_space(14.0);
    schedule_card(app, ui);
    ui.add_space(12.0);
}

// ---------------------------------------------------------------------------
// Folders
// ---------------------------------------------------------------------------

fn folders_card(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

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
            let path = app.config.sources[index].path.clone();
            let expanded = app.tree.expanded_sources.contains(&path);
            let width = ui.available_width();
            let (row_rect, row_response) =
                ui.allocate_exact_size(egui::vec2(width, 48.0), Sense::click());
            let mut row = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );

            // Chevron to open the contents tree.
            let (chevron_rect, chevron) =
                row.allocate_exact_size(egui::vec2(20.0, 30.0), Sense::click());
            paint_chevron(
                &row,
                chevron_rect,
                expanded,
                chevron.hovered() || row_response.hovered(),
            );
            let source_name = app.config.sources[index].display_name();
            chevron.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("{}: {source_name}", t.choose_contents),
                )
            });
            let chevron = chevron.on_hover_text(t.choose_contents);

            let source = &mut app.config.sources[index];
            let mixed = source.enabled && source.is_partial();
            let checkbox =
                row.add(egui::Checkbox::new(&mut source.enabled, "").indeterminate(mixed));
            let enabled_now = source.enabled;
            checkbox.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Checkbox,
                    true,
                    enabled_now,
                    &source_name,
                )
            });
            if checkbox.changed() {
                changed = true;
            }

            let text_width = (width - 250.0).max(120.0);
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

            let name_clicked = row_response.clicked()
                && row_response
                    .interact_pointer_pos()
                    .is_some_and(|pos| pos.x > text_left && pos.x < text_left + text_width);
            if chevron.clicked() || name_clicked {
                if expanded {
                    app.tree.expanded_sources.remove(&path);
                } else {
                    app.tree.expanded_sources.insert(path.clone());
                }
            }

            if expanded {
                ui.add_space(4.0);
                tree_panel(app, ui, index);
            }

            if index + 1 < row_count {
                let y = ui.cursor().top() + 3.0;
                ui.painter().line_segment(
                    [
                        egui::pos2(row_rect.left(), y),
                        egui::pos2(row_rect.right(), y),
                    ],
                    Stroke::new(1.0, p.border),
                );
                ui.add_space(6.0);
            }
        }
        if let Some(index) = remove {
            let removed = app.config.sources.remove(index);
            app.tree.expanded_sources.remove(&removed.path);
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
        });
        widgets::secondary_text(ui, t.drop_hint);
    });
}

fn paint_chevron(ui: &Ui, rect: egui::Rect, open: bool, hovered: bool) {
    let p = palette(ui);
    let c = rect.center();
    let color = if hovered { p.text } else { p.text_secondary };
    let points = if open {
        vec![
            c + egui::vec2(-4.5, -2.0),
            c + egui::vec2(0.0, 2.5),
            c + egui::vec2(4.5, -2.0),
        ]
    } else {
        vec![
            c + egui::vec2(-2.0, -4.5),
            c + egui::vec2(2.5, 0.0),
            c + egui::vec2(-2.0, 4.5),
        ]
    };
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.5, color)));
}

/// The "choose contents" tree below a source row.
fn tree_panel(app: &mut AeternaApp, ui: &mut Ui, index: usize) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let p = *palette(ui);
    let root = app.config.sources[index].path.clone();
    let mut changed = false;

    Frame::new()
        .fill(p.background)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let source = &mut app.config.sources[index];
                if widgets::button(ui, ButtonKind::Quiet, t.select_all, true).clicked() {
                    selection::select_all(&mut source.include_paths, &mut source.exclude_paths);
                    changed = true;
                }
                if widgets::button(ui, ButtonKind::Quiet, t.select_none, true).clicked() {
                    selection::select_none(&mut source.include_paths, &mut source.exclude_paths);
                    changed = true;
                }
                if source.is_partial() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        widgets::secondary_text(ui, t.partial_selection);
                    });
                }
            });

            if !root.is_dir() {
                widgets::secondary_text(ui, t.source_missing);
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt(("tree", root.display().to_string()))
                .max_height(300.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if render_dir(app, ui, index, &root, "", 0) {
                        changed = true;
                    }
                });
        });

    if changed {
        app.mark_dirty();
        app.invalidate_size(&ctx, &root);
    }
}

/// Draws the entries of one folder; returns `true` if the selection changed.
fn render_dir(
    app: &mut AeternaApp,
    ui: &mut Ui,
    index: usize,
    root: &Path,
    rel_dir: &str,
    depth: usize,
) -> bool {
    let p = *palette(ui);
    let t = app.lang.t();
    let lang = app.lang;
    let dir: PathBuf = if rel_dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel_dir.replace('/', "\\"))
    };
    let entries = app.tree_listing(&dir);
    let mut changed = false;

    if entries.is_empty() && depth == 0 {
        widgets::secondary_text(ui, t.folder_empty);
    }

    for entry in entries.iter().take(TREE_LIMIT) {
        let rel = if rel_dir.is_empty() {
            entry.name.clone()
        } else {
            format!("{rel_dir}/{}", entry.name)
        };
        let open_key = (root.to_path_buf(), rel.clone());
        let is_open = entry.is_dir && app.tree.open_dirs.contains(&open_key);
        let state = app.config.sources[index].selection().state(&rel);

        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, TREE_ROW), Sense::hover());
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(3), p.raised);
        }
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.add_space(depth as f32 * 18.0);

        let (arrow_rect, arrow) =
            row.allocate_exact_size(egui::vec2(18.0, TREE_ROW), Sense::click());
        if entry.is_dir {
            paint_chevron(&row, arrow_rect, is_open, arrow.hovered());
            if arrow.clicked() {
                if is_open {
                    app.tree.open_dirs.remove(&open_key);
                } else {
                    app.tree.open_dirs.insert(open_key.clone());
                }
            }
        }

        let mut checked = state == CheckState::Checked;
        let checkbox = row
            .add(egui::Checkbox::new(&mut checked, "").indeterminate(state == CheckState::Mixed));
        checkbox.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                true,
                state == CheckState::Checked,
                &entry.name,
            )
        });
        if checkbox.clicked() {
            let source = &mut app.config.sources[index];
            selection::toggle(&mut source.include_paths, &mut source.exclude_paths, &rel);
            changed = true;
        }

        paint_entry_icon(&row, entry.is_dir);
        let name_color = if state == CheckState::Unchecked {
            p.text_secondary
        } else {
            p.text
        };
        let left = row.cursor().left() + 22.0;
        let name_rect = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(rect.right() - 90.0, rect.bottom()),
        );
        widgets::paint_cell(
            ui,
            name_rect,
            &entry.name,
            FontId::proportional(13.5),
            name_color,
            Align::Min,
        );
        if !entry.is_dir {
            let size_rect = egui::Rect::from_min_max(
                egui::pos2(rect.right() - 86.0, rect.top()),
                egui::pos2(rect.right() - 6.0, rect.bottom()),
            );
            widgets::paint_cell(
                ui,
                size_rect,
                &lang.bytes(entry.size),
                FontId::proportional(12.5),
                p.text_secondary,
                Align::Max,
            );
        }

        if is_open && render_dir(app, ui, index, root, &rel, depth + 1) {
            changed = true;
        }
    }
    if entries.len() > TREE_LIMIT {
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 18.0 + 40.0);
            widgets::secondary_text(ui, lang.more_items(entries.len() - TREE_LIMIT));
        });
    }
    changed
}

/// Minimal line icons: a folder tab or a document with a folded corner.
fn paint_entry_icon(ui: &Ui, is_dir: bool) {
    let p = palette(ui);
    let origin = ui.cursor().left_top() + egui::vec2(2.0, (TREE_ROW - 12.0) / 2.0);
    let stroke = Stroke::new(1.2, p.text_secondary);
    let painter = ui.painter();
    if is_dir {
        let body = egui::Rect::from_min_size(origin + egui::vec2(0.0, 2.0), egui::vec2(15.0, 10.0));
        painter.rect_stroke(
            body,
            CornerRadius::same(2),
            stroke,
            egui::StrokeKind::Inside,
        );
        painter.line_segment(
            [origin + egui::vec2(1.0, 2.0), origin + egui::vec2(6.0, 2.0)],
            stroke,
        );
        painter.line_segment(
            [origin + egui::vec2(1.5, 0.5), origin + egui::vec2(5.5, 0.5)],
            stroke,
        );
    } else {
        let points = vec![
            origin + egui::vec2(2.0, 0.0),
            origin + egui::vec2(9.0, 0.0),
            origin + egui::vec2(12.0, 3.0),
            origin + egui::vec2(12.0, 12.0),
            origin + egui::vec2(2.0, 12.0),
        ];
        painter.add(egui::Shape::closed_line(points, stroke));
        let _ = Pos2::ZERO;
    }
}

// ---------------------------------------------------------------------------
// Application settings
// ---------------------------------------------------------------------------

fn apps_card(app: &mut AeternaApp, ui: &mut Ui) {
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.apps_card_title);

        let loaded = !app.apps.status.is_empty();
        let chosen: Vec<(String, Option<u64>)> = app
            .catalog
            .apps
            .iter()
            .filter(|a| app.config.app_enabled(&a.id) && (!loaded || app.app_detected(&a.id)))
            .map(|a| {
                (
                    a.display_name(lang).to_string(),
                    app.apps.status.get(&a.id).and_then(|s| s.bytes),
                )
            })
            .collect();

        if chosen.is_empty() {
            widgets::secondary_text(ui, t.apps_card_empty);
        } else {
            let total = chosen
                .iter()
                .map(|(_, b)| *b)
                .try_fold(0u64, |sum, b| b.map(|b| sum + b));
            widgets::strong_text(ui, lang.apps_summary(chosen.len(), total));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for (name, _) in chosen.iter().take(16) {
                    pill(ui, name);
                }
                if chosen.len() > 16 {
                    widgets::secondary_text(ui, lang.more_items(chosen.len() - 16));
                }
            });
        }

        let running = app.running_chosen_apps(lang);
        if !running.is_empty() {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(lang.running_warning(&running)).color(p.warning),
                )
                .wrap(),
            );
        }

        ui.add_space(10.0);
        if widgets::button(ui, ButtonKind::Secondary, t.choose_apps, true).clicked() {
            app.view = View::Apps;
        }
    });
}

fn pill(ui: &mut Ui, text: &str) {
    let p = *palette(ui);
    Frame::new()
        .fill(p.raised)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(11))
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(13.0).color(p.text));
        });
}

// ---------------------------------------------------------------------------
// Destination, encryption and kind of backup
// ---------------------------------------------------------------------------

fn destination_card(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

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
                    app.destination_changed(&ctx);
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

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let mut on = app.config.encryption.enabled;
            if widgets::toggle(ui, &mut on, !app.is_busy(), t.encrypt_backups) {
                app.set_encryption(on);
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                widgets::strong_text(ui, t.encrypt_backups);
                let hint = if app.config.encryption.enabled {
                    t.encrypt_hint_on
                } else {
                    t.encrypt_hint_off
                };
                widgets::secondary_text(ui, hint);
            });
        });

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
}

// ---------------------------------------------------------------------------
// Automatic backups
// ---------------------------------------------------------------------------

fn schedule_card(app: &mut AeternaApp, ui: &mut Ui) {
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    widgets::card(ui, |ui| {
        let before = app.config.schedule.clone();
        ui.horizontal(|ui| {
            ui.vertical(|ui| widgets::section_title(ui, t.schedule_title));
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                let mut on = app.config.schedule.enabled;
                if widgets::toggle(ui, &mut on, app.schedule.job.is_none(), t.schedule_title) {
                    app.config.schedule.enabled = on;
                }
                if app.schedule.job.is_some() {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
        });
        widgets::secondary_text(ui, t.schedule_hint);

        if app.config.schedule.enabled {
            ui.add_space(10.0);
            let schedule = &mut app.config.schedule;
            ui.horizontal_wrapped(|ui| {
                let label = |f: Frequency| match f {
                    Frequency::Daily => t.freq_daily,
                    Frequency::Weekly => t.freq_weekly,
                    Frequency::Hourly => t.freq_hourly,
                    Frequency::AtLogon => t.freq_logon,
                };
                egui::ComboBox::from_id_salt("schedule-frequency")
                    .selected_text(label(schedule.frequency))
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for f in [
                            Frequency::Daily,
                            Frequency::Weekly,
                            Frequency::Hourly,
                            Frequency::AtLogon,
                        ] {
                            ui.selectable_value(&mut schedule.frequency, f, label(f));
                        }
                    });

                if schedule.frequency == Frequency::Weekly {
                    ui.label(t.on_day);
                    egui::ComboBox::from_id_salt("schedule-weekday")
                        .selected_text(t.weekdays[schedule.weekday as usize])
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for day in Weekday::ALL {
                                ui.selectable_value(
                                    &mut schedule.weekday,
                                    day,
                                    t.weekdays[day as usize],
                                );
                            }
                        });
                }
                if matches!(schedule.frequency, Frequency::Daily | Frequency::Weekly) {
                    ui.label(t.at_time);
                    egui::ComboBox::from_id_salt("schedule-time")
                        .selected_text(schedule.time.clone())
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for hour in 0..24 {
                                for minute in [0, 30] {
                                    let value = format!("{hour:02}:{minute:02}");
                                    ui.selectable_value(&mut schedule.time, value.clone(), value);
                                }
                            }
                        });
                }
                if schedule.frequency == Frequency::Hourly {
                    egui::ComboBox::from_id_salt("schedule-hours")
                        .selected_text(lang.every_hours(schedule.every_hours))
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for hours in [1u8, 2, 3, 4, 6, 8, 12] {
                                ui.selectable_value(
                                    &mut schedule.every_hours,
                                    hours,
                                    lang.every_hours(hours),
                                );
                            }
                        });
                }
            });
            if schedule.frequency == Frequency::AtLogon {
                widgets::secondary_text(ui, t.logon_hint);
            }
            ui.add_space(4.0);
            ui.checkbox(&mut schedule.catch_up, t.catch_up);
            ui.checkbox(&mut schedule.only_on_ac_power, t.only_ac);

            ui.add_space(6.0);
            if let Some(next) = scheduler::next_run(schedule, chrono::Local::now()) {
                widgets::secondary_text(ui, lang.next_backup(next));
            }
            if let Some(run) = &app.state.last_automatic {
                let color = if matches!(
                    run.outcome,
                    crate::state::AutomaticOutcome::Complete
                        | crate::state::AutomaticOutcome::CompleteWithNotes
                ) {
                    p.text_secondary
                } else {
                    p.warning
                };
                ui.label(
                    egui::RichText::new(lang.last_automatic(run))
                        .size(13.0)
                        .color(color),
                );
            }

            if app.config.encryption.enabled && !app.vault.remembered {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(t.schedule_needs_key).color(p.warning));
                    if widgets::button(ui, ButtonKind::Quiet, t.remember_now, true).clicked() {
                        app.set_remembered(true);
                    }
                });
            }
        }

        if app.config.schedule != before {
            app.mark_dirty();
        }
    });
}

fn indented_hint(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(26.0);
        widgets::secondary_text(ui, text);
    });
    ui.add_space(2.0);
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
                        None if info.is_locked() => {
                            ui.label(format!("{} — {}", info.id, t.encrypted_label));
                        }
                        None => {
                            ui.label(format!("{} — {}", info.id, lang.status(None)));
                        }
                    }
                }
            });
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let ready = !app.is_busy()
                && (app.config.enabled_sources().next().is_some()
                    || app.config.apps.iter().any(|a| a.enabled));
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
