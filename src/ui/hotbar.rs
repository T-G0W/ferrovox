use super::kit::{self, ic};
use super::table::DragSounds;
use super::theme::{self, pal};
use super::{CaptureTarget, Dialog, FerrovoxApp};
use crate::actions::{Action, Source};
use crate::library::HotbarSlot;
use crate::t;
use eframe::egui::{self, Color32, FontId, RichText, Stroke};

struct DragSlot(usize, usize);

pub fn grid_panel(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let (rows, cols, locked) = {
        let s = app.core.settings.read();
        (s.hotbar_rows.clamp(1, 8), s.hotbar_cols.clamp(1, 12), s.hotbar_locked)
    };
    header(app, ui, rows, cols, locked);
    ui.add_space(6.0);
    grid(app, ui, rows, cols, locked);
}

fn header(app: &mut FerrovoxApp, ui: &mut egui::Ui, rows: usize, cols: usize, locked: bool) {
    let core = app.core.clone();
    let p = pal();
    let (pages, selected): (Vec<(String, Option<crate::library::Rgb>)>, usize) = {
        let lib = core.lib.read();
        (lib.hotbar.pages.iter().map(|pg| (pg.title.clone(), pg.color)).collect(), lib.hotbar.selected)
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("Seite", "Page")).small().color(p.dim));
        for (i, (title, color)) in pages.iter().enumerate() {
            let label = if title.is_empty() { format!("{}", i + 1) } else { format!("{}  {title}", i + 1) };
            let active = i == selected;
            let col = color.map(theme::rgb).unwrap_or(if active { p.text } else { p.dim });
            let galley = ui.painter().layout_no_wrap(label, FontId::proportional(12.5), col);
            let (r, resp) = ui.allocate_exact_size(galley.size() + egui::vec2(16.0, 8.0), egui::Sense::click());
            ui.painter().rect_filled(r, 10.0, if active { p.hover } else if resp.hovered() { p.raised } else { Color32::TRANSPARENT });
            ui.painter().galley(r.min + egui::vec2(8.0, 4.0), galley, col);
            if resp.clicked() {
                core.lib.write().hotbar.selected = i;
                core.mark_lib_dirty();
            }
            resp.context_menu(|ui| {
                if ui.button(t!("Seite umbenennen …", "Rename page …")).clicked() {
                    ui.close_menu();
                    app.st.dialog = Some(Dialog::HotbarPage { page: i, title: title.clone() });
                }
                ui.menu_button(t!("Farbe", "Color"), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(180.0);
                        for c in theme::PALETTE {
                            if kit::color_square(ui, theme::rgb(*c), 20.0).clicked() {
                                let c = *c;
                                core.modify_library(|lib| lib.hotbar.pages[i].color = Some(c));
                                ui.close_menu();
                            }
                        }
                    });
                });
                if pages.len() > 1 && ui.button(RichText::new(t!("Seite entfernen", "Remove page")).color(theme::ERR)).clicked() {
                    ui.close_menu();
                    core.modify_library(|lib| {
                        lib.hotbar.pages.remove(i);
                        lib.hotbar.selected = lib.hotbar.selected.min(lib.hotbar.pages.len() - 1);
                    });
                }
            });
        }
        if kit::icon_btn_sized(ui, ic::ADD, t!("Seite hinzufügen", "Add page"), false, egui::vec2(22.0, 20.0), 10.0).clicked() {
            core.modify_library(|lib| {
                lib.hotbar.pages.push(Default::default());
                lib.hotbar.selected = lib.hotbar.pages.len() - 1;
            });
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::icon_btn_sized(ui, if locked { ic::LOCK } else { ic::UNLOCK }, t!("Drag & Drop auf Hotbar sperren", "Lock hotbar drag and drop"), locked, egui::vec2(24.0, 20.0), 11.0).clicked() {
                core.settings.write().hotbar_locked = !locked;
                core.mark_settings_dirty();
            }
            ui.menu_button(RichText::new(format!("{rows} × {cols}")).small().color(theme::value()), |ui| {
                let mut r = rows;
                let mut c = cols;
                ui.add(egui::Slider::new(&mut r, 1..=8).text(t!("Zeilen", "Rows")));
                ui.add(egui::Slider::new(&mut c, 1..=12).text(t!("Spalten", "Columns")));
                if (r, c) != (rows, cols) {
                    let mut s = core.settings.write();
                    s.hotbar_rows = r;
                    s.hotbar_cols = c;
                    drop(s);
                    core.mark_settings_dirty();
                }
            });
            ui.label(kit::icon(ic::GRID, 11.0).color(p.faint));
        });
    });
}

