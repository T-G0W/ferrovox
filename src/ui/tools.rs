use super::kit::{self, ic};
use super::{theme, CaptureTarget, Dialog, FerrovoxApp};
use crate::actions::{Action, Source};
use crate::audio::RecSource;
use crate::core::ToastKind;
use crate::hotkey::Hotkey;
use crate::library::{format_duration, Category};
use crate::settings::{special_def, PlayMode, SpecialAction};
use crate::t;
use eframe::egui::{self, RichText};
use std::collections::HashMap;

pub fn windows(app: &mut FerrovoxApp, ctx: &egui::Context) {
    capture_window(app, ctx);
    recorder_window(app, ctx);
    tts_window(app, ctx);
    color_window(app, ctx);
    stats_window(app, ctx);
    overview_window(app, ctx);
    setup_window(app, ctx);
    about_window(app, ctx);
    update_window(app, ctx);
    dialog_window(app, ctx);
}

fn centered(title: impl Into<egui::WidgetText>) -> egui::Window<'static> {
    egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
}

fn target_label(app: &FerrovoxApp, t: CaptureTarget) -> String {
    let lib = app.core.lib.read();
    match t {
        CaptureTarget::Sound(id) => format!("Sound {}", crate::i18n::q(lib.sound(id).map(|s| s.display_name()).unwrap_or("?"))),
        CaptureTarget::CategoryRandom(id) => format!(
            "{} {}",
            crate::t!("Zufalls-Hotkey für Kategorie", "Random hotkey for category"),
            crate::i18n::q(lib.category(id).map(|c| c.name.as_str()).unwrap_or("?"))
        ),
        CaptureTarget::HotbarSlot(p, s) => format!("Hotbar {} / Slot {}", p + 1, s + 1),
        CaptureTarget::Special(a) => {
            let d = special_def(a);
            crate::t!(d.de, d.en).to_string()
        }
        CaptureTarget::AutoKey => "Auto-Key".into(),
    }
}

fn conflict(app: &FerrovoxApp, hk: &Hotkey, target: CaptureTarget) -> Option<String> {
    if matches!(target, CaptureTarget::AutoKey) {
        return None;
    }
    let s = app.st.settings_draft.clone().unwrap_or_else(|| app.core.settings.read().clone());
    let d = s.distinguish_sides;
    for (a, b) in &s.special {
        if b.hotkey.is_some_and(|h| h.match_key(d) == hk.match_key(d)) && target != CaptureTarget::Special(*a) {
            let def = special_def(*a);
            return Some(crate::t!(def.de, def.en).to_string());
        }
    }
    let lib = app.core.lib.read();
    let owner = lib.hotkey_owner(hk, d)?;
    let own = match target {
        CaptureTarget::Sound(id) => lib.sound(id).and_then(|s| s.hotkey) == Some(*hk),
        CaptureTarget::CategoryRandom(id) => lib.category(id).and_then(|c| c.random_hotkey) == Some(*hk),
        CaptureTarget::HotbarSlot(p, i) => {
            lib.hotbar.pages.get(p).and_then(|pg| pg.slots.get(i)).and_then(|s| s.as_ref()).and_then(|s| s.hotkey) == Some(*hk)
        }
        _ => false,
    };
    (!own).then_some(owner)
}

fn apply_capture(app: &mut FerrovoxApp, target: CaptureTarget, hk: Hotkey) {
    let core = app.core.clone();
    let d = app.st.settings_draft.as_ref().map(|s| s.distinguish_sides).unwrap_or_else(|| core.settings.read().distinguish_sides);
    let clear_special = |s: &mut crate::settings::Settings, keep: Option<SpecialAction>| {
        for (a, b) in s.special.iter_mut() {
            if Some(*a) != keep && b.hotkey.is_some_and(|h| h.match_key(d) == hk.match_key(d)) {
                b.hotkey = None;
            }
        }
    };
    match target {
        CaptureTarget::AutoKey => {
            let vk = hk.vk;
            match app.st.settings_draft.as_mut() {
                Some(dr) => {
                    if !dr.auto_keys.keys.contains(&vk) {
                        dr.auto_keys.keys.push(vk);
                    }
                }
                None => {
                    let mut s = core.settings.write();
                    if !s.auto_keys.keys.contains(&vk) {
                        s.auto_keys.keys.push(vk);
                    }
                    drop(s);
                    core.mark_settings_dirty();
                }
            }
        }
        CaptureTarget::Special(a) => {
            match app.st.settings_draft.as_mut() {
                Some(dr) => {
                    clear_special(dr, Some(a));
                    dr.special.entry(a).or_default().hotkey = Some(hk);
                }
                None => {
                    let mut s = core.settings.read().clone();
                    clear_special(&mut s, Some(a));
                    s.special.entry(a).or_default().hotkey = Some(hk);
                    core.apply_settings(s);
                }
            }
            core.modify_library(|lib| lib.clear_hotkey(&hk, d));
        }
        other => {
            {
                let mut s = core.settings.read().clone();
                let before = format!("{:?}", s.special);
                clear_special(&mut s, None);
                if before != format!("{:?}", s.special) {
                    core.apply_settings(s);
                }
            }
            if let Some(dr) = app.st.settings_draft.as_mut() {
                clear_special(dr, None);
            }
            core.modify_library(|lib| {
                lib.clear_hotkey(&hk, d);
                match other {
                    CaptureTarget::Sound(id) => {
                        if let Some(s) = lib.sound_mut(id) {
                            s.hotkey = Some(hk);
                        }
                    }
                    CaptureTarget::CategoryRandom(id) => {
                        if let Some(c) = lib.category_mut(id) {
                            c.random_hotkey = Some(hk);
                        }
                    }
                    CaptureTarget::HotbarSlot(p, i) => {
                        if let Some(Some(s)) = lib.hotbar.pages.get_mut(p).and_then(|pg| pg.slots.get_mut(i)) {
                            s.hotkey = Some(hk);
                        }
                    }
                    _ => {}
                }
            });
            if let CaptureTarget::Sound(id) = other {
                if let Some(path) = core.lib.read().sound(id).map(|s| s.path.clone()) {
                    let c = core.clone();
                    std::thread::spawn(move || {
                        let _ = c.cache.get(id, &path);
                    });
                }
            }
        }
    }
}

