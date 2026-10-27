use crate::actions::{Action, Source};
use crate::audio::buffer::db_to_lin;
use crate::audio::cache::SoundCache;
use crate::audio::ducking::{self, DuckCmd};
use crate::audio::engine::{DeviceConfig, EngineShared};
use crate::audio::{AudioEngine, EngineCmd, EngineEvent, PlayRequest, RecSource, SoundBuffer};
use crate::autokeys;
use crate::hook::{DigitCfg, HookShared};
use crate::hotkey::{Hotkey, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2};
use crate::library::{Category, CategoryId, Library, Snapshot, Sound, SoundId};
use crate::settings::{AutoKeyCondition, AutoKeyMode, NormMode, PlayMode, Settings, SpecialAction};
use crate::t;
use crossbeam_channel::{select, tick, unbounded, Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Info,
    Ok,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub at: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Notify {
    Enabled,
    Disabled,
    ModeBoth,
    ModeSpeakers,
    ModeMic,
    Page(u8),
    RecordStart,
    RecordStop,
    Error,
}

#[derive(Clone, Copy)]
struct VoiceInfo {
    mic: bool,
}

pub struct Runtime {
    pub current: Option<SoundId>,
    pub current_voice: Option<u64>,
    pub current_mode: PlayMode,
    pub current_by_hotkey: bool,
    pub current_rate: u32,
    pub current_dur_ms: u64,
    pub current_start_ms: u64,
    pub history: Vec<SoundId>,
    pub selected: Vec<SoundId>,
    pub selected_category: Option<CategoryId>,
    pub search: String,
    pub view_order: Vec<SoundId>,
    pub paused: bool,
    pub recording: Option<(RecSource, Instant)>,
    autokeys_held: bool,
    autokeys_tapped: bool,
    ducked: bool,
    active: HashMap<u64, VoiceInfo>,
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
    pub toasts: Vec<Toast>,
    first_play_done: bool,
    pub tts_text: String,
    next_voice: u64,
    pub status: String,
    lib_dirty_since: Option<Instant>,
    pub scroll_to: Option<SoundId>,
    pub focus_search: bool,
    pub finished: Vec<u64>,
    pub open_panel: Vec<String>,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime {
            current: None,
            current_voice: None,
            current_mode: PlayMode::Both,
            current_by_hotkey: false,
            current_rate: 48_000,
            current_dur_ms: 0,
            current_start_ms: 0,
            history: Vec::new(),
            selected: Vec::new(),
            selected_category: None,
            search: String::new(),
            view_order: Vec::new(),
            paused: false,
            recording: None,
            autokeys_held: false,
            autokeys_tapped: false,
            ducked: false,
            active: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            toasts: Vec::new(),
            first_play_done: false,
            tts_text: String::new(),
            next_voice: 1,
            status: String::new(),
            lib_dirty_since: None,
            scroll_to: None,
            focus_search: false,
            finished: Vec::new(),
            open_panel: Vec::new(),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct PlaybackStatus {
    pub sound: Option<SoundId>,
    pub name: String,
    pub pos_ms: u64,
    pub dur_ms: u64,
    pub playing: bool,
    pub paused: bool,
}

pub enum Msg {
    Action(Action, Source),
}

pub struct Core {
    pub settings: RwLock<Settings>,
    pub lib: RwLock<Library>,
    pub rt: Mutex<Runtime>,
    pub engine: AudioEngine,
    pub cache: SoundCache,
    pub hook: Arc<HookShared>,
    tx: Sender<Msg>,
    duck_tx: Sender<DuckCmd>,
    pub ctx: OnceLock<eframe::egui::Context>,
    pub hwnd: AtomicIsize,
    settings_dirty: AtomicBool,
    notify_cache: Mutex<HashMap<Notify, Arc<SoundBuffer>>>,
    noise: OnceLock<Arc<SoundBuffer>>,
    me: OnceLock<std::sync::Weak<Core>>,
}

pub fn masters(s: &Settings) -> (f32, f32) {
    let v = if s.muted {
        0.0
    } else if s.linear_volume_slider {
        s.volume
    } else {
        s.volume * s.volume
    };
    (v * s.speakers_gain, v * s.mic_gain)
}

fn device_config(s: &Settings) -> DeviceConfig {
    let (spk, mic) = masters(s);
    DeviceConfig {
        speakers: s.speaker_device.clone(),
        mic_out: s.mic_out_device.clone(),
        mic_in: s.mic_in_device.clone(),
        passthrough: s.mic_passthrough,
        pass_gain: s.mic_passthrough_gain,
        block_voice: s.block_voice_while_playing,
        spk_master: spk,
        mic_master: mic,
    }
}

impl Core {
    pub fn new(settings: Settings, lib: Library) -> Arc<Core> {
        let (tx, rx) = unbounded::<Msg>();
        let (hook_tx, hook_rx) = unbounded::<Action>();
        let hook = crate::hook::start(hook_tx);
        let engine = AudioEngine::start();
        let cache = SoundCache::new(settings.cache_mb);
        lib.backup_daily();
        let core = Arc::new(Core {
            settings: RwLock::new(settings),
            lib: RwLock::new(lib),
            rt: Mutex::new(Runtime::default()),
            engine,
            cache,
            hook,
            tx,
            duck_tx: ducking::start(),
            ctx: OnceLock::new(),
            hwnd: AtomicIsize::new(0),
            settings_dirty: AtomicBool::new(false),
            notify_cache: Mutex::new(HashMap::new()),
            noise: OnceLock::new(),
            me: OnceLock::new(),
        });
        let _ = core.me.set(Arc::downgrade(&core));
        {
            let s = core.settings.read();
            crate::i18n::set_language(s.language);
            core.engine.send(EngineCmd::Configure(device_config(&s)));
            core.rt.lock().current_mode = s.play_mode;
        }
        core.apply_hook_flags();
        core.rebuild_hotkeys();

        let c = core.clone();
        std::thread::Builder::new().name("ferrovox-core".into()).spawn(move || c.worker(rx, hook_rx)).ok();
        let c = core.clone();
        std::thread::Builder::new().name("ferrovox-preload".into()).spawn(move || c.preload()).ok();
        core
    }

    pub fn dispatch(&self, action: Action, src: Source) {
        let _ = self.tx.send(Msg::Action(action, src));
    }

    pub fn repaint(&self) {
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }

    pub fn toast(&self, text: impl Into<String>, kind: ToastKind) {
        let text = text.into();
        match kind {
            ToastKind::Error => log::error!("{text}"),
            ToastKind::Warn => log::warn!("{text}"),
            _ => log::info!("{text}"),
        }
        {
            let mut rt = self.rt.lock();
            rt.status = text.clone();
            rt.toasts.push(Toast { text, kind, at: Instant::now() });
            if rt.toasts.len() > 6 {
                rt.toasts.remove(0);
            }
        }
        self.repaint();
    }

    fn worker(self: Arc<Self>, rx: Receiver<Msg>, hook_rx: Receiver<Action>) {
        let ticker = tick(Duration::from_millis(250));
        let events = self.engine.events.clone();
        loop {
            select! {
                recv(rx) -> m => match m {
                    Ok(Msg::Action(a, src)) => self.execute(a, src),
                    Err(_) => break,
                },
                recv(hook_rx) -> a => if let Ok(a) = a { self.execute(a, Source::Hotkey) },
                recv(events) -> e => if let Ok(e) = e { self.on_engine_event(e) },
                recv(ticker) -> _ => self.on_tick(),
            }
        }
    }

    fn on_tick(&self) {
        let save = {
            let rt = self.rt.lock();
            rt.lib_dirty_since.is_some_and(|t| t.elapsed() > Duration::from_millis(1200))
        };
        if save {
            self.save_library_now();
        }
        if self.settings_dirty.swap(false, Ordering::Relaxed) {
            if let Err(e) = self.settings.read().save() {
                log::error!("Einstellungen speichern: {e}");
            }
        }
        let limit = self.settings.read().record_limit_s;
        let rec = self.rt.lock().recording;
        if let Some((_, started)) = rec {
            if limit > 0 && started.elapsed() >= Duration::from_secs(limit as u64) {
                self.stop_recording();
            }
            self.repaint();
        }
        let playing = self.rt.lock().current_voice.is_some();
        if playing {
            self.repaint();
        }
    }

    pub fn mark_lib_dirty(&self) {
        let mut rt = self.rt.lock();
        if rt.lib_dirty_since.is_none() {
            rt.lib_dirty_since = Some(Instant::now());
        }
    }

    pub fn mark_settings_dirty(&self) {
        self.settings_dirty.store(true, Ordering::Relaxed);
    }

    pub fn save_library_now(&self) {
        self.rt.lock().lib_dirty_since = None;
        let res = self.lib.read().save(&crate::paths::library_file());
        if let Err(e) = res {
            self.toast(format!("{}: {e}", t!("Soundliste konnte nicht gespeichert werden", "Could not save sound list")), ToastKind::Error);
        }
    }

    pub fn shutdown(&self) {
        self.stop();
        self.release_autokeys();
        let _ = self.duck_tx.send(DuckCmd::Restore);
        self.save_library_now();
        let _ = self.settings.read().save();
    }

    fn preload(self: Arc<Self>) {
        let (enabled, items) = {
            let s = self.settings.read();
            let lib = self.lib.read();
            let mut ids: Vec<(SoundId, PathBuf)> =
                lib.sounds.iter().filter(|s| s.hotkey.is_some()).map(|s| (s.id, s.path.clone())).collect();
            for page in &lib.hotbar.pages {
                for slot in page.slots.iter().flatten() {
                    if let Some(snd) = lib.sound(slot.sound) {
                        ids.push((snd.id, snd.path.clone()));
                    }
                }
            }
            (s.preload_hotkey_sounds, ids)
        };
        if !enabled {
            return;
        }
        let mut seen = HashSet::new();
        for (id, path) in items {
            if seen.insert(id) {
                if let Ok(buf) = self.cache.get(id, &path) {
                    self.store_analysis(id, &buf);
                }
            }
        }
        log::info!("{} Sounds vorgeladen", seen.len());
        self.analyze_missing_blocking();
    }

    fn store_analysis(&self, id: SoundId, buf: &SoundBuffer) {
        let loud = buf.loudness_db;
        let changed = {
            let mut lib = self.lib.write();
            match lib.sound_mut(id) {
                Some(s) => {
                    let mut changed = false;
                    if s.loudness_db.map_or(true, |l| (l - loud).abs() > 0.05) {
                        s.loudness_db = Some(loud);
                        changed = true;
                    }
                    if s.wave.is_empty() {
                        s.wave = buf.wave_summary(96);
                        changed = true;
                    }
                    if s.duration_ms == 0 {
                        s.duration_ms = buf.duration_ms();
                        changed = true;
                    }
                    changed
                }
                None => false,
            }
        };
        if changed {
            self.mark_lib_dirty();
            self.repaint();
        }
    }

    pub fn analyze_missing(self: &Arc<Self>) {
        let core = self.clone();
        std::thread::Builder::new().name("ferrovox-analyze".into()).spawn(move || core.analyze_missing_blocking()).ok();
    }

    fn analyze_missing_blocking(&self) {
        let todo: Vec<(SoundId, PathBuf)> = self
            .lib
            .read()
            .sounds
            .iter()
            .filter(|s| (s.wave.is_empty() || s.loudness_db.is_none()) && s.path.exists())
            .map(|s| (s.id, s.path.clone()))
            .collect();
        for (id, path) in todo {
            let buf = match self.cache.peek(id) {
                Some(b) => b,
                None => match crate::audio::decode::decode_file(&path) {
                    Ok(b) => Arc::new(b),
                    Err(e) => {
                        log::warn!("Analyse {path:?}: {e}");
                        continue;
                    }
                },
            };
            self.store_analysis(id, &buf);
        }
    }

    pub fn apply_settings(&self, new: Settings) {
        let old = self.settings.read().clone();
        let devices_changed = device_config(&old) != device_config(&new)
            && (old.speaker_device != new.speaker_device
                || old.mic_out_device != new.mic_out_device
                || old.mic_in_device != new.mic_in_device
                || old.mic_passthrough != new.mic_passthrough);
        crate::i18n::set_language(new.language);
        self.cache.set_capacity(new.cache_mb);
        let dev = device_config(&new);
        *self.settings.write() = new;
        if devices_changed {
            self.engine.send(EngineCmd::Configure(dev));
        } else {
            self.engine.send(EngineCmd::Masters { speakers: dev.spk_master, mic: dev.mic_master });
            self.engine.send(EngineCmd::BlockVoice(dev.block_voice));
            self.engine.send(EngineCmd::PassGain(dev.pass_gain));
        }
        self.apply_hook_flags();
        self.rebuild_hotkeys();
        self.mark_settings_dirty();
    }

    pub fn reconnect_devices(&self) {
        let dev = device_config(&self.settings.read());
        self.engine.send(EngineCmd::Configure(dev));
    }

    fn apply_hook_flags(&self) {
        let s = self.settings.read();
        self.hook.enabled.store(s.hotkeys_enabled, Ordering::Relaxed);
        self.hook.pass_through.store(s.pass_through_hotkeys, Ordering::Relaxed);
        self.hook.distinguish.store(s.distinguish_sides, Ordering::Relaxed);
        self.hook.set_digits(DigitCfg {
            index_enabled: s.index_hotkeys,
            index_mods: s.index_modifier,
            numcode_enabled: s.numcode_hotkeys,
            numcode_mods: s.numcode_modifier,
        });
    }

    pub fn rebuild_hotkeys(&self) {
        let s = self.settings.read();
        let lib = self.lib.read();
        let d = s.distinguish_sides;
        let mut map: HashMap<u32, Action> = HashMap::new();
        let mut always = HashSet::new();
        let mut mouse = false;
        let mut add = |hk: Hotkey, a: Action, map: &mut HashMap<u32, Action>| {
            if matches!(hk.vk, VK_MBUTTON | VK_XBUTTON1 | VK_XBUTTON2) {
                mouse = true;
            }
            map.entry(hk.match_key(d)).or_insert(a);
        };
        for (action, b) in &s.special {
            if let Some(hk) = b.hotkey {
                add(hk, Action::Special(*action), &mut map);
                if matches!(action, SpecialAction::HotkeysEnable | SpecialAction::HotkeysToggle | SpecialAction::ShowWindow) {
                    always.insert(hk.match_key(d));
                }
            }
        }
        for snd in &lib.sounds {
            if let Some(hk) = snd.hotkey {
                add(hk, Action::PlaySound { id: snd.id, mode: None }, &mut map);
            }
        }
        for cat in &lib.categories {
            if let Some(hk) = cat.random_hotkey {
                add(hk, Action::RandomInCategory { cat: Some(cat.id), mode: None }, &mut map);
            }
        }
        for (pi, page) in lib.hotbar.pages.iter().enumerate() {
            for (si, slot) in page.slots.iter().enumerate() {
                if let Some(hk) = slot.as_ref().and_then(|s| s.hotkey) {
                    add(hk, Action::PlayHotbarSlot { page: pi, slot: si }, &mut map);
                }
            }
        }
        self.hook.set_map(map, always, mouse);
    }

    pub fn modify_library<R>(&self, f: impl FnOnce(&mut Library) -> R) -> R {
        let r = {
            let mut lib = self.lib.write();
            let snap = lib.snapshot();
            let r = f(&mut lib);
            let mut rt = self.rt.lock();
            rt.undo.push(snap);
            if rt.undo.len() > 60 {
                rt.undo.remove(0);
            }
            rt.redo.clear();
            r
        };
        self.mark_lib_dirty();
        self.rebuild_hotkeys();
        self.repaint();
        r
    }

    pub fn touch_library(&self, f: impl FnOnce(&mut Library)) {
        f(&mut self.lib.write());
        self.mark_lib_dirty();
    }

    pub fn undo(&self) {
        let ok = {
            let mut lib = self.lib.write();
            let mut rt = self.rt.lock();
            match rt.undo.pop() {
                Some(snap) => {
                    rt.redo.push(lib.snapshot());
                    lib.restore(snap);
                    true
                }
                None => false,
            }
        };
        if ok {
            self.mark_lib_dirty();
            self.rebuild_hotkeys();
        }
    }

    pub fn redo(&self) {
        let ok = {
            let mut lib = self.lib.write();
            let mut rt = self.rt.lock();
            match rt.redo.pop() {
                Some(snap) => {
                    rt.undo.push(lib.snapshot());
                    lib.restore(snap);
                    true
                }
                None => false,
            }
        };
        if ok {
            self.mark_lib_dirty();
            self.rebuild_hotkeys();
        }
    }

    pub fn import_paths(self: &Arc<Self>, paths: Vec<PathBuf>, category: Option<CategoryId>) {
        let core = self.clone();
        std::thread::spawn(move || {
            let mut files = Vec::new();
            let mut unsupported = 0usize;
            fn walk(p: &Path, files: &mut Vec<PathBuf>, unsupported: &mut usize) {
                if p.is_dir() {
                    if let Ok(rd) = std::fs::read_dir(p) {
                        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
                        entries.sort();
                        for e in entries {
                            walk(&e, files, unsupported);
                        }
                    }
                } else if crate::audio::decode::is_supported(p) {
                    files.push(p.to_path_buf());
                } else {
                    *unsupported += 1;
                }
            }
            for p in &paths {
                walk(p, &mut files, &mut unsupported);
            }
            if files.is_empty() {
                core.toast(
                    format!("{unsupported} {}", t!("Dateien nicht importiert (Format nicht unterstützt)", "files not imported (unsupported format)")),
                    ToastKind::Warn,
                );
                return;
            }
            if files.len() > 3 {
                core.toast(format!("{} {} …", t!("Importiere", "Importing"), files.len()), ToastKind::Info);
            }
            let mut sounds = Vec::new();
            let mut failed = 0usize;
            for f in files {
                match crate::audio::decode::probe_meta(&f) {
                    Ok(meta) => {
                        let title = meta
                            .title
                            .filter(|t| !t.trim().is_empty())
                            .unwrap_or_else(|| f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                        sounds.push(Sound {
                            hash: crate::library::file_hash(&f),
                            title,
                            artist: meta.artist.unwrap_or_default(),
                            duration_ms: meta.duration_ms,
                            path: f,
                            ..Default::default()
                        });
                    }
                    Err(e) => {
                        log::warn!("Import {f:?}: {e}");
                        failed += 1;
                    }
                }
            }
            let n = sounds.len();
            let ids = core.modify_library(|lib| {
                sounds.into_iter().map(|s| lib.add_sound(s, category, false)).collect::<Vec<_>>()
            });
            if let Some(last) = ids.last() {
                core.rt.lock().scroll_to = Some(*last);
            }
            core.analyze_missing();
            let mut msg = if n == 1 {
                format!("1 {}", t!("Sound hinzugefügt", "sound added"))
            } else {
                format!("{n} {}", t!("Sounds hinzugefügt", "sounds added"))
            };
            if failed + unsupported > 0 {
                msg += &format!(" · {} {}", failed + unsupported, t!("übersprungen", "skipped"));
            }
            core.toast(msg, ToastKind::Ok);
        });
    }

    pub fn import_spl(&self, path: &Path, replace: bool) {
        match crate::spl::import(path) {
            Ok((imported, report)) => {
                self.modify_library(|lib| {
                    if replace {
                        *lib = imported;
                    } else {
                        let mut map = HashMap::new();
                        for s in imported.sounds {
                            let old = s.id;
                            let id = lib.add_sound(s, None, false);
                            map.insert(old, id);
                        }
                        for mut c in imported.categories {
                            c.id = lib.alloc_id();
                            c.sounds = c.sounds.iter().filter_map(|i| map.get(i).copied()).collect();
                            lib.categories.push(c);
                        }
                    }
                });
                self.cache.clear();
                if let Some(c) = self.me.get().and_then(|w| w.upgrade()) {
                    c.analyze_missing();
                }
                self.toast(
                    format!(
                        "{}: {} Sounds, {} {}, {} {}{}",
                        t!("Soundpad-Liste importiert", "Soundpad list imported"),
                        report.sounds,
                        report.categories,
                        t!("Kategorien", "categories"),
                        report.stats_merged,
                        t!("Statistiken", "statistics"),
                        if report.missing_files > 0 {
                            format!(", {} {}", report.missing_files, t!("Dateien fehlen", "files missing"))
                        } else {
                            String::new()
                        }
                    ),
                    ToastKind::Ok,
                );
            }
            Err(e) => self.toast(format!("Import fehlgeschlagen: {e}"), ToastKind::Error),
        }
    }

    pub fn load_list(&self, path: &Path) {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("spl")) {
            self.import_spl(path, true);
        } else {
            let txt = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => return self.toast(format!("{e}"), ToastKind::Error),
            };
            match serde_json::from_str::<Library>(&txt) {
                Ok(new) => {
                    self.modify_library(|lib| *lib = new);
                    self.cache.clear();
                    self.toast(t!("Soundliste geladen", "Sound list loaded"), ToastKind::Ok);
                }
                Err(e) => self.toast(format!("Ungültige Soundliste: {e}"), ToastKind::Error),
            }
        }
        self.remember_recent(path);
    }

    pub fn save_list_as(&self, path: &Path) {
        let res = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("spl")) {
            let xml = crate::spl::to_xml(&self.lib.read(), false, None);
            crate::library::write_atomic(path, xml.as_bytes())
        } else {
            self.lib.read().save(path)
        };
        match res {
            Ok(()) => {
                self.toast(format!("{} {}", t!("Gespeichert:", "Saved:"), path.display()), ToastKind::Ok);
                self.remember_recent(path);
            }
            Err(e) => self.toast(format!("{e}"), ToastKind::Error),
        }
    }

    fn remember_recent(&self, path: &Path) {
        let mut s = self.settings.write();
        s.recent_lists.retain(|p| p != path);
        s.recent_lists.insert(0, path.to_path_buf());
        s.recent_lists.truncate(8);
        drop(s);
        self.mark_settings_dirty();
    }

    pub fn execute(&self, action: Action, src: Source) {
        match action {
            Action::PlaySound { id, mode } => self.play_sound(id, mode, src),
            Action::PlayIndex { index, mode } => {
                let id = self.lib.read().by_index(index).map(|s| s.id);
                match id {
                    Some(id) => self.play_sound(id, mode, src),
                    None => self.toast(format!("Index {index} {}", t!("existiert nicht", "does not exist")), ToastKind::Warn),
                }
            }
            Action::PlayCode(code) => self.play_code(&code, src),
            Action::PlayNumcode(n) => {
                let id = self.lib.read().sounds.iter().find(|s| s.numcode == Some(n)).map(|s| s.id);
                if let Some(id) = id {
                    self.play_sound(id, None, src);
                }
            }
            Action::PlayHotbarSlot { page, slot } => {
                let id = self.lib.read().hotbar.pages.get(page).and_then(|p| p.slots.get(slot)).and_then(|s| s.as_ref()).map(|s| s.sound);
                if let Some(id) = id {
                    self.play_sound(id, None, src);
                }
            }
            Action::RandomInCategory { cat, mode } => self.play_random(cat, mode, src),
            Action::PlaySelected { mode } => {
                let sel = self.rt.lock().selected.last().copied();
                if let Some(id) = sel {
                    self.play_sound(id, mode, src);
                }
            }
            Action::PlayTts { text, mode } => self.play_tts(text, mode),
            Action::Stop => self.stop(),
            Action::TogglePause => self.toggle_pause(),
            Action::SeekMs(ms) => self.seek_ms(ms),
            Action::SetVolume(v) => self.set_volume(v),
            Action::ToggleMute => self.toggle_mute(),
            Action::SetPlayMode(m) => self.set_play_mode(m, true),
            Action::Special(sa) => self.special(sa, src),
        }
    }

    fn special(&self, sa: SpecialAction, src: Source) {
        use SpecialAction::*;
        let param = self.settings.read().binding(sa).param;
        match sa {
            Stop => self.stop(),
            PlaySelected => self.execute(Action::PlaySelected { mode: None }, src),
            TogglePause => self.toggle_pause(),
            PlayPrevious => self.play_relative(-1, src),
            PlayNext => self.play_relative(1, src),
            SelectPrevious => self.select_relative(-1),
            SelectNext => self.select_relative(1),
            PlayCurrentAgain => {
                let cur = self.rt.lock().current;
                if let Some(id) = cur {
                    self.play_sound(id, None, src);
                }
            }
            PlayPreviouslyPlayed => {
                let prev = {
                    let rt = self.rt.lock();
                    let n = rt.history.len();
                    if n >= 2 {
                        Some(rt.history[n - 2])
                    } else {
                        rt.history.last().copied()
                    }
                };
                if let Some(id) = prev {
                    self.play_sound(id, None, src);
                }
            }
            RandomFromCategory => {
                let cat = self.rt.lock().selected_category;
                self.play_random(cat, None, src);
            }
            RandomAll => self.play_random(None, None, src),
            SelectPreviousCategory => self.select_category_relative(-1),
            SelectNextCategory => self.select_category_relative(1),
            RecordSpeakers => self.start_recording(RecSource::Speakers),
            RecordMicrophone => self.start_recording(RecSource::Microphone),
            RecordStop => {
                self.stop_recording();
            }
            JumpBack1 | JumpBack2 => self.jump_ms(-(param as i64) * 1000),
            JumpForward1 | JumpForward2 => self.jump_ms(param as i64 * 1000),
            VolumeDown1 | VolumeDown2 => {
                let v = self.settings.read().volume;
                self.set_volume(v - param as f32 / 100.0);
            }
            VolumeUp1 | VolumeUp2 => {
                let v = self.settings.read().volume;
                self.set_volume(v + param as f32 / 100.0);
            }
            VolumeSet1 | VolumeSet2 => self.set_volume(param as f32 / 100.0),
            ToggleMute => self.toggle_mute(),
            PlayModeBoth => self.set_play_mode(PlayMode::Both, true),
            PlayModeSpeakers => self.set_play_mode(PlayMode::Speakers, true),
            PlayModeMicrophone => self.set_play_mode(PlayMode::Microphone, true),
            PlayModeCycle => {
                let m = self.rt.lock().current_mode.next();
                self.set_play_mode(m, true);
            }
            AutoKeysEnable => self.set_autokeys(true),
            AutoKeysDisable => self.set_autokeys(false),
            AutoKeysToggle => {
                let on = self.settings.read().auto_keys.enabled;
                self.set_autokeys(!on);
            }
            HotkeysEnable => self.set_hotkeys_enabled(true),
            HotkeysDisable => self.set_hotkeys_enabled(false),
            HotkeysToggle => {
                let on = self.settings.read().hotkeys_enabled;
                self.set_hotkeys_enabled(!on);
            }
            TtsPlay => self.play_tts(None, None),
            TtsSpeakers => self.play_tts(None, Some(PlayMode::Speakers)),
            TtsMicrophone => self.play_tts(None, Some(PlayMode::Microphone)),
            HotbarPreviousPage => self.hotbar_page(-1),
            HotbarNextPage => self.hotbar_page(1),
            ShowWindow => crate::winutil::show_window(self.hwnd.load(Ordering::Relaxed)),
        }
    }

    fn play_code(&self, code: &str, src: Source) {
        let (cat_index, two) = {
            let s = self.settings.read();
            (s.category_index, s.category_index_two_digits)
        };
        let lib = self.lib.read();
        let id = if cat_index {
            let split = if two { 2 } else { 1 };
            if code.len() < split {
                return;
            }
            let (c, rest) = code.split_at(split);
            let Some(cat) = c.parse::<u8>().ok().and_then(|ci| lib.category_by_index(ci)) else {
                drop(lib);
                return self.toast(format!("{} {c}", t!("Keine Kategorie mit Index", "No category with index")), ToastKind::Warn);
            };
            let pos = rest.parse::<usize>().unwrap_or(1).max(1);
            cat.sounds.get(pos - 1).copied()
        } else {
            code.parse::<usize>().ok().and_then(|i| lib.by_index(i)).map(|s| s.id)
        };
        drop(lib);
        match id {
            Some(id) => self.play_sound(id, None, src),
            None => self.toast(format!("Index {code} {}", t!("existiert nicht", "does not exist")), ToastKind::Warn),
        }
    }

    fn sound_gain(s: &Settings, snd: &Sound, buf: &SoundBuffer) -> f32 {
        let norm = match s.normalization {
            NormMode::Off => 0.0,
            NormMode::Fixed => s.fixed_adjust_db,
            NormMode::Dynamic => {
                if buf.loudness_db > -90.0 {
                    (s.target_loudness_db - buf.loudness_db).clamp(-30.0, 18.0)
                } else {
                    0.0
                }
            }
        };
        let mut g = db_to_lin((snd.gain_db + norm).clamp(-60.0, 24.0));
        if buf.peak > 0.0 && g * buf.peak > 1.25 {
            g = 1.25 / buf.peak;
        }
        g
    }

    fn alloc_voice(&self) -> u64 {
        let mut rt = self.rt.lock();
        let id = rt.next_voice;
        rt.next_voice += 1;
        id
    }

    pub fn play_sound(&self, id: SoundId, mode: Option<PlayMode>, src: Source) {
        let s = self.settings.read().clone();
        let Some(snd) = self.lib.read().sound(id).cloned() else { return };
        if !snd.path.exists() {
            self.notify(Notify::Error);
            return self.toast(
                format!("{}: {}", t!("Datei nicht gefunden", "File not found"), snd.path.display()),
                ToastKind::Error,
            );
        }
        let buf = match self.cache.get(id, &snd.path) {
            Ok(b) => b,
            Err(e) => {
                self.notify(Notify::Error);
                return self.toast(format!("{} {}: {e}", t!("Abspielen fehlgeschlagen", "Playback failed"), crate::i18n::q(snd.display_name())), ToastKind::Error);
            }
        };
        if snd.loudness_db.is_none() || snd.wave.is_empty() {
            self.store_analysis(id, &buf);
        }
        let mode = mode.or(snd.play_mode).unwrap_or_else(|| self.rt.lock().current_mode);
        let gain = Self::sound_gain(&s, &snd, &buf);

        if !s.overlap_sounds {
            let old = self.rt.lock().current_voice.take();
            if old.is_some() {
                self.engine.send(EngineCmd::Stop { id: None, include_notify: false });
            }
        }

        let first = {
            let mut rt = self.rt.lock();
            let first = !rt.first_play_done;
            rt.first_play_done = true;
            first
        };
        if first && s.notify_first_play {
            self.notify(match mode {
                PlayMode::Both => Notify::ModeBoth,
                PlayMode::Speakers => Notify::ModeSpeakers,
                PlayMode::Microphone => Notify::ModeMic,
            });
        }

        let mut spk_delay = if mode.mic() { s.ptt_delay_ms } else { 0 };
        let mut mic_delay = s.ptt_delay_ms + s.mic_delay_ms;
        if mode.mic() && s.voice_activation_sound {
            let noise = self
                .noise
                .get_or_init(|| Arc::new(SoundBuffer::activation_noise(48_000, 2000, 0.35)))
                .clone();
            let len = s.voice_activation_ms.clamp(50, 2000) as u64;
            let vid = self.alloc_voice();
            self.engine.send(EngineCmd::Play(PlayRequest {
                id: vid,
                buf: noise,
                gain: 1.0,
                start_ms: 0,
                end_ms: len,
                speakers: false,
                mic: true,
                spk_delay_ms: 0,
                mic_delay_ms: mic_delay,
                looped: false,
                notify: false,
            }));
            mic_delay += len as u32;
            if mode.speakers() {
                spk_delay += len as u32;
            }
        }

        let vid = self.alloc_voice();
        let start_ms = snd.trim_start_ms.min(buf.duration_ms());
        self.engine.send(EngineCmd::Play(PlayRequest {
            id: vid,
            buf: buf.clone(),
            gain,
            start_ms,
            end_ms: snd.trim_end_ms,
            speakers: mode.speakers(),
            mic: mode.mic(),
            spk_delay_ms: spk_delay,
            mic_delay_ms: mic_delay,
            looped: false,
            notify: false,
        }));

        {
            let mut rt = self.rt.lock();
            rt.current = Some(id);
            rt.current_voice = Some(vid);
            rt.current_by_hotkey = src == Source::Hotkey;
            rt.current_rate = buf.rate;
            rt.current_start_ms = start_ms;
            rt.current_dur_ms = if snd.trim_end_ms > 0 { snd.trim_end_ms.min(buf.duration_ms()) } else { buf.duration_ms() };
            rt.paused = false;
            rt.active.insert(vid, VoiceInfo { mic: mode.mic() });
            if rt.history.last() != Some(&id) {
                rt.history.push(id);
                if rt.history.len() > 100 {
                    rt.history.remove(0);
                }
            }
            rt.status = format!("▶ {}", snd.display_name());
        }
        if s.duck_other_apps {
            self.set_ducked(true, s.duck_level);
        }
        if mode.mic() {
            self.autokeys_begin(&s, src == Source::Hotkey);
        }
        let today = crate::paths::today();
        self.touch_library(|lib| {
            if let Some(x) = lib.sound_mut(id) {
                x.play_count += 1;
                x.last_played = Some(today);
            }
        });
        if src == Source::Hotkey && !s.overlap_sounds {
            let mut rt = self.rt.lock();
            rt.selected = vec![id];
            rt.scroll_to = Some(id);
        }
        self.repaint();
    }

    pub fn stop(&self) {
        let had = {
            let mut rt = self.rt.lock();
            rt.paused = false;
            rt.current_voice.take().is_some() || !rt.active.is_empty()
        };
        self.engine.send(EngineCmd::Stop { id: None, include_notify: false });
        if had {
            self.rt.lock().status = t!("Gestoppt", "Stopped").into();
        }
        self.repaint();
    }

    pub fn toggle_pause(&self) {
        let (voice, paused) = {
            let rt = self.rt.lock();
            (rt.current_voice, rt.paused)
        };
        if voice.is_none() {
            return;
        }
        let now_paused = !paused;
        self.engine.send(EngineCmd::Pause(now_paused));
        self.rt.lock().paused = now_paused;
        let s = self.settings.read().clone();
        if now_paused {
            self.release_autokeys();
            if s.duck_other_apps {
                self.set_ducked(false, 1.0);
            }
        } else {
            let (mic, by_hotkey) = {
                let rt = self.rt.lock();
                (rt.active.values().any(|v| v.mic), rt.current_by_hotkey)
            };
            if mic {
                self.autokeys_begin(&s, by_hotkey);
            }
            if s.duck_other_apps {
                self.set_ducked(true, s.duck_level);
            }
        }
        self.repaint();
    }

    pub fn seek_ms(&self, ms: u64) {
        let voice = self.rt.lock().current_voice;
        if let Some(id) = voice {
            self.engine.send(EngineCmd::Seek { id, ms });
        }
    }

    pub fn jump_ms(&self, delta: i64) {
        let st = self.status();
        if st.playing {
            let target = (st.pos_ms as i64 + delta).clamp(0, st.dur_ms.saturating_sub(10) as i64);
            self.seek_ms(target as u64);
        }
    }

    pub fn set_volume(&self, v: f32) {
        let (spk, mic) = {
            let mut s = self.settings.write();
            s.volume = v.clamp(0.0, 1.0);
            masters(&s)
        };
        self.engine.send(EngineCmd::Masters { speakers: spk, mic });
        self.mark_settings_dirty();
        self.repaint();
    }

    pub fn toggle_mute(&self) {
        let (spk, mic) = {
            let mut s = self.settings.write();
            s.muted = !s.muted;
            masters(&s)
        };
        self.engine.send(EngineCmd::Masters { speakers: spk, mic });
        self.mark_settings_dirty();
        self.repaint();
    }

    pub fn set_play_mode(&self, mode: PlayMode, announce: bool) {
        self.rt.lock().current_mode = mode;
        self.settings.write().play_mode = mode;
        self.mark_settings_dirty();
        if announce {
            self.notify(match mode {
                PlayMode::Both => Notify::ModeBoth,
                PlayMode::Speakers => Notify::ModeSpeakers,
                PlayMode::Microphone => Notify::ModeMic,
            });
        }
        self.repaint();
    }

    pub fn set_hotkeys_enabled(&self, on: bool) {
        self.settings.write().hotkeys_enabled = on;
        self.hook.enabled.store(on, Ordering::Relaxed);
        self.notify(if on { Notify::Enabled } else { Notify::Disabled });
        self.toast(
            if on { t!("Hotkeys aktiviert", "Hotkeys enabled") } else { t!("Hotkeys deaktiviert", "Hotkeys disabled") },
            ToastKind::Info,
        );
        self.mark_settings_dirty();
    }

    pub fn set_autokeys(&self, on: bool) {
        self.settings.write().auto_keys.enabled = on;
        if !on {
            self.release_autokeys();
        }
        self.notify(if on { Notify::Enabled } else { Notify::Disabled });
        self.toast(
            if on { t!("Auto-Keys aktiviert", "Auto keys enabled") } else { t!("Auto-Keys deaktiviert", "Auto keys disabled") },
            ToastKind::Info,
        );
        self.mark_settings_dirty();
    }

    fn view_list(&self) -> Vec<SoundId> {
        let (order, cat) = {
            let rt = self.rt.lock();
            (rt.view_order.clone(), rt.selected_category)
        };
        if !order.is_empty() {
            return order;
        }
        self.lib.read().view(cat)
    }

    pub fn play_relative(&self, dir: i32, src: Source) {
        let list = self.view_list();
        if list.is_empty() {
            return;
        }
        let wrap = self.settings.read().repeat_list;
        let cur = {
            let rt = self.rt.lock();
            rt.current.or_else(|| rt.selected.last().copied())
        };
        let pos = cur.and_then(|c| list.iter().position(|&x| x == c));
        let next = match pos {
            None => Some(0),
            Some(p) => {
                let n = p as i64 + dir as i64;
                if n < 0 {
                    wrap.then(|| list.len() - 1)
                } else if n as usize >= list.len() {
                    wrap.then_some(0)
                } else {
                    Some(n as usize)
                }
            }
        };
        if let Some(i) = next {
            self.play_sound(list[i], None, src);
        }
    }

    pub fn play_random(&self, cat: Option<CategoryId>, mode: Option<PlayMode>, src: Source) {
        let list = self.lib.read().view(cat);
        if list.is_empty() {
            return;
        }
        let cur = self.rt.lock().current;
        let choices: Vec<SoundId> = if list.len() > 1 { list.into_iter().filter(|&x| Some(x) != cur).collect() } else { list };
        if let Some(&id) = choices.choose(&mut rand::thread_rng()) {
            self.play_sound(id, mode, src);
        }
    }

    pub fn select_relative(&self, dir: i32) {
        let list = self.view_list();
        if list.is_empty() {
            return;
        }
        let mut rt = self.rt.lock();
        let pos = rt.selected.last().and_then(|c| list.iter().position(|x| x == c));
        let i = match pos {
            None => 0,
            Some(p) => (p as i64 + dir as i64).clamp(0, list.len() as i64 - 1) as usize,
        };
        rt.selected = vec![list[i]];
        rt.scroll_to = Some(list[i]);
        drop(rt);
        self.repaint();
    }

    pub fn select_category_relative(&self, dir: i32) {
        let cats: Vec<Option<CategoryId>> = std::iter::once(None)
            .chain(self.lib.read().categories.iter().filter(|c| !c.hidden).map(|c| Some(c.id)))
            .collect();
        let mut rt = self.rt.lock();
        let pos = cats.iter().position(|c| *c == rt.selected_category).unwrap_or(0) as i64;
        let n = (pos + dir as i64).rem_euclid(cats.len() as i64) as usize;
        rt.selected_category = cats[n];
        rt.view_order.clear();
        drop(rt);
        self.repaint();
    }

    pub fn hotbar_page(&self, dir: i32) {
        let (page, announce) = {
            let s = self.settings.read();
            let mut lib = self.lib.write();
            let n = lib.hotbar.pages.len().max(1) as i64;
            let p = (lib.hotbar.selected as i64 + dir as i64).rem_euclid(n) as usize;
            lib.hotbar.selected = p;
            (p, s.hotbar_announce_page)
        };
        if announce {
            self.notify(Notify::Page((page + 1).min(9) as u8));
        }
        self.mark_lib_dirty();
        self.repaint();
    }

    pub fn status(&self) -> PlaybackStatus {
        let rt = self.rt.lock();
        let Some(voice) = rt.current_voice else {
            return PlaybackStatus { sound: rt.current, ..Default::default() };
        };
        let pos_ms = if self.engine.shared.pos_voice.load(Ordering::Relaxed) == voice {
            self.engine.shared.pos_frames.load(Ordering::Relaxed) * 1000 / rt.current_rate.max(1) as u64
        } else {
            rt.current_start_ms
        };
        let sound = rt.current;
        let paused = rt.paused;
        let dur = rt.current_dur_ms;
        drop(rt);
        let name = sound
            .and_then(|id| self.lib.read().sound(id).map(|s| s.display_name().to_string()))
            .unwrap_or_else(|| "TTS".into());
        PlaybackStatus { sound, name, pos_ms: pos_ms.min(dur), dur_ms: dur, playing: true, paused }
    }

    pub fn shared(&self) -> &EngineShared {
        &self.engine.shared
    }

    fn on_engine_event(&self, ev: EngineEvent) {
        match ev {
            EngineEvent::Error(e) => self.toast(e, ToastKind::Error),
            EngineEvent::DeviceSwitched(name) => {
                self.toast(format!("{} {name}", t!("Ausgabe folgt Windows:", "Output follows Windows:")), ToastKind::Info)
            }
            EngineEvent::Finished { id } => {
                let (was_current, any_mic_left, any_left, by_hotkey, cur, mode) = {
                    let mut rt = self.rt.lock();
                    rt.active.remove(&id);
                    rt.finished.push(id);
                    if rt.finished.len() > 32 {
                        rt.finished.remove(0);
                    }
                    let was_current = rt.current_voice == Some(id);
                    if was_current {
                        rt.current_voice = None;
                        rt.paused = false;
                        rt.status.clear();
                    }
                    (
                        was_current,
                        rt.active.values().any(|v| v.mic),
                        !rt.active.is_empty(),
                        rt.current_by_hotkey,
                        rt.current,
                        rt.current_mode,
                    )
                };
                if !any_mic_left {
                    self.release_autokeys();
                }
                if !any_left {
                    self.set_ducked(false, 1.0);
                }
                if was_current {
                    let s = self.settings.read().clone();
                    if s.auto_stop_by_hotkey && by_hotkey {
                    } else if s.repeat_file {
                        if let Some(c) = cur {
                            self.play_sound(c, Some(mode), Source::Internal);
                        }
                    } else if s.continue_playback || s.repeat_list {
                        self.play_relative(1, Source::Internal);
                    }
                }
                self.repaint();
            }
        }
    }

    fn autokeys_condition(ak: &crate::settings::AutoKeySettings) -> bool {
        match ak.condition {
            AutoKeyCondition::Always => true,
            AutoKeyCondition::ForegroundFullscreen => crate::winutil::foreground_is_fullscreen(),
            AutoKeyCondition::AppForeground => crate::winutil::foreground_exe()
                .is_some_and(|fg| ak.apps.iter().any(|a| a.eq_ignore_ascii_case(&fg))),
            AutoKeyCondition::AppRunning => {
                let running = crate::winutil::running_exes();
                ak.apps.iter().any(|a| running.contains(&a.to_lowercase()))
            }
        }
    }

    fn autokeys_begin(&self, s: &Settings, by_hotkey: bool) {
        let ak = &s.auto_keys;
        if !ak.enabled || ak.keys.is_empty() || (ak.only_by_hotkey && !by_hotkey) {
            return;
        }
        if !Self::autokeys_condition(ak) {
            return;
        }
        let mut rt = self.rt.lock();
        match ak.mode {
            AutoKeyMode::Hold => {
                if !rt.autokeys_held {
                    autokeys::press_all(&ak.keys, true);
                    rt.autokeys_held = true;
                }
            }
            AutoKeyMode::TapBeforeAfter => {
                if !rt.autokeys_tapped {
                    rt.autokeys_tapped = true;
                    drop(rt);
                    autokeys::tap_all(&ak.keys);
                }
            }
        }
    }

    fn release_autokeys(&self) {
        let keys = self.settings.read().auto_keys.keys.clone();
        let (held, tapped) = {
            let mut rt = self.rt.lock();
            let r = (rt.autokeys_held, rt.autokeys_tapped);
            rt.autokeys_held = false;
            rt.autokeys_tapped = false;
            r
        };
        if held {
            autokeys::press_all(&keys, false);
        }
        if tapped {
            autokeys::tap_all(&keys);
        }
    }

    fn set_ducked(&self, on: bool, level: f32) {
        let mut rt = self.rt.lock();
        if rt.ducked == on {
            return;
        }
        rt.ducked = on;
        let _ = self.duck_tx.send(if on { DuckCmd::Duck(level) } else { DuckCmd::Restore });
    }

    pub fn notify(&self, kind: Notify) {
        if !self.settings.read().notification_sounds {
            return;
        }
        let buf = {
            let mut cache = self.notify_cache.lock();
            cache
                .entry(kind)
                .or_insert_with(|| {
                    let r = 48_000;
                    let notes: Vec<(f32, u32)> = match kind {
                        Notify::Enabled => vec![(660.0, 70), (0.0, 20), (990.0, 110)],
                        Notify::Disabled => vec![(990.0, 70), (0.0, 20), (660.0, 110)],
                        Notify::ModeBoth => vec![(784.0, 60), (0.0, 25), (784.0, 60), (0.0, 25), (1046.0, 90)],
                        Notify::ModeSpeakers => vec![(523.0, 140)],
                        Notify::ModeMic => vec![(1046.0, 140)],
                        Notify::Page(n) => {
                            let mut v = Vec::new();
                            for _ in 0..n.max(1) {
                                v.push((880.0, 45));
                                v.push((0.0, 55));
                            }
                            v
                        }
                        Notify::RecordStart => vec![(880.0, 200)],
                        Notify::RecordStop => vec![(440.0, 90), (0.0, 30), (440.0, 90)],
                        Notify::Error => vec![(196.0, 220)],
                    };
                    Arc::new(SoundBuffer::tones(r, &notes, 0.25))
                })
                .clone()
        };
        let id = self.alloc_voice();
        self.engine.send(EngineCmd::Play(PlayRequest {
            id,
            buf,
            gain: 1.0,
            start_ms: 0,
            end_ms: 0,
            speakers: true,
            mic: false,
            spk_delay_ms: 0,
            mic_delay_ms: 0,
            looped: false,
            notify: true,
        }));
    }

    pub fn start_recording(&self, src: RecSource) {
        if self.rt.lock().recording.is_some() {
            self.stop_recording();
            return;
        }
        self.engine.send(EngineCmd::StartRecording(src));
        self.rt.lock().recording = Some((src, Instant::now()));
        self.notify(Notify::RecordStart);
        self.toast(
            match src {
                RecSource::Speakers => t!("Aufnahme der Lautsprecher läuft …", "Recording speakers …"),
                RecSource::Microphone => t!("Aufnahme des Mikrofons läuft …", "Recording microphone …"),
            },
            ToastKind::Info,
        );
    }

    pub fn stop_recording(&self) -> Option<SoundId> {
        self.rt.lock().recording.take()?;
        let buf = self.engine.stop_recording();
        self.notify(Notify::RecordStop);
        let Some(mut buf) = buf.filter(|b| b.frames() > 0) else {
            self.toast(t!("Aufnahme abgebrochen – es wurde nichts aufgezeichnet.", "Recording canceled – nothing was recorded."), ToastKind::Warn);
            return None;
        };
        let s = self.settings.read().clone();
        if s.trim_silence {
            crate::audio::buffer::trim_silence(&mut buf.samples, -50.0, buf.rate);
        }
        if s.normalize_recordings {
            crate::audio::buffer::normalize_to(&mut buf.samples, buf.rate, s.recording_target_db);
        }
        if buf.samples.is_empty() {
            self.toast(t!("Aufnahme enthielt nur Stille.", "Recording contained only silence."), ToastKind::Warn);
            return None;
        }
        buf.analyze();
        let name = format!("{} {}", t!("Aufnahme", "Recording"), chrono::Local::now().format("%Y-%m-%d %H-%M-%S"));
        let path = s.recordings_dir().join(format!("{name}.wav"));
        if let Err(e) = crate::audio::wav::save_buffer(&path, &buf) {
            self.toast(format!("{}: {e}", t!("Aufnahme nicht gespeichert", "Recording not saved")), ToastKind::Error);
            return None;
        }
        let id = self.add_generated_sound(path, name, buf, s.insert_recordings_at_top, t!("Aufnahmen", "Recordings"));
        self.toast(t!("Aufnahme gespeichert und zur Liste hinzugefügt", "Recording saved and added to list"), ToastKind::Ok);
        Some(id)
    }

    fn add_generated_sound(&self, path: PathBuf, title: String, buf: SoundBuffer, top: bool, cat_name: &str) -> SoundId {
        let dur = buf.duration_ms();
        let loud = buf.loudness_db;
        let id = self.modify_library(|lib| {
            let cat = match lib.categories.iter().find(|c| c.name == cat_name) {
                Some(c) => c.id,
                None => {
                    let id = lib.alloc_id();
                    lib.categories.push(Category { id, name: cat_name.into(), icon: "🎙".into(), ..Default::default() });
                    id
                }
            };
            lib.add_sound(
                Sound {
                    hash: crate::library::file_hash(&path),
                    title,
                    duration_ms: dur,
                    loudness_db: Some(loud),
                    path: path.clone(),
                    ..Default::default()
                },
                Some(cat),
                top,
            )
        });
        self.cache.insert(id, &path, Arc::new(buf));
        self.rt.lock().scroll_to = Some(id);
        id
    }

    pub fn recording_info(&self) -> Option<(RecSource, f32)> {
        let rt = self.rt.lock();
        rt.recording.map(|(src, t)| (src, t.elapsed().as_secs_f32()))
    }

    pub fn play_tts(&self, text: Option<String>, mode: Option<PlayMode>) {
        let text = text.unwrap_or_else(|| self.rt.lock().tts_text.clone());
        if text.trim().is_empty() {
            return;
        }
        let s = self.settings.read().clone();
        let mode = mode.unwrap_or_else(|| self.rt.lock().current_mode);
        let vid = self.alloc_voice();
        let Some(core_static) = self.me.get().and_then(|w| w.upgrade()) else { return };
        std::thread::spawn(move || match crate::audio::tts::synthesize(&text, s.tts_voice.as_deref(), s.tts_rate, s.tts_pitch) {
            Ok(buf) => {
                let buf = Arc::new(buf);
                let gain = if s.normalization == NormMode::Dynamic && buf.loudness_db > -90.0 {
                    db_to_lin((s.target_loudness_db - buf.loudness_db).clamp(-20.0, 18.0))
                } else {
                    1.0
                };
                if !s.overlap_sounds {
                    core_static.engine.send(EngineCmd::Stop { id: None, include_notify: false });
                }
                core_static.engine.send(EngineCmd::Play(PlayRequest {
                    id: vid,
                    buf: buf.clone(),
                    gain,
                    start_ms: 0,
                    end_ms: 0,
                    speakers: mode.speakers(),
                    mic: mode.mic(),
                    spk_delay_ms: if mode.mic() { s.ptt_delay_ms } else { 0 },
                    mic_delay_ms: s.ptt_delay_ms + s.mic_delay_ms,
                    looped: false,
                    notify: false,
                }));
                {
                    let mut rt = core_static.rt.lock();
                    rt.current = None;
                    rt.current_voice = Some(vid);
                    rt.current_rate = buf.rate;
                    rt.current_dur_ms = buf.duration_ms();
                    rt.current_start_ms = 0;
                    rt.current_by_hotkey = false;
                    rt.active.insert(vid, VoiceInfo { mic: mode.mic() });
                }
                if mode.mic() {
                    core_static.autokeys_begin(&s, false);
                }
                core_static.repaint();
            }
            Err(e) => core_static.toast(format!("{}: {e}", t!("Es konnte kein Sound vom Text erzeugt werden", "Failed to create sound from text")), ToastKind::Error),
        });
    }

    pub fn tts_to_library(self: &Arc<Self>, text: String) {
        let core = self.clone();
        std::thread::spawn(move || {
            let s = core.settings.read().clone();
            match crate::audio::tts::synthesize(&text, s.tts_voice.as_deref(), s.tts_rate, s.tts_pitch) {
                Ok(buf) => {
                    let short: String = text.chars().take(40).filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
                    let path = s.recordings_dir().join(format!("TTS - {} - {}.wav", short.trim(), chrono::Local::now().format("%H%M%S")));
                    if let Err(e) = crate::audio::wav::save_buffer(&path, &buf) {
                        return core.toast(format!("{e}"), ToastKind::Error);
                    }
                    core.add_generated_sound(path, text.chars().take(60).collect(), buf, false, "TTS");
                    core.toast(t!("TTS als Sound gespeichert", "TTS saved as sound"), ToastKind::Ok);
                }
                Err(e) => core.toast(format!("TTS: {e}"), ToastKind::Error),
            }
        });
    }

    pub fn preview(&self, buf: Arc<SoundBuffer>, start_ms: u64, end_ms: u64) -> u64 {
        let vid = self.alloc_voice();
        self.engine.send(EngineCmd::Play(PlayRequest {
            id: vid,
            buf,
            gain: 1.0,
            start_ms,
            end_ms,
            speakers: true,
            mic: false,
            spk_delay_ms: 0,
            mic_delay_ms: 0,
            looped: false,
            notify: false,
        }));
        vid
    }

    pub fn test_tone(&self, mode: PlayMode) {
        let buf = Arc::new(SoundBuffer::tones(48_000, &[(523.0, 180), (659.0, 180), (784.0, 180), (1046.0, 360)], 0.3));
        let vid = self.alloc_voice();
        self.engine.send(EngineCmd::Play(PlayRequest {
            id: vid,
            buf,
            gain: 1.0,
            start_ms: 0,
            end_ms: 0,
            speakers: mode.speakers(),
            mic: mode.mic(),
            spk_delay_ms: 0,
            mic_delay_ms: 0,
            looped: false,
            notify: false,
        }));
    }

    pub fn stop_voice(&self, id: u64) {
        self.engine.send(EngineCmd::Stop { id: Some(id), include_notify: false });
    }

    pub fn voice_pos_frames(&self, id: u64) -> Option<u64> {
        (self.engine.shared.pos_voice.load(Ordering::Relaxed) == id).then(|| self.engine.shared.pos_frames.load(Ordering::Relaxed))
    }

    pub fn measure_voice(self: &Arc<Self>, secs: u32, done: impl FnOnce(Option<f32>) + Send + 'static) {
        let core = self.clone();
        std::thread::spawn(move || {
            core.engine.send(EngineCmd::StartRecording(RecSource::Microphone));
            std::thread::sleep(Duration::from_secs(secs as u64));
            let buf = core.engine.stop_recording();
            done(buf.map(|b| b.loudness_db).filter(|l| *l > -90.0));
            core.repaint();
        });
    }
}