fn grid(app: &mut FerrovoxApp, ui: &mut egui::Ui, rows: usize, cols: usize, locked: bool) {
    let core = app.core.clone();
    let p = pal();
    let n = rows * cols;
    let (page_idx, slots, names) = {
        let lib = core.lib.read();
        let page_idx = lib.hotbar.selected.min(lib.hotbar.pages.len().saturating_sub(1));
        let mut slots: Vec<Option<HotbarSlot>> = lib.hotbar.pages.get(page_idx).map(|pg| pg.slots.clone()).unwrap_or_default();
        slots.resize(n, None);
        let names: Vec<Option<(String, Option<crate::library::Rgb>, u64)>> = slots
            .iter()
            .map(|s| s.as_ref().and_then(|s| lib.sound(s.sound).map(|x| (x.display_name().to_string(), x.color, x.duration_ms))))
            .collect();
        (page_idx, slots, names)
    };
    let playing = {
        let rt = core.rt.lock();
        rt.current_voice.and(rt.current)
    };
    let gap = 8.0;
    let avail = ui.available_size();
    let empty = slots.iter().all(|s| s.is_none());
    let w = ((avail.x - gap * (cols as f32 - 1.0)) / cols as f32).max(40.0);
    let h = ((avail.y - gap * (rows as f32 - 1.0)) / rows as f32).clamp(30.0, 140.0);

    for r in 0..rows {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for c in 0..cols {
                let i = r * cols + c;
                let slot = &slots[i];
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click_and_drag());
                let painter = ui.painter();
                match (slot, &names[i]) {
                    (Some(slot), Some((name, snd_color, dur))) => {
                        let is_playing = playing == Some(slot.sound);
                        let color = slot.color.or(*snd_color).map(theme::rgb);
                        let base = color.unwrap_or(p.raised);
                        let fill = if resp.is_pointer_button_down_on() { base.gamma_multiply(0.8) } else if resp.hovered() { base.gamma_multiply(1.18) } else { base };
                        painter.rect_filled(rect, 6.0, fill);
                        if color.is_none() {
                            painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())), egui::Rounding { nw: 6.0, sw: 6.0, ne: 0.0, se: 0.0 }, theme::accent());
                        }
                        if is_playing {
                            painter.rect_stroke(rect, 6.0, Stroke::new(2.0_f32, theme::accent()));
                        }
                        let text_col = if color.is_some() { Color32::WHITE } else { p.text };
                        let galley = painter.layout(name.clone(), FontId::new(13.0, egui::FontFamily::Name(theme::BOLD.into())), text_col, rect.width() - 16.0);
                        let clip = painter.with_clip_rect(rect.shrink(4.0));
                        clip.galley(rect.left_top() + egui::vec2(10.0, 22.0), galley, text_col);
                        clip.text(rect.left_top() + egui::vec2(10.0, 6.0), egui::Align2::LEFT_TOP, format!("{}", i + 1), FontId::proportional(10.0), text_col.gamma_multiply(0.55));
                        clip.text(rect.right_top() + egui::vec2(-8.0, 6.0), egui::Align2::RIGHT_TOP, crate::library::format_duration(*dur), FontId::proportional(10.0), text_col.gamma_multiply(0.55));
                        if let Some(hk) = slot.hotkey {
                            clip.text(rect.left_bottom() + egui::vec2(10.0, -6.0), egui::Align2::LEFT_BOTTOM, hk.label(), FontId::proportional(11.0), if color.is_some() { Color32::WHITE } else { theme::value() });
                        }
                    }
                    _ => {
                        painter.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, p.border));
                        if empty && i == 0 {
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                t!("Sounds hierher ziehen", "Drag sounds here"),
                                FontId::proportional(11.5),
                                p.faint,
                            );
                        } else if resp.hovered() {
                            painter.text(rect.center(), egui::Align2::CENTER_CENTER, t!("Sound hierher ziehen", "Drop sound here"), FontId::proportional(11.0), p.faint);
                        }
                    }
                }

                if let Some(slot) = slot {
                    if resp.clicked() {
                        if let Some(pick) = app.st.color_pick {
                            core.modify_library(|lib| {
                                if let Some(Some(s)) = lib.hotbar.pages.get_mut(page_idx).and_then(|pg| pg.slots.get_mut(i)) {
                                    s.color = pick;
                                }
                            });
                        } else {
                            core.dispatch(Action::PlayHotbarSlot { page: page_idx, slot: i }, Source::Ui);
                            core.rt.lock().selected = vec![slot.sound];
                        }
                    }
                    if !locked && resp.drag_started() {
                        resp.dnd_set_drag_payload(DragSlot(page_idx, i));
                    }
                    let sound = slot.sound;
                    let has_hk = slot.hotkey.is_some();
                    resp.context_menu(|ui| {
                        if ui.button(t!("Abspielen", "Play")).clicked() {
                            ui.close_menu();
                            core.dispatch(Action::PlaySound { id: sound, mode: None }, Source::Ui);
                        }
                        if ui.button(t!("Hotkey festlegen …", "Set hotkey …")).clicked() {
                            ui.close_menu();
                            app.begin_capture(CaptureTarget::HotbarSlot(page_idx, i));
                        }
                        if has_hk && ui.button(t!("Hotkey entfernen", "Remove hotkey")).clicked() {
                            ui.close_menu();
                            core.modify_library(|lib| {
                                if let Some(Some(s)) = lib.hotbar.pages.get_mut(page_idx).and_then(|pg| pg.slots.get_mut(i)) {
                                    s.hotkey = None;
                                }
                            });
                        }
                        ui.menu_button(t!("Farbe", "Color"), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.set_max_width(180.0);
                                for col in theme::PALETTE {
                                    if kit::color_square(ui, theme::rgb(*col), 20.0).clicked() {
                                        let col = *col;
                                        core.modify_library(|lib| {
                                            if let Some(Some(s)) = lib.hotbar.pages.get_mut(page_idx).and_then(|pg| pg.slots.get_mut(i)) {
                                                s.color = Some(col);
                                            }
                                        });
                                        ui.close_menu();
                                    }
                                }
                            });
                            if ui.button(t!("Farbe zurücksetzen", "Reset color")).clicked() {
                                ui.close_menu();
                                core.modify_library(|lib| {
                                    if let Some(Some(s)) = lib.hotbar.pages.get_mut(page_idx).and_then(|pg| pg.slots.get_mut(i)) {
                                        s.color = None;
                                    }
                                });
                            }
                        });
                        if ui.button(t!("In Soundliste auswählen", "Select in sound list")).clicked() {
                            ui.close_menu();
                            let mut rt = core.rt.lock();
                            rt.selected = vec![sound];
                            rt.scroll_to = Some(sound);
                        }
                        ui.separator();
                        if ui.button(RichText::new(t!("Von Hotbar entfernen", "Remove from hotbar")).color(theme::ERR)).clicked() {
                            ui.close_menu();
                            core.modify_library(|lib| {
                                if let Some(s) = lib.hotbar.pages.get_mut(page_idx).and_then(|pg| pg.slots.get_mut(i)) {
                                    *s = None;
                                }
                            });
                        }
                    });
                }

                if !locked {
                    let hovering = resp.dnd_hover_payload::<DragSounds>().is_some() || resp.dnd_hover_payload::<DragSlot>().is_some();
                    if hovering {
                        ui.painter().rect_stroke(rect, 6.0, Stroke::new(2.0_f32, theme::accent()));
                    }
                    if let Some(pl) = resp.dnd_release_payload::<DragSounds>() {
                        let ids = pl.0.clone();
                        core.modify_library(|lib| {
                            let page = lib.hotbar.page_mut(page_idx, n);
                            let mut target = i;
                            for id in ids {
                                if target >= page.slots.len() {
                                    break;
                                }
                                let keep_hk = page.slots[target].as_ref().and_then(|s| s.hotkey);
                                page.slots[target] = Some(HotbarSlot { sound: id, color: None, hotkey: keep_hk });
                                target += 1;
                            }
                        });
                    }
                    if let Some(pl) = resp.dnd_release_payload::<DragSlot>() {
                        let (from_page, from) = (pl.0, pl.1);
                        if from_page == page_idx && from != i {
                            let move_meta = ui.input(|inp| inp.modifiers.shift);
                            core.modify_library(|lib| {
                                let page = lib.hotbar.page_mut(page_idx, n);
                                if move_meta {
                                    page.slots.swap(from, i);
                                } else {
                                    let a = page.slots[from].as_ref().map(|s| s.sound);
                                    let b = page.slots[i].as_ref().map(|s| s.sound);
                                    let set = |slot: &mut Option<HotbarSlot>, snd: Option<u64>| match (slot.as_mut(), snd) {
                                        (Some(s), Some(x)) => s.sound = x,
                                        (None, Some(x)) => *slot = Some(HotbarSlot { sound: x, color: None, hotkey: None }),
                                        (Some(_), None) => *slot = None,
                                        (None, None) => {}
                                    };
                                    set(&mut page.slots[from], b);
                                    set(&mut page.slots[i], a);
                                }
                            });
                        }
                    }
                }
            }
        });
        if r + 1 < rows {
            ui.add_space(gap - ui.spacing().item_spacing.y);
        }
    }
}