fn capture_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    let Some(cap) = app.st.capture.as_mut() else { return };
    if let Ok(hk) = cap.rx.try_recv() {
        if hk.vk == 0x1B && hk.mods == 0 {
            app.core.hook.cancel_capture();
            app.st.capture = None;
            return;
        }
        cap.result = Some(hk);
    }
    let target = cap.target;
    let result = cap.result;
    let label = target_label(app, target);
    let conflict_text = result.and_then(|hk| conflict(app, &hk, target));
    let mut close = false;
    let mut apply = None;
    let mut again = false;
    centered(format!("⌨  {}", t!("Hotkey festlegen", "Set hotkey"))).show(ctx, |ui| {
        ui.set_min_width(380.0);
        ui.label(RichText::new(&label).strong());
        ui.add_space(8.0);
        match result {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t!(
                        "Drücke jetzt eine Taste oder Kombination …\n(auch Maus 4/5/Mitte · Esc = Abbrechen)",
                        "Press a key or combination now …\n(mouse 4/5/middle work too · Esc = cancel)"
                    ));
                });
            }
            Some(hk) => {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(if matches!(target, CaptureTarget::AutoKey) { crate::hotkey::vk_name(hk.vk) } else { hk.label() }).size(24.0).strong().color(theme::accent()));
                });
                if let Some(c) = &conflict_text {
                    ui.label(RichText::new(format!("⚠ {} {c}. {}", t!("Bereits vergeben an", "Already used by"), t!("Überschreiben?", "Overwrite?"))).color(theme::WARN));
                }
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if let Some(hk) = result {
                if ui.add(egui::Button::new(RichText::new(t!("Übernehmen", "Apply")).strong()).fill(theme::accent_dim())).clicked() {
                    apply = Some(hk);
                }
                if ui.button(t!("Neu aufnehmen", "Record again")).clicked() {
                    again = true;
                }
            }
            if ui.button(t!("Abbrechen", "Cancel")).clicked() {
                close = true;
            }
        });
    });
    if again {
        app.begin_capture(target);
    } else if let Some(hk) = apply {
        app.core.hook.cancel_capture();
        app.st.capture = None;
        apply_capture(app, target, hk);
    } else if close {
        app.core.hook.cancel_capture();
        app.st.capture = None;
    }
}

fn recorder_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    if !app.st.recorder_open {
        return;
    }
    let core = app.core.clone();
    let mut open = true;
    egui::Window::new(format!("⏺  {}", t!("Soundrecorder", "Sound recorder"))).open(&mut open).resizable(false).show(ctx, |ui| {
        ui.set_min_width(360.0);
        ui.label(RichText::new(t!("Nimm auf, was du hörst (Lautsprecher) oder dein Mikrofon.", "Record what you hear (speakers) or your microphone.")).small());
        ui.add_space(6.0);
        match core.recording_info() {
            None => {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(format!("🔊 {}", t!("Lautsprecher aufnehmen", "Record speakers"))).min_size(egui::vec2(170.0, 32.0))).clicked() {
                        core.start_recording(RecSource::Speakers);
                    }
                    if ui.add(egui::Button::new(format!("🎙 {}", t!("Mikrofon aufnehmen", "Record microphone"))).min_size(egui::vec2(170.0, 32.0))).clicked() {
                        core.start_recording(RecSource::Microphone);
                    }
                });
            }
            Some((src, secs)) => {
                let peak = f32::from_bits(core.shared().rec_peak.swap(0, std::sync::atomic::Ordering::Relaxed));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(theme::ERR).size(18.0));
                    ui.label(RichText::new(format!(
                        "{} {:.1} s",
                        match src {
                            RecSource::Speakers => t!("Lautsprecher", "Speakers"),
                            RecSource::Microphone => t!("Mikrofon", "Microphone"),
                        },
                        secs
                    )).strong());
                    theme::meter(ui, theme::level_to_meter(peak), 140.0, "Pegel");
                });
                if ui.add(egui::Button::new(format!("⏹ {}", t!("Aufnahme beenden & speichern", "Stop & save recording"))).min_size(egui::vec2(340.0, 32.0)).fill(theme::accent_dim())).clicked() {
                    core.stop_recording();
                }
            }
        }
        ui.add_space(4.0);
        let s = core.settings.read().clone();
        ui.label(RichText::new(format!(
            "{}: {}  ·  {} {}s  ·  {}",
            t!("Ziel", "Target"),
            s.recordings_dir().display(),
            t!("Limit", "limit"),
            s.record_limit_s,
            if s.trim_silence { t!("Stille wird entfernt", "silence trimmed") } else { "" }
        )).small().color(ui.visuals().weak_text_color()));
    });
    app.st.recorder_open = open;
}

