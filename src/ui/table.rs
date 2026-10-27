use super::kit::{self, ic};
use super::theme::{self, pal};
use super::{CaptureTarget, Dialog, FerrovoxApp, SortCol};
use crate::actions::{Action, Source};
use crate::library::{format_duration, HotbarSlot, Sound, SoundId};
use crate::settings::PlayMode;
use crate::t;
use eframe::egui::{self, Color32, FontFamily, FontId, Rect, RichText, Stroke};
use std::collections::HashMap;

pub struct DragSounds(pub Vec<SoundId>);

enum RowEvent {
    Click(SoundId, egui::Modifiers),
    Play(SoundId),
    Hotkey(SoundId),
    MoveBefore(Vec<SoundId>, SoundId),
}

#[derive(Clone, Copy, PartialEq)]
enum Col {
    Play,
    Index,
    Name,
    Artist,
    Duration,
    Hotkey,
    Plays,
    LastPlayed,
    Added,
    Path,
}

impl Col {
    fn sort(self) -> Option<SortCol> {
        Some(match self {
            Col::Play => return None,
            Col::Index => SortCol::Index,
            Col::Name => SortCol::Name,
            Col::Artist => SortCol::Artist,
            Col::Duration => SortCol::Duration,
            Col::Hotkey => SortCol::Hotkey,
            Col::Plays => SortCol::Plays,
            Col::LastPlayed => SortCol::LastPlayed,
            Col::Added => SortCol::Added,
            Col::Path => SortCol::Path,
        })
    }

    fn title(self) -> &'static str {
        match self {
            Col::Play => "",
            Col::Index => "#",
            Col::Name => t!("Titel / Tag", "Title / tag"),
            Col::Artist => t!("Interpret", "Artist"),
            Col::Duration => t!("Dauer", "Duration"),
            Col::Hotkey => "Hotkey",
            Col::Plays => t!("Gespielt", "Plays"),
            Col::LastPlayed => t!("Zuletzt", "Last played"),
            Col::Added => t!("Hinzugefügt", "Added"),
            Col::Path => t!("Pfad", "Path"),
        }
    }
}

pub fn panel(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let core = app.core.clone();
    let title = {
        let rt = core.rt.lock();
        if !rt.search.is_empty() {
            t!("Suchergebnisse", "Search results").to_string()
        } else {
            rt.selected_category
                .and_then(|c| core.lib.read().category(c).map(|c| format!("{} {}", c.icon, c.name)))
                .unwrap_or_else(|| t!("Alle Sounds", "All sounds").to_string())
        }
    };
    let mut tab = 0usize;
    let mut add = false;
    kit::card(
        ui,
        "list",
        &[title.as_str()],
        &mut tab,
        |ui| {
            if kit::icon_btn_sized(ui, ic::ADD, t!("Sounds hinzufügen", "Add sounds"), false, egui::vec2(22.0, 20.0), 11.0).clicked() {
                add = true;
            }
        },
        |ui, _| sound_list(app, ui),
    );
    if add {
        app.add_files_dialog();
    }
}

