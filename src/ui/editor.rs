use super::{theme, FerrovoxApp};
use crate::audio::buffer::{self, SoundBuffer};
use crate::core::ToastKind;
use crate::library::{format_duration, SoundId};
use crate::t;
use eframe::egui::{self, RichText};
use std::path::PathBuf;
use std::sync::Arc;

pub struct EditorState {
    id: SoundId,
    name: String,
    path: PathBuf,
    buf: Arc<SoundBuffer>,
    undo: Vec<Arc<SoundBuffer>>,
    redo: Vec<Arc<SoundBuffer>>,
    sel: Option<(usize, usize)>,
    view: (usize, usize),
    drag_from: Option<usize>,
    preview: Option<(u64, usize)>,
    dirty: bool,
    boost_db: f32,
    confirm_overwrite: bool,
}

pub fn open(app: &mut FerrovoxApp, id: SoundId) {
    let Some(snd) = app.core.lib.read().sound(id).cloned() else { return };
    let (use_ext, ext) = {
        let s = app.core.settings.read();
        (s.use_external_editor, s.external_editor.clone())
    };
    if use_ext {
        match ext {
            Some(exe) => {
                if let Err(e) = std::process::Command::new(&exe).arg(&snd.path).spawn() {
                    app.core.toast(format!("{}: {e}", t!("Externer Soundeditor konnte nicht geöffnet werden", "Failed to open external sound editor")), ToastKind::Error);
                }
            }
            None => app.core.toast(t!("Externer Soundeditor nicht festgelegt", "External sound editor not set"), ToastKind::Warn),
        }
        return;
    }
    if snd.duration_ms > 15 * 60 * 1000 {
        app.core.toast(
            t!("Die Datei ist sehr lang – der Editor kann langsam werden.", "The file is very long – the editor may become slow."),
            ToastKind::Warn,
        );
    }
    match app.core.cache.get(id, &snd.path) {
        Ok(buf) => {
            let frames = buf.frames();
            app.st.editor = Some(EditorState {
                id,
                name: snd.display_name().to_string(),
                path: snd.path.clone(),
                buf,
                undo: Vec::new(),
                redo: Vec::new(),
                sel: None,
                view: (0, frames),
                drag_from: None,
                preview: None,
                dirty: false,
                boost_db: 3.0,
                confirm_overwrite: false,
            });
        }
        Err(e) => app.core.toast(format!("{}: {e}", t!("Datei konnte nicht geladen werden", "Failed to load file")), ToastKind::Error),
    }
}

impl EditorState {
    fn range(&self) -> (usize, usize) {
        self.sel.filter(|(a, b)| b > a).unwrap_or((0, self.buf.frames()))
    }

    fn apply(&mut self, f: impl FnOnce(&mut Vec<f32>, usize, usize)) {
        let (a, b) = self.range();
        let mut samples = self.buf.samples.clone();
        f(&mut samples, a, b);
        self.undo.push(self.buf.clone());
        if self.undo.len() > 30 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.buf = Arc::new(SoundBuffer::new(samples, self.buf.rate));
        let frames = self.buf.frames();
        self.view = (self.view.0.min(frames.saturating_sub(1)), self.view.1.min(frames).max(1));
        if let Some((sa, sb)) = self.sel {
            self.sel = Some((sa.min(frames), sb.min(frames)));
        }
        self.dirty = true;
    }

    fn zoom(&mut self, factor: f64, center: usize) {
        let total = self.buf.frames();
        let span = (self.view.1 - self.view.0) as f64;
        let new_span = (span * factor).clamp(64.0, total as f64);
        let rel = (center - self.view.0) as f64 / span.max(1.0);
        let start = (center as f64 - rel * new_span).clamp(0.0, total as f64 - new_span);
        self.view = (start as usize, (start + new_span) as usize);
    }
}