fn tts_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    if !app.st.tts_open {
        return;
    }
    let core = app.core.clone();
    let mut open = true;
    let voices = app.st.tts_voices.lock().clone();
    egui::Window::new("💬  Text-to-Speech").open(&mut open).default_width(440.0).show(ctx, |ui| {
        let mut text = core.rt.lock().tts_text.clone();
        let resp = ui.add(
            egui::TextEdit::multiline(&mut text)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .hint_text(t!("Text eingeben … (Strg+Enter spielt ab)", "Enter text … (Ctrl+Enter plays)")),
        );
        if resp.changed() {
            core.rt.lock().tts_text = text.clone();
        }
        let ctrl_enter = resp.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(format!("▶ {}", t!("Abspielen", "Play"))).fill(theme::accent_dim())).clicked() || ctrl_enter {
                core.dispatch(Action::PlayTts { text: None, mode: None }, Source::Ui);
            }
            if ui.button("🔊").on_hover_text(t!("Nur Lautsprecher (Vorschau)", "Speakers only (preview)")).clicked() {
                core.dispatch(Action::PlayTts { text: None, mode: Some(PlayMode::Speakers) }, Source::Ui);
            }
            if ui.button("🎤").on_hover_text(t!("Nur Mikrofon", "Microphone only")).clicked() {
                core.dispatch(Action::PlayTts { text: None, mode: Some(PlayMode::Microphone) }, Source::Ui);
            }
            if ui.button(format!("💾 {}", t!("Als Sound speichern", "Save as sound"))).clicked() && !text.trim().is_empty() {
                core.tts_to_library(text.clone());
            }
        });
        ui.separator();
        let mut s = core.settings.read().clone();
        let before = (s.tts_voice.clone(), s.tts_rate, s.tts_pitch);
        ui.horizontal(|ui| {
            ui.label(t!("Stimme", "Voice"));
            match &voices {
                None => {
                    ui.spinner();
                }
                Some(list) if list.is_empty() => {
                    ui.label(RichText::new(t!("Keine Stimmen verfügbar", "No voices available")).color(theme::WARN));
                }
                Some(list) => {
                    let shown = s.tts_voice.clone().unwrap_or_else(|| t!("Systemstandard", "System default").into());
                    egui::ComboBox::from_id_salt("tts_quick").width(260.0).selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.tts_voice, None, t!("Systemstandard", "System default"));
                        for v in list {
                            ui.selectable_value(&mut s.tts_voice, Some(v.clone()), v);
                        }
                    });
                }
            }
        });
        ui.add(egui::Slider::new(&mut s.tts_rate, 0.5..=3.0).text(t!("Geschwindigkeit", "Speed")));
        ui.add(egui::Slider::new(&mut s.tts_pitch, 0.0..=2.0).text(t!("Stimmlage", "Pitch")));
        if before != (s.tts_voice.clone(), s.tts_rate, s.tts_pitch) {
            let mut w = core.settings.write();
            w.tts_voice = s.tts_voice;
            w.tts_rate = s.tts_rate;
            w.tts_pitch = s.tts_pitch;
            drop(w);
            core.mark_settings_dirty();
        }
        let hk = core.settings.read().binding(SpecialAction::TtsPlay).hotkey;
        ui.label(RichText::new(match hk {
            Some(h) => format!("{} {}", t!("Hotkey zum Abspielen:", "Hotkey to play:"), h.label()),
            None => t!("Tipp: Unter Einstellungen → Hotkeys kannst du „TTS abspielen“ auf eine Taste legen.", "Tip: assign \"Play TTS\" to a key under Preferences → Hotkeys.").to_string(),
        }).small().color(ui.visuals().weak_text_color()));
    });
    app.st.tts_open = open;
}

fn color_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    if !app.st.color_tool {
        return;
    }
    let mut open = true;
    egui::Window::new(format!("🎨  {}", t!("Farbwerkzeug", "Color tool"))).open(&mut open).resizable(false).show(ctx, |ui| {
        ui.label(RichText::new(t!("Farbe wählen und dann auf Sounds oder Hotbar-Einträge klicken.", "Select a color, then click sounds or hotbar elements.")).small());
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(260.0);
            for c in theme::PALETTE {
                let (r, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
                ui.painter().rect_filled(r, 5.0, theme::rgb(*c));
                if app.st.color_pick == Some(Some(*c)) {
                    ui.painter().rect_stroke(r.expand(2.0), 6.0, egui::Stroke::new(2.0_f32, egui::Color32::WHITE));
                }
                if resp.clicked() {
                    app.st.color_pick = Some(Some(*c));
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.selectable_label(app.st.color_pick == Some(None), format!("⌫ {}", t!("Radierer", "Eraser"))).clicked() {
                app.st.color_pick = Some(None);
            }
            if ui.selectable_label(app.st.color_pick.is_none(), format!("🖱 {}", t!("Mauszeiger", "Pointer"))).clicked() {
                app.st.color_pick = None;
            }
        });
    });
    if !open {
        app.st.color_tool = false;
        app.st.color_pick = None;
    }
}

fn stats_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    if !app.st.stats_open {
        return;
    }
    let core = app.core.clone();
    let mut open = true;
    egui::Window::new(format!("📊  {}", t!("Statistik", "Stats"))).open(&mut open).default_size([460.0, 480.0]).show(ctx, |ui| {
        let lib = core.lib.read();
        let total_plays: u64 = lib.sounds.iter().map(|s| s.play_count as u64).sum();
        let total_ms: u64 = lib.sounds.iter().map(|s| s.duration_ms).sum();
        let never = lib.sounds.iter().filter(|s| s.play_count == 0).count();
        egui::Grid::new("stats_grid").num_columns(2).spacing([20.0, 4.0]).show(ui, |ui| {
            ui.label(t!("Sounds", "Sounds"));
            ui.label(RichText::new(lib.sounds.len().to_string()).strong());
            ui.end_row();
            ui.label(t!("Kategorien", "Categories"));
            ui.label(RichText::new(lib.categories.len().to_string()).strong());
            ui.end_row();
            ui.label(t!("Gesamtdauer", "Total duration"));
            ui.label(RichText::new(format_duration(total_ms)).strong());
            ui.end_row();
            ui.label(t!("Wiedergaben gesamt", "Total plays"));
            ui.label(RichText::new(total_plays.to_string()).strong().color(theme::accent()));
            ui.end_row();
            ui.label(t!("Nie gespielt", "Never played"));
            ui.label(RichText::new(never.to_string()).strong());
            ui.end_row();
        });
        ui.separator();
        ui.label(RichText::new(t!("Meistgespielt", "Most played")).strong());
        let mut top: Vec<&crate::library::Sound> = lib.sounds.iter().filter(|s| s.play_count > 0).collect();
        top.sort_by(|a, b| b.play_count.cmp(&a.play_count));
        let max = top.first().map(|s| s.play_count).unwrap_or(1).max(1);
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for s in top.iter().take(25) {
                ui.horizontal(|ui| {
                    let w = 120.0 * s.play_count as f32 / max as f32;
                    let (r, _) = ui.allocate_exact_size(egui::vec2(120.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(w.max(2.0), 12.0)), 3.0, theme::accent());
                    ui.label(RichText::new(format!("{:>4}", s.play_count)).monospace());
                    ui.add(egui::Label::new(s.display_name()).truncate());
                });
            }
        });
    });
    app.st.stats_open = open;
}

