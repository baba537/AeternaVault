//! Modal dialogs: confirmation before starting, and the encryption dialogs.

use eframe::egui::{self, Align, CornerRadius, FontFamily, Frame, Layout, Margin, Stroke, Ui};

use super::theme::{self, palette};
use super::views;
use super::widgets::{self, ButtonKind, NoticeKind};
use super::{AeternaApp, Pending, Screen, VaultDialog};
use crate::engine::crypto::passphrase_strength;
use crate::engine::plan::ItemKind;

fn modal_frame(ui: &Ui) -> Frame {
    let p = palette(ui);
    Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(26))
        .shadow(ui.visuals().window_shadow)
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .family(FontFamily::Name(theme::SERIF.into()))
            .size(22.0),
    );
    ui.add_space(6.0);
}

fn wrapped(ui: &mut Ui, text: &str, color: Option<egui::Color32>) {
    let mut rich = egui::RichText::new(text);
    if let Some(color) = color {
        rich = rich.color(color);
    }
    ui.add(egui::Label::new(rich).wrap());
}

/// A password field that fills the dialog width.
fn secret_field(ui: &mut Ui, value: &mut String, hint: &str, focus: bool) -> egui::Response {
    let response = ui.add(
        egui::TextEdit::singleline(value)
            .password(true)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    if focus && !response.has_focus() && ui.memory(|m| m.focused().is_none()) {
        response.request_focus();
    }
    response
}

pub fn confirmation(app: &mut AeternaApp, ui: &mut Ui) {
    let Some(pending) = &app.pending else {
        return;
    };
    let t = app.lang.t();
    let lang = app.lang;
    let mut action = 0; // 1 = start, 2 = details, 3 = cancel

    let modal = egui::Modal::new(egui::Id::new("confirm"))
        .frame(modal_frame(ui))
        .show(ui.ctx(), |ui| {
            let p = *palette(ui);
            ui.set_max_width(500.0);
            let (title, body, overwrite, running, restore) = match pending {
                Pending::Backup(plan) => {
                    let (files, bytes) = plan.copy_totals();
                    (
                        t.confirm_backup_title,
                        lang.confirm_backup(files, bytes, &plan.destination.display().to_string()),
                        false,
                        &plan.running_apps,
                        false,
                    )
                }
                Pending::Restore(plan) => (
                    t.confirm_restore_title,
                    lang.confirm_restore(
                        plan.summary.count(ItemKind::New),
                        plan.summary.count(ItemKind::Changed),
                    ),
                    plan.summary.count(ItemKind::Changed) > 0,
                    &plan.running_apps,
                    true,
                ),
            };
            heading(ui, title);
            wrapped(ui, &body, None);
            if overwrite {
                ui.add_space(4.0);
                wrapped(ui, t.confirm_overwrite, Some(p.warning));
            }
            if !running.is_empty() {
                ui.add_space(4.0);
                let text = if restore {
                    lang.close_before_restore(running)
                } else {
                    lang.running_warning(running)
                };
                wrapped(ui, &text, Some(p.warning));
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if widgets::button(ui, ButtonKind::Quiet, t.show_details, true).clicked() {
                    action = 2;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, ButtonKind::Primary, t.start, true).clicked() {
                        action = 1;
                    }
                    if widgets::button(ui, ButtonKind::Secondary, t.cancel, true).clicked() {
                        action = 3;
                    }
                });
            });
        });
    if modal.should_close() && action == 0 {
        action = 3;
    }

    let ctx = ui.ctx().clone();
    match action {
        1 => match app.pending.take() {
            Some(Pending::Backup(plan)) => app.start_backup(&ctx, plan),
            Some(Pending::Restore(plan)) => app.start_restore(&ctx, plan),
            None => {}
        },
        2 => match app.pending.take() {
            Some(Pending::Backup(plan)) => {
                app.screen = Screen::Preview(Box::new(views::preview::PreviewState::backup(*plan)));
            }
            Some(Pending::Restore(plan)) => {
                app.screen =
                    Screen::Preview(Box::new(views::preview::PreviewState::restore(*plan)));
            }
            None => {}
        },
        3 => app.pending = None,
        _ => {}
    }
}

