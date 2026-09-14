//! Small, consistent building blocks for the AeternaVault interface.

use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Galley, Layout,
    Margin, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, WidgetText, text::LayoutJob,
    text::TextWrapping,
};
use std::f32::consts::TAU;
use std::sync::Arc;

use super::theme::{Palette, SANS_STRONG, SERIF, SERIF_ITALIC, palette};

/// A quiet panel with a hairline border.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = palette(ui);
    Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(22, 18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Letter-spaced small capitals with a short gold rule, like a museum label.
pub fn section_title(ui: &mut Ui, text: &str) {
    let p = palette(ui);
    let mut job = LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: FontId::new(11.5, FontFamily::Name(SANS_STRONG.into())),
            extra_letter_spacing: 1.4,
            color: p.text_secondary,
            ..Default::default()
        },
    );
    ui.label(job);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.accent);
    ui.add_space(4.0);
}

pub fn title(ui: &mut Ui, text: &str) {
    let p = palette(ui);
    ui.label(
        egui::RichText::new(text)
            .family(FontFamily::Name(SERIF.into()))
            .size(28.0)
            .color(p.text),
    );
}

pub fn slogan(ui: &mut Ui, text: &str) {
    let p = palette(ui);
    ui.label(
        egui::RichText::new(text)
            .family(FontFamily::Name(SERIF_ITALIC.into()))
            .size(14.0)
            .color(p.text_secondary),
    );
}

pub fn secondary_text(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = palette(ui);
    ui.label(
        egui::RichText::new(text.into())
            .size(13.0)
            .color(p.text_secondary),
    )
}

pub fn strong_text(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = palette(ui);
    ui.label(
        egui::RichText::new(text.into())
            .family(FontFamily::Name(SANS_STRONG.into()))
            .color(p.text),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Quiet,
}

/// Custom-painted button: gold for the one main action, outlined otherwise.
pub fn button(ui: &mut Ui, kind: ButtonKind, text: &str, enabled: bool) -> Response {
    let p = *palette(ui);
    let font = FontId::new(15.0, FontFamily::Name(SANS_STRONG.into()));
    let galley = WidgetText::from(egui::RichText::new(text).font(font)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let padding = match kind {
        ButtonKind::Quiet => egui::vec2(10.0, 6.0),
        _ => egui::vec2(18.0, 8.0),
    };
    let mut size = galley.size() + 2.0 * padding;
    size.y = size.y.max(if kind == ButtonKind::Quiet {
        30.0
    } else {
        36.0
    });
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);

    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let pressed = enabled && response.is_pointer_button_down_on();
        let painter = ui.painter();
        let (fill, stroke, text_color) = match kind {
            ButtonKind::Primary => {
                let fill = if pressed {
                    p.accent
                } else if hovered {
                    p.accent_hover
                } else {
                    p.accent
                };
                (fill, Stroke::NONE, p.on_accent)
            }
            ButtonKind::Secondary => (
                if hovered { p.raised } else { p.panel },
                Stroke::new(1.0, if hovered { p.accent } else { p.border }),
                p.text,
            ),
            ButtonKind::Quiet => (
                if hovered {
                    p.raised
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::NONE,
                if hovered { p.text } else { p.text_secondary },
            ),
        };
        let alpha = if enabled { 1.0 } else { 0.45 };
        painter.rect(
            rect,
            CornerRadius::same(4),
            fill.gamma_multiply(alpha),
            Stroke::new(stroke.width, stroke.color.gamma_multiply(alpha)),
            egui::StrokeKind::Inside,
        );
        let pos = rect.center() - galley.size() / 2.0;
        painter.galley(pos, galley, text_color.gamma_multiply(alpha));
    }

    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Navigation entry with a gold underline when selected.
pub fn nav_tab(ui: &mut Ui, text: &str, selected: bool, enabled: bool) -> Response {
    let p = *palette(ui);
    let font = FontId::new(15.0, FontFamily::Name(SANS_STRONG.into()));
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, p.text);
    let size = egui::vec2(galley.size().x + 4.0, galley.size().y + 14.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let color = if selected || (enabled && response.hovered()) {
        p.text
    } else {
        p.text_secondary
    };
    let text_pos = Pos2::new(rect.left() + 2.0, rect.top() + 3.0);
    ui.painter().galley(
        text_pos,
        galley,
        color.gamma_multiply(if enabled { 1.0 } else { 0.5 }),
    );
    if selected {
        let y = rect.bottom() - 2.0;
        ui.painter().line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(2.0, p.accent),
        );
    }
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

pub fn status_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// A thin, calm progress line. `None` shows a slowly travelling segment.
pub fn progress_line(ui: &mut Ui, fraction: Option<f32>) {
    let p = *palette(ui);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 4.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), p.raised);
    match fraction {
        Some(f) => {
            let mut filled = rect;
            filled.set_width(rect.width() * f.clamp(0.0, 1.0));
            painter.rect_filled(filled, CornerRadius::same(2), p.accent);
        }
        None => {
            let t = ui.input(|i| i.time) as f32;
            let phase = (t / 2.6).fract();
            let segment = rect.width() * 0.22;
            let x = rect.left() - segment + (rect.width() + segment) * phase;
            let bar = Rect::from_min_max(
                Pos2::new(x.max(rect.left()), rect.top()),
                Pos2::new((x + segment).min(rect.right()), rect.bottom()),
            );
            painter.rect_filled(bar, CornerRadius::same(2), p.accent);
            ui.ctx().request_repaint();
        }
    }
}