fn overview_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    if !app.st.overview_open {
        return;
    }
    let core = app.core.clone();
    let mut open = true;
    egui::Window::new(format!("⌨  {}", t!("Hotkey-Übersicht", "Hotkey overview"))).open(&mut open).default_size([520.0, 520.0]).show(ctx, |ui| {
        let s = core.settings.read().clone();
        let lib = core.lib.read();
        let d = s.distinguish_sides;
        let mut rows: Vec<(String, String, u32)> = Vec::new();
        for (a, b) in &s.special {
            if let Some(h) = b.hotkey {
                let def = special_def(*a);
                rows.push((h.label(), format!("⚙ {}", t!(def.de, def.en)), h.match_key(d)));
            }
        }
        for snd in &lib.sounds {
            if let Some(h) = snd.hotkey {
                rows.push((h.label(), format!("🎵 {}", snd.display_name()), h.match_key(d)));
            }
        }
        for c in &lib.categories {
            if let Some(h) = c.random_hotkey {
                rows.push((h.label(), format!("🔀 {}", c.name), h.match_key(d)));
            }
        }
        for (pi, p) in lib.hotbar.pages.iter().enumerate() {
            for (si, slot) in p.slots.iter().enumerate() {
                if let Some((h, snd)) = slot.as_ref().and_then(|x| x.hotkey.map(|h| (h, x.sound))) {
                    let name = lib.sound(snd).map(|x| x.display_name().to_string()).unwrap_or_default();
                    rows.push((h.label(), format!("⚡ Hotbar {}/{}: {name}", pi + 1, si + 1), h.match_key(d)));
                }
            }
        }
        let mut counts: HashMap<u32, usize> = HashMap::new();
        for r in &rows {
            *counts.entry(r.2).or_default() += 1;
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        if counts.values().any(|c| *c > 1) {
            ui.label(RichText::new(t!("⚠ Konflikte in der Hotkeykonfiguration entdeckt (rot markiert).", "⚠ Conflicting hotkey configuration detected (marked red).")).color(theme::ERR));
        }
        if rows.is_empty() {
            ui.label(t!("Noch keine Hotkeys vergeben. Rechtsklick auf einen Sound → „Hotkey festlegen“.", "No hotkeys yet. Right-click a sound → \"Set hotkey\"."));
        }
        if s.index_hotkeys {
            ui.label(format!("{} + Index → Sound #", crate::hotkey::mods_label(s.index_modifier)));
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("ov_grid").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                for (key, what, mk) in &rows {
                    let col = if counts[mk] > 1 { theme::ERR } else { theme::accent() };
                    ui.label(RichText::new(key).strong().color(col));
                    ui.label(what);
                    ui.end_row();
                }
            });
        });
    });
    app.st.overview_open = open;
}

