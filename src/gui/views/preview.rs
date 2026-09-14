//! Preview screen (dry run): a filterable, exportable list of what would happen.
//! Nothing is written until "Start" is pressed.

use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Layout, Sense, Stroke, Ui,
};

use crate::config::BackupMode;
use crate::engine::export::{self, ExportLabels};
use crate::engine::plan::{BackupPlan, ItemKind, Plan, PlanItem, RestorePlan};
use crate::gui::theme::{Palette, SANS_STRONG, SERIF, palette};
use crate::gui::widgets::{self, ButtonKind, NoticeKind};
use crate::gui::{AeternaApp, Screen};
use crate::i18n::Lang;

pub enum PreviewPlan {
    Backup(Box<BackupPlan>),
    Restore(Box<RestorePlan>),
}

impl PreviewPlan {
    fn plan(&self) -> &dyn Plan {
        match self {
            PreviewPlan::Backup(p) => p.as_ref(),
            PreviewPlan::Restore(p) => p.as_ref(),
        }
    }

    fn is_restore(&self) -> bool {
        matches!(self, PreviewPlan::Restore(_))
    }
}

pub struct PreviewState {
    pub plan: PreviewPlan,
    filter: Option<ItemKind>,
    search: String,
    visible: Vec<usize>,
    visible_key: Option<(Option<ItemKind>, String)>,
}

impl PreviewState {
    pub fn backup(plan: BackupPlan) -> Self {
        Self::new(PreviewPlan::Backup(Box::new(plan)))
    }

    pub fn restore(plan: RestorePlan) -> Self {
        Self::new(PreviewPlan::Restore(Box::new(plan)))
    }

    fn new(plan: PreviewPlan) -> Self {
        Self {
            plan,
            filter: None,
            search: String::new(),
            visible: Vec::new(),
            visible_key: None,
        }
    }

    fn update_visible(&mut self) {
        let key = (self.filter, self.search.trim().to_lowercase());
        if self.visible_key.as_ref() == Some(&key) {
            return;
        }
        let plan = self.plan.plan();
        let needle = &key.1;
        self.visible = plan
            .items()
            .iter()
            .enumerate()
            .filter(|(_, item)| key.0.is_none_or(|k| item.kind == k))
            .filter(|(_, item)| {
                needle.is_empty()
                    || item.rel.to_lowercase().contains(needle)
                    || plan.sources()[item.source]
                        .name
                        .to_lowercase()
                        .contains(needle)
            })
            .map(|(i, _)| i)
            .collect();
        // What changes comes first; unchanged files last. The sort is stable,
        // so folder order is kept within each group.
        let items = plan.items();
        self.visible.sort_by_key(|&i| match items[i].kind {
            ItemKind::New => 0,
            ItemKind::Changed => 1,
            ItemKind::Removed => 2,
            ItemKind::Skipped => 3,
            ItemKind::Unchanged => 4,
        });
        self.visible_key = Some(key);
    }
}

fn kind_color(p: &Palette, kind: ItemKind, restore: bool) -> Color32 {
    match (kind, restore) {
        (ItemKind::New, _) => p.success,
        (ItemKind::Changed, false) => p.accent,
        (ItemKind::Changed, true) => p.warning,
        (ItemKind::Unchanged, _) | (ItemKind::Removed, _) => p.text_secondary,
        (ItemKind::Skipped, _) => p.warning,
    }
}

enum Action {
    None,
    Back,
    Start,
}

pub fn show(app: &mut AeternaApp, ui: &mut Ui) {
    let Screen::Preview(state) = &mut app.screen else {
        return;
    };
    let lang = app.lang;
    let mut notice = None;
    let action = widgets::centered_column(ui, 1100.0, |ui| render(state, lang, ui, &mut notice));
    if let Some((kind, text)) = notice {
        app.notify(kind, text);
    }

    let ctx = ui.ctx().clone();
    match action {
        Action::None => {}
        Action::Back => app.screen = Screen::Main,
        Action::Start => {
            if let Screen::Preview(state) = std::mem::replace(&mut app.screen, Screen::Main) {
                match state.plan {
                    PreviewPlan::Backup(plan) => app.start_backup(&ctx, plan),
                    PreviewPlan::Restore(plan) => app.start_restore(&ctx, plan),
                }
            }
        }
    }
}

