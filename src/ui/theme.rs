use crate::library::Rgb;
use crate::settings::Theme;
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, Rounding, Stroke, Visuals};
use std::sync::RwLock;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_dim: Color32,
    pub value: Color32,
    pub row_sel: Color32,
    pub row_alt: Color32,
}

const fn c(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const STUDIO: Palette = Palette {
    bg: c(0x0D0D0E),
    panel: c(0x1A1A1B),
    raised: c(0x252527),
    hover: c(0x303033),
    border: c(0x2B2B2E),
    text: c(0xE4E4E7),
    dim: c(0x9A9AA0),
    faint: c(0x66666C),
    accent: c(0x3B82F6),
    accent_dim: c(0x1E4FA8),
    value: c(0x5B9BFF),
    row_sel: c(0x2C2C30),
    row_alt: c(0x1E1E20),
};

const EMBER: Palette = Palette {
    accent: c(0xF2701F),
    accent_dim: c(0x9C4210),
    value: c(0xFF9A52),
    ..STUDIO
};

const LIGHT: Palette = Palette {
    bg: c(0xC9CBD0),
    panel: c(0xF3F4F6),
    raised: c(0xE3E5E9),
    hover: c(0xD6D9DF),
    border: c(0xCDD0D6),
    text: c(0x1C1D21),
    dim: c(0x5D6068),
    faint: c(0x8B8F98),
    accent: c(0x2563EB),
    accent_dim: c(0x93B4F5),
    value: c(0x1D5BD8),
    row_sel: c(0xDCE4F7),
    row_alt: c(0xECEDF0),
};

static PAL: RwLock<Palette> = RwLock::new(STUDIO);

pub fn pal() -> Palette {
    *PAL.read().unwrap_or_else(|e| e.into_inner())
}

pub fn accent() -> Color32 {
    pal().accent
}

pub fn accent_dim() -> Color32 {
    pal().accent_dim
}

pub fn value() -> Color32 {
    pal().value
}

pub const OK: Color32 = c(0x4CC36B);
pub const WARN: Color32 = c(0xF2B134);
pub const ERR: Color32 = c(0xE5484D);

pub const PALETTE: &[Rgb] = &[
    [0xE5, 0x48, 0x4D],
    [0xFF, 0x7A, 0x1F],
    [0xF2, 0xB1, 0x34],
    [0xE8, 0xD8, 0x4A],
    [0x7E, 0xC8, 0x4B],
    [0x2E, 0xA0, 0x5A],
    [0x2C, 0xB5, 0xA9],
    [0x38, 0x9B, 0xE8],
    [0x3C, 0x5B, 0xD6],
    [0x8A, 0x5C, 0xE6],
    [0xC2, 0x4F, 0xD9],
    [0xE8, 0x5D, 0xA8],
    [0x9C, 0x6B, 0x45],
    [0x8A, 0x90, 0x9C],
];

pub fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub const ICONS: &str = "icons";
pub const BOLD: &str = "bold";

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::path::Path::new("C:\\Windows\\Fonts");
    let read = |f: &str| std::fs::read(dir.join(f)).ok();

    let body = read("SegUIVar.ttf").or_else(|| read("segoeui.ttf"));
    if let Some(bytes) = body {
        fonts.font_data.insert("ui".into(), FontData::from_owned(bytes));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    if let Some(bytes) = read("seguisym.ttf") {
        fonts.font_data.insert("symbols".into(), FontData::from_owned(bytes));
        fonts.families.entry(FontFamily::Proportional).or_default().push("symbols".into());
    }
    let proportional = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let mut bold = Vec::new();
    if let Some(bytes) = read("seguisb.ttf") {
        fonts.font_data.insert("ui-bold".into(), FontData::from_owned(bytes));
        bold.push("ui-bold".to_string());
    }
    bold.extend(proportional.iter().cloned());
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold);

    let mut icons = Vec::new();
    if let Some(bytes) = read("SegoeIcons.ttf").or_else(|| read("segmdl2.ttf")) {
        let mut fd = FontData::from_owned(bytes);
        fd.tweak.y_offset_factor = 0.08;
        fonts.font_data.insert("fluent".into(), fd);
        icons.push("fluent".to_string());
    }
    icons.extend(proportional.iter().cloned());
    fonts.families.insert(FontFamily::Name(ICONS.into()), icons);

    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, theme: Theme, scale: f32) {
    let p = match theme {
        Theme::Studio => STUDIO,
        Theme::Ember => EMBER,
        Theme::Light => LIGHT,
    };
    *PAL.write().unwrap_or_else(|e| e.into_inner()) = p;

    let mut v = if theme == Theme::Light { Visuals::light() } else { Visuals::dark() };
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = if theme == Theme::Light { c(0xFFFFFF) } else { c(0x121213) };
    v.faint_bg_color = p.row_alt;
    v.code_bg_color = p.raised;
    v.override_text_color = None;
    v.hyperlink_color = p.value;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_shadow = egui::Shadow { offset: egui::vec2(0.0, 8.0), blur: 24.0, spread: 0.0, color: Color32::from_black_alpha(140) };
    v.popup_shadow = egui::Shadow { offset: egui::vec2(0.0, 4.0), blur: 12.0, spread: 0.0, color: Color32::from_black_alpha(120) };
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.slider_trailing_fill = true;
    v.striped = false;

    let r = Rounding::same(4.0);
    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.noninteractive.weak_bg_fill = p.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.inactive.bg_fill = p.raised;
    v.widgets.inactive.weak_bg_fill = p.raised;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.border);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.weak_bg_fill = p.accent_dim;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.widgets.open.bg_fill = p.hover;
    v.widgets.open.weak_bg_fill = p.hover;
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, p.border);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = r;
        w.expansion = 0.0;
    }
    v.window_rounding = Rounding::same(8.0);
    v.menu_rounding = Rounding::same(6.0);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.spacing.menu_margin = egui::Margin::same(6.0);
        s.spacing.window_margin = egui::Margin::same(10.0);
        s.spacing.slider_width = 120.0;
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
        s.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
        s.text_styles.insert(egui::TextStyle::Heading, egui::FontId::new(17.0, FontFamily::Name(BOLD.into())));
    });
    ctx.set_zoom_factor(scale.clamp(0.7, 2.0));
}

pub fn level_to_meter(peak: f32) -> f32 {
    if peak <= 0.001 {
        0.0
    } else {
        ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
    }
}

pub fn meter(ui: &mut egui::Ui, level: f32, width: f32, label: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 6.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, pal().raised);
    let l = level.clamp(0.0, 1.0);
    if l > 0.0 {
        let mut r = rect;
        r.set_width(rect.width() * l);
        p.rect_filled(r, 2.0, if l > 0.92 { ERR } else if l > 0.75 { WARN } else { OK });
    }
    resp.on_hover_text(label)
}
