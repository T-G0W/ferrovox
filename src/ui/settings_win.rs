use super::{kit, theme, CaptureTarget, FerrovoxApp, SettingsTab};
use crate::core::ToastKind;
use crate::hotkey::{mods_label, vk_name, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN};
use crate::i18n::Language;
use crate::settings::{AutoKeyCondition, AutoKeyMode, DateFormat, NormMode, PlayMode, Settings, Theme, SPECIAL_DEFS};
use crate::t;
use eframe::egui::{self, RichText};

pub fn window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    if app.st.settings_draft.is_none() {
        return;
    }
    let mut open = true;
    let mut action: Option<&str> = None;
    egui::Window::new(format!("⚙  {}", t!("Einstellungen", "Preferences")))
        .open(&mut open)
        .default_size([760.0, 560.0])
        .min_width(640.0)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    use kit::ic;
                    for (tab, icon, label) in [
                        (SettingsTab::Audio, ic::SPEAKERS, "Audio"),
                        (SettingsTab::Microphone, ic::MIC, t!("Mikrofon", "Microphone")),
                        (SettingsTab::Hotkeys, ic::KEYBOARD, "Hotkeys"),
                        (SettingsTab::AutoKeys, ic::ROBOT, "Auto-Keys"),
                        (SettingsTab::Interface, ic::LAYERS, t!("Oberfläche", "Interface")),
                        (SettingsTab::Recording, ic::RECORD, t!("Aufnahme & TTS", "Recording & TTS")),
                        (SettingsTab::Remote, ic::CODE, t!("Fernsteuerung", "Remote control")),
                        (SettingsTab::System, ic::DEVICES, "System"),
                    ] {
                        let selected = app.st.settings_tab == tab;
                        let col = if selected { egui::Color32::WHITE } else { theme::pal().text };
                        let mut job = egui::text::LayoutJob::default();
                        job.append(icon, 0.0, egui::TextFormat { font_id: egui::FontId::new(13.0, egui::FontFamily::Name(theme::ICONS.into())), color: col, valign: egui::Align::Center, ..Default::default() });
                        job.append(&format!("   {label}"), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(13.0), color: col, valign: egui::Align::Center, ..Default::default() });
                        if ui.add_sized([150.0, 28.0], egui::SelectableLabel::new(selected, job)).clicked() {
                            app.st.settings_tab = tab;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical().max_height(ui.available_height() - 44.0).auto_shrink([false, false]).show(ui, |ui| {
                        match app.st.settings_tab {
                            SettingsTab::Audio => audio_tab(app, ui),
                            SettingsTab::Microphone => mic_tab(app, ui),
                            SettingsTab::Hotkeys => hotkeys_tab(app, ui),
                            SettingsTab::AutoKeys => autokeys_tab(app, ui),
                            SettingsTab::Interface => interface_tab(app, ui),
                            SettingsTab::Recording => recording_tab(app, ui),
                            SettingsTab::Remote => remote_tab(app, ui),
                            SettingsTab::System => system_tab(app, ui),
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(t!("Abbrechen", "Cancel")).clicked() {
                                action = Some("cancel");
                            }
                            if ui.button(t!("Übernehmen", "Apply")).clicked() {
                                action = Some("apply");
                            }
                            if ui.add(egui::Button::new(RichText::new("  OK  ").strong()).fill(theme::accent_dim())).clicked() {
                                action = Some("ok");
                            }
                        });
                    });
                });
            });
        });
    if !open {
        action = Some("cancel");
    }
    match action {
        Some("ok") | Some("apply") => {
            if let Some(draft) = app.st.settings_draft.clone() {
                let lang_changed = draft.language != app.core.settings.read().language;
                app.core.apply_settings(draft);
                if lang_changed {
                    app.core.toast(t!("Sprache geändert", "Language changed"), ToastKind::Ok);
                }
            }
            if action == Some("ok") {
                app.st.settings_draft = None;
            }
        }
        Some("cancel") => app.st.settings_draft = None,
        _ => {}
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().color(theme::accent()));
    ui.add_space(2.0);
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(ui.visuals().weak_text_color()));
}

fn device_combo(ui: &mut egui::Ui, id: &str, value: &mut Option<String>, list: &[String], auto_label: &str, allow_off: bool) {
    let shown = match value {
        None => auto_label.to_string(),
        Some(s) if s.is_empty() => t!("Aus", "Off").to_string(),
        Some(s) => s.clone(),
    };
    egui::ComboBox::from_id_salt(id).width(380.0).selected_text(shown).show_ui(ui, |ui| {
        ui.selectable_value(value, None, auto_label);
        if allow_off {
            ui.selectable_value(value, Some(String::new()), t!("Aus", "Off"));
        }
        for d in list {
            ui.selectable_value(value, Some(d.clone()), d);
        }
    });
}

