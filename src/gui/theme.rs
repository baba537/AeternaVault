//! The AeternaVault look: calm, warm, archival.
//!
//! * Colors: deep anthracite / warm ivory grounds with a single gold accent.
//! * Type: a serif for headings (Georgia from the Windows font folder) and a
//!   humanist sans for text (Segoe UI). Both are part of every Windows
//!   installation, so nothing has to be bundled or licensed. Custom fonts
//!   (e.g. EB Garamond, Inter) can be set in the configuration file.
//! * Motion: short, soft fades only.
//!
//! See docs/DESIGN.md for the full concept.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Theme, ThemePreference, Visuals, style::WidgetVisuals,
};

use crate::config::{Appearance, Fonts};

pub const SERIF: &str = "serif";
pub const SERIF_ITALIC: &str = "serif-italic";
pub const SANS_STRONG: &str = "sans-strong";

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const DARK: Palette = Palette {
    background: hex(0x14161A),
    panel: hex(0x1C1F24),
    raised: hex(0x252930),
    border: hex(0x2E3239),
    text: hex(0xEDE6D6),
    text_secondary: hex(0x9A958A),
    accent: hex(0xC9A227),
    accent_hover: hex(0xE0B93A),
    on_accent: hex(0x14161A),
    // Slightly lighter than the fill tones so they stay readable as text.
    success: hex(0x8BA877),
    warning: hex(0xD19A3E),
    error: hex(0xC7675A),
};

pub const LIGHT: Palette = Palette {
    background: hex(0xF6F1E7),
    panel: hex(0xFBF8F1),
    raised: hex(0xFFFDF8),
    border: hex(0xE3DACA),
    text: hex(0x1C1F24),
    text_secondary: hex(0x6E6A60),
    accent: hex(0x8C6A1F),
    accent_hover: hex(0xB08A2E),
    on_accent: hex(0xFBF8F1),
    success: hex(0x55703F),
    warning: hex(0x94661B),
    error: hex(0x9A3F32),
};

pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn apply(ctx: &egui::Context, appearance: Appearance) {
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, true));
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, false));
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 6.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.window_margin = Margin::same(20);
        style.spacing.indent = 22.0;
        style.spacing.icon_spacing = 8.0;
        style.animation_time = 0.18;
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(24.0, FontFamily::Name(SERIF.into())),
            ),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(15.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.5, FontFamily::Monospace),
            ),
        ]
        .into();
    });
    ctx.set_theme(match appearance {
        Appearance::System => ThemePreference::System,
        Appearance::Dark => ThemePreference::Dark,
        Appearance::Light => ThemePreference::Light,
    });
}

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(4);

    v.panel_fill = p.background;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 110 } else { 36 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 80 } else { 24 }),
    };
    v.extreme_bg_color = p.background;
    v.text_edit_bg_color = Some(p.background);
    v.faint_bg_color = p.raised;
    v.code_bg_color = p.raised;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.accent.gamma_multiply(if dark { 0.30 } else { 0.22 });
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.striped = false;
    v.indent_has_left_vline = false;
    v.collapsing_header_frame = false;

    let widget = |bg: Color32, stroke: Color32, fg: Color32| WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, stroke),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, fg),
        expansion: 0.0,
    };
    v.widgets.noninteractive = widget(p.panel, p.border, p.text);
    v.widgets.inactive = widget(p.raised, p.border, p.text);
    v.widgets.hovered = widget(p.raised, p.accent, p.text);
    v.widgets.active = widget(p.raised, p.accent_hover, p.text);
    v.widgets.open = widget(p.raised, p.accent, p.text);
    v
}

/// Installs heading and body fonts. Missing files are simply skipped; egui's
/// built-in fonts always remain as fallback (they also cover symbols).
pub fn install_fonts(ctx: &egui::Context, overrides: &Fonts) {
    let mut fonts = FontDefinitions::default();
    let windows_fonts = std::env::var_os("WINDIR")
        .map(|w| PathBuf::from(w).join("Fonts"))
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\Fonts"));

    let mut load = |name: &str, candidates: &[PathBuf]| -> bool {
        for path in candidates {
            if let Some(data) = read_font(path) {
                fonts
                    .font_data
                    .insert(name.to_string(), Arc::new(FontData::from_owned(data)));
                return true;
            }
        }
        false
    };

    let heading_candidates: Vec<PathBuf> = overrides
        .heading
        .iter()
        .cloned()
        .chain([
            windows_fonts.join("georgia.ttf"),
            windows_fonts.join("constan.ttf"),
        ])
        .collect();
    let body_candidates: Vec<PathBuf> = overrides
        .body
        .iter()
        .cloned()
        .chain([windows_fonts.join("segoeui.ttf")])
        .collect();

    let has_serif = load("av-serif", &heading_candidates);
    let has_serif_italic = load("av-serif-italic", &[windows_fonts.join("georgiai.ttf")]);
    let has_sans = load("av-sans", &body_candidates);
    let has_sans_strong = load("av-sans-strong", &[windows_fonts.join("seguisb.ttf")]);

    let defaults = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let family = |primary: &[(&str, bool)]| -> Vec<String> {
        primary
            .iter()
            .filter(|(_, ok)| *ok)
            .map(|(n, _)| n.to_string())
            .chain(defaults.iter().cloned())
            .collect()
    };

    let proportional = family(&[("av-sans", has_sans)]);
    let serif = family(&[("av-serif", has_serif), ("av-sans", has_sans)]);
    let serif_italic = family(&[
        ("av-serif-italic", has_serif_italic),
        ("av-serif", has_serif),
    ]);
    let strong = family(&[("av-sans-strong", has_sans_strong), ("av-sans", has_sans)]);

    fonts
        .families
        .insert(FontFamily::Proportional, proportional);
    fonts.families.insert(FontFamily::Name(SERIF.into()), serif);
    fonts
        .families
        .insert(FontFamily::Name(SERIF_ITALIC.into()), serif_italic);
    fonts
        .families
        .insert(FontFamily::Name(SANS_STRONG.into()), strong);
    ctx.set_fonts(fonts);
}

fn read_font(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    // Very small sanity check for TrueType / OpenType signatures.
    let valid = data.len() > 4 && matches!(&data[..4], [0, 1, 0, 0] | b"OTTO" | b"true");
    if !valid {
        tracing::warn!(
            "ignoring font {}: not a TrueType/OpenType file",
            path.display()
        );
    }
    valid.then_some(data)
}