fn sound_list(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let core = app.core.clone();
    let p = pal();
    let s = core.settings.read().clone();
    let (sel_cat, mut search, selected, scroll_to, current_voice, current) = {
        let mut rt = core.rt.lock();
        let scroll = rt.scroll_to.take();
        (rt.selected_category, rt.search.clone(), rt.selected.clone(), scroll, rt.current_voice, rt.current)
    };
    let playing = current_voice.and(current);

    let (mut rows, index_of, empty_lib) = {
        let lib = core.lib.read();
        let needle = search.to_lowercase();
        let base = if !needle.is_empty() && s.search_all_categories { lib.view(None) } else { lib.view(sel_cat) };
        let rows: Vec<Sound> = base.iter().filter_map(|id| lib.sound(*id)).filter(|x| x.matches(&needle)).cloned().collect();
        let index_of: HashMap<SoundId, usize> = lib.sounds.iter().enumerate().map(|(i, x)| (x.id, i + 1)).collect();
        (rows, index_of, lib.sounds.is_empty())
    };

    if let Some((col, asc)) = app.st.sort.filter(|_| !s.disable_sorting) {
        rows.sort_by(|a, b| {
            let o = match col {
                SortCol::Index => index_of.get(&a.id).cmp(&index_of.get(&b.id)),
                SortCol::Name => a.display_name().to_lowercase().cmp(&b.display_name().to_lowercase()),
                SortCol::Artist => a.artist.to_lowercase().cmp(&b.artist.to_lowercase()),
                SortCol::Duration => a.duration_ms.cmp(&b.duration_ms),
                SortCol::Hotkey => a.hotkey.map(|h| h.label()).cmp(&b.hotkey.map(|h| h.label())),
                SortCol::Plays => a.play_count.cmp(&b.play_count),
                SortCol::LastPlayed => a.last_played.cmp(&b.last_played),
                SortCol::Added => a.added_on.cmp(&b.added_on),
                SortCol::Path => a.path.cmp(&b.path),
            };
            if asc {
                o
            } else {
                o.reverse()
            }
        });
    }
    let order: Vec<SoundId> = rows.iter().map(|r| r.id).collect();
    {
        let mut rt = core.rt.lock();
        if rt.view_order != order {
            rt.view_order = order.clone();
        }
    }

    ui.horizontal(|ui| {
        let w = (ui.available_width() * 0.45).clamp(160.0, 320.0);
        let resp = ui.allocate_ui(egui::vec2(w, 24.0), |ui| super::sidebar::search_box(ui, &mut search, t!("Suchen …  (Strg+F)", "Search …  (Ctrl+F)"))).inner;
        if app.st.request_search_focus || std::mem::take(&mut core.rt.lock().focus_search) {
            resp.request_focus();
            app.st.request_search_focus = false;
        }
        if resp.changed() {
            core.rt.lock().search = search.clone();
        }
        if !search.is_empty() && kit::icon_btn_sized(ui, ic::CANCEL, t!("Suche zurücksetzen", "Reset search"), false, egui::vec2(22.0, 22.0), 10.0).clicked() {
            core.rt.lock().search.clear();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let total: u64 = rows.iter().map(|r| r.duration_ms).sum();
            ui.label(kit::faint(format!("{} Sounds · {}", rows.len(), format_duration(total))).small());
            if app.st.color_pick.is_some() {
                ui.label(RichText::new(t!("Farbwerkzeug aktiv – auf einen Sound klicken (Esc beendet)", "Color tool active – click a sound (Esc ends)")).small().color(theme::value()));
            }
        });
    });
    ui.add_space(6.0);

    if empty_lib {
        empty_state(app, ui);
        return;
    }

    let c = &s.columns;
    let mut cols: Vec<(Col, f32)> = vec![(Col::Play, 26.0)];
    if c.index {
        cols.push((Col::Index, 40.0));
    }
    cols.push((Col::Name, 0.0));
    if c.artist {
        cols.push((Col::Artist, 150.0));
    }
    if c.duration {
        cols.push((Col::Duration, 64.0));
    }
    if c.hotkey {
        cols.push((Col::Hotkey, 132.0));
    }
    if c.play_count {
        cols.push((Col::Plays, 66.0));
    }
    if c.last_played {
        cols.push((Col::LastPlayed, 92.0));
    }
    if c.added_on {
        cols.push((Col::Added, 92.0));
    }
    if c.path {
        cols.push((Col::Path, 220.0));
    }
    let full = ui.available_rect_before_wrap();
    let fixed: f32 = cols.iter().map(|(_, w)| *w).sum();
    let name_w = (full.width() - fixed - 8.0).max(140.0);
    let mut xs = Vec::with_capacity(cols.len());
    let mut x = full.left() + 4.0;
    for (col, w) in &cols {
        let w = if *col == Col::Name { name_w } else { *w };
        xs.push((*col, x, w));
        x += w;
    }

    let head = Rect::from_min_size(full.min, egui::vec2(full.width(), 24.0));
    for (col, x, w) in &xs {
        let r = Rect::from_min_size(egui::pos2(*x, head.top()), egui::vec2(*w, head.height()));
        let Some(sc) = col.sort() else {
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, ic::VOLUME, FontId::new(11.0, FontFamily::Name(theme::ICONS.into())), p.faint);
            continue;
        };
        let resp = ui.interact(r, egui::Id::new(("colhead", col.title())), egui::Sense::click());
        let arrow = match app.st.sort {
            Some((c, asc)) if c == sc => {
                if asc {
                    "  ▲"
                } else {
                    "  ▼"
                }
            }
            _ => "",
        };
        let col_text = if resp.hovered() { p.text } else { p.dim };
        ui.painter().text(r.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, format!("{}{arrow}", col.title()), FontId::proportional(12.0), col_text);
        if resp.clicked() && !s.disable_sorting {
            app.st.sort = match app.st.sort {
                Some((c, true)) if c == sc => Some((sc, false)),
                Some((c, false)) if c == sc => None,
                _ => Some((sc, true)),
            };
        }
    }
    ui.painter().hline(full.x_range(), head.bottom(), Stroke::new(1.0_f32, p.border));
    if rows.is_empty() {
        ui.painter().text(
            egui::pos2(full.center().x, head.bottom() + 60.0),
            egui::Align2::CENTER_CENTER,
            if search.is_empty() { t!("Diese Kategorie ist leer – Sounds hierher ziehen", "This category is empty – drag sounds here") } else { t!("Keine Treffer", "No hits") },
            FontId::proportional(13.0),
            p.faint,
        );
    }

    let row_h = if s.compact_list { 22.0 } else { 28.0 };
    let rows_rect = Rect::from_min_max(egui::pos2(full.left(), head.bottom() + 2.0), full.max);
    let mut events: Vec<RowEvent> = Vec::new();
    let mut area = egui::ScrollArea::vertical().id_salt("sound_rows").auto_shrink([false, false]);
    if let Some(i) = scroll_to.and_then(|id| order.iter().position(|x| *x == id)) {
        area = area.vertical_scroll_offset((i as f32 * row_h - rows_rect.height() / 2.0).max(0.0));
    }
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rows_rect), |ui| {
        area.show_rows(ui, row_h, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in range {
                let snd = &rows[i];
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(rows_rect.width(), row_h), egui::Sense::click_and_drag());
                let is_sel = selected.contains(&snd.id);
                let is_playing = playing == Some(snd.id);
                let missing = !snd.path.exists();
                let painter = ui.painter();
                if is_sel {
                    painter.rect_filled(rect, 4.0, p.row_sel);
                } else if resp.hovered() {
                    painter.rect_filled(rect, 4.0, p.raised.gamma_multiply(0.7));
                } else if s.alternating_rows && i % 2 == 1 {
                    painter.rect_filled(rect, 0.0, p.row_alt);
                }
                if let Some(col) = snd.color {
                    painter.rect_filled(Rect::from_min_size(rect.min + egui::vec2(0.0, 4.0), egui::vec2(3.0, rect.height() - 8.0)), 1.0, theme::rgb(col));
                }
                let y = rect.center().y;
                let text_col = if missing { theme::ERR } else if is_playing { theme::value() } else { p.text };
                for (col, x, w) in &xs {
                    let cell = Rect::from_min_size(egui::pos2(*x, rect.top()), egui::vec2(*w, rect.height()));
                    let clip = painter.with_clip_rect(cell.shrink2(egui::vec2(4.0, 0.0)).intersect(ui.clip_rect()));
                    let left = egui::pos2(x + 4.0, y);
                    match col {
                        Col::Play => {
                            let r = Rect::from_center_size(cell.center(), egui::vec2(22.0, row_h));
                            let pr = ui.interact(r, egui::Id::new(("rowplay", snd.id)), egui::Sense::click());
                            let icon_col = if is_playing { theme::value() } else if pr.hovered() { p.text } else if resp.hovered() { p.dim } else { p.faint };
                            painter.text(r.center(), egui::Align2::CENTER_CENTER, if is_playing { ic::VOLUME } else { ic::PLAY }, FontId::new(11.0, FontFamily::Name(theme::ICONS.into())), icon_col);
                            if pr.clicked() {
                                events.push(RowEvent::Play(snd.id));
                            }
                        }
                        Col::Index => {
                            clip.text(left, egui::Align2::LEFT_CENTER, index_of.get(&snd.id).copied().unwrap_or(0).to_string(), FontId::proportional(12.0), p.dim);
                        }
                        Col::Name => {
                            let mut name = snd.display_name().to_string();
                            if missing {
                                name.push_str(t!("  (Datei fehlt)", "  (file missing)"));
                            }
                            let font = if is_playing || is_sel { FontId::new(13.0, FontFamily::Name(theme::BOLD.into())) } else { FontId::proportional(13.0) };
                            clip.text(left, egui::Align2::LEFT_CENTER, name, font, text_col);
                        }
                        Col::Artist => {
                            clip.text(left, egui::Align2::LEFT_CENTER, &snd.artist, FontId::proportional(12.5), p.dim);
                        }
                        Col::Duration => {
                            clip.text(left, egui::Align2::LEFT_CENTER, format_duration(snd.duration_ms), FontId::proportional(12.5), theme::value());
                        }
                        Col::Hotkey => {
                            let hk_r = Rect::from_min_size(egui::pos2(x + 2.0, rect.top() + 3.0), egui::vec2(w - 10.0, row_h - 6.0));
                            let hk = ui.interact(hk_r, egui::Id::new(("rowhk", snd.id)), egui::Sense::click());
                            painter.rect_filled(hk_r, 4.0, if hk.hovered() { p.hover } else { p.raised.gamma_multiply(0.8) });
                            let (txt, col) = match snd.hotkey {
                                Some(h) => (h.label(), theme::value()),
                                None => (t!("Keiner", "None").to_string(), p.faint),
                            };
                            painter.with_clip_rect(hk_r.shrink2(egui::vec2(6.0, 0.0)).intersect(ui.clip_rect())).text(
                                hk_r.left_center() + egui::vec2(8.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                txt,
                                FontId::proportional(12.0),
                                col,
                            );
                            painter.text(hk_r.right_center() - egui::vec2(9.0, 0.0), egui::Align2::CENTER_CENTER, ic::CHEV_DOWN, FontId::new(8.0, FontFamily::Name(theme::ICONS.into())), p.faint);
                            if hk.clicked() {
                                events.push(RowEvent::Hotkey(snd.id));
                            }
                            let _ = hk.on_hover_text(t!("Klicken, um einen Hotkey festzulegen", "Click to set a hotkey"));
                        }
                        Col::Plays => {
                            clip.text(left, egui::Align2::LEFT_CENTER, snd.play_count.to_string(), FontId::proportional(12.5), p.dim);
                        }
                        Col::LastPlayed => {
                            let d = snd.last_played.as_deref().map(|d| s.date_format.format(d)).unwrap_or_default();
                            clip.text(left, egui::Align2::LEFT_CENTER, d, FontId::proportional(12.0), p.dim);
                        }
                        Col::Added => {
                            clip.text(left, egui::Align2::LEFT_CENTER, s.date_format.format(&snd.added_on), FontId::proportional(12.0), p.dim);
                        }
                        Col::Path => {
                            clip.text(left, egui::Align2::LEFT_CENTER, snd.path.display().to_string(), FontId::proportional(11.5), p.faint);
                        }
                    }
                }

                if resp.double_clicked() {
                    events.push(RowEvent::Play(snd.id));
                } else if resp.clicked() {
                    events.push(RowEvent::Click(snd.id, ui.input(|inp| inp.modifiers)));
                }
                if resp.secondary_clicked() && !is_sel {
                    events.push(RowEvent::Click(snd.id, egui::Modifiers::NONE));
                }
                if resp.drag_started() {
                    let ids = if is_sel { selected.clone() } else { vec![snd.id] };
                    resp.dnd_set_drag_payload(DragSounds(ids));
                }
                if let Some(payload) = resp.dnd_hover_payload::<DragSounds>() {
                    if !payload.0.contains(&snd.id) {
                        ui.ctx()
                            .layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dropline")))
                            .hline(rect.x_range(), rect.top(), Stroke::new(2.0_f32, theme::accent()));
                    }
                }
                if let Some(payload) = resp.dnd_release_payload::<DragSounds>() {
                    if !payload.0.contains(&snd.id) {
                        events.push(RowEvent::MoveBefore(payload.0.clone(), snd.id));
                    }
                }
                if resp.hovered() {
                    app.st.hint = t!(
                        "Doppelklick spielt ab · Rechtsklick für mehr · auf Kategorie oder Hotbar ziehen",
                        "Double-click plays · right-click for more · drag onto a category or the hotbar"
                    )
                    .into();
                }
                resp.context_menu(|ui| sound_context_menu(MenuCtx { core: &core }, ui, snd.id));
            }
        });
    });
    ui.allocate_rect(full, egui::Sense::hover());

    for ev in events {
        match ev {
            RowEvent::Play(id) => {
                if app.st.color_pick.is_none() {
                    app.play(id, None);
                }
            }
            RowEvent::Hotkey(id) => app.begin_capture(CaptureTarget::Sound(id)),
            RowEvent::Click(id, mods) => {
                if let Some(pick) = app.st.color_pick {
                    core.modify_library(|lib| {
                        if let Some(x) = lib.sound_mut(id) {
                            x.color = pick;
                        }
                    });
                    continue;
                }
                let mut rt = core.rt.lock();
                if mods.command {
                    if let Some(pos) = rt.selected.iter().position(|x| *x == id) {
                        rt.selected.remove(pos);
                    } else {
                        rt.selected.push(id);
                    }
                    app.st.anchor = Some(id);
                } else if mods.shift {
                    let anchor = app.st.anchor.unwrap_or(id);
                    let a = order.iter().position(|x| *x == anchor).unwrap_or(0);
                    let b = order.iter().position(|x| *x == id).unwrap_or(0);
                    let (lo, hi) = (a.min(b), a.max(b));
                    rt.selected = order[lo..=hi].to_vec();
                } else {
                    rt.selected = vec![id];
                    app.st.anchor = Some(id);
                }
            }
            RowEvent::MoveBefore(ids, target) => {
                if app.st.sort.is_some() && !s.disable_sorting {
                    core.toast(t!("Zum Verschieben zuerst die Sortierung aufheben", "Clear sorting first to reorder"), crate::core::ToastKind::Info);
                    continue;
                }
                core.modify_library(|lib| match sel_cat.and_then(|c| lib.category_mut(c)) {
                    Some(cat) => {
                        cat.sounds.retain(|x| !ids.contains(x));
                        let pos = cat.sounds.iter().position(|x| *x == target).unwrap_or(cat.sounds.len());
                        for (i, id) in ids.iter().enumerate() {
                            cat.sounds.insert(pos + i, *id);
                        }
                    }
                    None => {
                        let moved: Vec<Sound> = lib.sounds.iter().filter(|x| ids.contains(&x.id)).cloned().collect();
                        lib.sounds.retain(|x| !ids.contains(&x.id));
                        let pos = lib.sounds.iter().position(|x| x.id == target).unwrap_or(lib.sounds.len());
                        for (i, m) in moved.into_iter().enumerate() {
                            lib.sounds.insert(pos + i, m);
                        }
                    }
                });
            }
        }
    }

    if let Some(dp) = egui::DragAndDrop::payload::<DragSounds>(ui.ctx()) {
        if let Some(pos) = ui.ctx().pointer_interact_pos() {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("dragpreview")));
            let text = format!("{} {}", dp.0.len(), if dp.0.len() == 1 { "Sound" } else { "Sounds" });
            let galley = painter.layout_no_wrap(text, FontId::proportional(12.0), Color32::WHITE);
            let r = Rect::from_min_size(pos + egui::vec2(14.0, 10.0), galley.size() + egui::vec2(14.0, 8.0));
            painter.rect_filled(r, 4.0, theme::accent());
            painter.galley(r.min + egui::vec2(7.0, 4.0), galley, Color32::WHITE);
        }
    }
}

