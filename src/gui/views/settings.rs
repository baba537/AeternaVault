//! Settings: language, appearance, exclusions, advanced options, config file.

use eframe::egui::{self, Align, Layout, Ui};

use crate::config::{Appearance, LanguageSetting, default_excludes};
use crate::engine::scan::Excludes;
use crate::gui::widgets::{self, ButtonKind, NoticeKind};
use crate::gui::{AeternaApp, theme};
use crate::i18n::Lang;
use crate::platform;

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.settings_general);
        egui::Grid::new("settings-general")
            .num_columns(2)
            .spacing([28.0, 10.0])
            .show(ui, |ui| {
                widgets::secondary_text(ui, t.language);
                ui.horizontal(|ui| {
                    let before = app.config.language;
                    ui.radio_value(
                        &mut app.config.language,
                        LanguageSetting::Auto,
                        t.language_auto,
                    );
                    ui.radio_value(&mut app.config.language, LanguageSetting::En, "English");
                    ui.radio_value(&mut app.config.language, LanguageSetting::De, "Deutsch");
                    if before != app.config.language {
                        app.lang = Lang::resolve(app.config.language);
                        app.mark_dirty();
                    }
                });
                ui.end_row();

                widgets::secondary_text(ui, t.appearance);
                ui.horizontal(|ui| {
                    let before = app.config.appearance;
                    ui.radio_value(
                        &mut app.config.appearance,
                        Appearance::System,
                        t.appearance_system,
                    );
                    ui.radio_value(
                        &mut app.config.appearance,
                        Appearance::Dark,
                        t.appearance_dark,
                    );
                    ui.radio_value(
                        &mut app.config.appearance,
                        Appearance::Light,
                        t.appearance_light,
                    );
                    if before != app.config.appearance {
                        theme::apply(&ctx, app.config.appearance);
                        app.mark_dirty();
                    }
                });
                ui.end_row();

                widgets::secondary_text(ui, t.interface_size);
                ui.horizontal(|ui| {
                    let before = app.config.interface_scale;
                    egui::ComboBox::from_id_salt("interface-scale")
                        .selected_text(format!("{} %", app.config.interface_scale))
                        .width(100.0)
                        .show_ui(ui, |ui| {
                            for scale in [90u16, 100, 110, 125, 150] {
                                ui.selectable_value(
                                    &mut app.config.interface_scale,
                                    scale,
                                    format!("{scale} %"),
                                );
                            }
                        });
                    if before != app.config.interface_scale {
                        ctx.set_zoom_factor(app.config.zoom_factor());
                        app.mark_dirty();
                    }
                });
                ui.end_row();
            });
    });

    ui.add_space(14.0);

    widgets::card(ui, |ui| {
        let p = *theme::palette(ui);
        widgets::section_title(ui, t.enc_settings_title);
        if app.config.encryption.enabled {
            widgets::strong_text(ui, t.enc_status_on);
        } else {
            widgets::secondary_text(ui, t.enc_status_off);
        }
        if app.vault.header.is_some() {
            ui.add_space(6.0);
            let mut remembered = app.vault.remembered;
            if ui
                .checkbox(&mut remembered, t.remember_on_computer)
                .changed()
            {
                app.set_remembered(remembered);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if widgets::button(
                    ui,
                    ButtonKind::Secondary,
                    t.change_passphrase,
                    !app.is_busy(),
                )
                .clicked()
                {
                    if app.vault.key.is_some() {
                        app.vault.dialog = Some(crate::gui::VaultDialog::Change {
                            passphrase: String::new(),
                            repeat: String::new(),
                            error: None,
                        });
                    } else {
                        app.open_unlock(crate::gui::AfterUnlock::ChangePassphrase);
                    }
                }
                if app.vault.key.is_some()
                    && !app.vault.remembered
                    && widgets::button(ui, ButtonKind::Quiet, t.lock_now, true).clicked()
                {
                    app.lock_vault(&ctx);
                }
            });
            ui.label(
                egui::RichText::new(
                    crate::engine::vault::vault_dir(&app.config.destination)
                        .display()
                        .to_string(),
                )
                .size(12.5)
                .color(p.text_secondary),
            );
        }
    });

    ui.add_space(14.0);

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.exclusions_title);
        widgets::secondary_text(ui, t.exclusions_hint);
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::multiline(&mut app.settings.exclude_text)
                .font(egui::TextStyle::Monospace)
                .desired_rows(7)
                .desired_width(f32::INFINITY),
        );
        if !app.settings.invalid_patterns.is_empty() {
            let p = *theme::palette(ui);
            ui.label(
                egui::RichText::new(app.lang.invalid_patterns(&app.settings.invalid_patterns))
                    .color(p.warning),
            );
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let patterns: Vec<String> = app
                .settings
                .exclude_text
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            let changed = patterns != app.config.exclude;
            if widgets::button(ui, ButtonKind::Secondary, t.apply, changed).clicked() {
                app.settings.invalid_patterns = Excludes::new(patterns.iter()).invalid;
                app.config.exclude = patterns;
                app.mark_dirty();
            }
            if widgets::button(ui, ButtonKind::Quiet, t.reset_defaults, true).clicked() {
                app.settings.exclude_text = default_excludes().join("\n");
            }
        });
    });

    ui.add_space(14.0);

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.advanced);
        let before = app.config.advanced.clone();
        ui.checkbox(
            &mut app.config.advanced.skip_online_only_files,
            t.adv_skip_online,
        );
        ui.checkbox(&mut app.config.advanced.hardlink_unchanged, t.adv_hardlinks);
        ui.checkbox(&mut app.config.advanced.confirm_before_start, t.adv_confirm);
        ui.checkbox(
            &mut app.config.advanced.verify_on_restore,
            t.verify_checksums,
        );
        ui.checkbox(
            &mut app.config.advanced.save_program_list,
            t.adv_program_list,
        );
        ui.checkbox(
            &mut app.config.advanced.compatibility_graphics,
            t.adv_compatibility_graphics,
        );
        ui.horizontal(|ui| {
            ui.add_space(26.0);
            widgets::secondary_text(ui, t.adv_compatibility_hint);
        });
        if before != app.config.advanced {
            app.mark_dirty();
        }
    });

    ui.add_space(14.0);

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.config_title);
        widgets::secondary_text(ui, t.config_hint);
        ui.add_space(4.0);
        ui.label(egui::RichText::new(app.paths.config_file.display().to_string()).monospace());
        if app.paths.portable {
            widgets::secondary_text(ui, t.portable_mode);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if widgets::button(ui, ButtonKind::Secondary, t.open_in_editor, true).clicked() {
                platform::open_in_editor(&app.paths.config_file);
            }
            if widgets::button(ui, ButtonKind::Secondary, t.reload, !app.is_busy()).clicked() {
                app.reload_config(&ctx);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, ButtonKind::Quiet, t.open_log_folder, true).clicked() {
                    if let Err(err) = std::fs::create_dir_all(&app.paths.log_dir) {
                        app.notify(NoticeKind::Warning, err.to_string());
                    }
                    platform::open_in_file_manager(&app.paths.log_dir);
                }
            });
        });
    });
    ui.add_space(12.0);
}
