use super::kit::{self, ic};
use super::theme::{self, pal};
use super::{Dialog, FerrovoxApp, SettingsTab};
use crate::actions::{Action, Source};
use crate::core::ToastKind;
use crate::settings::{PlayMode, SpecialAction, Theme};
use crate::t;
use eframe::egui::{self, RichText};
use rand::seq::SliceRandom;

fn bar_frame() -> egui::Frame {
    egui::Frame::none().fill(pal().panel).inner_margin(egui::Margin::symmetric(10.0, 0.0))
}

pub fn menubar(app: &mut FerrovoxApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("menubar").exact_height(28.0).frame(bar_frame()).show_separator_line(false).show(ctx, |ui| {
        let full = ui.max_rect();
        egui::menu::bar(ui, |ui| {
            ui.spacing_mut().button_padding = egui::vec2(7.0, 3.0);
            menus(app, ui);
        });
        let n = app.core.lib.read().sounds.len();
        let title = format!("Ferrovox  —  {} ({n} Sounds)", t!("Meine Soundliste", "My sound list"));
        ui.painter().text(
            full.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(13.0),
            pal().dim,
        );
        let mut right = ui.new_child(egui::UiBuilder::new().max_rect(full).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if kit::icon_btn_sized(&mut right, ic::FULLSCREEN, t!("Vollbild", "Fullscreen"), false, egui::vec2(26.0, 22.0), 12.0).clicked() {
            let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
        }
        let muted = app.core.settings.read().muted;
        if kit::icon_btn_sized(&mut right, if muted { ic::MUTE } else { ic::VOLUME }, t!("Stummschalten", "Mute"), muted, egui::vec2(26.0, 22.0), 12.0).clicked() {
            app.core.toggle_mute();
        }
        let toasts = app.core.rt.lock().toasts.len();
        if kit::icon_btn_sized(&mut right, ic::BELL, &format!("{toasts} {}", t!("Meldungen", "notifications")), false, egui::vec2(26.0, 22.0), 12.0).clicked() {
            app.st.stats_open = true;
        }
        if kit::icon_btn_sized(&mut right, ic::SEARCH, t!("Suchen (Strg+F)", "Search (Ctrl+F)"), false, egui::vec2(26.0, 22.0), 12.0).clicked() {
            app.st.request_search_focus = true;
        }
        if let Some(v) = crate::update::updater().state().pending_version() {
            right.add_space(4.0);
            if kit::pill(&mut right, ic::DOWNLOAD, &format!("Update {v}"), theme::accent_dim()).on_hover_text(t!("Neue Version – klicken für Details", "New version – click for details")).clicked() {
                app.st.update_open = true;
            }
            right.add_space(4.0);
        }
        right.label(RichText::new("SOUNDBOARD").small().color(pal().dim));
    });
}

fn menus(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let core = app.core.clone();
    ui.menu_button(t!("Datei", "File"), |ui| {
        if ui.button(t!("Sounds hinzufügen …", "Add sound files …")).clicked() {
            ui.close_menu();
            app.add_files_dialog();
        }
        if ui.button(t!("Ordner hinzufügen …", "Add folder …")).clicked() {
            ui.close_menu();
            app.add_folder_dialog();
        }
        ui.separator();
        if ui.button(t!("Neue Soundliste", "New sound list")).clicked() {
            ui.close_menu();
            core.modify_library(|lib| *lib = crate::library::Library::default());
            core.cache.clear();
        }
        if ui.button(t!("Soundliste laden …", "Load sound list …")).clicked() {
            ui.close_menu();
            if let Some(p) = rfd::FileDialog::new().add_filter("Ferrovox / Soundpad", &["json", "fvx", "spl"]).pick_file() {
                core.load_list(&p);
            }
        }
        let recent = core.settings.read().recent_lists.clone();
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button(t!("Zuletzt geladene Soundliste", "Load recent sound list"), |ui| {
                for p in &recent {
                    if ui.button(p.display().to_string()).clicked() {
                        ui.close_menu();
                        core.load_list(p);
                    }
                }
                ui.separator();
                if ui.button(t!("Liste leeren", "Clear list")).clicked() {
                    ui.close_menu();
                    core.settings.write().recent_lists.clear();
                    core.mark_settings_dirty();
                }
            });
        });
        if ui.button(t!("Soundliste speichern   (Strg+S)", "Save sound list   (Ctrl+S)")).clicked() {
            ui.close_menu();
            core.save_library_now();
            core.toast(t!("Soundliste gespeichert", "Sound list saved"), ToastKind::Ok);
        }
        if ui.button(t!("Soundliste speichern unter …", "Save sound list as …")).clicked() {
            ui.close_menu();
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Ferrovox", &["fvx"])
                .add_filter("Soundpad (.spl)", &["spl"])
                .set_file_name("soundliste.fvx")
                .save_file()
            {
                core.save_list_as(&p);
            }
        }
        ui.separator();
        if ui.button(t!("Soundpad-Liste importieren …", "Import Soundpad list …")).clicked() {
            ui.close_menu();
            let mut dlg = rfd::FileDialog::new().add_filter("Soundpad", &["spl"]);
            if let Some(dir) = crate::paths::soundpad_dir() {
                dlg = dlg.set_directory(dir);
            }
            if let Some(p) = dlg.pick_file() {
                core.import_spl(&p, false);
            }
        }
        if ui.button(t!("Sounddateien exportieren …", "Export sound files …")).clicked() {
            ui.close_menu();
            export_files(app);
        }
        ui.separator();
        if ui.button(t!("Einstellungen …", "Preferences …")).clicked() {
            ui.close_menu();
            app.open_settings(SettingsTab::Audio);
        }
        if ui.button(t!("Beenden", "Exit")).clicked() {
            ui.close_menu();
            app.st.quit = true;
        }
    });

    ui.menu_button(t!("Bearbeiten", "Edit"), |ui| {
        let (can_undo, can_redo) = {
            let rt = core.rt.lock();
            (!rt.undo.is_empty(), !rt.redo.is_empty())
        };
        if ui.add_enabled(can_undo, egui::Button::new(t!("Rückgängig   (Strg+Z)", "Undo   (Ctrl+Z)"))).clicked() {
            ui.close_menu();
            core.undo();
        }
        if ui.add_enabled(can_redo, egui::Button::new(t!("Wiederholen   (Strg+Y)", "Redo   (Ctrl+Y)"))).clicked() {
            ui.close_menu();
            core.redo();
        }
        ui.separator();
        if ui.button(t!("Alles auswählen", "Select everything")).clicked() {
            ui.close_menu();
            let order = core.rt.lock().view_order.clone();
            core.rt.lock().selected = order;
        }
        if ui.button(t!("Auswahl aufheben", "Deselect")).clicked() {
            ui.close_menu();
            core.rt.lock().selected.clear();
        }
        if ui.button(t!("Ausgewählte Einträge entfernen   (Entf)", "Remove selected entries   (Del)")).clicked() {
            ui.close_menu();
            let (ids, cat) = {
                let rt = core.rt.lock();
                (rt.selected.clone(), rt.selected_category)
            };
            if !ids.is_empty() {
                app.st.dialog = Some(Dialog::ConfirmRemove { ids, from_category: cat });
            }
        }
        ui.separator();
        if ui.button(t!("Tote Einträge finden", "Find dead entries")).clicked() {
            ui.close_menu();
            let dead = core.lib.read().dead_entries();
            app.st.dialog = Some(Dialog::DeadEntries(dead));
        }
        if ui.button(t!("Liste mischen", "Shuffle list")).clicked() {
            ui.close_menu();
            let cat = core.rt.lock().selected_category;
            core.modify_library(|lib| match cat.and_then(|c| lib.category_mut(c)) {
                Some(c) => c.sounds.shuffle(&mut rand::thread_rng()),
                None => lib.sounds.shuffle(&mut rand::thread_rng()),
            });
            app.st.sort = None;
        }
        if ui.button(t!("Abspielzähler aller Sounds zurücksetzen", "Reset play count of all sounds")).clicked() {
            ui.close_menu();
            core.modify_library(|lib| {
                lib.sounds.iter_mut().for_each(|s| {
                    s.play_count = 0;
                    s.last_played = None;
                })
            });
        }
    });

    ui.menu_button(t!("Ansicht", "View"), |ui| {
        let mut s = core.settings.read().clone();
        let before = format!("{:?}", (&s.columns, s.show_categories, s.show_hotbar, s.alternating_rows, s.always_on_top, s.theme, s.ui_scale, s.compact_list));
        kit::check(ui, &mut s.show_categories, t!("Kategorien", "Categories"));
        kit::check(ui, &mut s.show_hotbar, "Hotbar");
        ui.menu_button(t!("Spalten", "Columns"), |ui| {
            kit::check(ui, &mut s.columns.index, "#");
            kit::check(ui, &mut s.columns.artist, t!("Interpret", "Artist"));
            kit::check(ui, &mut s.columns.duration, t!("Dauer", "Duration"));
            kit::check(ui, &mut s.columns.hotkey, "Hotkey");
            kit::check(ui, &mut s.columns.play_count, t!("Abspielzähler", "Play count"));
            kit::check(ui, &mut s.columns.last_played, t!("Zuletzt gespielt", "Last played"));
            kit::check(ui, &mut s.columns.added_on, t!("Hinzugefügt am", "Added on"));
            kit::check(ui, &mut s.columns.path, t!("Pfad", "File path"));
        });
        ui.separator();
        ui.menu_button("Theme", |ui| {
            for th in [Theme::Studio, Theme::Ember, Theme::Light] {
                ui.radio_value(&mut s.theme, th, th.label());
            }
        });
        kit::check(ui, &mut s.compact_list, t!("Kompakte Zeilen", "Compact rows"));
        kit::check(ui, &mut s.alternating_rows, t!("Zeilen abwechselnd färben", "Alternating row colors"));
        kit::check(ui, &mut s.always_on_top, t!("Immer im Vordergrund", "Always on top"));
        ui.horizontal(|ui| {
            ui.label("Zoom");
            if ui.small_button("−").clicked() {
                s.ui_scale = (s.ui_scale - 0.1).max(0.7);
            }
            ui.label(kit::value(format!("{:.0}%", s.ui_scale * 100.0)));
            if ui.small_button("+").clicked() {
                s.ui_scale = (s.ui_scale + 0.1).min(2.0);
            }
        });
        if before != format!("{:?}", (&s.columns, s.show_categories, s.show_hotbar, s.alternating_rows, s.always_on_top, s.theme, s.ui_scale, s.compact_list)) {
            *core.settings.write() = s;
            core.mark_settings_dirty();
        }
    });

    ui.menu_button(t!("Wiedergabe", "Playback"), |ui| {
        if ui.button(t!("Ausgewählte Datei abspielen   (Enter)", "Play selected file   (Enter)")).clicked() {
            ui.close_menu();
            core.dispatch(Action::PlaySelected { mode: None }, Source::Ui);
        }
        if ui.button(t!("Ausgewählte nur auf Lautsprechern", "Selected on speakers only")).clicked() {
            ui.close_menu();
            core.dispatch(Action::PlaySelected { mode: Some(PlayMode::Speakers) }, Source::Ui);
        }
        if ui.button(t!("Ausgewählte nur auf dem Mikrofon", "Selected on microphone only")).clicked() {
            ui.close_menu();
            core.dispatch(Action::PlaySelected { mode: Some(PlayMode::Microphone) }, Source::Ui);
        }
        ui.separator();
        for (label, sa) in [
            (t!("Pause / Fortsetzen   (Leertaste)", "Pause / resume   (Space)"), SpecialAction::TogglePause),
            (t!("Wiedergabe stoppen", "Stop playback"), SpecialAction::Stop),
            (t!("Nächste Datei", "Next file"), SpecialAction::PlayNext),
            (t!("Vorherige Datei", "Previous file"), SpecialAction::PlayPrevious),
            (t!("Zufällige Datei aus Kategorie", "Random file from category"), SpecialAction::RandomFromCategory),
            (t!("Zufällige Datei aus allen", "Random file from all"), SpecialAction::RandomAll),
        ] {
            if ui.button(label).clicked() {
                ui.close_menu();
                core.dispatch(Action::Special(sa), Source::Ui);
            }
        }
        ui.separator();
        let mut s = core.settings.read().clone();
        let before = (s.repeat_file, s.repeat_list, s.continue_playback, s.overlap_sounds, s.auto_stop_by_hotkey);
        kit::check(ui, &mut s.repeat_file, t!("Datei wiederholen", "Repeat current file"));
        kit::check(ui, &mut s.repeat_list, t!("Soundliste wiederholen", "Repeat sound list"));
        kit::check(ui, &mut s.continue_playback, t!("Nach Datei weiterspielen", "Continue playback after file"));
        kit::check(ui, &mut s.overlap_sounds, t!("Sounds überlagern (mehrere gleichzeitig)", "Overlap sounds (play several at once)"));
        kit::check(ui, &mut s.auto_stop_by_hotkey, t!("Auto-Stop bei Hotkey-Wiedergabe", "Auto stop for hotkey playback"));
        if before != (s.repeat_file, s.repeat_list, s.continue_playback, s.overlap_sounds, s.auto_stop_by_hotkey) {
            *core.settings.write() = s;
            core.mark_settings_dirty();
        }
    });

    ui.menu_button(t!("Werkzeuge", "Tools"), |ui| {
        if ui.button(t!("Mikrofon einrichten …", "Set up microphone …")).clicked() {
            ui.close_menu();
            app.st.setup_open = true;
            app.refresh_devices();
        }
        ui.separator();
        if ui.button(t!("Soundrecorder", "Sound recorder")).clicked() {
            ui.close_menu();
            app.st.recorder_open = true;
        }
        if ui.button("Text-to-Speech").clicked() {
            ui.close_menu();
            app.st.tts_open = true;
            app.load_tts_voices();
        }
        if ui.button(t!("Farbwerkzeug", "Color tool")).clicked() {
            ui.close_menu();
            app.st.color_tool = true;
        }
        if ui.button(t!("Soundeditor (ausgewählter Sound)", "Sound editor (selected sound)")).clicked() {
            ui.close_menu();
            app.open_editor_for_selection();
        }
        ui.separator();
        if ui.button(t!("Hotkey-Übersicht", "Hotkey overview")).clicked() {
            ui.close_menu();
            app.st.overview_open = true;
        }
        if ui.button(t!("Statistik", "Stats")).clicked() {
            ui.close_menu();
            app.st.stats_open = true;
        }
        if ui.button(t!("Audiogeräte neu verbinden", "Reconnect audio devices")).clicked() {
            ui.close_menu();
            core.reconnect_devices();
            core.toast(t!("Audiogeräte werden neu verbunden", "Reconnecting audio devices"), ToastKind::Info);
        }
    });

    ui.menu_button(t!("Hilfe", "Help"), |ui| {
        if ui.button(t!("Nach Updates suchen …", "Check for updates …")).clicked() {
            ui.close_menu();
            crate::update::updater().check(app.core.clone(), true);
            app.st.update_open = true;
        }
        if ui.button(t!("Über Ferrovox", "About Ferrovox")).clicked() {
            ui.close_menu();
            app.st.about_open = true;
        }
        if ui.button(t!("Logdatei anzeigen", "Show log")).clicked() {
            ui.close_menu();
            crate::winutil::show_in_explorer(&crate::paths::log_file());
        }
        if ui.button(t!("Datenordner öffnen", "Open data folder")).clicked() {
            ui.close_menu();
            crate::winutil::open_folder(&crate::paths::data_dir());
        }
    });
}