fn render(
    state: &mut PreviewState,
    lang: Lang,
    ui: &mut Ui,
    notice: &mut Option<(NoticeKind, String)>,
) -> Action {
    let t = lang.t();
    let p = *palette(ui);
    let restore = state.plan.is_restore();
    let mut action = Action::None;

    ui.label(
        egui::RichText::new(if restore {
            t.preview_restore_title
        } else {
            t.preview_backup_title
        })
        .family(FontFamily::Name(SERIF.into()))
        .size(24.0),
    );
    ui.label(
        egui::RichText::new(t.preview_read_only)
            .color(p.success)
            .size(13.5),
    );

    let summary = *state.plan.plan().summary();
    if let PreviewPlan::Backup(plan) = &state.plan {
        if plan.mode == BackupMode::Full {
            widgets::secondary_text(ui, t.full_mode_note);
        }
        if summary.count(ItemKind::Removed) > 0 {
            widgets::secondary_text(ui, t.removed_note);
        }
    }
    ui.add_space(10.0);

    // Filter chips.
    ui.horizontal_wrapped(|ui| {
        let total: u64 = summary.counts.iter().sum();
        if chip(
            ui,
            &format!("{}  {}", t.filter_all, lang.count(total)),
            state.filter.is_none(),
            p.text,
        ) {
            state.filter = None;
        }
        for kind in ItemKind::ALL {
            let count = summary.count(kind);
            if count == 0 {
                continue;
            }
            let mut label = format!("{}  {}", lang.kind_label(kind, restore), lang.count(count));
            if summary.bytes(kind) > 0 {
                label.push_str(&format!(" · {}", lang.bytes(summary.bytes(kind))));
            }
            if chip(
                ui,
                &label,
                state.filter == Some(kind),
                kind_color(&p, kind, restore),
            ) {
                state.filter = Some(kind);
            }
        }
    });
    ui.add_space(6.0);
    ui.add(
        egui::TextEdit::singleline(&mut state.search)
            .hint_text(t.filter_hint)
            .desired_width(320.0),
    );
    ui.add_space(8.0);

    state.update_visible();

    // Column header.
    let width = ui.available_width();
    let (header_rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), Sense::hover());
    let columns = Columns::new(header_rect);
    let header_font = FontId::new(11.5, FontFamily::Name(SANS_STRONG.into()));
    for (rect, text, align) in [
        (columns.kind, t.col_status, Align::Min),
        (columns.source, t.col_source, Align::Min),
        (columns.path, t.col_path, Align::Min),
        (columns.size, t.col_size, Align::Max),
    ] {
        widgets::paint_cell(
            ui,
            rect,
            &text.to_uppercase(),
            header_font.clone(),
            p.text_secondary,
            align,
        );
    }
    ui.painter().line_segment(
        [header_rect.left_bottom(), header_rect.right_bottom()],
        Stroke::new(1.0, p.border),
    );

    // Rows (virtualised: only visible rows are laid out).
    let footer_height = 64.0;
    let list_height = (ui.available_height() - footer_height).max(120.0);
    let plan = state.plan.plan();
    if state.visible.is_empty() {
        ui.allocate_ui(egui::vec2(width, list_height), |ui| {
            ui.add_space(16.0);
            widgets::secondary_text(ui, t.nothing_for_filter);
        });
    } else {
        egui::ScrollArea::vertical()
            .id_salt("preview-rows")
            .max_height(list_height)
            .auto_shrink([false, false])
            .show_rows(ui, 26.0, state.visible.len(), |ui, range| {
                for &index in &state.visible[range] {
                    let item = &plan.items()[index];
                    row(ui, plan, item, lang, restore, &p);
                }
            });
    }

    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if widgets::button(ui, ButtonKind::Secondary, t.back, true).clicked() {
            action = Action::Back;
        }
        ui.add_space(8.0);
        if widgets::button(ui, ButtonKind::Quiet, t.export_text, true).clicked() {
            *notice = export(plan, lang, restore, false);
        }
        if widgets::button(ui, ButtonKind::Quiet, t.export_csv, true).clicked() {
            *notice = export(plan, lang, restore, true);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if restore {
                t.start_restore
            } else {
                t.start_backup
            };
            if widgets::button(ui, ButtonKind::Primary, label, true).clicked() {
                action = Action::Start;
            }
        });
    });

    action
}

struct Columns {
    kind: egui::Rect,
    source: egui::Rect,
    path: egui::Rect,
    size: egui::Rect,
}