pub struct MenuCtx<'a> {
    pub core: &'a std::sync::Arc<crate::core::Core>,
}

#[derive(Clone)]
pub enum MenuRequest {
    Capture(CaptureTarget),
    Dialog(DialogReq),
    Editor(SoundId),
}

#[derive(Clone)]
pub enum DialogReq {
    RenameTag(SoundId),
    RenameFile(SoundId),
    Properties(SoundId),
    Remove(Vec<SoundId>, Option<crate::library::CategoryId>),
}

pub fn take_menu_request(ctx: &egui::Context) -> Option<MenuRequest> {
    let id = egui::Id::new("menu_request");
    ctx.data_mut(|d| {
        let r = d.get_temp::<MenuRequest>(id);
        if r.is_some() {
            d.remove::<MenuRequest>(id);
        }
        r
    })
}

fn request(ctx: &egui::Context, r: MenuRequest) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("menu_request"), r));
}

pub fn sound_context_menu(m: MenuCtx<'_>, ui: &mut egui::Ui, id: SoundId) {
    let core = m.core;
    let Some(snd) = core.lib.read().sound(id).cloned() else { return };
    let (selected, cat) = {
        let rt = core.rt.lock();
        (rt.selected.clone(), rt.selected_category)
    };
    let targets: Vec<SoundId> = if selected.contains(&id) { selected } else { vec![id] };
    let ctx = ui.ctx().clone();

    ui.label(kit::bold(snd.display_name(), 13.0));
    ui.separator();
    if ui.button(t!("Abspielen", "Play")).clicked() {
        ui.close_menu();
        core.dispatch(Action::PlaySound { id, mode: None }, Source::Ui);
    }
    if ui.button(t!("Nur auf Lautsprechern", "On speakers only")).clicked() {
        ui.close_menu();
        core.dispatch(Action::PlaySound { id, mode: Some(PlayMode::Speakers) }, Source::Ui);
    }
    if ui.button(t!("Nur auf dem Mikrofon", "On microphone only")).clicked() {
        ui.close_menu();
        core.dispatch(Action::PlaySound { id, mode: Some(PlayMode::Microphone) }, Source::Ui);
    }
    ui.separator();
    if ui.button(t!("Hotkey festlegen …", "Set hotkey …")).clicked() {
        ui.close_menu();
        request(&ctx, MenuRequest::Capture(CaptureTarget::Sound(id)));
    }
    if snd.hotkey.is_some() && ui.button(t!("Hotkey entfernen", "Remove hotkey")).clicked() {
        ui.close_menu();
        core.modify_library(|lib| {
            if let Some(s) = lib.sound_mut(id) {
                s.hotkey = None;
            }
        });
    }
    ui.menu_button(t!("Lautstärke & Modus", "Volume & mode"), |ui| {
        let mut gain = snd.gain_db;
        ui.label(kit::dim(t!("Eigene Lautstärke", "Own volume")));
        if ui.add(egui::Slider::new(&mut gain, -24.0..=12.0).suffix(" dB")).changed() {
            core.touch_library(|lib| {
                for t in &targets {
                    if let Some(s) = lib.sound_mut(*t) {
                        s.gain_db = gain;
                    }
                }
            });
        }
        ui.separator();
        ui.label(kit::dim(t!("Wiedergabemodus für diesen Sound", "Play mode for this sound")));
        let mut mode = snd.play_mode;
        let before = mode;
        ui.radio_value(&mut mode, None, t!("Global (Standard)", "Global (default)"));
        ui.radio_value(&mut mode, Some(PlayMode::Both), PlayMode::Both.label());
        ui.radio_value(&mut mode, Some(PlayMode::Speakers), PlayMode::Speakers.label());
        ui.radio_value(&mut mode, Some(PlayMode::Microphone), PlayMode::Microphone.label());
        if mode != before {
            core.modify_library(|lib| {
                for t in &targets {
                    if let Some(s) = lib.sound_mut(*t) {
                        s.play_mode = mode;
                    }
                }
            });
        }
        ui.separator();
        let mut numcode = snd.numcode.map(|n| n.to_string()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Numcode:");
            if ui.add(egui::TextEdit::singleline(&mut numcode).desired_width(60.0)).changed() {
                let v = numcode.trim().parse::<u16>().ok();
                core.touch_library(|lib| {
                    if let Some(s) = lib.sound_mut(id) {
                        s.numcode = v;
                    }
                });
            }
        });
    });
    ui.menu_button(t!("Farbe", "Color"), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(180.0);
            for c in theme::PALETTE {
                if kit::color_square(ui, theme::rgb(*c), 20.0).clicked() {
                    let c = *c;
                    core.modify_library(|lib| {
                        for t in &targets {
                            if let Some(s) = lib.sound_mut(*t) {
                                s.color = Some(c);
                            }
                        }
                    });
                    ui.close_menu();
                }
            }
        });
        if ui.button(t!("Farbe zurücksetzen", "Reset color")).clicked() {
            ui.close_menu();
            core.modify_library(|lib| {
                for t in &targets {
                    if let Some(s) = lib.sound_mut(*t) {
                        s.color = None;
                    }
                }
            });
        }
    });
    ui.menu_button(t!("Zur Kategorie hinzufügen", "Add to category"), |ui| {
        let cats: Vec<(u64, String)> = core.lib.read().categories.iter().map(|c| (c.id, format!("{} {}", c.icon, c.name))).collect();
        for (cid, name) in cats {
            if ui.button(name).clicked() {
                ui.close_menu();
                core.modify_library(|lib| {
                    if let Some(c) = lib.category_mut(cid) {
                        for t in &targets {
                            if !c.sounds.contains(t) {
                                c.sounds.push(*t);
                            }
                        }
                    }
                });
            }
        }
    });
    if ui.button(t!("Zur Hotbar hinzufügen", "Add to hotbar")).clicked() {
        ui.close_menu();
        let (rows, cols) = {
            let s = core.settings.read();
            (s.hotbar_rows, s.hotbar_cols)
        };
        core.modify_library(|lib| {
            let page_idx = lib.hotbar.selected;
            let page = lib.hotbar.page_mut(page_idx, rows * cols);
            for t in &targets {
                if let Some(slot) = page.slots.iter_mut().find(|s| s.is_none()) {
                    *slot = Some(HotbarSlot { sound: *t, color: None, hotkey: None });
                }
            }
        });
    }
    ui.separator();
    if ui.button(t!("Soundeditor …", "Sound editor …")).clicked() {
        ui.close_menu();
        request(&ctx, MenuRequest::Editor(id));
    }
    if ui.button(t!("Tagtext bearbeiten …   (F2)", "Edit tag text …   (F2)")).clicked() {
        ui.close_menu();
        request(&ctx, MenuRequest::Dialog(DialogReq::RenameTag(id)));
    }
    if snd.tag.is_some() && ui.button(t!("Originalen Tag wiederherstellen", "Reset to original tag")).clicked() {
        ui.close_menu();
        core.modify_library(|lib| {
            if let Some(s) = lib.sound_mut(id) {
                s.tag = None;
            }
        });
    }
    if ui.button(t!("Datei umbenennen …", "Rename file …")).clicked() {
        ui.close_menu();
        request(&ctx, MenuRequest::Dialog(DialogReq::RenameFile(id)));
    }
    if ui.button(t!("Dateipfad ändern …", "Edit file location …")).clicked() {
        ui.close_menu();
        if let Some(path) = rfd::FileDialog::new().add_filter("Audio", crate::audio::decode::SUPPORTED_EXT).pick_file() {
            core.cache.invalidate(id);
            core.modify_library(|lib| {
                if let Some(s) = lib.sound_mut(id) {
                    s.path = path;
                    s.loudness_db = None;
                    s.wave.clear();
                }
            });
        }
    }
    if ui.button(t!("Im Dateiexplorer anzeigen", "Show in explorer")).clicked() {
        ui.close_menu();
        crate::winutil::show_in_explorer(&snd.path);
    }
    if ui.button(t!("Abspielzähler zurücksetzen", "Reset play count")).clicked() {
        ui.close_menu();
        core.modify_library(|lib| {
            for t in &targets {
                if let Some(s) = lib.sound_mut(*t) {
                    s.play_count = 0;
                    s.last_played = None;
                }
            }
        });
    }
    if ui.button(t!("Eigenschaften", "Properties")).clicked() {
        ui.close_menu();
        request(&ctx, MenuRequest::Dialog(DialogReq::Properties(id)));
    }
    ui.separator();
    let remove_label = if cat.is_some() { t!("Aus Kategorie entfernen", "Remove from category") } else { t!("Aus Soundliste entfernen", "Remove from sound list") };
    if ui.button(RichText::new(remove_label).color(theme::ERR)).clicked() {
        ui.close_menu();
        request(&ctx, MenuRequest::Dialog(DialogReq::Remove(targets.clone(), cat)));
    }
}