fn setup_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    if !app.st.setup_open {
        return;
    }
    let core = app.core.clone();
    let status = core.shared().status.lock().clone();
    let devices = app.st.devices.lock().clone();
    let step_state = app.st.vmic.step();
    let busy = app.st.vmic.busy();
    let present = status.virtual_found;
    let switched = core.settings.read().win_default_mic_switched;
    let mut open = true;
    let mut ask_reboot = false;
    egui::Window::new(t!("Mikrofon einrichten", "Set up microphone"))
        .open(&mut open)
        .default_width(600.0)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new((app.logo.id(), egui::vec2(46.0, 46.0))));
                ui.vertical(|ui| {
                    ui.label(kit::bold(t!("Sounds ins Mikrofon – in einem Klick", "Sounds into your mic – in one click"), 17.0));
                    ui.label(kit::dim(t!(
                        "Windows lässt Programme nur über ein virtuelles Mikrofon in den Voice-Chat. Ferrovox richtet es für dich ein.",
                        "Windows only lets programs into voice chat through a virtual microphone. Ferrovox sets it up for you."
                    )));
                });
            });
            kit::separator(ui);

            step(ui, "1", t!("Virtuelles Mikrofon installieren", "Install virtual microphone"), present);
            if present {
                ui.label(RichText::new(format!(
                    "✓ {} ({})",
                    t!("Installiert und verbunden", "Installed and connected"),
                    status.mic_out.clone().unwrap_or_else(|| "CABLE Input".into())
                )).color(theme::OK));
            } else if busy {
                let pct = step_state.progress().unwrap_or(0.0);
                ui.add(egui::ProgressBar::new(pct).desired_width(ui.available_width()).text(step_state.label()));
                if step_state == crate::vmic::Step::WaitingForAdmin {
                    ui.label(kit::dim(t!("Ein Windows-Fenster fragt nach Administratorrechten – bitte mit „Ja“ bestätigen.", "A Windows prompt asks for administrator rights – please confirm with \"Yes\".")));
                }
            } else {
                match &step_state {
                    crate::vmic::Step::NeedsReboot => {
                        ui.label(RichText::new(step_state.label()).color(theme::WARN));
                        ui.horizontal(|ui| {
                            if kit::pill(ui, ic::POWER, t!("PC jetzt neu starten", "Restart PC now"), theme::accent()).clicked() {
                                ask_reboot = true;
                            }
                            if kit::text_btn(ui, ic::REFRESH, t!("Erneut suchen", "Search again")).clicked() {
                                core.reconnect_devices();
                                app.refresh_devices();
                            }
                        });
                    }
                    crate::vmic::Step::Failed(e) => {
                        ui.label(RichText::new(e).color(theme::ERR));
                        ui.horizontal(|ui| {
                            if kit::pill(ui, ic::REFRESH, t!("Erneut versuchen", "Try again"), theme::accent()).clicked() {
                                app.st.vmic.install(core.clone(), true);
                            }
                            if kit::text_btn(ui, ic::OPEN_FILE, t!("Mit Setup-Fenster installieren", "Install with setup window")).clicked() {
                                app.st.vmic.install(core.clone(), false);
                            }
                        });
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            if kit::pill(ui, ic::DOWNLOAD, t!("Jetzt einrichten – 1 Klick", "Set up now – 1 click"), theme::accent()).clicked() {
                                app.st.vmic.install(core.clone(), true);
                            }
                            if ui
                                .add(egui::Label::new(kit::value(t!("oder mit Setup-Fenster", "or with setup window")).small()).sense(egui::Sense::click()))
                                .clicked()
                            {
                                app.st.vmic.install(core.clone(), false);
                            }
                        });
                    }
                }
            }
            ui.label(kit::faint(t!(
                "Nutzt das offizielle, kostenlose VB-CABLE (Donationware von VB-Audio). Es wird direkt von vb-audio.com geladen, die digitale Signatur geprüft und still installiert – Windows fragt einmal nach Administratorrechten.",
                "Uses the official, free VB-CABLE (donationware by VB-Audio). It is downloaded directly from vb-audio.com, its digital signature is verified and it is installed silently – Windows asks once for administrator rights."
            )).small());
            ui.horizontal_wrapped(|ui| {
                ui.label(kit::faint(t!("Mit der Installation akzeptierst du die Lizenz von VB-Audio.", "By installing you accept VB-Audio's license.")).small());
                if ui.add(egui::Label::new(kit::value(t!("Lizenz & Spenden", "License & donate")).small()).sense(egui::Sense::click())).clicked() {
                    crate::winutil::open_url("https://vb-audio.com/Cable/");
                }
            });
            if let Some(signer) = app.st.vmic.signer() {
                ui.label(kit::faint(format!("{} {signer}", t!("Signiert von:", "Signed by:"))).small());
            }

            step(ui, "2", t!("Für Discord & Spiele automatisch nutzen", "Use automatically in Discord & games"), switched);
            if !present {
                ui.label(kit::faint(t!("Verfügbar, sobald Schritt 1 erledigt ist.", "Available once step 1 is done.")));
            } else if switched {
                ui.label(RichText::new(t!(
                    "✓ Das Ferrovox-Mikrofon ist Windows-Standard – Apps mit Mikrofon „Standard“ hören jetzt dich und deine Sounds.",
                    "✓ The Ferrovox microphone is the Windows default – apps using the \"Default\" microphone now hear you and your sounds."
                )).color(theme::OK));
                if kit::text_btn(ui, ic::CANCEL, t!("Zurück zum echten Mikrofon", "Back to real microphone")).clicked() {
                    match crate::vmic::restore_windows_default(&core) {
                        Ok(()) => core.toast(t!("Echtes Mikrofon ist wieder Windows-Standard", "Real microphone is the Windows default again"), ToastKind::Ok),
                        Err(e) => core.toast(format!("{e}"), ToastKind::Error),
                    }
                }
            } else {
                ui.horizontal(|ui| {
                    if kit::pill(ui, ic::MIC, t!("Als Windows-Standardmikrofon setzen", "Set as Windows default microphone"), theme::accent()).clicked() {
                        match crate::vmic::use_as_windows_default(&core) {
                            Ok(name) => core.toast(format!("{} {}", crate::i18n::q(&name), t!("ist jetzt das Windows-Standardmikrofon", "is now the Windows default microphone")), ToastKind::Ok),
                            Err(e) => core.toast(format!("{e}"), ToastKind::Error),
                        }
                    }
                });
                ui.label(kit::faint(t!(
                    "Dein echtes Mikrofon leitet Ferrovox automatisch mit durch. Alternativ in Discord unter „Sprache & Video“ das Eingabegerät „CABLE Output“ wählen.",
                    "Ferrovox mixes your real microphone in automatically. Alternatively select \"CABLE Output\" as input device in Discord under \"Voice & Video\"."
                )).small());
            }

            step(ui, "3", t!("Testen", "Test"), false);
            ui.horizontal(|ui| {
                if kit::text_btn(ui, ic::SPEAKERS, t!("Testton Lautsprecher", "Test tone speakers")).clicked() {
                    core.test_tone(PlayMode::Speakers);
                }
                if kit::text_btn(ui, ic::MIC, t!("Testton Mikrofon", "Test tone microphone")).clicked() {
                    core.test_tone(PlayMode::Microphone);
                }
                theme::meter(ui, app.st.meters[1], 120.0, t!("Mikrofon-Ausgang", "Microphone output"));
            });

            ui.add_space(4.0);
            egui::CollapsingHeader::new(t!("Erweitert", "Advanced")).show(ui, |ui| {
                let mut mic_out = core.settings.read().mic_out_device.clone();
                let before = mic_out.clone();
                let shown = match &mic_out {
                    None => format!("{} ({})", t!("Automatisch", "Automatic"), status.mic_out.clone().unwrap_or_else(|| "—".into())),
                    Some(s) if s.is_empty() => t!("Aus", "Off").to_string(),
                    Some(s) => s.clone(),
                };
                ui.horizontal(|ui| {
                    ui.label(kit::dim(t!("Mikrofon-Ausgang", "Microphone output")));
                    egui::ComboBox::from_id_salt("setup_micout").width(380.0).selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut mic_out, None, t!("Automatisch (virtuelles Kabel)", "Automatic (virtual cable)"));
                        ui.selectable_value(&mut mic_out, Some(String::new()), t!("Aus", "Off"));
                        if let Some(d) = &devices {
                            for o in &d.outputs {
                                ui.selectable_value(&mut mic_out, Some(o.clone()), o);
                            }
                        }
                    });
                });
                if mic_out != before {
                    let mut s = core.settings.read().clone();
                    s.mic_out_device = mic_out;
                    core.apply_settings(s);
                    core.reconnect_devices();
                }
                if present && !busy && kit::text_btn(ui, ic::DELETE, t!("Virtuelles Mikrofon deinstallieren", "Uninstall virtual microphone")).clicked() {
                    if switched {
                        let _ = crate::vmic::restore_windows_default(&core);
                    }
                    app.st.vmic.uninstall(core.clone());
                }
            });
            if !status.errors.is_empty() {
                for e in &status.errors {
                    ui.label(RichText::new(format!("⚠ {e}")).small().color(theme::WARN));
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Label::new(kit::value(t!("Alle Audio-Einstellungen …", "All audio preferences …"))).sense(egui::Sense::click())).clicked() {
                    app.open_settings(super::SettingsTab::Audio);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new(t!("Fertig", "Done")).color(egui::Color32::WHITE)).fill(theme::accent())).clicked() {
                        app.st.setup_open = false;
                    }
                });
            });
        });
    if ask_reboot {
        app.st.dialog = Some(Dialog::ConfirmReboot);
    }
    if !open {
        app.st.setup_open = false;
    }
}