fn vsep(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 20.0), egui::Sense::hover());
    ui.painter().vline(r.center().x, r.y_range(), egui::Stroke::new(1.0_f32, pal().border));
}

fn mode_btn(ui: &mut egui::Ui, mode: PlayMode, active: bool) -> egui::Response {
    let w = if mode == PlayMode::Both { 40.0 } else { 28.0 };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), egui::Sense::click());
    let p = pal();
    let bg = if active { p.accent } else if resp.hovered() { p.raised } else { egui::Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, 4.0, bg);
    let col = if active { egui::Color32::WHITE } else { p.dim };
    let font = egui::FontId::new(14.0, egui::FontFamily::Name(theme::ICONS.into()));
    match mode {
        PlayMode::Both => {
            ui.painter().text(rect.center() - egui::vec2(8.0, 0.0), egui::Align2::CENTER_CENTER, ic::SPEAKERS, font.clone(), col);
            ui.painter().text(rect.center() + egui::vec2(9.0, 0.0), egui::Align2::CENTER_CENTER, ic::MIC, font, col);
        }
        PlayMode::Speakers => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, ic::SPEAKERS, font, col);
        }
        PlayMode::Microphone => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, ic::MIC, font, col);
        }
    }
    resp.on_hover_text(mode.label())
}

