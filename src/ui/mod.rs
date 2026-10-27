mod editor;
mod hotbar;
pub mod kit;
mod settings_win;
mod sidebar;
mod table;
pub mod theme;
mod toolbar;
mod tools;

use crate::actions::{Action, Source};
use crate::audio::engine::DeviceLists;
use crate::core::{Core, ToastKind};
use crate::hotkey::Hotkey;
use crate::library::{CategoryId, Rgb, SoundId};
use crate::settings::{Settings, SpecialAction, Theme};
use crate::t;
use crossbeam_channel::Receiver;
use eframe::egui::{self, Color32, TextureHandle};
use parking_lot::Mutex;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use theme::pal;

const LOGO_PNG: &[u8] = include_bytes!("../../assets/logo-128.png");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaptureTarget {
    Sound(SoundId),
    CategoryRandom(CategoryId),
    HotbarSlot(usize, usize),
    Special(SpecialAction),
    AutoKey,
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortCol {
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SettingsTab {
    #[default]
    Audio,
    Microphone,
    Hotkeys,
    AutoKeys,
    Interface,
    Recording,
    Remote,
    System,
}

pub struct CaptureState {
    pub target: CaptureTarget,
    pub rx: Receiver<Hotkey>,
    pub result: Option<Hotkey>,
}

pub enum Dialog {
    RenameTag { id: SoundId, text: String },
    RenameFile { id: SoundId, text: String },
    Category { id: Option<CategoryId>, name: String, icon: String, index: String },
    Properties(SoundId),
    DeadEntries(Vec<SoundId>),
    HotbarPage { page: usize, title: String },
    ConfirmRemove { ids: Vec<SoundId>, from_category: Option<CategoryId> },
    ConfirmReboot,
}

#[derive(Default)]
pub struct MeasureState {
    pub running: bool,
    pub result: Option<Option<f32>>,
}

pub struct UiState {
    pub settings_draft: Option<Settings>,
    pub settings_tab: SettingsTab,
    pub devices: Arc<Mutex<Option<DeviceLists>>>,
    pub tts_voices: Arc<Mutex<Option<Vec<String>>>>,
    pub running_apps: Vec<String>,
    pub editor: Option<editor::EditorState>,
    pub recorder_open: bool,
    pub tts_open: bool,
    pub color_tool: bool,
    pub color_pick: Option<Option<Rgb>>,
    pub stats_open: bool,
    pub about_open: bool,
    pub update_open: bool,
    pub overview_open: bool,
    pub setup_open: bool,
    pub capture: Option<CaptureState>,
    pub dialog: Option<Dialog>,
    pub sort: Option<(SortCol, bool)>,
    pub anchor: Option<SoundId>,
    pub request_search_focus: bool,
    pub measure: Arc<Mutex<MeasureState>>,
    pub meters: [f32; 3],
    pub peak_hold: f32,
    pub peak_hold_at: Instant,
    pub quit: bool,
    pub hint: String,
    pub hint_shown: String,
    pub vmic: Arc<crate::vmic::Installer>,
}

pub struct FerrovoxApp {
    pub core: Arc<Core>,
    pub logo: TextureHandle,
    pub st: UiState,
    tray: Option<tray_icon::TrayIcon>,
    applied_theme: Option<(Theme, f32)>,
    applied_on_top: Option<bool>,
    start_minimized: bool,
    frames: u64,
    last_frame: Instant,
}

fn load_rgba(png: &[u8]) -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory(png).map(|i| i.to_rgba8()).unwrap_or_else(|_| image::RgbaImage::new(1, 1));
    let (w, h) = img.dimensions();
    (img.into_raw(), w, h)
}

pub fn run(core: Arc<Core>, start_minimized: bool) -> anyhow::Result<()> {
    let (rgba, w, h) = load_rgba(LOGO_PNG);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ferrovox – by T-GOW")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(egui::IconData { rgba, width: w, height: h })
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Ferrovox",
        options,
        Box::new(move |cc| Ok(Box::new(FerrovoxApp::new(cc, core, start_minimized)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

impl FerrovoxApp {
    fn new(cc: &eframe::CreationContext<'_>, core: Arc<Core>, start_minimized: bool) -> Self {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = cc.window_handle() {
            if let RawWindowHandle::Win32(win) = handle.as_raw() {
                core.hwnd.store(win.hwnd.get(), Ordering::Relaxed);
                let c = core.clone();
                crate::winutil::on_close_while_inactive(win.hwnd.get(), move || {
                    let to_tray = {
                        let s = c.settings.read();
                        s.close_to_tray && s.show_tray_icon
                    };
                    if to_tray {
                        crate::winutil::hide_window(c.hwnd.load(Ordering::Relaxed));
                    } else {
                        crate::exit_app(&c);
                    }
                });
            }
        }
        let mut app = Self::with_context(&cc.egui_ctx, core, start_minimized);
        app.setup_tray();
        app
    }

    pub fn with_context(egui_ctx: &egui::Context, core: Arc<Core>, start_minimized: bool) -> Self {
        theme::install_fonts(egui_ctx);
        let _ = core.ctx.set(egui_ctx.clone());

        let (rgba, w, h) = load_rgba(LOGO_PNG);
        let logo = egui_ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba),
            egui::TextureOptions::LINEAR,
        );

        let first_start = core.settings.read().first_start;
        let app = FerrovoxApp {
            st: UiState {
                settings_draft: None,
                settings_tab: SettingsTab::Audio,
                devices: Arc::new(Mutex::new(None)),
                tts_voices: Arc::new(Mutex::new(None)),
                running_apps: Vec::new(),
                editor: None,
                recorder_open: false,
                tts_open: false,
                color_tool: false,
                color_pick: None,
                stats_open: false,
                about_open: false,
                update_open: false,
                overview_open: false,
                setup_open: first_start,
                capture: None,
                dialog: None,
                sort: None,
                anchor: None,
                request_search_focus: false,
                measure: Arc::new(Mutex::new(MeasureState::default())),
                meters: [0.0; 3],
                peak_hold: 0.0,
                peak_hold_at: Instant::now(),
                quit: false,
                hint: String::new(),
                hint_shown: String::new(),
                vmic: crate::vmic::Installer::new(),
            },
            tray: None,
            applied_theme: None,
            applied_on_top: None,
            start_minimized,
            frames: 0,
            last_frame: Instant::now(),
            logo,
            core,
        };
        if first_start {
            app.core.settings.write().first_start = false;
            app.core.mark_settings_dirty();
        }
        app.refresh_devices();
        app
    }

    pub fn refresh_devices(&self) {
        let core = self.core.clone();
        let slot = self.st.devices.clone();
        std::thread::spawn(move || {
            let lists = core.engine.list_devices();
            *slot.lock() = Some(lists);
            core.repaint();
        });
    }

    pub fn load_tts_voices(&self) {
        if self.st.tts_voices.lock().is_some() {
            return;
        }
        let slot = self.st.tts_voices.clone();
        let core = self.core.clone();
        std::thread::spawn(move || {
            *slot.lock() = Some(crate::audio::tts::voices());
            core.repaint();
        });
    }

    fn setup_tray(&mut self) {
        if !self.core.settings.read().show_tray_icon {
            return;
        }
        use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
        use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
        let (rgba, w, h) = load_rgba(LOGO_PNG);
        let Ok(icon) = Icon::from_rgba(rgba, w, h) else { return };
        let menu = Menu::new();
        let show = MenuItem::new(t!("Ferrovox anzeigen", "Show Ferrovox"), true, None);
        let stop = MenuItem::new(t!("Wiedergabe stoppen", "Stop playback"), true, None);
        let hotkeys = MenuItem::new(t!("Hotkeys an/aus", "Toggle hotkeys"), true, None);
        let quit = MenuItem::new(t!("Beenden", "Exit"), true, None);
        let _ = menu.append_items(&[&show, &stop, &hotkeys, &PredefinedMenuItem::separator(), &quit]);
        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("Ferrovox – by T-GOW")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build();
        match tray {
            Ok(t) => self.tray = Some(t),
            Err(e) => {
                log::warn!("Tray-Symbol: {e}");
                return;
            }
        }
        let core = self.core.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| match e {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
            | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                crate::winutil::show_window(core.hwnd.load(Ordering::Relaxed));
                core.repaint();
            }
            _ => {}
        }));
        let core = self.core.clone();
        let (show_id, stop_id, hk_id, quit_id) =
            (show.id().clone(), stop.id().clone(), hotkeys.id().clone(), quit.id().clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == show_id {
                crate::winutil::show_window(core.hwnd.load(Ordering::Relaxed));
                core.repaint();
            } else if e.id == stop_id {
                core.dispatch(Action::Stop, Source::Ui);
            } else if e.id == hk_id {
                core.dispatch(Action::Special(SpecialAction::HotkeysToggle), Source::Ui);
            } else if e.id == quit_id {
                crate::exit_app(&core);
            }
        }));
    }

    pub fn begin_capture(&mut self, target: CaptureTarget) {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.core.hook.begin_capture(tx);
        self.st.capture = Some(CaptureState { target, rx, result: None });
    }

    pub fn play(&self, id: SoundId, mode: Option<crate::settings::PlayMode>) {
        self.core.dispatch(Action::PlaySound { id, mode }, Source::Ui);
    }

    pub fn add_files_dialog(&self) {
        let cat = self.core.rt.lock().selected_category;
        let exts: Vec<&str> = crate::audio::decode::SUPPORTED_EXT.to_vec();
        if let Some(files) = rfd::FileDialog::new()
            .set_title(t!("Sounds hinzufügen", "Add sound files"))
            .add_filter(t!("Alle unterstützten Formate", "All supported formats"), &exts)
            .add_filter(t!("Alle Dateien", "All files"), &["*"])
            .pick_files()
        {
            self.core.import_paths(files, cat);
        }
    }

    pub fn add_folder_dialog(&self) {
        let cat = self.core.rt.lock().selected_category;
        if let Some(dir) = rfd::FileDialog::new().set_title(t!("Ordner hinzufügen", "Add folder")).pick_folder() {
            self.core.import_paths(vec![dir], cat);
        }
    }

    pub fn open_settings(&mut self, tab: SettingsTab) {
        self.st.settings_draft = Some(self.core.settings.read().clone());
        self.st.settings_tab = tab;
        self.refresh_devices();
    }

    pub fn open_editor_for_selection(&mut self) {
        let sel = self.core.rt.lock().selected.last().copied();
        if let Some(id) = sel {
            editor::open(self, id);
        }
    }

    fn handle_panel_request(&mut self) {
        let requests = std::mem::take(&mut self.core.rt.lock().open_panel);
        for req in requests {
            self.handle_one_panel(&req);
        }
    }

    fn handle_one_panel(&mut self, req: &str) {
        let (name, sub) = req.split_once(':').unwrap_or((req, ""));
        match name {
            "settings" => {
                let tab = match sub {
                    "mic" | "microphone" => SettingsTab::Microphone,
                    "hotkeys" => SettingsTab::Hotkeys,
                    "autokeys" => SettingsTab::AutoKeys,
                    "interface" | "ui" => SettingsTab::Interface,
                    "recording" | "tts" => SettingsTab::Recording,
                    "remote" => SettingsTab::Remote,
                    "system" => SettingsTab::System,
                    _ => SettingsTab::Audio,
                };
                self.open_settings(tab);
            }
            "recorder" => self.st.recorder_open = true,
            "tts" => {
                self.st.tts_open = true;
                self.load_tts_voices();
            }
            "stats" => self.st.stats_open = true,
            "hotkeys" => self.st.overview_open = true,
            "about" => self.st.about_open = true,
            "setup" => self.st.setup_open = true,
            "color" => self.st.color_tool = true,
            "editor" => self.open_editor_for_selection(),
            "close" => {
                self.st.settings_draft = None;
                self.st.editor = None;
                self.st.recorder_open = false;
                self.st.tts_open = false;
                self.st.stats_open = false;
                self.st.overview_open = false;
                self.st.about_open = false;
                self.st.setup_open = false;
                self.st.color_tool = false;
                self.st.color_pick = None;
            }
            _ => {}
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<std::path::PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            if dropped.len() == 1 && dropped[0].extension().is_some_and(|e| e.eq_ignore_ascii_case("spl")) {
                self.core.import_spl(&dropped[0], false);
            } else {
                let cat = self.core.rt.lock().selected_category;
                self.core.import_paths(dropped, cat);
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let focused = ctx.memory(|m| m.focused().is_some());
        let (undo, redo, find, save, enter, del, space, sel_all, f2, esc, typed) = ctx.input_mut(|i| {
            let typed: String = if !focused && !i.modifiers.ctrl && !i.modifiers.alt {
                i.events
                    .iter()
                    .filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None })
                    .collect()
            } else {
                String::new()
            };
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::F),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                !focused && i.key_pressed(egui::Key::Enter),
                !focused && i.key_pressed(egui::Key::Delete),
                !focused && i.key_pressed(egui::Key::Space),
                !focused && i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
                !focused && i.key_pressed(egui::Key::F2),
                i.key_pressed(egui::Key::Escape),
                typed,
            )
        });
        if self.st.capture.is_some() || self.st.dialog.is_some() {
            return;
        }
        if undo {
            self.core.undo();
        }
        if redo {
            self.core.redo();
        }
        if find {
            self.st.request_search_focus = true;
        }
        if save {
            self.core.save_library_now();
            self.core.toast(t!("Soundliste gespeichert", "Sound list saved"), ToastKind::Ok);
        }
        if enter {
            self.core.dispatch(Action::PlaySelected { mode: None }, Source::Ui);
        }
        if space {
            self.core.dispatch(Action::TogglePause, Source::Ui);
        }
        if del {
            let (ids, cat) = {
                let rt = self.core.rt.lock();
                (rt.selected.clone(), rt.selected_category)
            };
            if !ids.is_empty() {
                self.st.dialog = Some(Dialog::ConfirmRemove { ids, from_category: cat });
            }
        }
        if sel_all {
            let order = self.core.rt.lock().view_order.clone();
            self.core.rt.lock().selected = order;
        }
        if f2 {
            let sel = self.core.rt.lock().selected.last().copied();
            if let Some(id) = sel {
                let text = self.core.lib.read().sound(id).map(|s| s.display_name().to_string()).unwrap_or_default();
                self.st.dialog = Some(Dialog::RenameTag { id, text });
            }
        }
        if esc {
            if self.st.color_pick.is_some() {
                self.st.color_pick = None;
            } else {
                self.core.rt.lock().search.clear();
            }
        }
        if !typed.is_empty() && self.core.settings.read().typing_begins_search {
            self.core.rt.lock().search.push_str(&typed);
            self.st.request_search_focus = true;
        }
    }

    fn handle_window(&mut self, ctx: &egui::Context) {
        let (minimize_to_tray, close_to_tray, on_top, theme, scale) = {
            let s = self.core.settings.read();
            (s.minimize_to_tray && s.show_tray_icon, s.close_to_tray && s.show_tray_icon, s.always_on_top, s.theme, s.ui_scale)
        };
        if self.applied_theme != Some((theme, scale)) {
            theme::apply(ctx, theme, scale);
            self.applied_theme = Some((theme, scale));
            let c = pal().panel;
            crate::winutil::style_titlebar(self.core.hwnd.load(Ordering::Relaxed), theme != Theme::Light, [c.r(), c.g(), c.b()]);
        }
        if self.applied_on_top != Some(on_top) {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            self.applied_on_top = Some(on_top);
        }
        let hwnd = self.core.hwnd.load(Ordering::Relaxed);
        if self.frames == 2 && self.start_minimized {
            if self.tray.is_some() {
                crate::winutil::hide_window(hwnd);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }
        if minimize_to_tray && ctx.input(|i| i.viewport().minimized == Some(true)) {
            crate::winutil::hide_window(hwnd);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.st.quit && close_to_tray {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            crate::winutil::hide_window(hwnd);
        }
        if self.st.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn update_title(&self, ctx: &egui::Context) {
        if self.frames % 15 != 0 {
            return;
        }
        let st = self.core.status();
        let title = if st.playing { format!("{} – Ferrovox", st.name) } else { "Ferrovox – by T-GOW".to_string() };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        let toasts: Vec<crate::core::Toast> = {
            let mut rt = self.core.rt.lock();
            rt.toasts.retain(|t| t.at.elapsed().as_secs_f32() < 5.0);
            rt.toasts.clone()
        };
        if toasts.is_empty() {
            return;
        }
        let p = pal();
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -34.0])
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for t in toasts.iter().rev() {
                    let age = t.at.elapsed().as_secs_f32();
                    let alpha = if age > 4.3 { ((5.0 - age) / 0.7).clamp(0.0, 1.0) } else { 1.0 };
                    let (col, ic) = match t.kind {
                        ToastKind::Ok => (theme::OK, kit::ic::CHECK),
                        ToastKind::Warn => (theme::WARN, kit::ic::WARNING),
                        ToastKind::Error => (theme::ERR, kit::ic::WARNING),
                        ToastKind::Info => (theme::accent(), kit::ic::INFO),
                    };
                    let resp = egui::Frame::none()
                        .fill(p.raised.gamma_multiply(alpha))
                        .stroke(egui::Stroke::new(1.0_f32, p.border.gamma_multiply(alpha)))
                        .rounding(6.0)
                        .inner_margin(egui::Margin { left: 14.0, right: 14.0, top: 9.0, bottom: 9.0 })
                        .show(ui, |ui| {
                            ui.set_max_width(400.0);
                            ui.horizontal(|ui| {
                                ui.label(kit::icon(ic, 13.0).color(col.gamma_multiply(alpha)));
                                ui.label(egui::RichText::new(&t.text).color(p.text.gamma_multiply(alpha)));
                            });
                        });
                    let r = resp.response.rect;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(r.min, egui::vec2(3.0, r.height())),
                        egui::Rounding { nw: 6.0, sw: 6.0, ne: 0.0, se: 0.0 },
                        col.gamma_multiply(alpha),
                    );
                    ui.add_space(6.0);
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn update_meters(&mut self) {
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.2);
        self.last_frame = Instant::now();
        let sh = self.core.shared();
        let take = crate::audio::engine::EngineShared::take_peak;
        let raw = [take(&sh.spk_peak), take(&sh.mic_peak), take(&sh.mic_in_peak)];
        let peak = raw[0].max(raw[1]);
        if peak >= self.st.peak_hold || self.st.peak_hold_at.elapsed().as_secs_f32() > 2.0 {
            self.st.peak_hold = peak;
            self.st.peak_hold_at = Instant::now();
        }
        for (m, r) in self.st.meters.iter_mut().zip(raw) {
            let v = theme::level_to_meter(r);
            *m = if v > *m { v } else { (*m - dt * 1.2).max(v) };
        }
    }

}