fn step(ui: &mut egui::Ui, n: &str, title: &str, done: bool) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
        if done {
            ui.painter().circle_filled(r.center(), 11.0, theme::OK);
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, ic::CHECK, egui::FontId::new(11.0, egui::FontFamily::Name(theme::ICONS.into())), egui::Color32::WHITE);
        } else {
            ui.painter().circle_filled(r.center(), 11.0, theme::accent());
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, n, egui::FontId::proportional(12.0), egui::Color32::WHITE);
        }
        ui.label(kit::bold(title, 14.0));
    });
    ui.add_space(2.0);
}

fn about_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    if !app.st.about_open {
        return;
    }
    let mut open = true;
    centered(t!("Über Ferrovox", "About Ferrovox")).open(&mut open).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add(egui::Image::new((app.logo.id(), egui::vec2(112.0, 112.0))));
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() / 2.0 - 90.0);
                ui.label(RichText::new("FERRO").size(30.0).strong());
                ui.label(RichText::new("VOX").size(30.0).strong().color(theme::accent()));
            });
            ui.label(RichText::new(format!("Version {} · by T-GOW", crate::paths::VERSION)).strong());
            ui.add_space(6.0);
            ui.label(t!(
                "Soundboard mit eigener Mikrofon-Einspeisung, globalen Hotkeys,\nHotbar, Recorder, Editor, Text-to-Speech und Fernsteuerung.\nGeschrieben in Rust – schnell, schlank, ohne Treiber-Hacks.",
                "Soundboard with microphone injection, global hotkeys,\nhotbar, recorder, editor, text-to-speech and remote control.\nWritten in Rust – fast, lean, no driver hacks."
            ));
            ui.add_space(6.0);
            ui.label(RichText::new(t!("Ferrovox = „ferrum“ (Eisen/Rust) + „vox“ (Stimme)", "Ferrovox = \"ferrum\" (iron/rust) + \"vox\" (voice)")).small().italics());
            ui.add_space(8.0);
            if ui.button(t!("Nach Updates suchen", "Check for updates")).clicked() {
                crate::update::updater().check(app.core.clone(), true);
                app.st.update_open = true;
            }
            ui.add_space(4.0);
            ui.label(RichText::new("© 2026 T-GOW").small().color(ui.visuals().weak_text_color()));
        });
    });
    app.st.about_open = open;
}

fn update_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    use crate::update::{updater, State};
    if !app.st.update_open {
        return;
    }
    let state = updater().state();
    let core = app.core.clone();
    let mut open = true;
    let mut quit = false;
    centered(t!("Updates", "Updates")).open(&mut open).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.label(kit::dim(format!("{} {}", t!("Installierte Version:", "Installed version:"), crate::paths::VERSION)));
        ui.add_space(6.0);
        let label = state.label();
        match &state {
            State::Checking => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&label);
                });
            }
            State::Downloading { manifest, done } => {
                ui.label(&label);
                ui.add(egui::ProgressBar::new((*done as f32 / manifest.size.max(1) as f32).min(1.0)).show_percentage());
            }
            State::UpToDate | State::Ready { .. } => {
                ui.label(RichText::new(format!("✓ {label}")).color(theme::OK));
            }
            State::Failed(_) => {
                ui.label(RichText::new(&label).color(theme::ERR));
            }
            _ if !label.is_empty() => {
                ui.label(&label);
            }
            _ => {}
        }
        if let State::Available(m) | State::Downloading { manifest: m, .. } | State::Ready { manifest: m, .. } = &state {
            if !m.notes().is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(t!("Neu in dieser Version", "What's new")).strong());
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    ui.label(m.notes());
                });
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let primary = |ui: &mut egui::Ui, text: &str| {
                ui.add(egui::Button::new(RichText::new(text).color(egui::Color32::WHITE)).fill(theme::accent()))
            };
            match &state {
                State::Ready { .. } => {
                    if primary(ui, t!("Jetzt neu starten und aktualisieren", "Restart and update now")).clicked() {
                        quit = updater().install_and_restart();
                    }
                    if ui.button(t!("Beim Beenden installieren", "Install on exit")).clicked() {
                        updater().install_on_exit();
                        app.st.update_open = false;
                    }
                }
                State::Available(_) => {
                    if primary(ui, t!("Herunterladen", "Download")).clicked() {
                        updater().check(core.clone(), true);
                    }
                }
                State::Checking | State::Downloading { .. } | State::NotConfigured => {}
                _ => {
                    if ui.button(t!("Erneut suchen", "Check again")).clicked() {
                        updater().check(core.clone(), true);
                    }
                }
            }
        });
    });
    if quit {
        app.st.quit = true;
    }
    app.st.update_open &= open;
}

