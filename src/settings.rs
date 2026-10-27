use crate::hotkey::{Hotkey, MOD_ALT, MOD_CTRL};
use crate::i18n::Language;
use crate::t;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayMode {
    #[default]
    Both,
    Speakers,
    Microphone,
}

impl PlayMode {
    pub fn speakers(self) -> bool {
        matches!(self, PlayMode::Both | PlayMode::Speakers)
    }
    pub fn mic(self) -> bool {
        matches!(self, PlayMode::Both | PlayMode::Microphone)
    }
    pub fn next(self) -> Self {
        match self {
            PlayMode::Both => PlayMode::Speakers,
            PlayMode::Speakers => PlayMode::Microphone,
            PlayMode::Microphone => PlayMode::Both,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            PlayMode::Both => t!("Lautsprecher + Mikrofon", "Speakers + microphone"),
            PlayMode::Speakers => t!("Nur Lautsprecher", "Speakers only"),
            PlayMode::Microphone => t!("Nur Mikrofon", "Microphone only"),
        }
    }
    pub fn from_flags(speakers: bool, mic: bool) -> Self {
        match (speakers, mic) {
            (true, false) => PlayMode::Speakers,
            (false, true) => PlayMode::Microphone,
            _ => PlayMode::Both,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NormMode {
    Off,
    #[default]
    Dynamic,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    #[serde(alias = "Dark")]
    Studio,
    Ember,
    Light,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Theme::Studio => t!("Studio (Blau)", "Studio (blue)"),
            Theme::Ember => t!("Studio (Kupfer)", "Studio (copper)"),
            Theme::Light => t!("Hell", "Light"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateFormat {
    #[default]
    Iso,
    German,
    Us,
}

impl DateFormat {
    pub fn format(self, iso: &str) -> String {
        let parts: Vec<&str> = iso.split('-').collect();
        if parts.len() != 3 {
            return iso.to_string();
        }
        match self {
            DateFormat::Iso => iso.to_string(),
            DateFormat::German => format!("{}.{}.{}", parts[2], parts[1], parts[0]),
            DateFormat::Us => format!("{}/{}/{}", parts[1], parts[2], parts[0]),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoKeyMode {
    #[default]
    Hold,
    TapBeforeAfter,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoKeyCondition {
    #[default]
    Always,
    AppForeground,
    AppRunning,
    ForegroundFullscreen,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoKeySettings {
    pub enabled: bool,
    pub keys: Vec<u16>,
    pub mode: AutoKeyMode,
    pub only_by_hotkey: bool,
    pub allow_dangerous: bool,
    pub condition: AutoKeyCondition,
    pub apps: Vec<String>,
}

impl Default for AutoKeySettings {
    fn default() -> Self {
        AutoKeySettings {
            enabled: false,
            keys: Vec::new(),
            mode: AutoKeyMode::Hold,
            only_by_hotkey: false,
            allow_dangerous: false,
            condition: AutoKeyCondition::Always,
            apps: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SpecialAction {
    Stop,
    PlaySelected,
    TogglePause,
    PlayPrevious,
    PlayNext,
    SelectPrevious,
    SelectNext,
    PlayCurrentAgain,
    PlayPreviouslyPlayed,
    RandomFromCategory,
    RandomAll,
    SelectPreviousCategory,
    SelectNextCategory,
    RecordSpeakers,
    RecordMicrophone,
    RecordStop,
    JumpBack1,
    JumpBack2,
    JumpForward1,
    JumpForward2,
    VolumeDown1,
    VolumeDown2,
    VolumeUp1,
    VolumeUp2,
    VolumeSet1,
    VolumeSet2,
    ToggleMute,
    PlayModeBoth,
    PlayModeSpeakers,
    PlayModeMicrophone,
    PlayModeCycle,
    AutoKeysEnable,
    AutoKeysDisable,
    AutoKeysToggle,
    HotkeysEnable,
    HotkeysDisable,
    HotkeysToggle,
    TtsPlay,
    TtsSpeakers,
    TtsMicrophone,
    HotbarPreviousPage,
    HotbarNextPage,
    ShowWindow,
}

pub struct SpecialDef {
    pub action: SpecialAction,
    pub group: &'static str,
    pub de: &'static str,
    pub en: &'static str,
    pub param: Option<(i32, &'static str)>,
}

macro_rules! sd {
    ($a:ident, $g:expr, $de:expr, $en:expr) => {
        SpecialDef { action: SpecialAction::$a, group: $g, de: $de, en: $en, param: None }
    };
    ($a:ident, $g:expr, $de:expr, $en:expr, $p:expr, $u:expr) => {
        SpecialDef { action: SpecialAction::$a, group: $g, de: $de, en: $en, param: Some(($p, $u)) }
    };
}

pub const SPECIAL_DEFS: &[SpecialDef] = &[
    sd!(PlaySelected, "play", "Ausgewählte Datei abspielen", "Play selected file"),
    sd!(Stop, "play", "Wiedergabe stoppen", "Stop playback"),
    sd!(TogglePause, "play", "Pause / Fortsetzen", "Pause / resume"),
    sd!(PlayPrevious, "play", "Vorherige Datei abspielen", "Play previous file"),
    sd!(PlayNext, "play", "Nächste Datei abspielen", "Play next file"),
    sd!(PlayCurrentAgain, "play", "Aktuelle Datei erneut abspielen", "Play current file again"),
    sd!(PlayPreviouslyPlayed, "play", "Zuvor abgespielte Datei abspielen", "Play previously played file"),
    sd!(RandomFromCategory, "play", "Zufällige Datei aus der Kategorie", "Random file from category"),
    sd!(RandomAll, "play", "Zufällige Datei aus allen Kategorien", "Random file from all categories"),
    sd!(SelectPrevious, "select", "Vorherige Datei auswählen", "Select previous file"),
    sd!(SelectNext, "select", "Nächste Datei auswählen", "Select next file"),
    sd!(SelectPreviousCategory, "select", "Vorherige Kategorie auswählen", "Select previous category"),
    sd!(SelectNextCategory, "select", "Nächste Kategorie auswählen", "Select next category"),
    sd!(HotbarPreviousPage, "select", "Vorherige Hotbar-Seite", "Previous hotbar page"),
    sd!(HotbarNextPage, "select", "Nächste Hotbar-Seite", "Next hotbar page"),
    sd!(JumpBack1, "seek", "Springe zurück um", "Jump back by", 5, "s"),
    sd!(JumpBack2, "seek", "Springe zurück um", "Jump back by", 30, "s"),
    sd!(JumpForward1, "seek", "Springe vor um", "Jump forward by", 5, "s"),
    sd!(JumpForward2, "seek", "Springe vor um", "Jump forward by", 30, "s"),
    sd!(VolumeDown1, "volume", "Lautstärke senken um", "Volume down by", 5, "%"),
    sd!(VolumeDown2, "volume", "Lautstärke senken um", "Volume down by", 20, "%"),
    sd!(VolumeUp1, "volume", "Lautstärke erhöhen um", "Volume up by", 5, "%"),
    sd!(VolumeUp2, "volume", "Lautstärke erhöhen um", "Volume up by", 20, "%"),
    sd!(VolumeSet1, "volume", "Lautstärke setzen auf", "Set volume to", 0, "%"),
    sd!(VolumeSet2, "volume", "Lautstärke setzen auf", "Set volume to", 100, "%"),
    sd!(ToggleMute, "volume", "Stummschalten umschalten", "Toggle mute"),
    sd!(PlayModeBoth, "mode", "Wiedergabemodus: Lautsprecher + Mikrofon", "Play mode: speakers + mic"),
    sd!(PlayModeSpeakers, "mode", "Wiedergabemodus: Lautsprecher", "Play mode: speakers"),
    sd!(PlayModeMicrophone, "mode", "Wiedergabemodus: Mikrofon", "Play mode: microphone"),
    sd!(PlayModeCycle, "mode", "Zum nächsten Wiedergabemodus wechseln", "Switch to next play mode"),
    sd!(RecordSpeakers, "record", "Aufnahme der Lautsprecher starten", "Start recording speakers"),
    sd!(RecordMicrophone, "record", "Aufnahme des Mikrofons starten", "Start recording microphone"),
    sd!(RecordStop, "record", "Aufnahme beenden", "Stop recording"),
    sd!(HotkeysEnable, "toggle", "Hotkeys aktivieren", "Enable hotkeys"),
    sd!(HotkeysDisable, "toggle", "Hotkeys deaktivieren", "Disable hotkeys"),
    sd!(HotkeysToggle, "toggle", "Hotkeys umschalten", "Toggle hotkeys"),
    sd!(AutoKeysEnable, "toggle", "Auto-Keys aktivieren", "Enable auto keys"),
    sd!(AutoKeysDisable, "toggle", "Auto-Keys deaktivieren", "Disable auto keys"),
    sd!(AutoKeysToggle, "toggle", "Auto-Keys umschalten", "Toggle auto keys"),
    sd!(TtsPlay, "tts", "TTS abspielen", "Play TTS"),
    sd!(TtsSpeakers, "tts", "TTS auf den Lautsprechern", "Play TTS on speakers"),
    sd!(TtsMicrophone, "tts", "TTS auf dem Mikrofon", "Play TTS on microphone"),
    sd!(ShowWindow, "toggle", "Ferrovox-Fenster anzeigen", "Show Ferrovox window"),
];

pub fn special_def(action: SpecialAction) -> &'static SpecialDef {
    SPECIAL_DEFS.iter().find(|d| d.action == action).expect("SpecialDef fehlt")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SpecialBinding {
    pub hotkey: Option<Hotkey>,
    pub param: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Columns {
    pub index: bool,
    pub artist: bool,
    pub duration: bool,
    pub hotkey: bool,
    pub play_count: bool,
    pub last_played: bool,
    pub added_on: bool,
    pub path: bool,
}

impl Default for Columns {
    fn default() -> Self {
        Columns {
            index: true,
            artist: true,
            duration: true,
            hotkey: true,
            play_count: true,
            last_played: false,
            added_on: false,
            path: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(rename = "ui_language")]
    pub language: Language,
    pub theme: Theme,
    pub first_start: bool,
    pub win_default_mic_switched: bool,
    pub previous_default_mic: Option<String>,

    pub speaker_device: Option<String>,
    pub mic_out_device: Option<String>,
    pub mic_in_device: Option<String>,
    pub mic_passthrough: bool,
    pub mic_passthrough_gain: f32,
    pub block_voice_while_playing: bool,
    pub ptt_delay_ms: u32,
    pub mic_delay_ms: u32,
    pub play_mode: PlayMode,
    pub volume: f32,
    pub muted: bool,
    pub speakers_gain: f32,
    pub mic_gain: f32,
    pub linear_volume_slider: bool,
    pub normalization: NormMode,
    pub target_loudness_db: f32,
    pub fixed_adjust_db: f32,
    pub voice_activation_sound: bool,
    pub voice_activation_ms: u32,
    pub duck_other_apps: bool,
    pub duck_level: f32,
    pub overlap_sounds: bool,
    pub auto_stop_by_hotkey: bool,
    pub continue_playback: bool,
    pub repeat_file: bool,
    pub repeat_list: bool,
    pub resume_in_another_mode: bool,
    pub preload_hotkey_sounds: bool,
    pub cache_mb: u32,

    pub hotkeys_enabled: bool,
    pub remember_hotkeys_enabled: bool,
    pub pass_through_hotkeys: bool,
    pub distinguish_sides: bool,
    pub index_hotkeys: bool,
    pub index_modifier: u8,
    pub numcode_hotkeys: bool,
    pub numcode_modifier: u8,
    pub category_index: bool,
    pub category_index_two_digits: bool,
    pub special: BTreeMap<SpecialAction, SpecialBinding>,

    pub auto_keys: AutoKeySettings,

    pub start_minimized: bool,
    pub show_tray_icon: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub always_on_top: bool,
    pub alternating_rows: bool,
    pub typing_begins_search: bool,
    pub search_all_categories: bool,
    pub disable_sorting: bool,
    pub compact_list: bool,
    pub show_hotbar: bool,
    pub show_categories: bool,
    pub notification_sounds: bool,
    pub notify_first_play: bool,
    pub date_format: DateFormat,
    pub columns: Columns,
    pub ui_scale: f32,

    pub recordings_dir: Option<PathBuf>,
    pub record_limit_s: u32,
    pub trim_silence: bool,
    pub normalize_recordings: bool,
    pub recording_target_db: f32,
    pub insert_recordings_at_top: bool,

    pub tts_voice: Option<String>,
    pub tts_rate: f32,
    pub tts_pitch: f32,

    pub remote_enabled: bool,
    pub remote_soundpad_compat: bool,
    pub log_remote_calls: bool,

    pub use_external_editor: bool,
    pub external_editor: Option<PathBuf>,

    pub hotbar_rows: usize,
    pub hotbar_cols: usize,
    pub hotbar_locked: bool,
    pub hotbar_announce_page: bool,

    pub recent_lists: Vec<PathBuf>,

    pub update_check: bool,
    pub update_auto_install: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let mut special = BTreeMap::new();
        for def in SPECIAL_DEFS {
            special.insert(
                def.action,
                SpecialBinding { hotkey: None, param: def.param.map(|p| p.0).unwrap_or(0) },
            );
        }
        Settings {
            language: Language::Auto,
            theme: Theme::Studio,
            first_start: true,
            win_default_mic_switched: false,
            previous_default_mic: None,
            speaker_device: None,
            mic_out_device: None,
            mic_in_device: None,
            mic_passthrough: true,
            mic_passthrough_gain: 1.0,
            block_voice_while_playing: false,
            ptt_delay_ms: 0,
            mic_delay_ms: 0,
            play_mode: PlayMode::Both,
            volume: 0.8,
            muted: false,
            speakers_gain: 1.0,
            mic_gain: 1.0,
            linear_volume_slider: false,
            normalization: NormMode::Dynamic,
            target_loudness_db: -18.0,
            fixed_adjust_db: 0.0,
            voice_activation_sound: false,
            voice_activation_ms: 250,
            duck_other_apps: false,
            duck_level: 0.3,
            overlap_sounds: false,
            auto_stop_by_hotkey: true,
            continue_playback: false,
            repeat_file: false,
            repeat_list: false,
            resume_in_another_mode: false,
            preload_hotkey_sounds: true,
            cache_mb: 512,
            hotkeys_enabled: true,
            remember_hotkeys_enabled: false,
            pass_through_hotkeys: false,
            distinguish_sides: false,
            index_hotkeys: false,
            index_modifier: MOD_CTRL | MOD_ALT,
            numcode_hotkeys: false,
            numcode_modifier: MOD_CTRL,
            category_index: false,
            category_index_two_digits: false,
            special,
            auto_keys: AutoKeySettings::default(),
            start_minimized: false,
            show_tray_icon: true,
            minimize_to_tray: false,
            close_to_tray: false,
            always_on_top: false,
            alternating_rows: true,
            typing_begins_search: true,
            search_all_categories: true,
            disable_sorting: false,
            compact_list: false,
            show_hotbar: true,
            show_categories: true,
            notification_sounds: true,
            notify_first_play: false,
            date_format: if crate::i18n::system_language() == Language::De { DateFormat::German } else { DateFormat::Iso },
            columns: Columns::default(),
            ui_scale: 1.0,
            recordings_dir: None,
            record_limit_s: 300,
            trim_silence: true,
            normalize_recordings: true,
            recording_target_db: -18.0,
            insert_recordings_at_top: false,
            tts_voice: None,
            tts_rate: 1.0,
            tts_pitch: 1.0,
            remote_enabled: true,
            remote_soundpad_compat: true,
            log_remote_calls: false,
            use_external_editor: false,
            external_editor: None,
            hotbar_rows: 3,
            hotbar_cols: 4,
            hotbar_locked: false,
            hotbar_announce_page: true,
            recent_lists: Vec::new(),
            update_check: true,
            update_auto_install: true,
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        let path = crate::paths::settings_file();
        let mut s: Settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|txt| serde_json::from_str(&txt).ok())
            .unwrap_or_default();
        for def in SPECIAL_DEFS {
            s.special.entry(def.action).or_insert(SpecialBinding {
                hotkey: None,
                param: def.param.map(|p| p.0).unwrap_or(0),
            });
        }
        if !s.remember_hotkeys_enabled {
            s.hotkeys_enabled = true;
        }
        s
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let txt = serde_json::to_string_pretty(self)?;
        crate::library::write_atomic(&crate::paths::settings_file(), txt.as_bytes())
    }

    pub fn recordings_dir(&self) -> PathBuf {
        self.recordings_dir.clone().unwrap_or_else(crate::paths::default_recordings_dir)
    }

    pub fn binding(&self, action: SpecialAction) -> SpecialBinding {
        self.special.get(&action).cloned().unwrap_or_default()
    }
}