pub fn window(app: &mut FerrovoxApp, ctx: &egui::Context) {
    let Some(mut ed) = app.st.editor.take() else { return };
    let core = app.core.clone();
    let mut open = true;
    let mut close = false;
    egui::Window::new(format!("✏  {} – {}", t!("Soundeditor", "Sound editor"), ed.name))
        .open(&mut open)
        .default_size([860.0, 420.0])
        .min_width(600.0)
        .show(ctx, |ui| {
            let rate = ed.buf.rate as f64;
            let to_ms = |f: usize| (f as f64 * 1000.0 / rate) as u64;

            ui.horizontal_wrapped(|ui| {
                let playing = ed.preview.is_some();
                if ui.button(if playing { "⏹" } else { "▶" }).on_hover_text(t!("Abspielen / Stopp", "Play / stop")).clicked() {
                    if let Some((v, _)) = ed.preview.take() {
                        core.stop_voice(v);
                    } else {
                        let (a, b) = ed.range();
                        let v = core.preview(ed.buf.clone(), to_ms(a), to_ms(b));
                        ed.preview = Some((v, a));
                    }
                }
                ui.separator();
                let has_sel = ed.sel.is_some_and(|(a, b)| b > a);
                if ui.add_enabled(has_sel, egui::Button::new(t!("Zuschneiden", "Crop"))).on_hover_text(t!("Nur die Auswahl behalten", "Keep only the selection")).clicked() {
                    ed.apply(|s, a, b| *s = s[a * 2..b * 2].to_vec());
                    ed.sel = None;
                    ed.view = (0, ed.buf.frames());
                }
                if ui.add_enabled(has_sel, egui::Button::new(t!("Ausschneiden", "Cut"))).on_hover_text(t!("Auswahl entfernen", "Remove selection")).clicked() {
                    ed.apply(|s, a, b| {
                        s.drain(a * 2..b * 2);
                    });
                    ed.sel = None;
                    ed.view = (0, ed.buf.frames());
                }
                if ui.button("Fade-in").clicked() {
                    ed.apply(|s, a, b| buffer::fade(s, a, b, true));
                }
                if ui.button("Fade-out").clicked() {
                    ed.apply(|s, a, b| buffer::fade(s, a, b, false));
                }
                if ui.button(t!("Rückwärts", "Reverse")).clicked() {
                    ed.apply(|s, a, b| buffer::reverse(s, a, b));
                }
                if ui.button(t!("Stille", "Silence")).clicked() {
                    ed.apply(|s, a, b| s[a * 2..b * 2].iter_mut().for_each(|x| *x = 0.0));
                }
                ui.add(egui::DragValue::new(&mut ed.boost_db).range(-24.0..=24.0).speed(0.25).suffix(" dB"));
                if ui.button("Boost").clicked() {
                    let db = ed.boost_db;
                    ed.apply(|s, a, b| buffer::apply_gain(s, a, b, db));
                }
                if ui.button(t!("Normalisieren", "Normalize")).clicked() {
                    ed.apply(|s, a, b| {
                        let peak = s[a * 2..b * 2].iter().fold(0.0f32, |m, x| m.max(x.abs()));
                        if peak > 0.0 {
                            let g = 0.98 / peak;
                            s[a * 2..b * 2].iter_mut().for_each(|x| *x *= g);
                        }
                    });
                }
                ui.separator();
                if ui.add_enabled(!ed.undo.is_empty(), egui::Button::new("↶")).on_hover_text(t!("Schritt zurück", "Undo")).clicked() {
                    if let Some(prev) = ed.undo.pop() {
                        ed.redo.push(std::mem::replace(&mut ed.buf, prev));
                        ed.view = (0, ed.buf.frames());
                        ed.sel = None;
                    }
                }
                if ui.add_enabled(!ed.redo.is_empty(), egui::Button::new("↷")).on_hover_text(t!("Schritt vor", "Redo")).clicked() {
                    if let Some(next) = ed.redo.pop() {
                        ed.undo.push(std::mem::replace(&mut ed.buf, next));
                        ed.view = (0, ed.buf.frames());
                        ed.sel = None;
                    }
                }
                ui.separator();
                if ui.button("🔍+").clicked() {
                    let c = (ed.view.0 + ed.view.1) / 2;
                    ed.zoom(0.5, c);
                }
                if ui.button("🔍−").clicked() {
                    let c = (ed.view.0 + ed.view.1) / 2;
                    ed.zoom(2.0, c);
                }
                if ui.button(t!("Alles", "Fit")).clicked() {
                    ed.view = (0, ed.buf.frames());
                }
                if ui.add_enabled(has_sel, egui::Button::new(t!("Auswahl zoomen", "Zoom selection"))).clicked() {
                    if let Some((a, b)) = ed.sel {
                        ed.view = (a, b.max(a + 64).min(ed.buf.frames()));
                    }
                }
            });

            let height = (ui.available_height() - 90.0).clamp(120.0, 600.0);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 6.0, ui.visuals().extreme_bg_color);
            let (v0, v1) = ed.view;
            let span = (v1 - v0).max(1);
            let frame_at = |x: f32| -> usize { v0 + (((x - rect.left()) / rect.width()).clamp(0.0, 1.0) * span as f32) as usize };
            let x_at = |f: usize| -> f32 { rect.left() + (f.saturating_sub(v0)) as f32 / span as f32 * rect.width() };
            if let Some((a, b)) = ed.sel {
                if b > a && b > v0 && a < v1 {
                    let r = egui::Rect::from_x_y_ranges(x_at(a.max(v0))..=x_at(b.min(v1)), rect.y_range());
                    painter.rect_filled(r, 0.0, theme::accent().gamma_multiply(0.18));
                }
            }
            let cols = rect.width() as usize;
            let peaks = ed.buf.peaks(v0, v1, cols);
            let mid = rect.center().y;
            let amp = rect.height() / 2.0 - 4.0;
            painter.hline(rect.x_range(), mid, egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color().gamma_multiply(0.3)));
            for (i, (lo, hi)) in peaks.iter().enumerate() {
                let x = rect.left() + i as f32 + 0.5;
                painter.line_segment(
                    [egui::pos2(x, mid - hi * amp), egui::pos2(x, mid - lo * amp)],
                    egui::Stroke::new(1.0_f32, theme::accent()),
                );
            }
            if let Some((v, _)) = ed.preview {
                if core.rt.lock().finished.contains(&v) {
                    ed.preview = None;
                } else if let Some(pos) = core.voice_pos_frames(v) {
                    let x = x_at(pos as usize);
                    if rect.x_range().contains(x) {
                        painter.vline(x, rect.y_range(), egui::Stroke::new(1.5_f32, egui::Color32::WHITE));
                    }
                    ui.ctx().request_repaint();
                }
            }
            if resp.drag_started() {
                if let Some(p) = resp.interact_pointer_pos() {
                    ed.drag_from = Some(frame_at(p.x));
                }
            }
            if resp.dragged() {
                if let (Some(from), Some(p)) = (ed.drag_from, resp.interact_pointer_pos()) {
                    let to = frame_at(p.x);
                    ed.sel = Some((from.min(to), from.max(to)));
                }
            }
            if resp.drag_stopped() {
                ed.drag_from = None;
            }
            if resp.clicked() {
                ed.sel = None;
            }
            if resp.hovered() {
                let (scroll, zoom_mod) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.command));
                if zoom_mod && scroll.y != 0.0 {
                    if let Some(p) = resp.hover_pos() {
                        ed.zoom(if scroll.y > 0.0 { 0.85 } else { 1.0 / 0.85 }, frame_at(p.x));
                    }
                } else if scroll.y != 0.0 || scroll.x != 0.0 {
                    let d = -(scroll.x + scroll.y) / rect.width() * span as f32;
                    let total = ed.buf.frames() as f32;
                    let start = (v0 as f32 + d).clamp(0.0, total - span as f32);
                    ed.view = (start as usize, start as usize + span);
                }
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}: {}", t!("Länge", "Length"), format_duration(ed.buf.duration_ms()))).small());
                if let Some((a, b)) = ed.sel {
                    ui.label(RichText::new(format!(
                        "{}: {:.2}s – {:.2}s ({:.2}s)",
                        t!("Auswahl", "Selection"),
                        a as f64 / rate,
                        b as f64 / rate,
                        (b - a) as f64 / rate
                    )).small().color(theme::accent()));
                }
                ui.label(RichText::new(format!("{} Hz · {:.1} dBFS", ed.buf.rate, ed.buf.loudness_db)).small().color(ui.visuals().weak_text_color()));
                ui.label(RichText::new(t!("Strg+Mausrad = Zoom", "Ctrl+wheel = zoom")).small().color(ui.visuals().weak_text_color()));
            });

            ui.separator();
            ui.horizontal(|ui| {
                let has_sel = ed.sel.is_some_and(|(a, b)| b > a);
                if ui
                    .add_enabled(has_sel, egui::Button::new(t!("Auswahl als Start/Ende nutzen (Datei bleibt unverändert)", "Use selection as start/end (file unchanged)")))
                    .clicked()
                {
                    let (a, b) = ed.range();
                    let (sa, sb) = (to_ms(a), to_ms(b));
                    let id = ed.id;
                    core.modify_library(|lib| {
                        if let Some(s) = lib.sound_mut(id) {
                            s.trim_start_ms = sa;
                            s.trim_end_ms = sb;
                        }
                    });
                    core.toast(t!("Start/Ende gesetzt", "Start/end set"), ToastKind::Ok);
                }
                if ui.button(t!("Trimmen zurücksetzen", "Reset trim")).clicked() {
                    let id = ed.id;
                    core.modify_library(|lib| {
                        if let Some(s) = lib.sound_mut(id) {
                            s.trim_start_ms = 0;
                            s.trim_end_ms = 0;
                        }
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("Schließen", "Close")).clicked() {
                        close = true;
                    }
                    if ui.add_enabled(ed.dirty, egui::Button::new(t!("Speichern unter …", "Save as …"))).clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("WAV", &["wav"])
                            .set_file_name(format!("{}.wav", ed.path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default()))
                            .save_file()
                        {
                            save(&core, &mut ed, p);
                        }
                    }
                    let is_wav = ed.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav"));
                    let label = if is_wav { t!("Speichern (überschreiben)", "Save (overwrite)") } else { t!("Speichern als WAV", "Save as WAV") };
                    if ui.add_enabled(ed.dirty, egui::Button::new(RichText::new(label).strong()).fill(theme::accent_dim())).clicked() {
                        if is_wav {
                            ed.confirm_overwrite = true;
                        } else {
                            let stem = ed.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                            let target = ed.path.with_file_name(format!("{stem} ({}).wav", t!("bearbeitet", "edited")));
                            save(&core, &mut ed, target);
                        }
                    }
                    if ed.confirm_overwrite {
                        ui.label(RichText::new(t!("Originalen Sound überschreiben?", "Overwrite original sound?")).color(theme::WARN));
                        if ui.small_button(t!("Ja", "Yes")).clicked() {
                            ed.confirm_overwrite = false;
                            let p = ed.path.clone();
                            save(&core, &mut ed, p);
                        }
                        if ui.small_button(t!("Nein", "No")).clicked() {
                            ed.confirm_overwrite = false;
                        }
                    }
                });
            });
        });
    if open && !close {
        app.st.editor = Some(ed);
    } else if let Some((v, _)) = ed.preview {
        core.stop_voice(v);
    }
}

fn save(core: &Arc<crate::core::Core>, ed: &mut EditorState, path: PathBuf) {
    match crate::audio::wav::save_buffer(&path, &ed.buf) {
        Ok(()) => {
            let (id, dur, loud) = (ed.id, ed.buf.duration_ms(), ed.buf.loudness_db);
            core.cache.invalidate(id);
            let p = path.clone();
            core.modify_library(|lib| {
                if let Some(s) = lib.sound_mut(id) {
                    s.path = p;
                    s.duration_ms = dur;
                    s.loudness_db = Some(loud);
                    s.trim_start_ms = 0;
                    s.trim_end_ms = 0;
                }
            });
            core.cache.insert(id, &path, ed.buf.clone());
            ed.path = path;
            ed.dirty = false;
            core.toast(t!("Datei erfolgreich gespeichert.", "File successfully saved."), ToastKind::Ok);
        }
        Err(e) => core.toast(format!("{}: {e}", t!("Datei konnte nicht gespeichert werden", "Failed to save file")), ToastKind::Error),
    }
}