pub fn toolbar(app: &mut FerrovoxApp, ctx: &egui::Context) {
    let core = app.core.clone();
    egui::TopBottomPanel::top("toolbar")
        .exact_height(42.0)
        .frame(bar_frame().inner_margin(egui::Margin::symmetric(10.0, 0.0)))
        .show_separator_line(false)
        .show(ctx, |ui| {
            let st = core.status();
            let s = core.settings.read().clone();
            let mode = core.rt.lock().current_mode;
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                let (r, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
                ui.painter().rect_filled(r, 6.0, theme::accent());
                ui.painter().image(app.logo.id(), r.shrink(3.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                if resp.on_hover_text("Ferrovox – by T-GOW").clicked() {
                    app.st.about_open = true;
                }
                ui.add_space(4.0);
                if kit::icon_btn(ui, ic::HOME, t!("Alle Sounds", "All sounds"), false).clicked() {
                    let mut rt = core.rt.lock();
                    rt.selected_category = None;
                    rt.search.clear();
                    rt.view_order.clear();
                }
                vsep(ui);
                let pointer = app.st.color_pick.is_none();
                if kit::icon_btn(ui, ic::POINTER, t!("Auswahl", "Select"), pointer).clicked() {
                    app.st.color_pick = None;
                }
                if kit::icon_btn(ui, ic::COLOR, t!("Farbwerkzeug", "Color tool"), app.st.color_tool).clicked() {
                    app.st.color_tool = !app.st.color_tool;
                    if !app.st.color_tool {
                        app.st.color_pick = None;
                    }
                }
                if kit::icon_btn(ui, ic::EDIT, t!("Soundeditor", "Sound editor"), app.st.editor.is_some()).clicked() {
                    app.open_editor_for_selection();
                }
                let rec = core.recording_info().is_some();
                if kit::icon_btn(ui, ic::RECORD, t!("Soundrecorder", "Sound recorder"), rec || app.st.recorder_open).clicked() {
                    app.st.recorder_open = !app.st.recorder_open;
                }
                if kit::icon_btn(ui, ic::MESSAGE, "Text-to-Speech", app.st.tts_open).clicked() {
                    app.st.tts_open = !app.st.tts_open;
                    app.load_tts_voices();
                }
                vsep(ui);
                for m in [PlayMode::Both, PlayMode::Speakers, PlayMode::Microphone] {
                    if mode_btn(ui, m, mode == m).clicked() {
                        core.set_play_mode(m, false);
                    }
                }
                vsep(ui);
                if kit::icon_btn(ui, ic::PREV, t!("Vorherige Datei", "Previous file"), false).clicked() {
                    core.dispatch(Action::Special(SpecialAction::PlayPrevious), Source::Ui);
                }
                let playing = st.playing && !st.paused;
                if kit::icon_btn(ui, if playing { ic::PAUSE } else { ic::PLAY }, t!("Abspielen / Pause", "Play / pause"), false).clicked() {
                    if st.playing {
                        core.dispatch(Action::TogglePause, Source::Ui);
                    } else {
                        core.dispatch(Action::PlaySelected { mode: None }, Source::Ui);
                    }
                }
                if kit::icon_btn(ui, ic::STOP, t!("Stopp", "Stop"), false).clicked() {
                    core.dispatch(Action::Stop, Source::Ui);
                }
                if kit::icon_btn(ui, ic::NEXT, t!("Nächste Datei", "Next file"), false).clicked() {
                    core.dispatch(Action::Special(SpecialAction::PlayNext), Source::Ui);
                }
                vsep(ui);
                let mut changed = false;
                let mut w = s.clone();
                if kit::icon_btn(ui, ic::REPEAT_ONE, t!("Datei wiederholen", "Repeat file"), s.repeat_file).clicked() {
                    w.repeat_file = !s.repeat_file;
                    changed = true;
                }
                if kit::icon_btn(ui, ic::REPEAT_ALL, t!("Soundliste wiederholen", "Repeat list"), s.repeat_list).clicked() {
                    w.repeat_list = !s.repeat_list;
                    changed = true;
                }
                if kit::icon_btn(ui, ic::FORWARD, t!("Nach Datei weiterspielen", "Continue after file"), s.continue_playback).clicked() {
                    w.continue_playback = !s.continue_playback;
                    changed = true;
                }
                if kit::icon_btn(ui, ic::OVERLAP, t!("Sounds überlagern", "Overlap sounds"), s.overlap_sounds).clicked() {
                    w.overlap_sounds = !s.overlap_sounds;
                    changed = true;
                }
                if changed {
                    let mut g = core.settings.write();
                    g.repeat_file = w.repeat_file;
                    g.repeat_list = w.repeat_list;
                    g.continue_playback = w.continue_playback;
                    g.overlap_sounds = w.overlap_sounds;
                    drop(g);
                    core.mark_settings_dirty();
                }
                vsep(ui);
                if kit::icon_btn(ui, if s.muted || s.volume == 0.0 { ic::MUTE } else { ic::VOLUME }, t!("Stummschalten", "Mute"), s.muted).clicked() {
                    core.toggle_mute();
                }
                let mut vol = s.volume;
                if ui.add_sized([96.0, 18.0], egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false)).changed() {
                    core.set_volume(vol);
                }
                ui.add_sized([36.0, 18.0], egui::Label::new(kit::value(format!("{:.0}%", vol * 100.0))));
                ui.add_space(8.0);
                let mut hk = s.hotkeys_enabled;
                if kit::check(ui, &mut hk, "Hotkeys").on_hover_text(t!("Globale Hotkeys an/aus", "Global hotkeys on/off")).changed() {
                    core.set_hotkeys_enabled(hk);
                }
                ui.add_space(6.0);
                let mut ak = s.auto_keys.enabled;
                if kit::check(ui, &mut ak, "Auto-Keys").on_hover_text(t!("Push-to-Talk automatisch drücken", "Press push-to-talk automatically")).changed() {
                    if s.auto_keys.keys.is_empty() {
                        app.open_settings(SettingsTab::AutoKeys);
                    } else {
                        core.set_autokeys(ak);
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    mic_pill(app, ui);
                    ui.add_space(6.0);
                    use crate::i18n::Language;
                    let globe = kit::icon_btn(ui, ic::GLOBE, t!("Sprache", "Language"), false);
                    let popup = ui.make_persistent_id("lang_popup");
                    if globe.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(popup));
                    }
                    egui::popup_below_widget(ui, popup, &globe, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                        ui.set_min_width(180.0);
                        let cur = core.settings.read().language;
                        for l in [Language::Auto, Language::De, Language::En] {
                            if ui.selectable_label(cur == l, l.label()).clicked() && cur != l {
                                let mut s = core.settings.read().clone();
                                s.language = l;
                                core.apply_settings(s);
                            }
                        }
                    });
                    if kit::icon_btn(ui, ic::SETTINGS, t!("Einstellungen", "Preferences"), app.st.settings_draft.is_some()).clicked() {
                        app.open_settings(SettingsTab::Audio);
                    }
                });
            });
        });
}