fn mods_picker(ui: &mut egui::Ui, mods: &mut u8) {
    for (bit, label) in [(MOD_CTRL, t!("Strg", "Ctrl")), (MOD_ALT, "Alt"), (MOD_SHIFT, t!("Umschalt", "Shift")), (MOD_WIN, "Win")] {
        let mut on = *mods & bit != 0;
        if kit::check(ui, &mut on, label).changed() {
            if on {
                *mods |= bit;
            } else {
                *mods &= !bit;
            }
        }
    }
}

fn audio_tab(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let devices = app.st.devices.lock().clone().unwrap_or_default();
    let status = app.core.shared().status.lock().clone();
    let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();

    section(ui, t!("Ausgabegeräte", "Output devices"));
    egui::Grid::new("dev_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(t!("Lautsprecher (du hörst)", "Speakers (you hear)"));
        let def_label = match &devices.default_output {
            Some(n) => format!("{} ({n})", t!("Systemstandard", "System default")),
            None => t!("Systemstandard", "System default").to_string(),
        };
        device_combo(ui, "spk", &mut d.speaker_device, &devices.outputs, &def_label, false);
        ui.end_row();
        ui.label(t!("Mikrofon-Ausgang (andere hören)", "Microphone output (others hear)"));
        device_combo(ui, "micout", &mut d.mic_out_device, &devices.outputs, t!("Automatisch: virtuelles Kabel suchen", "Automatic: find virtual cable"), true);
        ui.end_row();
    });
    if status.virtual_found {
        ui.label(RichText::new(format!("✔ {}", t!("Virtuelles Audiokabel gefunden", "Virtual audio cable found"))).color(theme::OK));
    } else {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("⚠ {}", t!("Kein virtuelles Mikrofon installiert", "No virtual microphone installed"))).color(theme::WARN));
            if ui.link(t!("Jetzt mit 1 Klick einrichten", "Set up now with 1 click")).clicked() {
                app.st.setup_open = true;
                if !app.st.vmic.busy() {
                    app.st.vmic.install(app.core.clone(), true);
                }
            }
        });
    }
    hint(
        ui,
        t!(
            "Ferrovox spielt Sounds in den Eingang eines virtuellen Kabels (z.B. „CABLE Input“) und leitet dein echtes Mikrofon mit hinein.\nIn Discord/TeamSpeak/Spiel wählst du dann „CABLE Output“ als Mikrofon. Kein Treiber-Hack, kein Neustart nötig.",
            "Ferrovox plays sounds into a virtual cable input (e.g. \"CABLE Input\") and mixes your real microphone into it.\nIn Discord/TeamSpeak/your game select \"CABLE Output\" as microphone. No driver hacks, no reboot."
        ),
    );
    if ui.small_button(format!("↻ {}", t!("Geräteliste aktualisieren", "Refresh device list"))).clicked() {
        let core = app.core.clone();
        let slot = app.st.devices.clone();
        std::thread::spawn(move || {
            *slot.lock() = Some(core.engine.list_devices());
            core.repaint();
        });
    }
    let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();

    section(ui, t!("Lautstärke", "Volume"));
    egui::Grid::new("vol_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(t!("Lautsprecher-Pegel", "Speakers level"));
        ui.add(egui::Slider::new(&mut d.speakers_gain, 0.0..=2.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
        ui.end_row();
        ui.label(t!("Pegel für andere (Mikrofon)", "Level for others (microphone)"));
        ui.add(egui::Slider::new(&mut d.mic_gain, 0.0..=2.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
        ui.end_row();
    });
    kit::check(ui, &mut d.linear_volume_slider, t!("Lautstärkeregler funktioniert linear", "Volume slider works linearly"));

    section(ui, t!("Normalisierung der Lautstärke", "Volume normalization"));
    ui.radio_value(&mut d.normalization, NormMode::Dynamic, t!("Dynamische Lautstärkeangleichung (empfohlen)", "Dynamic volume adjustment (recommended)"));
    hint(ui, t!("Jede Datei wird beim Abspielen auf die Ziel-Lautheit angeglichen – ideal bei unterschiedlich lauten Sounds. Dateien werden nicht verändert.", "Each file is matched to the target loudness during playback – ideal for sounds with different volumes. Files are not modified."));
    ui.radio_value(&mut d.normalization, NormMode::Fixed, t!("Konstante Lautstärkeanpassung", "Fixed volume adjustment"));
    ui.radio_value(&mut d.normalization, NormMode::Off, t!("Keine Anpassung", "No adjustment"));
    egui::Grid::new("norm_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        match d.normalization {
            NormMode::Dynamic => {
                ui.label(t!("Ziel-Lautheit", "Target loudness"));
                ui.add(egui::Slider::new(&mut d.target_loudness_db, -36.0..=-6.0).suffix(" dBFS"));
                ui.end_row();
            }
            NormMode::Fixed => {
                ui.label(t!("Anpassung um", "Adjust by"));
                ui.add(egui::Slider::new(&mut d.fixed_adjust_db, -24.0..=24.0).suffix(" dB"));
                ui.end_row();
            }
            NormMode::Off => {}
        }
    });

    section(ui, t!("Wiedergabe", "Playback"));
    ui.horizontal(|ui| {
        ui.label(t!("Standard-Wiedergabemodus", "Default play mode"));
        egui::ComboBox::from_id_salt("playmode").selected_text(d.play_mode.label()).show_ui(ui, |ui| {
            for m in [PlayMode::Both, PlayMode::Speakers, PlayMode::Microphone] {
                ui.selectable_value(&mut d.play_mode, m, m.label());
            }
        });
    });
    kit::check(ui, &mut d.overlap_sounds, t!("Sounds überlagern (mehrere gleichzeitig abspielen)", "Overlap sounds (play several at once)"));
    kit::check(ui, &mut d.auto_stop_by_hotkey, t!("Auto-Stop: per Hotkey gestartete Sounds nicht wiederholen/fortsetzen", "Auto stop: hotkey sounds never repeat/continue"));
    kit::check(ui, &mut d.duck_other_apps, t!("Lautstärke anderer Anwendungen während der Wiedergabe verringern", "Reduce volume of other applications while playing"));
    if d.duck_other_apps {
        ui.add(egui::Slider::new(&mut d.duck_level, 0.0..=0.9).text(t!("Restlautstärke", "Remaining volume")).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
    }
    kit::check(ui, &mut d.preload_hotkey_sounds, t!("Sounds mit Hotkey/Hotbar beim Start vorladen (keine Verzögerung)", "Preload hotkey/hotbar sounds at startup (zero delay)"));
    ui.add(egui::Slider::new(&mut d.cache_mb, 64..=4096).text(t!("Sound-Cache", "Sound cache")).suffix(" MB").logarithmic(true));
}

fn mic_tab(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let devices = app.st.devices.lock().clone().unwrap_or_default();
    let meter = app.st.meters[2];
    let measuring = app.st.measure.lock().running;
    let measured = app.st.measure.lock().result;
    let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();

    section(ui, t!("Echtes Mikrofon", "Real microphone"));
    ui.horizontal(|ui| {
        ui.label(t!("Mikrofon", "Microphone"));
        let def_label = match &devices.default_input {
            Some(n) => format!("{} ({n})", t!("Systemstandard", "System default")),
            None => t!("Systemstandard", "System default").to_string(),
        };
        device_combo(ui, "micin", &mut d.mic_in_device, &devices.inputs, &def_label, false);
    });
    ui.horizontal(|ui| {
        ui.label(t!("Pegel:", "Level:"));
        theme::meter(ui, meter, 260.0, t!("Deine Stimme", "Your voice"));
    });
    kit::check(ui, &mut d.mic_passthrough, t!("Mikrofon in den Mikrofon-Ausgang durchleiten (empfohlen)", "Mix microphone into microphone output (recommended)"));
    hint(ui, t!("Damit andere dich UND deine Sounds über das virtuelle Kabel hören.", "So others hear you AND your sounds through the virtual cable."));
    if d.mic_passthrough {
        ui.add(egui::Slider::new(&mut d.mic_passthrough_gain, 0.0..=3.0).text(t!("Mikrofon-Verstärkung", "Microphone gain")).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
        kit::check(ui, &mut d.block_voice_while_playing, t!("Stimme stummschalten, während Sounds abgespielt werden", "Block voice while playing sounds"));
    }

    section(ui, t!("Verzögerungen", "Delays"));
    egui::Grid::new("delay_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(t!("Push-to-Talk-Verzögerung", "Push-to-talk delay"));
        ui.add(egui::Slider::new(&mut d.ptt_delay_ms, 0..=1500).suffix(" ms"));
        ui.end_row();
        ui.label(t!("Zusätzliche Mikrofon-Verzögerung", "Extra microphone delay"));
        ui.add(egui::Slider::new(&mut d.mic_delay_ms, 0..=1500).suffix(" ms"));
        ui.end_row();
    });
    hint(ui, t!("PTT-Verzögerung gibt dir bzw. den Auto-Keys Zeit, die Sprechtaste zu drücken. Nur-Lautsprecher-Wiedergabe ist nicht betroffen.", "PTT delay gives you / auto keys time to press the talk key. Speakers-only playback is not affected."));

    section(ui, t!("Sprachaktivierung", "Voice activation"));
    kit::check(ui, &mut d.voice_activation_sound, t!("Sprachaktivierung anderer Apps mit leisem Rauschen vorab auslösen", "Trigger other apps' voice activation with quiet noise first"));
    if d.voice_activation_sound {
        ui.add(egui::Slider::new(&mut d.voice_activation_ms, 50..=1000).suffix(" ms").text(t!("Dauer", "Duration")));
    }

    section(ui, t!("Sprachlautstärke messen", "Measure voice volume"));
    hint(ui, t!("Sprich 5 Sekunden lang normal in dein Mikrofon. Die gemessene Lautheit wird als Ziel für die Sound-Normalisierung übernommen, damit Sounds so laut wie deine Stimme sind.", "Speak normally into your microphone for 5 seconds. The measured loudness becomes the normalization target so sounds are as loud as your voice."));
    ui.horizontal(|ui| {
        if measuring {
            ui.spinner();
            ui.label(t!("Messung läuft – bitte sprechen …", "Measuring – please speak …"));
        } else if ui.button(format!("🎙 {}", t!("Messen", "Measure"))).clicked() {
            let slot = app.st.measure.clone();
            slot.lock().running = true;
            slot.lock().result = None;
            app.core.measure_voice(5, move |res| {
                let mut m = slot.lock();
                m.running = false;
                m.result = Some(res);
            });
        }
        if let Some(res) = measured {
            match res {
                Some(db) => {
                    ui.label(format!("{}: {:.1} dBFS", t!("Gemessen", "Measured"), db));
                    if ui.button(t!("Als Ziel übernehmen", "Use as target")).clicked() {
                        let d = app.st.settings_draft.as_mut().unwrap();
                        d.target_loudness_db = db.clamp(-36.0, -6.0);
                        d.normalization = NormMode::Dynamic;
                    }
                }
                None => {
                    ui.label(RichText::new(t!("Messung fehlgeschlagen – ist dein Mikrofon stummgeschaltet?", "Measurement failed – is your microphone muted?")).color(theme::WARN));
                }
            }
        }
    });
}

fn hotkeys_tab(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let mut capture: Option<CaptureTarget> = None;
    {
        let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();
        section(ui, t!("Allgemein", "General"));
        kit::check(ui, &mut d.hotkeys_enabled, t!("Hotkeys aktiviert", "Hotkeys enabled"));
        kit::check(ui, &mut d.remember_hotkeys_enabled, t!("Hotkey-Status beim Programmstart wiederherstellen", "Remember hotkey status throughout application starts"));
        kit::check(ui, &mut d.pass_through_hotkeys, t!("Hotkeys an andere Anwendungen weiterreichen statt sie zu blockieren", "Pass hotkeys to other applications instead of blocking them"));
        kit::check(ui, &mut d.distinguish_sides, t!("Linke und rechte Modifier unterscheiden (z.B. Strg+F1 ≠ RStrg+F1)", "Distinguish left and right modifiers (e.g. Ctrl+F1 ≠ RCtrl+F1)"));
        hint(ui, t!("Ferrovox nutzt einen Low-Level-Hook: funktioniert in Spielen, auch mit Maustasten (Maus 4/5/Mitte).", "Ferrovox uses a low-level hook: works in games, also with mouse buttons (mouse 4/5/middle)."));

        section(ui, t!("Index-Hotkeys", "Index hotkeys"));
        ui.horizontal(|ui| {
            kit::check(ui, &mut d.index_hotkeys, t!("Modifier + Index (Listenposition tippen)", "Modifier + index (type list position)"));
            mods_picker(ui, &mut d.index_modifier);
        });
        hint(ui, &format!(
            "{} {}+1, 2 → Sound #12",
            t!("Beispiel: Modifier halten, Ziffern tippen, loslassen. Z.B.", "Example: hold modifier, type digits, release. E.g."),
            mods_label(d.index_modifier)
        ));
        ui.horizontal(|ui| {
            kit::check(ui, &mut d.category_index, t!("Index auf Kategorien (erste Ziffer = Kategorie)", "Category index (first digit = category)"));
            ui.add_enabled(d.category_index, egui::Checkbox::new(&mut d.category_index_two_digits, t!("zweistellig (01–99)", "two digits (01–99)")));
        });
        ui.horizontal(|ui| {
            kit::check(ui, &mut d.numcode_hotkeys, t!("Modifier + Numcode (eigene Nummer pro Sound)", "Modifier + numcode (own number per sound)"));
            mods_picker(ui, &mut d.numcode_modifier);
        });

        section(ui, t!("Spezielle Hotkeys", "Special hotkeys"));
        let distinguish = d.distinguish_sides;
        egui::Grid::new("special_grid").num_columns(3).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
            let mut last_group = "";
            for def in SPECIAL_DEFS {
                if def.group != last_group {
                    last_group = def.group;
                    let heading = match def.group {
                        "play" => t!("Wiedergabe", "Playback"),
                        "select" => t!("Auswahl", "Selection"),
                        "seek" => t!("Springen", "Seeking"),
                        "volume" => t!("Lautstärke", "Volume"),
                        "mode" => t!("Wiedergabemodus", "Play mode"),
                        "record" => t!("Aufnahme", "Recording"),
                        "tts" => "Text-to-Speech",
                        _ => t!("Umschalten", "Toggles"),
                    };
                    ui.label(RichText::new(heading).small().strong().color(ui.visuals().weak_text_color()));
                    ui.end_row();
                }
                let b = d.special.entry(def.action).or_default();
                ui.horizontal(|ui| {
                    ui.label(t!(def.de, def.en));
                    if let Some((_, unit)) = def.param {
                        ui.add(egui::DragValue::new(&mut b.param).range(0..=600).suffix(format!(" {unit}")));
                    }
                });
                let label = b.hotkey.map(|h| h.label()).unwrap_or_else(|| "—".into());
                ui.label(RichText::new(label).color(if b.hotkey.is_some() { theme::accent() } else { ui.visuals().weak_text_color() }));
                ui.horizontal(|ui| {
                    if ui.small_button(t!("Festlegen", "Set")).clicked() {
                        capture = Some(CaptureTarget::Special(def.action));
                    }
                    if b.hotkey.is_some() && ui.small_button("✕").clicked() {
                        b.hotkey = None;
                    }
                });
                ui.end_row();
            }
        });
        let _ = distinguish;
    }
    if let Some(c) = capture {
        app.begin_capture(c);
    }
}