/// Lays out a single line that is shortened with "…" to fit `width`.
pub fn truncated(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = TextWrapping::truncate_at_width(width.max(10.0));
    ui.painter().layout_job(job)
}

/// Paints a truncated line of text into `rect`.
pub fn paint_cell(ui: &Ui, rect: Rect, text: &str, font: FontId, color: Color32, align: Align) {
    let galley = truncated(ui, text, font, color, rect.width());
    let y = rect.center().y - galley.size().y / 2.0;
    let x = match align {
        Align::Max => rect.right() - galley.size().x,
        Align::Center => rect.center().x - galley.size().x / 2.0,
        Align::Min => rect.left(),
    };
    ui.painter()
        .with_clip_rect(rect)
        .galley(Pos2::new(x, y), galley, color);
}

/// Stretches its contents to the full width and centers a column of at most `max_width`.
pub fn centered_column<R>(ui: &mut Ui, max_width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let available = ui.available_width();
    let width = available.min(max_width);
    let margin = ((available - width) / 2.0).max(0.0);
    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.add_space(margin);
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(width);
                add(ui)
            },
        )
        .inner
    })
    .inner
}

pub enum NoticeKind {
    Info,
    Success,
    Warning,
    Error,
}

/// A calm message strip with a colored edge. Returns `true` when dismissed.
pub fn notice(ui: &mut Ui, kind: &NoticeKind, text: &str) -> bool {
    let p = *palette(ui);
    let edge = kind_color(&p, kind);
    let mut dismissed = false;
    let response = Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin {
            left: 18,
            right: 10,
            top: 10,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let close_width = 36.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width() - close_width, 0.0),
                    Layout::left_to_right(Align::Center).with_main_wrap(true),
                    |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(text).color(p.text)).wrap());
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if button(ui, ButtonKind::Quiet, "×", true).clicked() {
                        dismissed = true;
                    }
                });
            });
        });
    let r = response.response.rect;
    ui.painter().rect_filled(
        Rect::from_min_max(r.min, Pos2::new(r.min.x + 3.0, r.max.y)),
        CornerRadius {
            nw: 6,
            sw: 6,
            ne: 0,
            se: 0,
        },
        edge,
    );
    dismissed
}

pub fn kind_color(p: &Palette, kind: &NoticeKind) -> Color32 {
    match kind {
        NoticeKind::Info => p.accent,
        NoticeKind::Success => p.success,
        NoticeKind::Warning => p.warning,
        NoticeKind::Error => p.error,
    }
}

/// The AeternaVault mark: a vault door dial with an infinity sign as handle.
pub fn logo(ui: &mut Ui, size: f32) -> Response {
    let p = *palette(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    let c = rect.center();
    let s = size;

    painter.circle_stroke(c, s * 0.44, Stroke::new((s * 0.045).max(1.4), p.accent));
    painter.circle_stroke(
        c,
        s * 0.32,
        Stroke::new((s * 0.02).max(1.0), p.accent.gamma_multiply(0.8)),
    );
    for i in 0..12 {
        let a = i as f32 * TAU / 12.0;
        let dir = egui::vec2(a.cos(), a.sin());
        painter.line_segment(
            [c + dir * (s * 0.35), c + dir * (s * 0.405)],
            Stroke::new((s * 0.02).max(1.0), p.accent.gamma_multiply(0.8)),
        );
    }

    // Lemniscate of Bernoulli.
    let a = s * 0.22;
    let points: Vec<Pos2> = (0..72)
        .map(|i| {
            let t = i as f32 * TAU / 72.0;
            let den = 1.0 + t.sin().powi(2);
            c + egui::vec2(a * t.cos() / den, a * t.sin() * t.cos() / den)
        })
        .collect();
    painter.add(egui::Shape::closed_line(
        points,
        Stroke::new((s * 0.05).max(1.6), p.accent_hover),
    ));
    response
}