fn dialog_window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.st.dialog.take() else { return };
    let core = app.core.clone();
    let mut keep = true;
    let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if esc {
        return;
    }
    match &mut dialog {
        Dialog::RenameTag { id, text } => {
            centered(t!("Tagtext bearbeiten", "Edit tag text")).show(ctx, |ui| {
                ui.label(RichText::new(t!("Nur der angezeigte Text wird geändert. Die Sounddatei bleibt unverändert.", "Only the displayed text changes. The sound file is not modified.")).small());
                let r = ui.add(egui::TextEdit::singleline(text).desired_width(360.0));
                r.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || enter {
                        let (id, t) = (*id, text.trim().to_string());
                        core.modify_library(|lib| {
                            if let Some(s) = lib.sound_mut(id) {
                                s.tag = (!t.is_empty() && t != s.title).then_some(t);
                            }
                        });
                        keep = false;
                    }
                    if ui.button(t!("Abbrechen", "Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
        }
        Dialog::RenameFile { id, text } => {
            centered(t!("Datei umbenennen", "Rename file")).show(ctx, |ui| {
                let r = ui.add(egui::TextEdit::singleline(text).desired_width(360.0));
                r.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || enter {
                        let id = *id;
                        let old = core.lib.read().sound(id).map(|s| s.path.clone());
                        if let Some(old) = old {
                            let ext = old.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
                            let new = old.with_file_name(format!("{}.{ext}", text.trim()));
                            match std::fs::rename(&old, &new) {
                                Ok(()) => {
                                    core.cache.invalidate(id);
                                    core.modify_library(|lib| {
                                        if let Some(s) = lib.sound_mut(id) {
                                            s.path = new;
                                        }
                                    });
                                }
                                Err(e) => core.toast(format!("{}: {e}", t!("Datei konnte nicht umbenannt werden", "Failed to rename file")), ToastKind::Error),
                            }
                        }
                        keep = false;
                    }
                    if ui.button(t!("Abbrechen", "Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
        }
        Dialog::Category { id, name, icon, index } => {
            let title = if id.is_some() { t!("Kategorie bearbeiten", "Edit category") } else { t!("Kategorie hinzufügen", "Add category") };
            centered(title).show(ctx, |ui| {
                egui::Grid::new("cat_dlg").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label(t!("Name", "Name"));
                    let r = ui.add(egui::TextEdit::singleline(name).desired_width(260.0));
                    if id.is_none() {
                        r.request_focus();
                    }
                    ui.end_row();
                    ui.label(t!("Symbol", "Icon"));
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(270.0);
                        for e in ["📁", "⭐", "🎵", "😂", "🎮", "🔥", "💀", "🎙", "📢", "🎬", "🐸", "💬", "🚨", "🎉", "👾", "⚡"] {
                            if ui.selectable_label(icon == e, e).clicked() {
                                *icon = e.to_string();
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Index");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(index).desired_width(50.0).hint_text("1–99"));
                        ui.label(RichText::new(t!("für Index-Hotkeys (optional)", "for index hotkeys (optional)")).small());
                    });
                    ui.end_row();
                });
                let idx: Option<u8> = index.trim().parse::<u8>().ok().filter(|i| (1..=99).contains(i));
                let taken = idx.and_then(|i| core.lib.read().category_by_index(i).filter(|c| Some(c.id) != *id).map(|c| c.name.clone()));
                if let Some(t) = &taken {
                    ui.label(RichText::new(format!("{} {} {}", t!("Index ist bereits vergeben für", "Index is already used by"), crate::i18n::q(t), t!("– wird überschrieben", "– will be overwritten"))).color(theme::WARN).small());
                }
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || enter {
                        let (cid, n, ic) = (*id, name.trim().to_string(), icon.clone());
                        core.modify_library(|lib| {
                            if let Some(i) = idx {
                                for c in lib.categories.iter_mut() {
                                    if c.index == Some(i) && Some(c.id) != cid {
                                        c.index = None;
                                    }
                                }
                            }
                            match cid.and_then(|c| lib.category_mut(c)) {
                                Some(c) => {
                                    c.name = n;
                                    c.icon = ic;
                                    c.index = idx;
                                }
                                None => {
                                    let nid = lib.alloc_id();
                                    lib.categories.push(Category { id: nid, name: n, icon: ic, index: idx, ..Default::default() });
                                }
                            }
                        });
                        keep = false;
                    }
                    if ui.button(t!("Abbrechen", "Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
        }
        Dialog::HotbarPage { page, title } => {
            centered(t!("Hotbar-Seite umbenennen", "Rename hotbar page")).show(ctx, |ui| {
                let r = ui.add(egui::TextEdit::singleline(title).desired_width(260.0));
                r.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || enter {
                        let (p, t) = (*page, title.trim().to_string());
                        core.modify_library(|lib| {
                            if let Some(pg) = lib.hotbar.pages.get_mut(p) {
                                pg.title = t;
                            }
                        });
                        keep = false;
                    }
                    if ui.button(t!("Abbrechen", "Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
        }
        Dialog::Properties(id) => {
            let snd = core.lib.read().sound(*id).cloned();
            let mut open = true;
            egui::Window::new(t!("Eigenschaften", "Properties")).open(&mut open).collapsible(false).show(ctx, |ui| {
                let Some(s) = snd else { return };
                let cached = core.cache.peek(s.id);
                egui::Grid::new("props").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
                    let mut row = |k: &str, v: String| {
                        ui.label(RichText::new(k).color(ui.visuals().weak_text_color()));
                        ui.label(v);
                        ui.end_row();
                    };
                    row(t!("Titel", "Title"), s.title.clone());
                    row(t!("Interpret", "Artist"), s.artist.clone());
                    row("Tag", s.tag.clone().unwrap_or_default());
                    row(t!("Pfad", "Path"), s.path.display().to_string());
                    row(t!("Dauer", "Duration"), format_duration(s.duration_ms));
                    if let Some(b) = &cached {
                        row(t!("Samplerate", "Sample rate"), format!("{} Hz", b.rate));
                        row(t!("Spitzenpegel", "Peak"), format!("{:.1} dBFS", crate::audio::buffer::lin_to_db(b.peak)));
                    }
                    row(t!("Lautheit", "Loudness"), s.loudness_db.map(|l| format!("{l:.1} dBFS")).unwrap_or_else(|| "—".into()));
                    row(t!("Eigene Lautstärke", "Own volume"), format!("{:+.1} dB", s.gain_db));
                    row(t!("Hinzugefügt am", "Added on"), s.added_on.clone());
                    row(t!("Zuletzt gespielt", "Last played"), s.last_played.clone().unwrap_or_default());
                    row(t!("Abspielzähler", "Play count"), s.play_count.to_string());
                    row("Hotkey", s.hotkey.map(|h| h.label()).unwrap_or_default());
                    if s.trim_start_ms > 0 || s.trim_end_ms > 0 {
                        row(t!("Zuschnitt", "Trim"), format!("{} – {}", format_duration(s.trim_start_ms), format_duration(s.trim_end_ms)));
                    }
                });
            });
            if !open {
                keep = false;
            }
        }
        Dialog::DeadEntries(ids) => {
            centered(t!("Tote Einträge", "Dead entries")).show(ctx, |ui| {
                if ids.is_empty() {
                    ui.label(t!("Keine toten Einträge gefunden – alle Dateien sind vorhanden.", "No dead entries found – all files exist."));
                    if ui.button("OK").clicked() || enter {
                        keep = false;
                    }
                    return;
                }
                ui.label(format!("{} {}", ids.len(), t!("Einträge verweisen auf fehlende Dateien:", "entries point to missing files:")));
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    let lib = core.lib.read();
                    for id in ids.iter() {
                        if let Some(s) = lib.sound(*id) {
                            ui.label(RichText::new(s.path.display().to_string()).small().color(theme::ERR));
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(t!("Aus der Soundliste entfernen", "Remove from sound list")).clicked() {
                        let v = ids.clone();
                        core.modify_library(|lib| lib.remove_sounds(&v));
                        core.toast(format!("{} {}", v.len(), t!("tote Einträge entfernt", "dead entries removed")), ToastKind::Ok);
                        keep = false;
                    }
                    if ui.button(t!("Schließen", "Close")).clicked() {
                        keep = false;
                    }
                });
            });
        }
        Dialog::ConfirmReboot => {
            centered(t!("PC neu starten", "Restart PC")).show(ctx, |ui| {
                ui.label(t!(
                    "Windows wird in 15 Sekunden neu gestartet, damit das virtuelle Mikrofon erscheint.\nBitte speichere vorher offene Arbeit in anderen Programmen.",
                    "Windows will restart in 15 seconds so the virtual microphone appears.\nPlease save open work in other programs first."
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(t!("Jetzt neu starten", "Restart now")).color(egui::Color32::WHITE)).fill(theme::accent())).clicked() {
                        core.shutdown();
                        let msg = t!("Ferrovox: Neustart fuer das virtuelle Mikrofon", "Ferrovox: restart for the virtual microphone");
                        let _ = std::process::Command::new("shutdown.exe").args(["/r", "/t", "15", "/c", msg]).spawn();
                        keep = false;
                    }
                    if ui.button(t!("Später", "Later")).clicked() {
                        keep = false;
                    }
                });
            });
        }
        Dialog::ConfirmRemove { ids, from_category } => {
            centered(t!("Einträge entfernen", "Remove entries")).show(ctx, |ui| {
                let what = if from_category.is_some() { t!("aus der Kategorie", "from the category") } else { t!("aus der Soundliste", "from the sound list") };
                ui.label(format!("{} {} {}?", ids.len(), t!("Einträge", "entries"), what));
                ui.label(RichText::new(t!("Die Dateien selbst werden nicht gelöscht.", "The files themselves are not deleted.")).small());
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(t!("Entfernen", "Remove")).fill(theme::ERR.gamma_multiply(0.6))).clicked() || enter {
                        let (v, cat) = (ids.clone(), *from_category);
                        core.modify_library(|lib| match cat {
                            Some(c) => lib.remove_from_category(c, &v),
                            None => lib.remove_sounds(&v),
                        });
                        core.rt.lock().selected.clear();
                        keep = false;
                    }
                    if ui.button(t!("Abbrechen", "Cancel")).clicked() {
                        keep = false;
                    }
                });
            });
        }
    }
    if keep {
        app.st.dialog = Some(dialog);
    }
}
