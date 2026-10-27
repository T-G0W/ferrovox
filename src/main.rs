#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod audio;
mod autokeys;
mod core;
mod hook;
mod hotkey;
mod i18n;
mod library;
mod paths;
mod remote;
mod settings;
mod spl;
#[cfg(test)]
mod tests;
mod ui;
mod update;
mod vmic;
mod winutil;

use std::io::Write;

struct FileLogger {
    file: parking_lot::Mutex<Option<std::fs::File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info || m.target().starts_with("ferrovox")
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) || !(r.target().starts_with("ferrovox") || r.level() <= log::Level::Warn) {
            return;
        }
        let line = format!("{} [{}] {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), r.level(), r.args());
        if let Some(f) = self.file.lock().as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
        #[cfg(debug_assertions)]
        eprint!("{line}");
    }
    fn flush(&self) {}
}

fn init_logging() {
    let path = paths::log_file();
    if std::fs::metadata(&path).map(|m| m.len() > 2_000_000).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("old.log"));
    }
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
    let logger = Box::leak(Box::new(FileLogger { file: parking_lot::Mutex::new(file) }));
    let _ = log::set_logger(logger);
    log::set_max_level(log::LevelFilter::Info);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    i18n::set_language(i18n::Language::Auto);

    if let Some(pos) = args.iter().position(|a| a == "-rc" || a == "--rc") {
        unsafe {
            let _ = windows::Win32::System::Console::AttachConsole(windows::Win32::System::Console::ATTACH_PARENT_PROCESS);
        }
        let cmd = args[pos + 1..].join(" ");
        match remote::send(remote::OWN_PIPE, &cmd) {
            Ok(resp) => println!("{resp}"),
            Err(e) => {
                eprintln!("{}: {e}", t!("Ferrovox läuft nicht", "Ferrovox is not running"));
                std::process::exit(1);
            }
        }
        return;
    }

    if winutil::another_instance_running() {
        let _ = remote::send(remote::OWN_PIPE, "ShowWindow()");
        return;
    }

    init_logging();
    log::info!("Ferrovox {} startet", paths::VERSION);
    std::panic::set_hook(Box::new(|info| {
        log::error!("Absturz: {info}");
    }));

    let mut settings = settings::Settings::load();
    if args.iter().any(|a| a == "--clean") {
        settings = settings::Settings::default();
    }
    i18n::set_language(settings.language);
    let start_minimized = settings.start_minimized || args.iter().any(|a| a == "--minimized");
    let library = library::Library::load(&paths::library_file());

    let core = core::Core::new(settings, library);
    remote::start(core.clone());
    update::updater().start_background(core.clone());

    if let Err(e) = ui::run(core.clone(), start_minimized) {
        log::error!("UI-Fehler: {e}");
    }
    finish(&core);
}

fn finish(core: &core::Core) {
    let auto_install = core.settings.read().update_auto_install;
    core.shutdown();
    update::updater().on_exit(auto_install);
    log::info!("Ferrovox beendet");
}

pub fn exit_app(core: &core::Core) -> ! {
    finish(core);
    std::process::exit(0);
}