pub fn now_playing(app: &mut FerrovoxApp, ctx: &egui::Context) {
    let core = app.core.clone();
    egui::TopBottomPanel::top("now_playing")
        .exact_height(34.0)
        .frame(egui::Frame::none().fill(pal().bg).inner_margin(egui::Margin { left: 14.0, right: 14.0, top: 2.0, bottom: 2.0 }))
        .show_separator_line(false)
        .show(ctx, |ui| {
            let p = pal();
            let st = core.status();
            ui.horizontal_centered(|ui| {
                let (icon, name, col) = if st.playing {
                    (if st.paused { ic::PAUSE } else { ic::VOLUME }, st.name.clone(), theme::value())
                } else {
                    (ic::MUSIC, t!("Bereit", "Ready").to_string(), p.dim)
                };
                ui.label(kit::icon(icon, 12.0).color(col));
                let name_resp = ui
                    .allocate_ui_with_layout(egui::vec2(240.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_width(240.0);
                        ui.add(egui::Label::new(RichText::new(name).color(col)).truncate().sense(egui::Sense::click()))
                    })
                    .inner;
                if let Some(id) = st.sound.filter(|_| name_resp.clicked()) {
                    let mut rt = core.rt.lock();
                    rt.selected = vec![id];
                    rt.scroll_to = Some(id);
                }

                let meters_w = 3.0 * 62.0 + 30.0;
                let time_w = 140.0;
                let picker_w = 250.0;
                let bar_w = (ui.available_width() - meters_w - time_w - picker_w).max(80.0);
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(bar_w, 18.0), egui::Sense::click_and_drag());
                let track = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), 4.0));
                ui.painter().rect_filled(track, 2.0, p.raised);
                let mut frac = if st.dur_ms > 0 { st.pos_ms as f32 / st.dur_ms as f32 } else { 0.0 };
                if st.playing {
                    if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged()) {
                        frac = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    }
                    if resp.drag_stopped() || resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                            core.dispatch(Action::SeekMs((f * st.dur_ms as f32) as u64), Source::Ui);
                        }
                    }
                    if let Some(pos) = resp.hover_pos() {
                        let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                        app.st.hint = format!("{} {}", t!("Springen zu", "Seek to"), kit::short_time((f * st.dur_ms as f32) as u64));
                    }
                }
                let mut fill = track;
                fill.set_width(track.width() * frac.clamp(0.0, 1.0));
                ui.painter().rect_filled(fill, 2.0, theme::accent());
                if st.playing {
                    ui.painter().circle_filled(egui::pos2(fill.right(), track.center().y), 5.0, egui::Color32::WHITE);
                }
                ui.add_sized(
                    [time_w, 18.0],
                    egui::Label::new(RichText::new(format!("{}  /  {}", kit::short_time(st.pos_ms), kit::short_time(st.dur_ms))).color(theme::value())),
                );
                output_picker(app, ui, picker_w - 12.0);
                ui.add_space(8.0);
                for (i, (short, tip)) in [
                    ("LS", t!("Lautsprecher", "Speakers")),
                    ("MIC", t!("Mikrofon-Ausgang (was andere hören)", "Mic output (what others hear)")),
                    ("IN", t!("Deine Stimme", "Your voice")),
                ]
                .iter()
                .enumerate()
                {
                    ui.label(RichText::new(*short).small().color(p.faint));
                    theme::meter(ui, app.st.meters[i], 34.0, tip);
                }
            });
        });
}

