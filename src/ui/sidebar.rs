use super::kit::{self, ic};
use super::table::DragSounds;
use super::theme::{self, pal};
use super::{CaptureTarget, Dialog, FerrovoxApp};
use crate::actions::{Action, Source};
use crate::library::{Category, CategoryId};
use crate::t;
use eframe::egui::{self, RichText};

pub fn panel(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let mut tab = 0usize;
    let mut add_cat = false;
    kit::card(
        ui,
        "categories",
        &[t!("Kategorien", "Categories")],
        &mut tab,
        |ui| {
            if kit::icon_btn_sized(ui, ic::ADD, t!("Kategorie hinzufügen", "Add category"), false, egui::vec2(22.0, 20.0), 11.0).clicked() {
                add_cat = true;
            }
        },
        |ui, _| list(app, ui),
    );
    if add_cat {
        app.st.dialog = Some(Dialog::Category {
            id: None,
            name: t!("Neue Kategorie", "New category").into(),
            icon: "📁".into(),
            index: String::new(),
        });
    }
}

fn list(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let core = app.core.clone();
    let (cats, total) = {
        let lib = core.lib.read();
        (lib.categories.clone(), lib.sounds.len())
    };
    let selected = core.rt.lock().selected_category;
    let mut show_hidden = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("show_hidden_cats")).unwrap_or(false));

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        if row(ui, ic::LIBRARY, t!("Alle Sounds", "All sounds"), None, total, selected.is_none(), false).clicked() {
            select(app, None);
        }
        ui.add_space(4.0);
        let n = cats.len();
        for (i, cat) in cats.iter().enumerate() {
            if cat.hidden && !show_hidden {
                continue;
            }
            let color = cat.color.map(theme::rgb);
            let resp = row(ui, ic::FOLDER, &format!("{} {}", cat.icon, cat.name), color, cat.sounds.len(), selected == Some(cat.id), cat.hidden);
            if resp.clicked() {
                select(app, Some(cat.id));
            }
            if resp.double_clicked() {
                core.dispatch(Action::RandomInCategory { cat: Some(cat.id), mode: None }, Source::Ui);
            }
            if resp.hovered() {
                app.st.hint = t!("Doppelklick spielt einen zufälligen Sound · Sounds hierher ziehen zum Hinzufügen", "Double-click plays a random sound · drag sounds here to add them").into();
            }
            if resp.dnd_hover_payload::<DragSounds>().is_some() {
                ui.painter().rect_stroke(resp.rect, 4.0, egui::Stroke::new(1.5_f32, theme::accent()));
            }
            if let Some(payload) = resp.dnd_release_payload::<DragSounds>() {
                let ids = payload.0.clone();
                let cid = cat.id;
                core.modify_library(|lib| {
                    if let Some(c) = lib.category_mut(cid) {
                        for id in ids {
                            if !c.sounds.contains(&id) {
                                c.sounds.push(id);
                            }
                        }
                    }
                });
            }
            resp.context_menu(|ui| context_menu(app, ui, cat, i, n));
        }
        let hidden = cats.iter().filter(|c| c.hidden).count();
        if hidden > 0 {
            ui.add_space(6.0);
            let label = if show_hidden { t!("Versteckte ausblenden", "Hide hidden").to_string() } else { format!("{} ({hidden})", t!("Versteckte anzeigen", "Show hidden")) };
            if ui.add(egui::Label::new(kit::value(label).small()).sense(egui::Sense::click())).clicked() {
                show_hidden = !show_hidden;
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("show_hidden_cats"), show_hidden));
            }
        }
    });
}

pub fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
    let p = pal();
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 12.0, ui.visuals().extreme_bg_color);
    ui.painter().rect_stroke(rect, 12.0, egui::Stroke::new(1.0_f32, p.border));
    ui.painter().text(
        rect.left_center() + egui::vec2(12.0, 0.0),
        egui::Align2::CENTER_CENTER,
        ic::SEARCH,
        egui::FontId::new(11.0, egui::FontFamily::Name(theme::ICONS.into())),
        p.dim,
    );
    let inner = egui::Rect::from_min_max(rect.min + egui::vec2(24.0, 3.0), rect.max - egui::vec2(10.0, 3.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.add_sized(inner.size(), egui::TextEdit::singleline(text).hint_text(hint).frame(false))
}

fn row(ui: &mut egui::Ui, icon: &str, name: &str, color: Option<egui::Color32>, count: usize, selected: bool, hidden: bool) -> egui::Response {
    let p = pal();
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 4.0, p.row_sel);
        painter.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 5.0), egui::vec2(2.0, rect.height() - 10.0)), 1.0, theme::accent());
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, p.raised);
    }
    let text_col = if hidden { p.faint } else { p.text };
    painter.text(
        rect.left_center() + egui::vec2(14.0, 0.0),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::new(12.0, egui::FontFamily::Name(theme::ICONS.into())),
        if selected { theme::value() } else { p.dim },
    );
    let right = rect.right() - 40.0;
    let galley = painter.layout_no_wrap(name.to_string(), egui::FontId::proportional(13.0), text_col);
    let clip = egui::Rect::from_min_max(egui::pos2(rect.left() + 28.0, rect.top()), egui::pos2(right, rect.bottom()));
    painter.with_clip_rect(clip.intersect(ui.clip_rect())).galley(egui::pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0), galley, text_col);
    if let Some(c) = color {
        painter.rect_filled(egui::Rect::from_center_size(egui::pos2(right + 10.0, rect.center().y), egui::vec2(8.0, 8.0)), 2.0, c);
    }
    painter.text(rect.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, count.to_string(), egui::FontId::proportional(12.0), p.dim);
    resp
}

