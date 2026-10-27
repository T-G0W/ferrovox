use super::theme::{self, pal, BOLD, ICONS};
use eframe::egui::{self, Color32, FontFamily, FontId, Response, RichText, Sense, Stroke, Ui};

#[allow(dead_code)]
pub mod ic {
    pub const PLAY: &str = "\u{E768}";
    pub const PAUSE: &str = "\u{E769}";
    pub const STOP: &str = "\u{E71A}";
    pub const PREV: &str = "\u{E892}";
    pub const NEXT: &str = "\u{E893}";
    pub const REWIND: &str = "\u{EB9E}";
    pub const FASTFWD: &str = "\u{EB9D}";
    pub const VOLUME: &str = "\u{E767}";
    pub const MUTE: &str = "\u{E74F}";
    pub const MIC: &str = "\u{E720}";
    pub const SPEAKERS: &str = "\u{E7F5}";
    pub const HEADSET: &str = "\u{E95B}";
    pub const REPEAT_ALL: &str = "\u{E8EE}";
    pub const REPEAT_ONE: &str = "\u{E8ED}";
    pub const SHUFFLE: &str = "\u{E8B1}";
    pub const RECORD: &str = "\u{E7C8}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const SEARCH: &str = "\u{E721}";
    pub const ADD: &str = "\u{E710}";
    pub const FOLDER: &str = "\u{E8B7}";
    pub const EDIT: &str = "\u{E70F}";
    pub const DELETE: &str = "\u{E74D}";
    pub const COLOR: &str = "\u{E790}";
    pub const KEYBOARD: &str = "\u{E765}";
    pub const ROBOT: &str = "\u{E99A}";
    pub const MESSAGE: &str = "\u{E8BD}";
    pub const HOME: &str = "\u{E80F}";
    pub const MORE: &str = "\u{E712}";
    pub const GLOBE: &str = "\u{E774}";
    pub const LOCK: &str = "\u{E72E}";
    pub const UNLOCK: &str = "\u{E785}";
    pub const BELL: &str = "\u{EA8F}";
    pub const FULLSCREEN: &str = "\u{E740}";
    pub const VIEW: &str = "\u{E890}";
    pub const STAR: &str = "\u{E734}";
    pub const CLOCK: &str = "\u{E823}";
    pub const LIBRARY: &str = "\u{E8F1}";
    pub const GRID: &str = "\u{E80A}";
    pub const LIST: &str = "\u{EA37}";
    pub const UNDO: &str = "\u{E7A7}";
    pub const REDO: &str = "\u{E7A6}";
    pub const DOWNLOAD: &str = "\u{E896}";
    pub const INFO: &str = "\u{E946}";
    pub const WARNING: &str = "\u{E7BA}";
    pub const CHECK: &str = "\u{E73E}";
    pub const CANCEL: &str = "\u{E711}";
    pub const SYNC: &str = "\u{E895}";
    pub const MUSIC: &str = "\u{EC4F}";
    pub const EQUALIZER: &str = "\u{E9E9}";
    pub const POINTER: &str = "\u{E7C9}";
    pub const LAYERS: &str = "\u{E81E}";
    pub const HAMBURGER: &str = "\u{E700}";
    pub const CHEV_DOWN: &str = "\u{E70D}";
    pub const CHEV_RIGHT: &str = "\u{E76C}";
    pub const STOPWATCH: &str = "\u{E916}";
    pub const OVERLAP: &str = "\u{E8A9}";
    pub const HOTKEY: &str = "\u{EDA7}";
    pub const FORWARD: &str = "\u{E72A}";
    pub const CODE: &str = "\u{E943}";
    pub const IMPORT: &str = "\u{E8B5}";
    pub const EXPORT: &str = "\u{EDE1}";
    pub const SAVE: &str = "\u{E74E}";
    pub const PLUG: &str = "\u{E96E}";
    pub const POWER: &str = "\u{E7E8}";
    pub const SHIELD: &str = "\u{EA18}";
    pub const REFRESH: &str = "\u{E72C}";
    pub const MARKER: &str = "\u{E7B7}";
    pub const TRIM: &str = "\u{E78A}";
    pub const CIRCLE_FILL: &str = "\u{EA3B}";
    pub const OPEN_FILE: &str = "\u{E8E5}";
    pub const FONT: &str = "\u{E8D2}";
    pub const DEVICES: &str = "\u{E772}";
}

pub fn icon(code: &str, size: f32) -> RichText {
    RichText::new(code).family(FontFamily::Name(ICONS.into())).size(size)
}

pub fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).family(FontFamily::Name(BOLD.into())).size(size)
}

pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).color(pal().dim)
}

pub fn faint(text: impl Into<String>) -> RichText {
    RichText::new(text).color(pal().faint)
}

pub fn value(text: impl Into<String>) -> RichText {
    RichText::new(text).color(theme::value())
}

pub fn icon_btn(ui: &mut Ui, code: &str, tip: &str, active: bool) -> Response {
    icon_btn_sized(ui, code, tip, active, egui::vec2(28.0, 26.0), 15.0)
}

pub fn icon_btn_sized(ui: &mut Ui, code: &str, tip: &str, active: bool, size: egui::Vec2, icon_size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = pal();
    let bg = if active {
        p.accent
    } else if resp.is_pointer_button_down_on() {
        p.hover
    } else if resp.hovered() {
        p.raised
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, bg);
    let col = if active { Color32::WHITE } else if resp.hovered() { p.text } else { p.dim };
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, code, FontId::new(icon_size, FontFamily::Name(ICONS.into())), col);
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    }
}