fn output_picker(app: &mut FerrovoxApp, ui: &mut egui::Ui, width: f32) {
    let core = app.core.clone();
    let p = pal();
    let status = core.shared().status.lock().clone();
    let chosen = core.settings.read().speaker_device.clone();
    let current = status.speakers.clone();
    let inaudible = current.as_deref().map(crate::audio::winaudio::is_virtual_render).unwrap_or(true);
    let label = match &current {
        Some(n) => n.clone(),
        None => t!("Kein Lautsprecher!", "No speakers!").to_string(),
    };
    let devices = app.st.devices.lock().clone().unwrap_or_default();
    ui.label(kit::icon(if inaudible { ic::WARNING } else { ic::SPEAKERS }, 12.0).color(if inaudible { theme::WARN } else { p.dim }));
    let mut selected = chosen.clone();
    let combo = egui::ComboBox::from_id_salt("output_picker")
        .width(width - 24.0)
        .selected_text(RichText::new(label).color(if inaudible { theme::WARN } else { p.text }))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, None, t!("Windows-Standard (wechselt automatisch mit)", "Windows default (follows automatically)"));
            ui.separator();
            for name in &devices.outputs {
                let virt = crate::audio::winaudio::is_virtual_render(name);
                let text = if virt { format!("{name}  ({})", t!("virtuell – unhörbar", "virtual – inaudible")) } else { name.clone() };
                ui.add_enabled_ui(!virt, |ui| {
                    ui.selectable_value(&mut selected, Some(name.clone()), text);
                });
            }
            ui.separator();
            if ui.button(t!("Geräteliste aktualisieren", "Refresh device list")).clicked() {
                app.refresh_devices();
            }
        });
    let _ = combo.response.on_hover_text(t!(
        "Hier hörst DU die Sounds. Was andere hören, läuft über das virtuelle Mikrofon.",
        "This is where YOU hear the sounds. What others hear goes through the virtual microphone."
    ));
    if selected != chosen {
        let mut s = core.settings.read().clone();
        s.speaker_device = selected;
        core.apply_settings(s);
    }
}

