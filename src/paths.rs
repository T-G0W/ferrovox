use std::path::PathBuf;

pub const APP_NAME: &str = "Ferrovox";
pub const VENDOR: &str = "T-GOW";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn data_dir() -> PathBuf {
    let dir = match std::env::var_os("FERROVOX_DATA_DIR") {
        Some(d) => PathBuf::from(d),
        None => dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join(VENDOR).join(APP_NAME),
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn library_file() -> PathBuf {
    data_dir().join("library.json")
}

pub fn backups_dir() -> PathBuf {
    let dir = data_dir().join("backups");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn default_recordings_dir() -> PathBuf {
    let base = dirs::audio_dir().unwrap_or_else(data_dir);
    base.join(crate::t!("Ferrovox Aufnahmen", "Ferrovox Recordings"))
}

pub fn log_file() -> PathBuf {
    data_dir().join("ferrovox.log")
}

pub fn soundpad_dir() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("Leppsoft");
    dir.is_dir().then_some(dir)
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