fn autokeys_tab(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let mut capture = false;
    let mut refresh_apps = false;
    let running = app.st.running_apps.clone();
    {
        let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();
        let ak = &mut d.auto_keys;
        section(ui, "Auto-Keys");
        hint(ui, t!("Drückt automatisch Tasten (z.B. die Push-to-Talk-Taste von Discord oder deinem Spiel), während ein Sound auf dem Mikrofon läuft.", "Presses keys automatically (e.g. the push-to-talk key of Discord or your game) while a sound plays on the microphone."));
        kit::check(ui, &mut ak.enabled, t!("Auto-Keys aktivieren", "Enable auto keys"));

        section(ui, t!("Was soll gedrückt werden", "What to press"));
        ui.horizontal_wrapped(|ui| {
            let mut remove = None;
            for (i, vk) in ak.keys.iter().enumerate() {
                let col = if crate::autokeys::is_dangerous(*vk) { theme::WARN } else { theme::accent() };
                if ui.button(RichText::new(format!("{}  ✕", vk_name(*vk))).color(col)).on_hover_text(t!("Entfernen", "Remove")).clicked() {
                    remove = Some(i);
                }
            }
            if let Some(i) = remove {
                ak.keys.remove(i);
            }
            if ui.button(format!("➕ {}", t!("Taste aufnehmen …", "Record key …"))).clicked() {
                capture = true;
            }
            egui::ComboBox::from_id_salt("ak_common").width(170.0).selected_text(t!("… oder Taste wählen", "… or pick key")).show_ui(ui, |ui| {
                let common: &[u16] = &[0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x14, 0x05, 0x06, 0x04, 0x7C, 0x7D, 0x7E, 0x7F, 0x56, 0x42, 0x54, 0x58, 0x43];
                for &vk in common {
                    if ui.selectable_label(false, vk_name(vk)).clicked() && !ak.keys.contains(&vk) {
                        if crate::autokeys::is_dangerous(vk) && !ak.allow_dangerous {
                            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ak_warn"), true));
                        } else {
                            ak.keys.push(vk);
                        }
                    }
                }
            });
        });
        let warn = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("ak_warn")).unwrap_or(false));
        if warn && !ak.allow_dangerous {
            ui.label(RichText::new(t!(
                "⚠ Strg, Alt, Umschalt und Win lösen Systemaktionen aus (Alt+Tab, Alt+F4, Win+L …), wenn du währenddessen andere Tasten drückst. Sicherer ist eine eigene Push-to-Talk-Taste. Wenn du es trotzdem willst, erlaube es unten.",
                "⚠ Ctrl, Alt, Shift and Win trigger system actions (Alt+Tab, Alt+F4, Win+L …) if you press other keys meanwhile. A separate push-to-talk key is safer. If you still want it, allow it below."
            )).color(theme::WARN));
        }
        kit::check(ui, &mut ak.allow_dangerous, t!("Potentiell gefährliche Tasten erlauben", "Allow potentially dangerous keys"));

        section(ui, t!("Wie soll gedrückt werden", "How to press"));
        ui.radio_value(&mut ak.mode, AutoKeyMode::Hold, t!("Gedrückt halten (Push-to-Talk)", "Press and hold (push-to-talk)"));
        ui.radio_value(&mut ak.mode, AutoKeyMode::TapBeforeAfter, t!("Davor und danach drücken (Umschalt-Sprechtaste)", "Press before and after (toggle talk key)"));
        kit::check(ui, &mut ak.only_by_hotkey, t!("Nur drücken, wenn Sounds per Hotkey abgespielt werden", "Press only if sound is played by hotkey"));

        section(ui, t!("Wann soll gedrückt werden", "When to press"));
        ui.radio_value(&mut ak.condition, AutoKeyCondition::Always, t!("Immer", "Always"));
        ui.radio_value(&mut ak.condition, AutoKeyCondition::AppForeground, t!("Bestimmte Anwendung ist im Vordergrund", "Certain application is in foreground"));
        ui.radio_value(&mut ak.condition, AutoKeyCondition::AppRunning, t!("Bestimmte Anwendung wird ausgeführt", "Certain application is running"));
        ui.radio_value(&mut ak.condition, AutoKeyCondition::ForegroundFullscreen, t!("Vordergrundanwendung ist im Vollbild", "Foreground application is fullscreen"));
        if matches!(ak.condition, AutoKeyCondition::AppForeground | AutoKeyCondition::AppRunning) {
            ui.horizontal_wrapped(|ui| {
                let mut remove = None;
                for (i, a) in ak.apps.iter().enumerate() {
                    if ui.button(format!("{a}  ✕")).clicked() {
                        remove = Some(i);
                    }
                }
                if let Some(i) = remove {
                    ak.apps.remove(i);
                }
            });
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("running_apps")
                    .width(260.0)
                    .selected_text(t!("Aus laufenden Anwendungen auswählen", "Select from running applications"))
                    .show_ui(ui, |ui| {
                        for a in &running {
                            if ui.selectable_label(false, a).clicked() && !ak.apps.contains(a) {
                                ak.apps.push(a.clone());
                            }
                        }
                    });
                if ui.small_button("↻").clicked() {
                    refresh_apps = true;
                }
                if ui.button(t!("Auswählen …", "Browse …")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Programme", &["exe"]).pick_file() {
                        if let Some(n) = p.file_name() {
                            let n = n.to_string_lossy().to_lowercase();
                            if !ak.apps.contains(&n) {
                                ak.apps.push(n);
                            }
                        }
                    }
                }
            });
        }
    }
    if refresh_apps || app.st.running_apps.is_empty() {
        app.st.running_apps = crate::winutil::running_exe_list();
    }
    if capture {
        app.begin_capture(CaptureTarget::AutoKey);
    }
}