fn select(app: &mut FerrovoxApp, cat: Option<CategoryId>) {
    let mut rt = app.core.rt.lock();
    rt.selected_category = cat;
    rt.view_order.clear();
}

fn context_menu(app: &mut FerrovoxApp, ui: &mut egui::Ui, cat: &Category, index: usize, count: usize) {
    let core = app.core.clone();
    let id = cat.id;
    if ui.button(t!("Zufällige Datei abspielen", "Play random file")).clicked() {
        ui.close_menu();
        core.dispatch(Action::RandomInCategory { cat: Some(id), mode: None }, Source::Ui);
    }
    if ui.button(t!("Kategorie bearbeiten …", "Edit category …")).clicked() {
        ui.close_menu();
        app.st.dialog = Some(Dialog::Category {
            id: Some(id),
            name: cat.name.clone(),
            icon: cat.icon.clone(),
            index: cat.index.map(|i| i.to_string()).unwrap_or_default(),
        });
    }
    if ui.button(t!("Zufalls-Hotkey festlegen …", "Set random hotkey …")).clicked() {
        ui.close_menu();
        app.begin_capture(CaptureTarget::CategoryRandom(id));
    }
    if cat.random_hotkey.is_some() && ui.button(t!("Zufalls-Hotkey entfernen", "Remove random hotkey")).clicked() {
        ui.close_menu();
        core.modify_library(|lib| {
            if let Some(c) = lib.category_mut(id) {
                c.random_hotkey = None;
            }
        });
    }
    ui.menu_button(t!("Farbe", "Color"), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(180.0);
            for c in theme::PALETTE {
                if kit::color_square(ui, theme::rgb(*c), 20.0).clicked() {
                    let c = *c;
                    core.modify_library(|lib| {
                        if let Some(x) = lib.category_mut(id) {
                            x.color = Some(c);
                        }
                    });
                    ui.close_menu();
                }
            }
        });
        if ui.button(t!("Farbe zurücksetzen", "Reset color")).clicked() {
            ui.close_menu();
            core.modify_library(|lib| {
                if let Some(x) = lib.category_mut(id) {
                    x.color = None;
                }
            });
        }
    });
    let hide_label = if cat.hidden { t!("Kategorie anzeigen", "Show category") } else { t!("Kategorie verstecken", "Hide category") };
    if ui.button(hide_label).clicked() {
        ui.close_menu();
        core.modify_library(|lib| {
            if let Some(c) = lib.category_mut(id) {
                c.hidden = !c.hidden;
            }
        });
    }
    ui.separator();
    if ui.add_enabled(index > 0, egui::Button::new(t!("Nach oben", "Move up"))).clicked() {
        ui.close_menu();
        core.modify_library(|lib| lib.categories.swap(index, index - 1));
    }
    if ui.add_enabled(index + 1 < count, egui::Button::new(t!("Nach unten", "Move down"))).clicked() {
        ui.close_menu();
        core.modify_library(|lib| lib.categories.swap(index, index + 1));
    }
    if ui.button(t!("Als eigene Soundliste speichern …", "Save as separate sound list …")).clicked() {
        ui.close_menu();
        if let Some(path) = rfd::FileDialog::new().add_filter("Ferrovox", &["fvx"]).set_file_name(format!("{}.fvx", cat.name)).save_file() {
            let lib = core.lib.read();
            let mut out = crate::library::Library { categories: Vec::new(), ..Default::default() };
            for sid in &cat.sounds {
                if let Some(s) = lib.sound(*sid) {
                    out.add_sound(s.clone(), None, false);
                }
            }
            drop(lib);
            match out.save(&path) {
                Ok(()) => core.toast(t!("Gespeichert", "Saved"), crate::core::ToastKind::Ok),
                Err(e) => core.toast(format!("{e}"), crate::core::ToastKind::Error),
            }
        }
    }
    ui.separator();
    if ui.button(RichText::new(t!("Kategorie entfernen", "Remove category")).color(theme::ERR)).clicked() {
        ui.close_menu();
        core.modify_library(|lib| lib.categories.retain(|c| c.id != id));
        let mut rt = core.rt.lock();
        if rt.selected_category == Some(id) {
            rt.selected_category = None;
            rt.view_order.clear();
        }
    }
}