fn mic_pill(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let status = app.core.shared().status.lock().clone();
    let step = app.st.vmic.step();
    let resp = if app.st.vmic.busy() {
        let pct = step.progress().unwrap_or(0.0) * 100.0;
        kit::pill(ui, ic::SYNC, &format!("{} {pct:.0}%", t!("Richte ein …", "Setting up …")), theme::accent())
    } else if status.mic_out.is_some() {
        kit::pill(ui, ic::CIRCLE_FILL, t!("Mikrofon bereit", "Microphone ready"), pal().raised)
    } else {
        kit::pill(ui, ic::MIC, t!("Mikrofon einrichten", "Set up microphone"), theme::accent())
    };
    if resp
        .on_hover_text(t!("Damit andere deine Sounds im Voice-Chat hören", "So others hear your sounds in voice chat"))
        .clicked()
    {
        app.st.setup_open = true;
        app.refresh_devices();
    }
}

pub fn status_bar(app: &mut FerrovoxApp, ctx: &egui::Context) {
    let core = app.core.clone();
    egui::TopBottomPanel::bottom("status")
        .exact_height(24.0)
        .frame(bar_frame())
        .show_separator_line(false)
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                let p = pal();
                let hint = if !app.st.hint_shown.is_empty() {
                    app.st.hint_shown.clone()
                } else {
                    let s = core.rt.lock().status.clone();
                    if s.is_empty() {
                        t!(
                            "Klicken zum Auswählen, Doppelklick spielt ab. Ziehen auf Kategorie oder Hotbar. Strg/Umschalt für Mehrfachauswahl.",
                            "Click to select, double-click to play. Drag onto a category or the hotbar. Ctrl/Shift for multi-select."
                        )
                        .to_string()
                    } else {
                        s
                    }
                };
                ui.add(egui::Label::new(RichText::new(hint).small().color(p.dim)).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("Ferrovox {} · T-GOW", crate::paths::VERSION)).small().color(p.faint));
                    let dev = core.shared().status.lock().clone();
                    if !dev.errors.is_empty() {
                        ui.label(kit::icon(ic::WARNING, 11.0).color(theme::WARN)).on_hover_text(dev.errors.join("\n"));
                    }
                    if let Some((src, secs)) = core.recording_info() {
                        let what = match src {
                            crate::audio::RecSource::Speakers => t!("Lautsprecher", "speakers"),
                            crate::audio::RecSource::Microphone => t!("Mikrofon", "microphone"),
                        };
                        ui.label(RichText::new(format!("● REC {what} {secs:.1}s")).small().color(theme::ERR));
                    }
                    let rate = if dev.speakers_rate > 0 { format!("{:.1} kHz", dev.speakers_rate as f32 / 1000.0) } else { "—".into() };
                    ui.label(RichText::new(format!("{} {rate}  ·  Cache {:.0} MB", t!("Ausgabe", "Output"), core.cache.usage_mb())).small().color(p.faint));
                });
            });
        });
}

fn export_files(app: &mut FerrovoxApp) {
    let core = app.core.clone();
    let Some(dir) = rfd::FileDialog::new().set_title(t!("Zielordner wählen", "Choose directory")).pick_folder() else { return };
    let ids = core.rt.lock().view_order.clone();
    let sounds: Vec<crate::library::Sound> = ids.iter().filter_map(|id| core.lib.read().sound(*id).cloned()).collect();
    std::thread::spawn(move || {
        let mut ok = 0;
        let mut exported = crate::library::Library { categories: Vec::new(), ..Default::default() };
        for s in sounds {
            let Some(name) = s.path.file_name() else { continue };
            let target = dir.join(name);
            if std::fs::copy(&s.path, &target).is_ok() {
                ok += 1;
                let mut copy = s.clone();
                copy.path = target;
                exported.add_sound(copy, None, false);
            }
        }
        let _ = exported.save(&dir.join("soundliste.fvx"));
        core.toast(format!("{ok} {}", t!("Sounddateien exportiert", "sound files exported")), ToastKind::Ok);
    });
}
