use crate::actions::Source;
use crate::audio::RecSource;
use crate::core::Core;
use crate::library::Category;
use crate::settings::{PlayMode, SpecialAction};
use std::io::{Read, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_PIPE_CONNECTED, HANDLE};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

pub const OWN_PIPE: &str = "ferrovox";
pub const SOUNDPAD_PIPE: &str = "sp_remote_control";
pub const REMOTE_VERSION: &str = "1.1.2";

pub fn start(core: Arc<Core>) {
    let (enabled, compat) = {
        let s = core.settings.read();
        (s.remote_enabled, s.remote_soundpad_compat)
    };
    let c = core.clone();
    std::thread::Builder::new().name("ferrovox-pipe".into()).spawn(move || serve(c, OWN_PIPE, true)).ok();
    if enabled && compat {
        std::thread::Builder::new()
            .name("ferrovox-sp-pipe".into())
            .spawn(move || serve(core, SOUNDPAD_PIPE, false))
            .ok();
    }
}

fn serve(core: Arc<Core>, name: &'static str, always: bool) {
    let full = HSTRING::from(format!(r"\\.\pipe\{name}"));
    let mut first = true;
    loop {
        let mut mode = PIPE_ACCESS_DUPLEX;
        if first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let handle = unsafe {
            CreateNamedPipeW(
                &full,
                mode,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                1 << 20,
                1 << 16,
                0,
                None,
            )
        };
        if handle.is_invalid() {
            if first {
                log::warn!("Pipe {name} belegt (läuft Soundpad?) – Fernsteuerung darüber deaktiviert");
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }
        first = false;
        let connected = unsafe { ConnectNamedPipe(handle, None).is_ok() || GetLastError() == ERROR_PIPE_CONNECTED };
        if !connected {
            unsafe {
                let _ = CloseHandle(handle);
            }
            continue;
        }
        let c = core.clone();
        let raw = handle.0 as isize;
        std::thread::spawn(move || client(c, HANDLE(raw as *mut std::ffi::c_void), always));
    }
}

fn client(core: Arc<Core>, handle: HANDLE, own_pipe: bool) {
    let mut file = unsafe { std::fs::File::from_raw_handle(handle.0) };
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let cmd = String::from_utf8_lossy(&buf[..n]).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
        if cmd.is_empty() {
            continue;
        }
        let enabled = own_pipe || core.settings.read().remote_enabled;
        let mut resp = if enabled { execute(&core, &cmd) } else { "R-403: Remote control disabled.".into() };
        if resp.is_empty() {
            resp = "R-204".into();
        }
        if core.settings.read().log_remote_calls {
            log::info!("Remote: {cmd} -> {}", resp.chars().take(80).collect::<String>());
        }
        if file.write_all(resp.as_bytes()).is_err() {
            break;
        }
        let _ = file.flush();
    }
    unsafe {
        let _ = DisconnectNamedPipe(handle);
    }
}

