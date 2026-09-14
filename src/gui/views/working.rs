//! Progress and result screens.

use eframe::egui::{self, Align, FontFamily, Layout, Ui};

use crate::engine::Phase;
use crate::engine::manifest::SnapshotStatus;
use crate::gui::tasks::TaskKind;
use crate::gui::theme::{SERIF, palette};
use crate::gui::widgets::{self, ButtonKind};
use crate::gui::{AeternaApp, Done, Screen};
use crate::platform;

fn heading(ui: &mut Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .family(FontFamily::Name(SERIF.into()))
            .size(26.0),
    );
}

pub fn show_working(app: &mut AeternaApp, ui: &mut Ui) {
    let t = app.lang.t();
    let lang = app.lang;
    let Some(task) = &app.task else {
        return;
    };
    let progress = task.progress.clone();
    let cancelling = task.cancel.is_cancelled();
    let kind = task.kind;

    widgets::centered_column(ui, 680.0, |ui| {
        ui.add_space((ui.available_height() * 0.18).clamp(20.0, 140.0));
        widgets::card(ui, |ui| {
            ui.add_space(6.0);
            let title = match (kind, progress.phase) {
                (_, Phase::Finishing) if matches!(kind, TaskKind::Backup | TaskKind::Restore) => {
                    t.working_finishing
                }
                (TaskKind::PlanBackup { .. }, _) => t.working_scan_backup,
                (TaskKind::PlanRestore { .. }, _) => t.working_scan_restore,
                (TaskKind::Backup, _) => t.working_backup,
                (TaskKind::Restore, _) => t.working_restore,
            };
            heading(ui, title);
            ui.add_space(14.0);

            match kind {
                TaskKind::PlanBackup { .. } => {
                    widgets::progress_line(ui, None);
                    ui.add_space(8.0);
                    widgets::secondary_text(
                        ui,
                        lang.files_found(progress.files_done, progress.bytes_done),
                    );
                }
                TaskKind::PlanRestore { .. } => {
                    let fraction = (progress.files_total > 0)
                        .then(|| progress.files_done as f32 / progress.files_total as f32);
                    widgets::progress_line(ui, fraction);
                    ui.add_space(8.0);
                    widgets::secondary_text(
                        ui,
                        lang.files_of(progress.files_done, progress.files_total),
                    );
                }
                TaskKind::Backup | TaskKind::Restore => {
                    let fraction = if progress.bytes_total > 0 {
                        Some(progress.bytes_done as f32 / progress.bytes_total as f32)
                    } else if progress.files_total > 0 {
                        Some(progress.files_done as f32 / progress.files_total as f32)
                    } else {
                        None
                    };
                    widgets::progress_line(ui, fraction);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        widgets::secondary_text(
                            ui,
                            lang.files_of(progress.files_done, progress.files_total),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            widgets::secondary_text(
                                ui,
                                lang.bytes_of(
                                    progress.bytes_done.min(progress.bytes_total),
                                    progress.bytes_total,
                                ),
                            );
                        });
                    });
                }
            }

            ui.add_space(4.0);
            let p = *palette(ui);
            let width = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 18.0), egui::Sense::hover());
            widgets::paint_cell(
                ui,
                rect,
                &progress.current,
                egui::FontId::proportional(12.0),
                p.text_secondary,
                Align::Min,
            );

            ui.add_space(18.0);
            // `horizontal` keeps the right-aligned row as tall as its buttons.
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if cancelling { t.stopping } else { t.cancel };
                    if widgets::button(ui, ButtonKind::Secondary, label, !cancelling).clicked()
                        && let Some(task) = &app.task
                    {
                        task.cancel.cancel();
                        tracing::info!("cancellation requested");
                    }
                });
            });
        });
    });
}

pub fn show_done(app: &mut AeternaApp, ui: &mut Ui) {
    let Screen::Done(done) = &app.screen else {
        return;
    };
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);
    let mut back = false;

    widgets::centered_column(ui, 760.0, |ui| {
        ui.add_space(24.0);
        widgets::card(ui, |ui| {
            ui.add_space(4.0);
            let mut warnings: &[String] = &[];
            let mut open_folder = None;
            match done.as_ref() {
                Done::Backup(Ok(report)) => {
                    let h = &report.header;
                    heading(
                        ui,
                        match h.status {
                            SnapshotStatus::Complete => t.done_backup,
                            SnapshotStatus::CompleteWithWarnings => t.done_backup_notes,
                            SnapshotStatus::Cancelled | SnapshotStatus::Failed => {
                                t.done_backup_cancelled
                            }
                        },
                    );
                    ui.add_space(8.0);
                    ui.label(lang.backup_result(
                        h.stats.copied_files,
                        h.stats.linked_files + h.stats.referenced_files,
                        h.stats.bytes,
                        report.duration,
                    ));
                    widgets::secondary_text(ui, report.snapshot_dir.display().to_string());
                    if h.stats.failed > 0 {
                        ui.label(
                            egui::RichText::new(lang.failed_files(h.stats.failed)).color(p.warning),
                        );
                    }
                    warnings = &report.warnings;
                    open_folder = Some(report.snapshot_dir.clone());
                }
                Done::Restore(Ok(report)) => {
                    heading(
                        ui,
                        if report.cancelled {
                            t.done_restore_cancelled
                        } else if report.failed > 0 {
                            t.done_restore_notes
                        } else {
                            t.done_restore
                        },
                    );
                    ui.add_space(8.0);
                    ui.label(lang.restore_result(
                        report.restored_files,
                        report.restored_bytes,
                        report.skipped,
                        report.duration,
                    ));
                    if report.failed > 0 {
                        ui.label(
                            egui::RichText::new(lang.failed_files(report.failed)).color(p.warning),
                        );
                    }
                    warnings = &report.warnings;
                }
                Done::Backup(Err(message)) | Done::Restore(Err(message)) => {
                    heading(ui, t.done_failed);
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(egui::RichText::new(message).color(p.warning)).wrap());
                }
            }

            if !warnings.is_empty() {
                ui.add_space(10.0);
                egui::CollapsingHeader::new(
                    egui::RichText::new(t.notes_title).color(p.text_secondary),
                )
                .id_salt("done-notes")
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for warning in warnings.iter().take(500) {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(warning).size(12.5))
                                        .wrap(),
                                );
                            }
                        });
                });
            }

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, ButtonKind::Primary, t.to_overview, true).clicked() {
                        back = true;
                    }
                    if let Some(folder) = &open_folder
                        && widgets::button(ui, ButtonKind::Secondary, t.open_backup_folder, true)
                            .clicked()
                    {
                        platform::open_in_file_manager(folder);
                    }
                });
            });
        });
    });

    if back {
        app.screen = Screen::Main;
    }
}
