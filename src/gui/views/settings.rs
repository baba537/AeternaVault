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
            });
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