pub fn send(pipe: &str, cmd: &str) -> std::io::Result<String> {
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\{pipe}"))?;
    f.write_all(cmd.as_bytes())?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        out.extend_from_slice(&buf[..n]);
        if n < buf.len() {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn parse(cmd: &str) -> (String, Vec<String>) {
    let cmd = cmd.trim();
    let Some(open) = cmd.find('(') else { return (cmd.to_string(), Vec::new()) };
    let name = cmd[..open].trim().to_string();
    let inner = cmd[open + 1..].trim_end().trim_end_matches(')');
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in inner.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                args.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() || !args.is_empty() {
        args.push(cur.trim().to_string());
    }
    (name, args)
}

fn arg_usize(args: &[String], i: usize) -> Option<usize> {
    args.get(i).and_then(|a| a.parse().ok())
}

fn arg_bool(args: &[String], i: usize) -> Option<bool> {
    args.get(i).map(|a| matches!(a.to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
}

fn mode_from(args: &[String], first: usize) -> Option<PlayMode> {
    match (arg_bool(args, first), arg_bool(args, first + 1)) {
        (Some(s), Some(m)) => Some(PlayMode::from_flags(s, m)),
        _ => None,
    }
}

fn text_and_mode(args: &[String]) -> (String, Option<PlayMode>) {
    let is_bool = |s: &String| matches!(s.to_ascii_lowercase().as_str(), "true" | "false");
    if args.len() >= 3 && is_bool(&args[args.len() - 1]) && is_bool(&args[args.len() - 2]) {
        (args[..args.len() - 2].join(", "), mode_from(args, args.len() - 2))
    } else {
        (args.join(", "), None)
    }
}

const OK: &str = "R-200";

pub fn execute(core: &Arc<Core>, cmd: &str) -> String {
    let (name, a) = parse(cmd);
    let cat_at = |i: usize| -> Option<Category> { i.checked_sub(1).and_then(|i| core.lib.read().categories.get(i).cloned()) };
    match name.as_str() {
        "DoPlaySound" => match arg_usize(&a, 0) {
            Some(i) if core.lib.read().by_index(i).is_some() => {
                let id = core.lib.read().by_index(i).map(|s| s.id).unwrap_or_default();
                core.play_sound(id, mode_from(&a, 1), Source::Remote);
                OK.into()
            }
            _ => "R-404: Sound index not found.".into(),
        },
        "DoPlayRandomSound" => {
            core.play_random(None, mode_from(&a, 0), Source::Remote);
            OK.into()
        }
        "DoPlaySoundFromCategory" => {
            let (Some(cat), Some(pos)) = (arg_usize(&a, 0).and_then(cat_at), arg_usize(&a, 1)) else {
                return "R-404: Category or sound not found.".into();
            };
            match pos.checked_sub(1).and_then(|p| cat.sounds.get(p)) {
                Some(&id) => {
                    core.play_sound(id, mode_from(&a, 2), Source::Remote);
                    OK.into()
                }
                None => "R-404: Sound not found.".into(),
            }
        }
        "DoPlayRandomSoundFromCategory" => match arg_usize(&a, 0).and_then(cat_at) {
            Some(cat) => {
                core.play_random(Some(cat.id), mode_from(&a, 1), Source::Remote);
                OK.into()
            }
            None => "R-404: Category not found.".into(),
        },
        "DoPlaySoundByName" => {
            let (needle, mode) = text_and_mode(&a);
            let needle = needle.to_lowercase();
            let id = core.lib.read().sounds.iter().find(|s| s.matches(&needle)).map(|s| s.id);
            match id {
                Some(id) => {
                    core.play_sound(id, mode, Source::Remote);
                    OK.into()
                }
                None => "R-404: No sound matches.".into(),
            }
        }
        "DoStopSound" => {
            core.stop();
            OK.into()
        }
        "DoTogglePause" => {
            core.toggle_pause();
            OK.into()
        }
        "DoToggleMute" => {
            core.toggle_mute();
            OK.into()
        }
        "DoPlayNextSound" => {
            core.play_relative(1, Source::Remote);
            OK.into()
        }
        "DoPlayPreviousSound" => {
            core.play_relative(-1, Source::Remote);
            OK.into()
        }
        "DoPlayCurrentSoundAgain" => {
            core.execute(crate::actions::Action::Special(SpecialAction::PlayCurrentAgain), Source::Remote);
            OK.into()
        }
        "DoPlayPreviouslyPlayedSound" => {
            core.execute(crate::actions::Action::Special(SpecialAction::PlayPreviouslyPlayed), Source::Remote);
            OK.into()
        }
        "DoPlaySelectedSound" => {
            core.execute(crate::actions::Action::PlaySelected { mode: mode_from(&a, 0) }, Source::Remote);
            OK.into()
        }
        "DoJumpMs" => {
            core.jump_ms(a.first().and_then(|v| v.parse().ok()).unwrap_or(0));
            OK.into()
        }
        "DoSeekMs" => {
            core.seek_ms(a.first().and_then(|v| v.parse().ok()).unwrap_or(0));
            OK.into()
        }
        "DoSetPlayMode" => {
            let m = match a.first().map(|s| s.to_lowercase()).as_deref() {
                Some("speakers") => PlayMode::Speakers,
                Some("microphone") | Some("mic") => PlayMode::Microphone,
                _ => PlayMode::Both,
            };
            core.set_play_mode(m, false);
            OK.into()
        }
        "DoTts" => {
            let (text, mode) = text_and_mode(&a);
            core.play_tts(Some(text), mode);
            OK.into()
        }
        "DoAddCategory" => {
            let name = a.first().cloned().unwrap_or_else(|| crate::t!("Neue Kategorie", "New category").into());
            core.modify_library(|lib| {
                let id = lib.alloc_id();
                lib.categories.push(Category { id, name, ..Default::default() });
            });
            OK.into()
        }
        "DoRemoveCategory" => match arg_usize(&a, 0).and_then(cat_at) {
            Some(cat) => {
                core.modify_library(|lib| lib.categories.retain(|c| c.id != cat.id));
                OK.into()
            }
            None => "R-404: Category not found.".into(),
        },
        "DoAddSound" => {
            let Some(path) = a.first().map(std::path::PathBuf::from) else { return "R-400: Path missing.".into() };
            if !path.exists() {
                return "R-404: File not found.".into();
            }
            let cat = if a.len() >= 3 { arg_usize(&a, 1).and_then(cat_at).map(|c| c.id) } else { None };
            core.import_paths(vec![path], cat);
            OK.into()
        }
        "DoStartRecording" | "DoStartRecordingSpeakers" => {
            core.start_recording(RecSource::Speakers);
            OK.into()
        }
        "DoStartRecordingMicrophone" => {
            core.start_recording(RecSource::Microphone);
            OK.into()
        }
        "DoStopRecording" => {
            core.stop_recording();
            OK.into()
        }
        "DoSearch" => {
            let mut rt = core.rt.lock();
            rt.search = a.first().cloned().unwrap_or_default();
            rt.focus_search = true;
            drop(rt);
            core.repaint();
            OK.into()
        }
        "DoResetSearch" => {
            core.rt.lock().search.clear();
            core.repaint();
            OK.into()
        }
        "DoSelectNextHit" => {
            core.select_relative(1);
            OK.into()
        }
        "DoSelectPreviousHit" => {
            core.select_relative(-1);
            OK.into()
        }
        "DoSelectIndex" | "DoScrollTo" => match arg_usize(&a, 0).and_then(|i| core.lib.read().by_index(i).map(|s| s.id)) {
            Some(id) => {
                let mut rt = core.rt.lock();
                if name == "DoSelectIndex" {
                    rt.selected = vec![id];
                }
                rt.scroll_to = Some(id);
                drop(rt);
                core.repaint();
                OK.into()
            }
            None => "R-404: Index not found.".into(),
        },
        "DoScrollBy" => {
            let n: i32 = a.first().and_then(|v| v.parse().ok()).unwrap_or(0);
            core.select_relative(n.signum());
            OK.into()
        }
        "DoSelectCategory" => match arg_usize(&a, 0).and_then(cat_at) {
            Some(cat) => {
                let mut rt = core.rt.lock();
                rt.selected_category = Some(cat.id);
                rt.view_order.clear();
                drop(rt);
                core.repaint();
                OK.into()
            }
            None => "R-404: Category not found.".into(),
        },
        "DoSelectNextCategory" => {
            core.select_category_relative(1);
            OK.into()
        }
        "DoSelectPreviousCategory" => {
            core.select_category_relative(-1);
            OK.into()
        }
        "DoRemoveSelectedEntries" => {
            let (sel, cat) = {
                let rt = core.rt.lock();
                (rt.selected.clone(), rt.selected_category)
            };
            core.modify_library(|lib| match cat {
                Some(c) => lib.remove_from_category(c, &sel),
                None => lib.remove_sounds(&sel),
            });
            OK.into()
        }
        "DoUndo" => {
            core.undo();
            OK.into()
        }
        "DoRedo" => {
            core.redo();
            OK.into()
        }
        "DoSaveSoundlist" => {
            match a.first() {
                Some(p) => core.save_list_as(std::path::Path::new(p)),
                None => core.save_library_now(),
            }
            OK.into()
        }
        "GetSoundFileCount" => core.lib.read().sounds.len().to_string(),
        "GetSoundlist" => {
            let range = match (arg_usize(&a, 0), arg_usize(&a, 1)) {
                (Some(f), Some(t)) => Some((f, t)),
                (Some(f), None) => Some((f, usize::MAX)),
                _ => None,
            };
            crate::spl::to_xml(&core.lib.read(), true, range)
        }
        "GetCategories" => crate::spl::categories_xml(&core.lib.read(), None),
        "GetCategory" => crate::spl::categories_xml(&core.lib.read(), arg_usize(&a, 0)),
        "GetPlaybackDurationInMs" => core.status().dur_ms.to_string(),
        "GetPlaybackPositionInMs" => core.status().pos_ms.to_string(),
        "GetRecordingPositionInMs" => core.recording_info().map(|(_, s)| (s * 1000.0) as u64).unwrap_or(0).to_string(),
        "GetRecordingPeak" => {
            let p = f32::from_bits(core.shared().rec_peak.load(Ordering::Relaxed));
            ((p * 100.0) as u32).to_string()
        }
        "GetStatusBarText" => {
            let s = core.rt.lock().status.clone();
            if s.is_empty() {
                crate::t!("Bereit", "Ready").into()
            } else {
                s
            }
        }
        "GetTitleText" => {
            let st = core.status();
            if st.playing {
                format!("{} - Ferrovox", st.name)
            } else {
                "Ferrovox".into()
            }
        }
        "GetVersion" => crate::paths::VERSION.into(),
        "GetRemoteControlVersion" => REMOTE_VERSION.into(),
        "GetPlayStatus" => {
            let st = core.status();
            (if !st.playing {
                "STOPPED"
            } else if st.paused {
                "PAUSED"
            } else {
                "PLAYING"
            })
            .into()
        }
        "GetVolume" => ((core.settings.read().volume * 100.0).round() as u32).to_string(),
        "SetVolume" => {
            let v: f32 = a.first().and_then(|v| v.parse().ok()).unwrap_or(100.0);
            core.set_volume(v / 100.0);
            OK.into()
        }
        "IsMuted" => (if core.settings.read().muted { "1" } else { "0" }).into(),
        "IsAlive" => OK.into(),
        "IsTrial" => "0".into(),
        "ShowWindow" => {
            crate::winutil::show_window(core.hwnd.load(Ordering::Relaxed));
            OK.into()
        }
        "ShowPanel" => {
            crate::winutil::show_window(core.hwnd.load(Ordering::Relaxed));
            core.rt.lock().open_panel.push(a.first().cloned().unwrap_or_default().to_lowercase());
            core.repaint();
            OK.into()
        }
        _ => "R-404: Command not found.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_soundpad_commands() {
        assert_eq!(parse("IsAlive()"), ("IsAlive".to_string(), vec![]));
        assert_eq!(parse("DoPlaySound(3)"), ("DoPlaySound".to_string(), vec!["3".to_string()]));
        assert_eq!(
            parse("DoPlaySound(3, true, false)"),
            ("DoPlaySound".to_string(), vec!["3".into(), "true".into(), "false".into()])
        );
        assert_eq!(
            parse(r#"DoAddSound("C:\a, b.mp3", 2, 1)"#),
            ("DoAddSound".to_string(), vec![r"C:\a, b.mp3".into(), "2".into(), "1".into()])
        );
        assert_eq!(parse("GetVolume"), ("GetVolume".to_string(), vec![]));
    }
}