fn interface_tab(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();
    section(ui, t!("Darstellung", "Appearance"));
    egui::Grid::new("ui_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(t!("Sprache", "Language"));
        egui::ComboBox::from_id_salt("lang").selected_text(d.language.label()).show_ui(ui, |ui| {
            for l in [Language::Auto, Language::De, Language::En] {
                ui.selectable_value(&mut d.language, l, l.label());
            }
        });
        ui.end_row();
        ui.label("Theme");
        egui::ComboBox::from_id_salt("theme").selected_text(d.theme.label()).show_ui(ui, |ui| {
            for th in [Theme::Studio, Theme::Ember, Theme::Light] {
                ui.selectable_value(&mut d.theme, th, th.label());
            }
        });
        ui.end_row();
        ui.label(t!("Skalierung", "Scale"));
        ui.add(egui::Slider::new(&mut d.ui_scale, 0.7..=2.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
        ui.end_row();
        ui.label(t!("Datumsformat", "Date format"));
        egui::ComboBox::from_id_salt("datefmt").selected_text(d.date_format.format("2026-10-26")).show_ui(ui, |ui| {
            for f in [DateFormat::German, DateFormat::Iso, DateFormat::Us] {
                ui.selectable_value(&mut d.date_format, f, f.format("2026-10-26"));
            }
        });
        ui.end_row();
    });
    kit::check(ui, &mut d.alternating_rows, t!("Zeilen in der Soundliste abwechselnd färben", "Use alternating row colors"));
    kit::check(ui, &mut d.compact_list, t!("Kompakte Soundliste", "Compact sound list"));
    kit::check(ui, &mut d.always_on_top, t!("Immer im Vordergrund anzeigen", "Show always on top"));

    section(ui, t!("Fenster & Tray", "Window & tray"));
    kit::check(ui, &mut d.start_minimized, t!("Minimiert starten", "Start minimized"));
    kit::check(ui, &mut d.show_tray_icon, t!("Symbol im Systemtray anzeigen (Neustart)", "Show system tray icon (restart)"));
    ui.add_enabled(d.show_tray_icon, egui::Checkbox::new(&mut d.minimize_to_tray, t!("In den Systemtray minimieren", "Minimize to system tray")));
    ui.add_enabled(d.show_tray_icon, egui::Checkbox::new(&mut d.close_to_tray, t!("Schließen-Knopf [x] minimiert in den Tray", "Close button [x] minimizes to tray")));

    section(ui, t!("Bedienung", "Behaviour"));
    kit::check(ui, &mut d.typing_begins_search, t!("Tippen startet Suche", "Typing begins search"));
    kit::check(ui, &mut d.search_all_categories, t!("Suche in allen Kategorien", "Search in all categories"));
    kit::check(ui, &mut d.disable_sorting, t!("Sortieren in der Soundliste deaktivieren", "Disable sorting in sound list"));

    section(ui, t!("Benachrichtigungen", "Notifications"));
    kit::check(ui, &mut d.notification_sounds, t!("Hinweistöne (Hotkeys an/aus, Modus, Aufnahme …)", "Notification tones (hotkeys on/off, mode, recording …)"));
    kit::check(ui, &mut d.notify_first_play, t!("Wiedergabemodus beim ersten Sound nach Programmstart ansagen", "Announce play mode on first sound after start"));
    kit::check(ui, &mut d.hotbar_announce_page, t!("Hotbar-Seitenwechsel mit Tönen ansagen", "Announce hotbar page changes with tones"));
}

fn recording_tab(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    app.load_tts_voices();
    let voices = app.st.tts_voices.lock().clone().unwrap_or_default();
    let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();
    section(ui, t!("Soundrecorder", "Sound recorder"));
    ui.horizontal(|ui| {
        ui.label(t!("Aufnahmen speichern in", "Save recordings in"));
        ui.label(RichText::new(d.recordings_dir().display().to_string()).small());
        if ui.button(t!("Auswählen …", "Browse …")).clicked() {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                d.recordings_dir = Some(p);
            }
        }
    });
    ui.add(egui::Slider::new(&mut d.record_limit_s, 0..=3600).text(t!("Maximale Aufnahmedauer (0 = unbegrenzt)", "Maximum recording time (0 = unlimited)")).suffix(" s"));
    kit::check(ui, &mut d.trim_silence, t!("Stille am Anfang und Ende entfernen", "Trim silence"));
    ui.horizontal(|ui| {
        kit::check(ui, &mut d.normalize_recordings, t!("Aufnahmen automatisch normalisieren auf", "Automatically normalize recordings to"));
        ui.add_enabled(d.normalize_recordings, egui::Slider::new(&mut d.recording_target_db, -36.0..=-6.0).suffix(" dBFS"));
    });
    kit::check(ui, &mut d.insert_recordings_at_top, t!("Aufnahmen am Anfang der Liste einfügen", "Insert recordings at the beginning"));

    section(ui, "Text-to-Speech");
    ui.horizontal(|ui| {
        ui.label(t!("Stimme", "Voice"));
        let shown = d.tts_voice.clone().unwrap_or_else(|| t!("Systemstandard", "System default").into());
        egui::ComboBox::from_id_salt("ttsvoice").width(300.0).selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut d.tts_voice, None, t!("Systemstandard", "System default"));
            for v in &voices {
                ui.selectable_value(&mut d.tts_voice, Some(v.clone()), v);
            }
        });
    });
    ui.add(egui::Slider::new(&mut d.tts_rate, 0.5..=3.0).text(t!("Geschwindigkeit", "Speed")));
    ui.add(egui::Slider::new(&mut d.tts_pitch, 0.0..=2.0).text(t!("Stimmlage", "Pitch")));
    if ui.link(t!("Weitere Stimmen in Windows installieren", "Install more voices in Windows")).clicked() {
        crate::winutil::open_url("ms-settings:speech");
    }

    section(ui, t!("Externer Soundeditor", "External sound editor"));
    kit::check(ui, &mut d.use_external_editor, t!("Externen Soundeditor statt des eingebauten benutzen", "Use external sound editor instead of the built-in one"));
    ui.horizontal(|ui| {
        ui.label(d.external_editor.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "—".into()));
        if ui.button(t!("Auswählen …", "Browse …")).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("Programme", &["exe"]).pick_file() {
                d.external_editor = Some(p);
            }
        }
    });
}