pub fn process_menu_requests(app: &mut FerrovoxApp, ctx: &egui::Context) {
    let Some(req) = take_menu_request(ctx) else { return };
    match req {
        MenuRequest::Capture(t) => app.begin_capture(t),
        MenuRequest::Editor(id) => super::editor::open(app, id),
        MenuRequest::Dialog(d) => {
            let lib = app.core.lib.read();
            app.st.dialog = match d {
                DialogReq::RenameTag(id) => Some(Dialog::RenameTag { id, text: lib.sound(id).map(|s| s.display_name().to_string()).unwrap_or_default() }),
                DialogReq::RenameFile(id) => Some(Dialog::RenameFile {
                    id,
                    text: lib.sound(id).and_then(|s| s.path.file_stem().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default(),
                }),
                DialogReq::Properties(id) => Some(Dialog::Properties(id)),
                DialogReq::Remove(ids, cat) => Some(Dialog::ConfirmRemove { ids, from_category: cat }),
            };
        }
    }
}

fn empty_state(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let p = pal();
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.2);
        ui.label(kit::icon(ic::MUSIC, 34.0).color(p.faint));
        ui.add_space(8.0);
        ui.label(kit::bold(t!("Deine Soundliste ist noch leer", "Your sound list is empty"), 16.0));
        ui.label(kit::dim(t!(
            "Ziehe Audio- oder Videodateien (MP3, WAV, FLAC, OGG, M4A, MP4, MKV …) hierher oder füge sie hinzu.",
            "Drag audio or video files (MP3, WAV, FLAC, OGG, M4A, MP4, MKV …) here or add them."
        )));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 330.0).max(0.0) / 2.0);
            if kit::pill(ui, ic::ADD, t!("Sounds hinzufügen", "Add sounds"), theme::accent()).clicked() {
                app.add_files_dialog();
            }
            if kit::text_btn(ui, ic::FOLDER, t!("Ordner hinzufügen", "Add folder")).clicked() {
                app.add_folder_dialog();
            }
        });
        if let Some(dir) = crate::paths::soundpad_dir() {
            let spl = dir.join("soundlist.spl");
            if spl.exists() {
                ui.add_space(8.0);
                if kit::text_btn(ui, ic::IMPORT, t!("Vorhandene Soundpad-Liste übernehmen", "Import existing Soundpad list")).clicked() {
                    app.core.import_spl(&spl, true);
                }
            }
        }
    });
}