impl eframe::App for FerrovoxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.core.hook.cancel_capture();
    }
}

impl FerrovoxApp {
    pub fn frame(&mut self, ctx: &egui::Context) {
        self.frames += 1;
        self.handle_panel_request();
        self.handle_window(ctx);
        self.handle_dropped_files(ctx);
        self.handle_shortcuts(ctx);
        self.update_meters();
        self.update_title(ctx);
        self.st.hint_shown = std::mem::take(&mut self.st.hint);

        toolbar::menubar(self, ctx);
        toolbar::toolbar(self, ctx);
        toolbar::now_playing(self, ctx);
        toolbar::status_bar(self, ctx);

        let gap = egui::Frame::none().fill(pal().bg).inner_margin(egui::Margin::same(3.0));
        let (show_cats, show_hotbar, rows) = {
            let s = self.core.settings.read();
            (s.show_categories, s.show_hotbar, s.hotbar_rows.clamp(1, 8))
        };
        if show_hotbar {
            egui::TopBottomPanel::bottom("hotbar")
                .frame(gap)
                .resizable(true)
                .default_height(rows as f32 * 66.0 + 80.0)
                .height_range(130.0..=700.0)
                .show(ctx, |ui| {
                    let mut t = 0usize;
                    kit::card(ui, "hotbar", &["Hotbar"], &mut t, |_| {}, |ui, _| hotbar::grid_panel(self, ui));
                });
        }
        if show_cats {
            egui::SidePanel::left("categories")
                .frame(gap)
                .resizable(true)
                .default_width(240.0)
                .width_range(170.0..=400.0)
                .show(ctx, |ui| sidebar::panel(self, ui));
        }
        egui::CentralPanel::default().frame(gap).show(ctx, |ui| table::panel(self, ui));
        table::process_menu_requests(self, ctx);

        settings_win::window(self, ctx);
        editor::window(self, ctx);
        tools::windows(self, ctx);
        self.toasts(ctx);

        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            let rect = ctx.screen_rect();
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(160));
            painter.rect_stroke(rect.shrink(12.0), 10.0, egui::Stroke::new(2.0_f32, theme::accent()));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                t!("Dateien hier ablegen zum Hinzufügen", "Drop files here to add them"),
                egui::FontId::new(24.0, egui::FontFamily::Name(theme::BOLD.into())),
                Color32::WHITE,
            );
        }

        let busy = self.st.vmic.busy();
        let playing = self.core.rt.lock().current_voice.is_some();
        if playing || busy || self.st.capture.is_some() || self.core.recording_info().is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod smoke {
    use super::*;
    use crate::library::{HotbarSlot, Library, Sound};
    use crate::settings::PlayMode;

    fn write_sine(path: &std::path::Path, freq: f32) {
        let samples: Vec<f32> = (0..24_000)
            .flat_map(|i| {
                let v = (i as f32 / 48_000.0 * freq * std::f32::consts::TAU).sin() * 0.3;
                [v, v]
            })
            .collect();
        crate::audio::wav::write_wav(path, &samples, 48_000).unwrap();
    }

    fn collect_text(shape: &egui::Shape, out: &mut std::collections::BTreeSet<String>) {
        match shape {
            egui::Shape::Text(t) => {
                out.insert(t.galley.job.text.clone());
            }
            egui::Shape::Vec(v) => v.iter().for_each(|s| collect_text(s, out)),
            _ => {}
        }
    }

    fn looks_german(text: &str) -> bool {
        const WORDS: &[&str] = &[
            "und", "oder", "nicht", "kein", "keine", "mit", "ohne", "für", "von", "ist", "sind", "wird", "der", "die",
            "das", "den", "dem", "des", "ein", "eine", "einen", "zum", "zur", "beim", "Datei", "Dateien", "Fehler",
            "Einstellungen", "Gerät", "Aufnahme", "Bitte", "Löschen", "Wiedergabe", "Lautstärke", "Kategorie",
            "Kategorien", "Liste", "Ausgabe", "Eingabe", "Mikrofon", "Fenster", "Speichern", "Bereit", "Jetzt",
            "Sekunden", "Stimme", "Taste", "Tasten", "Suche", "Abbrechen", "Schließen", "Hinzufügen", "Neu", "Alle",
            "Seite", "Maus", "Lautsprecher", "Ansicht", "Bearbeiten", "Werkzeuge", "Hilfe", "Datei",
        ];
        text.chars().any(|c| "äöüÄÖÜß„“".contains(c))
            || text.split(|c: char| !c.is_alphanumeric()).any(|w| WORDS.contains(&w))
    }

    #[test]
    fn all_windows_render_without_panic() {
        let dir = std::env::temp_dir().join(format!("ferrovox-ui-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("FERROVOX_DATA_DIR", &dir);
        let (w1, w2) = (dir.join("alpha.wav"), dir.join("beta.wav"));
        write_sine(&w1, 440.0);
        write_sine(&w2, 660.0);

        let _lang = crate::i18n::TEST_LOCK.lock();
        crate::i18n::set_language(crate::i18n::Language::En);
        let mut settings = Settings::default();
        settings.language = crate::i18n::Language::En;
        settings.show_tray_icon = false;
        settings.notification_sounds = false;
        settings.volume = 0.0;
        let mut lib = Library::default();
        let cat = lib.categories[0].id;
        let id1 = lib.add_sound(
            Sound {
                path: w1.clone(),
                title: "Alpha".into(),
                hotkey: Some(Hotkey::new(0x7B, 0, 0)),
                color: Some([200, 80, 40]),
                duration_ms: 500,
                wave: vec![120; 96],
                trim_start_ms: 50,
                ..Default::default()
            },
            Some(cat),
            false,
        );
        let id2 = lib.add_sound(Sound { path: w2, title: "Beta".into(), tag: Some("My tag".into()), ..Default::default() }, None, false);
        let _missing = lib.add_sound(Sound { path: dir.join("gone.mp3"), title: "Gone".into(), ..Default::default() }, None, false);
        lib.hotbar.page_mut(0, 12).slots[0] = Some(HotbarSlot { sound: id1, color: None, hotkey: None });

        let core = Core::new(settings, lib);
        let ctx = egui::Context::default();
        let mut app = FerrovoxApp::with_context(&ctx, core.clone(), false);
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))),
            ..Default::default()
        };
        let texts = std::cell::RefCell::new(std::collections::BTreeSet::new());
        let run = |app: &mut FerrovoxApp, n: usize| {
            for _ in 0..n {
                let out = ctx.run(input(), |c| app.frame(c));
                for cs in &out.shapes {
                    collect_text(&cs.shape, &mut texts.borrow_mut());
                }
            }
        };

        run(&mut app, 3);
        app.st.setup_open = false;
        core.rt.lock().selected = vec![id1];
        run(&mut app, 2);
        for (cats, hotbar) in [(false, true), (true, false), (false, false), (true, true)] {
            {
                let mut s = core.settings.write();
                s.show_categories = cats;
                s.show_hotbar = hotbar;
                s.columns.last_played = !cats;
                s.columns.path = !hotbar;
            }
            run(&mut app, 2);
        }

        for tab in [
            SettingsTab::Audio,
            SettingsTab::Microphone,
            SettingsTab::Hotkeys,
            SettingsTab::AutoKeys,
            SettingsTab::Interface,
            SettingsTab::Recording,
            SettingsTab::Remote,
            SettingsTab::System,
        ] {
            app.open_settings(tab);
            run(&mut app, 2);
        }
        app.st.settings_draft = None;

        editor::open(&mut app, id1);
        assert!(app.st.editor.is_some(), "Editor öffnet nicht");
        run(&mut app, 3);
        app.st.editor = None;

        app.st.recorder_open = true;
        app.st.tts_open = true;
        app.st.color_tool = true;
        app.st.color_pick = Some(Some([1, 2, 3]));
        app.st.stats_open = true;
        app.st.overview_open = true;
        app.st.about_open = true;
        app.st.setup_open = true;
        app.st.update_open = true;
        run(&mut app, 3);
        app.st.update_open = false;
        app.st.recorder_open = false;
        app.st.tts_open = false;
        app.st.color_tool = false;
        app.st.color_pick = None;
        app.st.stats_open = false;
        app.st.overview_open = false;
        app.st.about_open = false;
        app.st.setup_open = false;

        let dialogs = vec![
            Dialog::RenameTag { id: id1, text: "x".into() },
            Dialog::RenameFile { id: id1, text: "x".into() },
            Dialog::Category { id: None, name: "Fresh".into(), icon: "📁".into(), index: "5".into() },
            Dialog::Category { id: Some(cat), name: "Old".into(), icon: "⭐".into(), index: String::new() },
            Dialog::Properties(id2),
            Dialog::DeadEntries(vec![]),
            Dialog::DeadEntries(vec![id2]),
            Dialog::HotbarPage { page: 0, title: "Page".into() },
            Dialog::ConfirmRemove { ids: vec![id2], from_category: None },
            Dialog::ConfirmReboot,
        ];
        for d in dialogs {
            app.st.dialog = Some(d);
            run(&mut app, 2);
            app.st.dialog = None;
        }

        app.begin_capture(CaptureTarget::Sound(id1));
        run(&mut app, 2);
        app.core.hook.cancel_capture();
        app.st.capture = None;

        core.rt.lock().search = "beta".into();
        run(&mut app, 2);
        core.rt.lock().search.clear();
        core.rt.lock().selected_category = Some(cat);
        run(&mut app, 2);
        core.rt.lock().selected_category = None;
        app.st.sort = Some((SortCol::Name, false));
        run(&mut app, 2);

        core.play_sound(id1, Some(PlayMode::Speakers), crate::actions::Source::Ui);
        run(&mut app, 3);
        core.stop();

        for theme in [Theme::Light, Theme::Ember, Theme::Studio] {
            core.settings.write().theme = theme;
            run(&mut app, 2);
        }
        let devices = core.engine.list_devices();
        let system_names: Vec<String> = devices.outputs.iter().chain(devices.inputs.iter()).cloned().collect();
        let german: Vec<String> = texts
            .borrow()
            .iter()
            .filter(|t| looks_german(t))
            .filter(|t| !system_names.iter().any(|n| t.contains(n.as_str())))
            .cloned()
            .collect();
        let collected = texts.borrow().len();
        let saw_setup = texts.borrow().iter().any(|t| t.contains("Install virtual microphone"));
        crate::i18n::set_language(crate::i18n::Language::De);
        core.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(collected > 100 && saw_setup, "Textsammlung greift nicht ({collected} Texte)");
        assert!(looks_german("Datei öffnen") && !looks_german("Open file"));
        assert!(german.is_empty(), "Deutsche Texte in der englischen Oberfläche:\n{}", german.join("\n"));
    }

    #[test]
    fn icon_font_contains_all_icons() {
        use ab_glyph::Font;
        let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\SegoeIcons.ttf") else { return };
        let font = ab_glyph::FontRef::try_from_slice(&bytes).unwrap();
        for code in [
            kit::ic::PLAY, kit::ic::PAUSE, kit::ic::STOP, kit::ic::MIC, kit::ic::SPEAKERS, kit::ic::RECORD,
            kit::ic::HAMBURGER, kit::ic::STOPWATCH, kit::ic::OVERLAP, kit::ic::HOTKEY, kit::ic::PLUG, kit::ic::SHIELD,
            kit::ic::TRIM, kit::ic::MARKER, kit::ic::CIRCLE_FILL, kit::ic::DEVICES, kit::ic::FONT, kit::ic::OPEN_FILE,
        ] {
            let ch = code.chars().next().unwrap();
            assert_ne!(font.glyph_id(ch).0, 0, "Icon {:X} fehlt", ch as u32);
        }
    }
}