fn remote_tab(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();
    section(ui, t!("Fernsteuerung", "Remote control"));
    kit::check(ui, &mut d.remote_enabled, t!("Fernsteuerung über Named Pipe erlauben", "Allow remote control via named pipe"));
    kit::check(ui, &mut d.remote_soundpad_compat, t!("Soundpad-kompatible Pipe „sp_remote_control“ anbieten (Neustart)", "Offer Soundpad-compatible pipe \"sp_remote_control\" (restart)"));
    kit::check(ui, &mut d.log_remote_calls, t!("Fernsteuerungs-Aufrufe protokollieren", "Log remote control calls"));
    hint(ui, t!("Damit funktionieren bestehende Soundpad-Integrationen (Stream Deck, Skripte, Bots) direkt mit Ferrovox.", "This makes existing Soundpad integrations (Stream Deck, scripts, bots) work with Ferrovox directly."));
    section(ui, t!("Beispiele", "Examples"));
    for line in [
        "ferrovox.exe -rc \"DoPlaySound(3)\"",
        t!("ferrovox.exe -rc \"DoPlaySound(3, true, false)\"   → nur Lautsprecher", "ferrovox.exe -rc \"DoPlaySound(3, true, false)\"   → speakers only"),
        "ferrovox.exe -rc \"DoPlaySoundByName(airhorn)\"",
        "ferrovox.exe -rc \"DoPlayRandomSoundFromCategory(2)\"",
        "ferrovox.exe -rc \"DoStopSound()\"   /  \"SetVolume(50)\"  /  \"GetSoundlist()\"",
        t!("ferrovox.exe -rc \"DoTts(Hallo zusammen)\"", "ferrovox.exe -rc \"DoTts(Hello everyone)\""),
    ] {
        ui.label(RichText::new(line).monospace().small());
    }
}

