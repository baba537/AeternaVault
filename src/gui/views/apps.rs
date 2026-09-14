//! Applications: suggested data folders and an overview of installed programs.

use eframe::egui::{self, Align, CornerRadius, FontFamily, FontId, Layout, Sense, Ui};

use crate::config::Source;
use crate::gui::AeternaApp;
use crate::gui::tasks::Job;
use crate::gui::theme::{SANS_STRONG, palette};
use crate::gui::widgets::{self, ButtonKind};
use crate::platform::{self, apps};

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    if app.apps.profiles.as_ref().is_none_or(|(l, _)| *l != lang) {
        app.apps.profiles = Some((lang, apps::known_profiles(lang)));
    }
    if app.apps.installed.is_none() && app.apps.job.is_none() {
        app.apps.job = Some(Job::spawn(&ctx, |_, send| send(apps::installed_apps())));
    }

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.apps_profiles_title);
        widgets::secondary_text(ui, t.apps_profiles_hint);
        ui.add_space(6.0);

        let profiles = app
            .apps
            .profiles
            .as_ref()
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        if profiles.is_empty() {
            widgets::secondary_text(ui, t.apps_profiles_none);
        }
        for profile in profiles {
            let width = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 46.0), Sense::hover());
            let text_width = width - 170.0;
            widgets::paint_cell(
                ui,
                egui::Rect::from_min_size(
                    rect.min + egui::vec2(0.0, 4.0),
                    egui::vec2(text_width, 20.0),
                ),
                &profile.name,
                FontId::new(15.0, FontFamily::Name(SANS_STRONG.into())),
                p.text,
                Align::Min,
            );
            widgets::paint_cell(
                ui,
                egui::Rect::from_min_size(
                    rect.min + egui::vec2(0.0, 24.0),
                    egui::vec2(text_width, 18.0),
                ),
                &profile.path.display().to_string(),
                FontId::proportional(12.5),
                p.text_secondary,
                Align::Min,
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::right_to_left(Align::Center)),
            );
            if app.config.has_source_path(&profile.path) {
                widgets::secondary_text(&mut child, t.in_list);
            } else if widgets::button(&mut child, ButtonKind::Secondary, t.add, true).clicked() {
                let mut source = Source::new(profile.name.clone(), profile.path.clone(), true);
                source.exclude = profile.exclude.clone();
                app.add_source(&ctx, source);
            }
        }
    });

    ui.add_space(14.0);

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.apps_installed_title);
        widgets::secondary_text(ui, t.apps_installed_hint);
        ui.add_space(6.0);

        let Some(installed) = &app.apps.installed else {
            widgets::progress_line(ui, None);
            widgets::secondary_text(ui, t.loading);
            return;
        };

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.apps.search)
                    .hint_text(t.search)
                    .desired_width(280.0),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::secondary_text(ui, lang.apps_count(installed.len()));
            });
        });
        ui.add_space(6.0);

        let needle = app.apps.search.trim().to_lowercase();
        let visible: Vec<&apps::InstalledApp> = installed
            .iter()
            .filter(|a| {
                needle.is_empty()
                    || a.name.to_lowercase().contains(&needle)
                    || a.publisher.to_lowercase().contains(&needle)
            })
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("installed-apps")
            .max_height(380.0)
            .auto_shrink([false, true])
            .show_rows(ui, 40.0, visible.len(), |ui, range| {
                for installed_app in &visible[range] {
                    let width = ui.available_width();
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(width, 40.0), Sense::hover());
                    if response.hovered() {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(3), p.raised);
                    }
                    let name_w = width * 0.55;
                    widgets::paint_cell(
                        ui,
                        egui::Rect::from_min_size(
                            rect.min + egui::vec2(8.0, 3.0),
                            egui::vec2(name_w, 20.0),
                        ),
                        &installed_app.name,
                        FontId::proportional(14.5),
                        p.text,
                        Align::Min,
                    );
                    widgets::paint_cell(
                        ui,
                        egui::Rect::from_min_size(
                            rect.min + egui::vec2(8.0, 21.0),
                            egui::vec2(name_w, 16.0),
                        ),
                        &installed_app.publisher,
                        FontId::proportional(12.0),
                        p.text_secondary,
                        Align::Min,
                    );
                    widgets::paint_cell(
                        ui,
                        egui::Rect::from_min_max(
                            egui::pos2(rect.left() + name_w + 20.0, rect.top()),
                            egui::pos2(rect.right() - 60.0, rect.bottom()),
                        ),
                        &installed_app.version,
                        FontId::proportional(12.5),
                        p.text_secondary,
                        Align::Min,
                    );
                    if let Some(location) = installed_app
                        .install_location
                        .as_ref()
                        .filter(|l| l.is_dir())
                    {
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(rect.shrink2(egui::vec2(4.0, 5.0)))
                                .layout(Layout::right_to_left(Align::Center)),
                        );
                        if widgets::button(&mut child, ButtonKind::Quiet, "↗", true)
                            .on_hover_text(t.open_install_folder)
                            .clicked()
                        {
                            platform::open_in_file_manager(location);
                        }
                    }
                }
            });
    });
    ui.add_space(12.0);
}