fn strength_meter(ui: &mut Ui, passphrase: &str, labels: [&str; 5]) {
    let p = *palette(ui);
    let strength = passphrase_strength(passphrase);
    let color = match strength {
        0 | 1 => p.error,
        2 => p.warning,
        _ => p.success,
    };
    ui.horizontal(|ui| {
        let width = 160.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 4.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(2), p.raised);
        if !passphrase.is_empty() {
            let mut filled = rect;
            filled.set_width(width * (f32::from(strength) + 1.0) / 5.0);
            ui.painter()
                .rect_filled(filled, CornerRadius::same(2), color);
        }
        ui.add_space(6.0);
        if !passphrase.is_empty() {
            ui.label(
                egui::RichText::new(labels[strength as usize])
                    .size(12.5)
                    .color(color),
            );
        }
    });
}

/// The encryption dialogs. Each frame draws the active one, if any.
pub fn vault_dialog(app: &mut AeternaApp, ui: &mut Ui) {
    let Some(mut dialog) = app.vault.dialog.take() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let frame = modal_frame(ui);
    let mut keep_open = true;
    let mut next: Option<VaultDialog> = None;

    match &mut dialog {
        VaultDialog::Create {
            passphrase,
            repeat,
            remember,
            error,
        } => {
            let mut submit = false;
            let modal = egui::Modal::new(egui::Id::new("vault-create"))
                .frame(frame)
                .show(&ctx, |ui| {
                    let p = *palette(ui);
                    ui.set_width(460.0);
                    heading(ui, t.enc_create_title);
                    wrapped(ui, t.enc_create_hint, Some(p.text_secondary));
                    ui.add_space(10.0);
                    secret_field(ui, passphrase, t.passphrase, true);
                    strength_meter(ui, passphrase, t.strength);
                    let repeat_field = secret_field(ui, repeat, t.passphrase_repeat, false);
                    if repeat_field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    ui.add_space(4.0);
                    ui.checkbox(remember, t.remember_on_computer);
                    ui.add_space(8.0);
                    wrapped(ui, t.enc_warning, Some(p.warning));
                    if let Some(error) = error.as_ref() {
                        ui.add_space(4.0);
                        wrapped(ui, error, Some(p.error));
                    }
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if widgets::button(ui, ButtonKind::Primary, t.set_up, true).clicked() {
                                submit = true;
                            }
                            if widgets::button(ui, ButtonKind::Secondary, t.cancel, true).clicked()
                            {
                                keep_open = false;
                            }
                        });
                    });
                });
            if modal.should_close() {
                keep_open = false;
            }
            if submit {
                if let Some(problem) = app.passphrase_problem(passphrase, repeat) {
                    *error = Some(problem);
                } else {
                    match app.create_vault(passphrase, *remember) {
                        Ok(key) => {
                            next = Some(VaultDialog::ShowRecovery {
                                key,
                                confirmed: false,
                            });
                        }
                        Err(message) => *error = Some(message),
                    }
                }
            }
        }

        VaultDialog::ShowRecovery { key, confirmed } => {
            let modal = egui::Modal::new(egui::Id::new("vault-recovery"))
                .frame(frame)
                .show(&ctx, |ui| {
                    let p = *palette(ui);
                    ui.set_width(480.0);
                    heading(ui, t.recovery_title);
                    wrapped(ui, t.recovery_hint, Some(p.text_secondary));
                    ui.add_space(12.0);
                    Frame::new()
                        .fill(p.raised)
                        .stroke(Stroke::new(1.0, p.accent))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(key.as_str())
                                        .monospace()
                                        .size(16.0)
                                        .color(p.text),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if widgets::button(ui, ButtonKind::Quiet, t.copy, true)
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(key.clone());
                                    }
                                });
                            });
                        });
                    ui.add_space(10.0);
                    ui.checkbox(confirmed, t.recovery_confirm);
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if widgets::button(ui, ButtonKind::Primary, t.done, *confirmed)
                                .clicked()
                            {
                                keep_open = false;
                            }
                        });
                    });
                });
            // Closing by Escape or clicking outside is not allowed before confirming.
            let _ = modal;
        }

        VaultDialog::Unlock {
            secret,
            remember,
            error,
            then,
        } => {
            let mut submit = false;
            let then = *then;
            let modal = egui::Modal::new(egui::Id::new("vault-unlock"))
                .frame(frame)
                .show(&ctx, |ui| {
                    let p = *palette(ui);
                    ui.set_width(440.0);
                    heading(ui, t.unlock_title);
                    wrapped(ui, t.unlock_hint, Some(p.text_secondary));
                    ui.add_space(10.0);
                    let field = secret_field(ui, secret, t.passphrase, true);
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    ui.add_space(4.0);
                    ui.checkbox(remember, t.remember_on_computer);
                    if let Some(error) = error.as_ref() {
                        ui.add_space(4.0);
                        wrapped(ui, error, Some(p.error));
                    }
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let label = t.unlock.trim_end_matches('…');
                            if widgets::button(ui, ButtonKind::Primary, label, !secret.is_empty())
                                .clicked()
                            {
                                submit = true;
                            }
                            if widgets::button(ui, ButtonKind::Secondary, t.cancel, true).clicked()
                            {
                                keep_open = false;
                            }
                        });
                    });
                });
            if modal.should_close() {
                keep_open = false;
            }
            if submit && !secret.is_empty() {
                let secret_value = std::mem::take(secret);
                match app.unlock_vault(&ctx, &secret_value, *remember, then) {
                    Ok(()) => {
                        // The follow-up step may have opened the next dialog.
                        return;
                    }
                    Err(message) => *error = Some(message),
                }
            }
        }

        VaultDialog::Change {
            passphrase,
            repeat,
            error,
        } => {
            let mut submit = false;
            let modal = egui::Modal::new(egui::Id::new("vault-change"))
                .frame(frame)
                .show(&ctx, |ui| {
                    let p = *palette(ui);
                    ui.set_width(440.0);
                    heading(ui, t.change_title);
                    wrapped(ui, t.change_hint, Some(p.text_secondary));
                    ui.add_space(10.0);
                    secret_field(ui, passphrase, t.new_passphrase, true);
                    strength_meter(ui, passphrase, t.strength);
                    secret_field(ui, repeat, t.passphrase_repeat, false);
                    if let Some(error) = error.as_ref() {
                        ui.add_space(4.0);
                        wrapped(ui, error, Some(p.error));
                    }
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if widgets::button(ui, ButtonKind::Primary, t.save, true).clicked() {
                                submit = true;
                            }
                            if widgets::button(ui, ButtonKind::Secondary, t.cancel, true).clicked()
                            {
                                keep_open = false;
                            }
                        });
                    });
                });
            if modal.should_close() {
                keep_open = false;
            }
            if submit {
                if let Some(problem) = app.passphrase_problem(passphrase, repeat) {
                    *error = Some(problem);
                } else {
                    match app.change_passphrase(passphrase) {
                        Ok(()) => {
                            app.notify(NoticeKind::Success, t.passphrase_changed);
                            keep_open = false;
                        }
                        Err(message) => *error = Some(message),
                    }
                }
            }
        }
    }

    if let Some(next) = next {
        dialog = next;
    }
    // A dialog opened by a follow-up step takes precedence.
    if keep_open && app.vault.dialog.is_none() {
        app.vault.dialog = Some(dialog);
    }
}
