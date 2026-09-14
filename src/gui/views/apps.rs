//! Applications: choose whose settings are kept, plus an overview of
//! installed programs.

use eframe::egui::{self, Align, CornerRadius, FontFamily, FontId, Layout, Sense, Ui};

use crate::gui::AeternaApp;
use crate::gui::tasks::Job;
use crate::gui::theme::{SANS_STRONG, palette};
use crate::gui::widgets::{self, ButtonKind};
use crate::platform;
use crate::platform::apps::{self, Category};

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = app.lang.t();
    let lang = app.lang;
    let p = *palette(ui);

    if app.apps.installed.is_none() && app.apps.installed_job.is_none() {
        app.apps.installed_job = Some(Job::spawn(&ctx, |_, send| send(apps::installed_apps())));
    }

    widgets::card(ui, |ui| {
        widgets::section_title(ui, t.apps_card_title);
        ui.add(
            egui::Label::new(
                egui::RichText::new(t.apps_hint)
                    .size(13.0)
                    .color(p.text_secondary),
            )
            .wrap(),
        );
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.apps.search)
                    .hint_text(t.search)
                    .desired_width(240.0),
            );
            if widgets::button(ui, ButtonKind::Quiet, t.select_all_found, true).clicked() {
                let found: Vec<String> = app
                    .catalog
                    .apps
                    .iter()
                    .filter(|a| app.app_detected(&a.id))
                    .map(|a| a.id.clone())
                    .collect();
                for id in found {
                    app.config.set_app_enabled(&id, true);
                }
                app.mark_dirty();
            }
            if widgets::button(ui, ButtonKind::Quiet, t.select_none, true).clicked() {
                for choice in &mut app.config.apps {
                    choice.enabled = false;
                }
                app.mark_dirty();
            }
        });
        ui.checkbox(&mut app.apps.show_not_found, t.show_not_found);
        ui.add_space(6.0);

        if app.apps.status.is_empty() {
            widgets::progress_line(ui, None);
            widgets::secondary_text(ui, t.loading);
            return;
        }

        let needle = app.apps.search.trim().to_lowercase();
        let running = app.running.clone();
        let mut toggled: Option<(String, bool)> = None;
        let mut any = false;

        for category in Category::ALL {
            let apps_in_category: Vec<&apps::AppDef> = app
                .catalog
                .apps
                .iter()
                .filter(|a| a.category == category)
                .filter(|a| app.apps.show_not_found || app.app_detected(&a.id))
                .filter(|a| {
                    needle.is_empty()
                        || a.display_name(lang).to_lowercase().contains(&needle)
                        || a.id.contains(&needle)
                })
                .collect();
            if apps_in_category.is_empty() {
                continue;
            }
            any = true;
            ui.add_space(10.0);
            widgets::section_title(ui, t.categories[category as usize]);

            for def in apps_in_category {
                let status = app.apps.status.get(&def.id).cloned().unwrap_or_default();
                let enabled = app.config.app_enabled(&def.id);
                let width = ui.available_width();
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(width, 46.0), Sense::hover());
                if response.hovered() {
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(4), p.raised);
                }
                let mut row = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink2(egui::vec2(6.0, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                let mut checked = enabled && status.detected;
                let checkbox =
                    row.add_enabled(status.detected, egui::Checkbox::without_text(&mut checked));
                checkbox.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::Checkbox,
                        status.detected,
                        checked,
                        def.display_name(lang),
                    )
                });
                if checkbox.changed() {
                    toggled = Some((def.id.clone(), checked));
                }

                let left = row.cursor().left() + 4.0;
                let text_width = (rect.width() - (left - rect.left()) - 120.0).max(100.0);
                let name_color = if status.detected {
                    p.text
                } else {
                    p.text_secondary
                };
                widgets::paint_cell(
                    ui,
                    egui::Rect::from_min_size(
                        egui::pos2(left, rect.top() + 5.0),
                        egui::vec2(text_width, 20.0),
                    ),
                    def.display_name(lang),
                    FontId::new(15.0, FontFamily::Name(SANS_STRONG.into())),
                    name_color,
                    Align::Min,
                );
                let detail = if status.detected {
                    let contents = lang.app_contents(status.folders, status.registry_keys);
                    match def.note(lang) {
                        Some(note) => format!("{contents} · {note}"),
                        None => contents,
                    }
                } else {
                    t.not_found.to_string()
                };
                let detail_rect = egui::Rect::from_min_size(
                    egui::pos2(left, rect.top() + 25.0),
                    egui::vec2(text_width, 17.0),
                );
                widgets::paint_cell(
                    ui,
                    detail_rect,
                    &detail,
                    FontId::proportional(12.5),
                    p.text_secondary,
                    Align::Min,
                );
                if let Some(note) = def.note(lang) {
                    ui.interact(detail_rect, ui.id().with(("note", &def.id)), Sense::hover())
                        .on_hover_text(note);
                }

                let right = egui::Rect::from_min_max(
                    egui::pos2(rect.right() - 116.0, rect.top()),
                    egui::pos2(rect.right() - 10.0, rect.bottom()),
                );
                if status.detected && def.is_running(&running) {
                    widgets::paint_cell(
                        ui,
                        right,
                        t.app_open,
                        FontId::proportional(12.5),
                        p.warning,
                        Align::Max,
                    );
                } else if let Some(bytes) = status.bytes.filter(|_| status.detected) {
                    widgets::paint_cell(
                        ui,
                        right,
                        &lang.bytes(bytes),
                        FontId::proportional(12.5),
                        p.text_secondary,
                        Align::Max,
                    );
                }
            }
        }

        if !any {
            ui.add_space(8.0);
            widgets::secondary_text(ui, t.no_matches);
        }
        if let Some((id, on)) = toggled {
            app.config.set_app_enabled(&id, on);
            app.mark_dirty();
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
        widgets::secondary_text(ui, lang.apps_count(installed.len()));
        ui.add_space(4.0);

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
            .max_height(320.0)
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