impl Columns {
    fn new(rect: egui::Rect) -> Self {
        let gap = 14.0;
        let kind_w = 150.0;
        let source_w = (rect.width() * 0.16).clamp(90.0, 180.0);
        let size_w = 84.0;
        let x0 = rect.left() + 8.0;
        let kind = egui::Rect::from_min_max(
            egui::pos2(x0, rect.top()),
            egui::pos2(x0 + kind_w, rect.bottom()),
        );
        let source = egui::Rect::from_min_max(
            egui::pos2(kind.right() + gap, rect.top()),
            egui::pos2(kind.right() + gap + source_w, rect.bottom()),
        );
        let size = egui::Rect::from_min_max(
            egui::pos2(rect.right() - 8.0 - size_w, rect.top()),
            egui::pos2(rect.right() - 8.0, rect.bottom()),
        );
        let path = egui::Rect::from_min_max(
            egui::pos2(source.right() + gap, rect.top()),
            egui::pos2(size.left() - gap, rect.bottom()),
        );
        Self {
            kind,
            source,
            path,
            size,
        }
    }
}

fn row(ui: &mut Ui, plan: &dyn Plan, item: &PlanItem, lang: Lang, restore: bool, p: &Palette) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.0), Sense::hover());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(3), p.raised);
    }
    let columns = Columns::new(rect);
    let body = FontId::proportional(13.5);

    let color = kind_color(p, item.kind, restore);
    ui.painter().circle_filled(
        egui::pos2(columns.kind.left() + 3.0, rect.center().y),
        3.0,
        color,
    );
    let label_rect = columns.kind.with_min_x(columns.kind.left() + 14.0);
    widgets::paint_cell(
        ui,
        label_rect,
        lang.kind_label(item.kind, restore),
        body.clone(),
        color,
        Align::Min,
    );
    widgets::paint_cell(
        ui,
        columns.source,
        &plan.sources()[item.source].name,
        body.clone(),
        p.text_secondary,
        Align::Min,
    );

    let path = plan.item_path(item).display().to_string();
    let note = item.note.as_ref().map(|n| lang.note(n));
    let path_text = match &note {
        Some(note) => format!("{path}  —  {note}"),
        None => path.clone(),
    };
    widgets::paint_cell(
        ui,
        columns.path,
        &path_text,
        body.clone(),
        p.text,
        Align::Min,
    );
    if item.size > 0 {
        widgets::paint_cell(
            ui,
            columns.size,
            &lang.bytes(item.size),
            body,
            p.text_secondary,
            Align::Max,
        );
    }
    response.on_hover_text(path_text);
}

fn chip(ui: &mut Ui, text: &str, selected: bool, dot: Color32) -> bool {
    let p = *palette(ui);
    let font = FontId::new(13.5, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, p.text);
    let size = egui::vec2(galley.size().x + 36.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(14),
        if selected { p.raised } else { p.panel },
        Stroke::new(
            1.0,
            if selected || hovered {
                p.accent
            } else {
                p.border
            },
        ),
        egui::StrokeKind::Inside,
    );
    painter.circle_filled(egui::pos2(rect.left() + 14.0, rect.center().y), 3.5, dot);
    painter.galley(
        egui::pos2(rect.left() + 24.0, rect.center().y - galley.size().y / 2.0),
        galley,
        if selected { p.text } else { p.text_secondary },
    );
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

fn export(plan: &dyn Plan, lang: Lang, restore: bool, csv: bool) -> Option<(NoticeKind, String)> {
    let t = lang.t();
    let (name, filter_name, extension) = if csv {
        ("aeterna-vault-preview.csv", "CSV", "csv")
    } else {
        ("aeterna-vault-preview.txt", "Text", "txt")
    };
    let path = rfd::FileDialog::new()
        .set_file_name(name)
        .add_filter(filter_name, &[extension])
        .save_file()?;

    let kind = move |k: ItemKind| lang.kind_label(k, restore).to_string();
    let note = move |item: &PlanItem| item.note.as_ref().map(|n| lang.note(n)).unwrap_or_default();
    let title = if restore {
        t.preview_restore_title
    } else {
        t.preview_backup_title
    };
    let labels = ExportLabels {
        title,
        kind: &kind,
        note: &note,
        columns: t.csv_columns,
    };

    let result = if csv {
        export::write_csv(plan, &labels, &path)
    } else {
        let summary = plan.summary();
        let lines: Vec<String> = ItemKind::ALL
            .iter()
            .filter(|k| summary.count(**k) > 0)
            .map(|k| {
                format!(
                    "{:<22} {:>10}  {:>10}",
                    lang.kind_label(*k, restore),
                    lang.count(summary.count(*k)),
                    lang.bytes(summary.bytes(*k))
                )
            })
            .collect();
        export::write_text(plan, &labels, &lines, &path)
    };

    Some(match result {
        Ok(()) => (
            NoticeKind::Success,
            lang.exported(&path.display().to_string()),
        ),
        Err(err) => (NoticeKind::Error, format!("{}: {err}", path.display())),
    })
}