pub fn text_btn(ui: &mut Ui, code: &str, text: &str) -> Response {
    let job = {
        let mut j = egui::text::LayoutJob::default();
        j.append(code, 0.0, egui::TextFormat { font_id: FontId::new(12.0, FontFamily::Name(ICONS.into())), color: pal().text, valign: egui::Align::Center, ..Default::default() });
        j.append(&format!("  {text}"), 0.0, egui::TextFormat { font_id: FontId::proportional(13.0), color: pal().text, valign: egui::Align::Center, ..Default::default() });
        j
    };
    ui.add(egui::Button::new(job).fill(pal().raised).stroke(Stroke::new(1.0_f32, pal().border)))
}

pub fn pill(ui: &mut Ui, code: &str, text: &str, fill: Color32) -> Response {
    let font = FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    let size = egui::vec2(galley.size().x + 44.0, 26.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if resp.hovered() { fill.gamma_multiply(1.15) } else { fill };
    let painter = ui.painter();
    painter.rect_filled(rect, 13.0, fill);
    painter.text(rect.left_center() + egui::vec2(16.0, 0.0), egui::Align2::CENTER_CENTER, code, FontId::new(13.0, FontFamily::Name(ICONS.into())), Color32::WHITE);
    painter.galley(egui::pos2(rect.left() + 30.0, rect.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
    resp
}

pub fn check(ui: &mut Ui, value: &mut bool, text: impl Into<String>) -> Response {
    let text = text.into();
    let font = FontId::proportional(13.0);
    let galley = ui.painter().layout(text, font, pal().text, ui.available_width().max(60.0) - 24.0);
    let size = egui::vec2(20.0 + galley.size().x, galley.size().y.max(18.0));
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *value = !*value;
        resp.mark_changed();
    }
    let p = pal();
    let boxr = egui::Rect::from_center_size(egui::pos2(rect.left() + 7.0, rect.center().y), egui::vec2(14.0, 14.0));
    let painter = ui.painter();
    if *value {
        painter.rect_filled(boxr, 3.0, p.accent);
        painter.text(boxr.center(), egui::Align2::CENTER_CENTER, ic::CHECK, FontId::new(10.0, FontFamily::Name(ICONS.into())), Color32::WHITE);
    } else {
        painter.rect_filled(boxr, 3.0, p.raised);
        painter.rect_stroke(boxr, 3.0, Stroke::new(1.0_f32, if resp.hovered() { p.dim } else { p.border }));
    }
    let col = if ui.is_enabled() { p.text } else { p.faint };
    painter.galley(egui::pos2(rect.left() + 20.0, rect.center().y - galley.size().y / 2.0), galley, col);
    resp
}

pub fn focused_panel(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.data(|d| d.get_temp::<egui::Id>(egui::Id::new("focused_panel")))
}

pub fn card<R>(
    ui: &mut Ui,
    id: &str,
    tabs: &[&str],
    active: &mut usize,
    header_right: impl FnOnce(&mut Ui),
    add: impl FnOnce(&mut Ui, usize) -> R,
) -> R {
    let p = pal();
    let rect = ui.available_rect_before_wrap();
    let pid = egui::Id::new(("panel", id));
    let focused = focused_panel(ui.ctx()) == Some(pid);
    if ui.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|pos| rect.contains(pos))) {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("focused_panel"), pid));
    }
    ui.painter().rect_filled(rect, 5.0, p.panel);

    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 30.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(header.shrink2(egui::vec2(10.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    for (i, t) in tabs.iter().enumerate() {
        let is_active = i == *active;
        let col = if is_active { p.text } else { p.dim };
        let galley = child.painter().layout_no_wrap(t.to_string(), FontId::proportional(13.0), col);
        let w = galley.size().x + if is_active && tabs.len() > 0 { 22.0 } else { 6.0 };
        let (r, resp) = child.allocate_exact_size(egui::vec2(w, 22.0), Sense::click());
        if resp.clicked() {
            *active = i;
        }
        let painter = child.painter();
        let text_pos = egui::pos2(r.left() + 3.0, r.center().y - galley.size().y / 2.0);
        let text_w = galley.size().x;
        painter.galley(text_pos, galley, col);
        if is_active {
            painter.hline(text_pos.x..=text_pos.x + text_w, r.bottom() - 1.0, Stroke::new(1.0_f32, p.text));
            painter.text(
                egui::pos2(text_pos.x + text_w + 12.0, r.center().y),
                egui::Align2::CENTER_CENTER,
                ic::HAMBURGER,
                FontId::new(10.0, FontFamily::Name(ICONS.into())),
                p.dim,
            );
        }
        child.add_space(10.0);
    }
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), header_right);

    let body = egui::Rect::from_min_max(egui::pos2(rect.left(), header.bottom()), rect.max).shrink2(egui::vec2(10.0, 6.0));
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
    content.set_clip_rect(body.expand(2.0).intersect(ui.clip_rect()));
    let r = add(&mut content, *active);
    ui.allocate_rect(rect, Sense::hover());
    if focused {
        ui.painter().rect_stroke(rect.shrink(0.5), 5.0, Stroke::new(1.0_f32, p.accent));
    }
    r
}

pub fn separator(ui: &mut Ui) {
    let r = ui.available_rect_before_wrap();
    let y = ui.cursor().top() + 3.0;
    ui.painter().hline(r.x_range(), y, Stroke::new(1.0_f32, pal().border));
    ui.add_space(7.0);
}

pub fn short_time(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}.{}", s / 60, s % 60, (ms / 100) % 10)
}

pub fn color_square(ui: &mut Ui, color: Color32, size: f32) -> Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());
    ui.painter().rect_filled(r, 2.0, color);
    resp
}