fn system_tab(app: &mut FerrovoxApp, ui: &mut egui::Ui) {
    section(ui, "Windows");
    let mut autostart = crate::winutil::autostart_enabled();
    if kit::check(ui, &mut autostart, t!("Ferrovox automatisch starten, wenn Windows startet", "Run Ferrovox when Windows starts")).changed() {
        crate::winutil::set_autostart(autostart);
    }
    section(ui, "Updates");
    {
        let d: &mut Settings = app.st.settings_draft.as_mut().unwrap();
        kit::check(ui, &mut d.update_check, t!("Automatisch nach Updates suchen", "Check for updates automatically"));
        let check = d.update_check;
        ui.add_enabled_ui(check, |ui| {
            kit::check(ui, &mut d.update_auto_install, t!("Updates im Hintergrund laden und beim Beenden installieren", "Download updates in the background and install them on exit"));
        });
    }
    ui.horizontal(|ui| {
        ui.label(kit::value(format!("Version {}", crate::paths::VERSION)));
        if ui.button(t!("Jetzt nach Updates suchen", "Check for updates now")).clicked() {
            crate::update::updater().check(app.core.clone(), true);
            app.st.update_open = true;
        }
    });
    hint(ui, t!(
        "Updates kommen signiert von der offiziellen Release-Seite; manipulierte Dateien werden verworfen.",
        "Updates come signed from the official release page; tampered files are discarded."
    ));
    section(ui, t!("Einstellungen", "Preferences"));
    ui.horizontal(|ui| {
        if ui.button(t!("Exportieren …", "Export …")).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("JSON", &["json"]).set_file_name(t!("ferrovox-einstellungen.json", "ferrovox-preferences.json")).save_file() {
                let d = app.st.settings_draft.clone().unwrap_or_default();
                match serde_json::to_string_pretty(&d).map_err(anyhow::Error::from).and_then(|t| std::fs::write(&p, t).map_err(Into::into)) {
                    Ok(()) => app.core.toast(t!("Einstellungen exportiert", "Preferences exported"), ToastKind::Ok),
                    Err(e) => app.core.toast(format!("{e}"), ToastKind::Error),
                }
            }
        }
        if ui.button(t!("Importieren …", "Import …")).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                match std::fs::read_to_string(&p).map_err(anyhow::Error::from).and_then(|t| serde_json::from_str::<Settings>(&t).map_err(Into::into)) {
                    Ok(s) => app.st.settings_draft = Some(s),
                    Err(e) => app.core.toast(format!("{e}"), ToastKind::Error),
                }
            }
        }
        if ui.button(RichText::new(t!("Alle Einstellungen zurücksetzen", "Reset all preferences")).color(theme::ERR)).clicked() {
            let mut s = Settings::default();
            s.first_start = false;
            app.st.settings_draft = Some(s);
        }
    });
    section(ui, t!("Speicher", "Storage"));
    ui.horizontal(|ui| {
        ui.label(format!("Cache: {:.0} MB · {} Sounds", app.core.cache.usage_mb(), app.core.cache.len()));
        if ui.button(t!("Cache leeren", "Clear cache")).clicked() {
            app.core.cache.clear();
        }
        if ui.button(t!("Datenordner öffnen", "Open data folder")).clicked() {
            crate::winutil::open_folder(&crate::paths::data_dir());
        }
        if ui.button(t!("Backups öffnen", "Open backups")).clicked() {
            crate::winutil::open_folder(&crate::paths::backups_dir());
        }
    });
}
