use crate::core::{Core, ToastKind};
use crate::t;
use parking_lot::Mutex;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const NO_WINDOW: u32 = 0x0800_0000;
const PAGE_URL: &str = "https://vb-audio.com/Cable/";
const DOWNLOAD_BASE: &str = "https://download.vb-audio.com/Download_CABLE/";
const FALLBACK_PACK: u32 = 45;
const PACK_PREFIX: &str = "VBCABLE_Driver_Pack";

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Idle,
    Resolving,
    Downloading { done: u64, total: u64 },
    Extracting,
    Verifying,
    WaitingForAdmin,
    Detecting,
    Done,
    Removed,
    NeedsReboot,
    Failed(String),
}

impl Step {
    pub fn label(&self) -> String {
        match self {
            Step::Idle => String::new(),
            Step::Resolving => t!("Suche aktuelles Paket …", "Looking up latest package …").into(),
            Step::Downloading { done, total } => {
                if *total > 0 {
                    format!("{} {:.1} / {:.1} MB", t!("Lade herunter", "Downloading"), *done as f64 / 1e6, *total as f64 / 1e6)
                } else {
                    format!("{} {:.1} MB", t!("Lade herunter", "Downloading"), *done as f64 / 1e6)
                }
            }
            Step::Extracting => t!("Entpacke …", "Extracting …").into(),
            Step::Verifying => t!("Prüfe digitale Signatur …", "Verifying digital signature …").into(),
            Step::WaitingForAdmin => t!("Bitte die Windows-Abfrage (Administrator) bestätigen …", "Please confirm the Windows prompt (administrator) …").into(),
            Step::Detecting => t!("Installiere und suche das neue Gerät …", "Installing and detecting the new device …").into(),
            Step::Done => t!("Virtuelles Mikrofon ist bereit.", "Virtual microphone is ready.").into(),
            Step::Removed => t!("Virtuelles Mikrofon wurde entfernt.", "Virtual microphone was removed.").into(),
            Step::NeedsReboot => t!("Installiert – Windows braucht einen Neustart, bis das Gerät erscheint.", "Installed – Windows needs a restart until the device appears.").into(),
            Step::Failed(e) => e.clone(),
        }
    }

    pub fn progress(&self) -> Option<f32> {
        match self {
            Step::Resolving => Some(0.03),
            Step::Downloading { done, total } if *total > 0 => Some(0.05 + 0.6 * (*done as f32 / *total as f32).min(1.0)),
            Step::Downloading { .. } => Some(0.3),
            Step::Extracting => Some(0.68),
            Step::Verifying => Some(0.72),
            Step::WaitingForAdmin => Some(0.76),
            Step::Detecting => Some(0.88),
            _ => None,
        }
    }
}

pub struct Installer {
    step: Mutex<Step>,
    signer: Mutex<Option<String>>,
    busy: AtomicBool,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Install { silent: bool },
    Uninstall,
}

impl Installer {
    pub fn new() -> Arc<Installer> {
        Arc::new(Installer { step: Mutex::new(Step::Idle), signer: Mutex::new(None), busy: AtomicBool::new(false) })
    }

    pub fn step(&self) -> Step {
        self.step.lock().clone()
    }

    pub fn signer(&self) -> Option<String> {
        self.signer.lock().clone()
    }

    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }

    fn set(&self, core: &Core, s: Step) {
        *self.step.lock() = s;
        core.repaint();
    }

    pub fn install(self: &Arc<Self>, core: Arc<Core>, silent: bool) {
        self.start(core, Mode::Install { silent });
    }

    pub fn uninstall(self: &Arc<Self>, core: Arc<Core>) {
        self.start(core, Mode::Uninstall);
    }

    fn start(self: &Arc<Self>, core: Arc<Core>, mode: Mode) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("ferrovox-vmic".into())
            .spawn(move || {
                let result = me.run(&core, mode);
                match result {
                    Ok(step) => {
                        let msg = step.label();
                        me.set(&core, step);
                        core.toast(msg, ToastKind::Ok);
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        log::error!("Virtuelles Mikrofon: {msg}");
                        me.set(&core, Step::Failed(msg.clone()));
                        core.toast(msg, ToastKind::Error);
                    }
                }
                me.busy.store(false, Ordering::SeqCst);
            })
            .ok();
    }

    fn run(&self, core: &Arc<Core>, mode: Mode) -> anyhow::Result<Step> {
        let work = crate::paths::data_dir().join("vbcable");
        std::fs::create_dir_all(&work)?;

        let zip = match local_package() {
            Some(z) => z,
            None => {
                self.set(core, Step::Resolving);
                let url = resolve_url();
                let name = url.rsplit('/').next().unwrap_or("VBCABLE.zip").to_string();
                let dest = work.join(&name);
                if !is_zip(&dest) {
                    self.download(core, &url, &dest)?;
                }
                dest
            }
        };

        self.set(core, Step::Extracting);
        let ex = work.join("ex");
        extract(&zip, &ex)?;
        let setup = ex.join(if windows_is_64bit() { "VBCABLE_Setup_x64.exe" } else { "VBCABLE_Setup.exe" });
        anyhow::ensure!(setup.exists(), "{}", t!("Setup-Datei fehlt im Paket", "Setup file missing in package"));

        self.set(core, Step::Verifying);
        let signer = verify_signature(&setup)?;
        *self.signer.lock() = Some(signer);

        let prev_out = crate::audio::winaudio::default_render();
        let prev_in = crate::audio::winaudio::default_capture();

        self.set(core, Step::WaitingForAdmin);
        let args = match mode {
            Mode::Install { silent: true } => "-i -h",
            Mode::Install { silent: false } => "-i",
            Mode::Uninstall => "-u -h",
        };
        let code = run_elevated(&setup, args)?;
        log::info!("VB-CABLE-Setup {args} beendet mit Code {code}");

        self.set(core, Step::Detecting);
        let want_present = !matches!(mode, Mode::Uninstall);
        for _ in 0..20 {
            std::thread::sleep(Duration::from_secs(1));
            let present = virtual_present(core);
            if present == want_present {
                if want_present {
                    restore_defaults(prev_out.clone(), prev_in.clone());
                }
                core.reconnect_devices();
                return Ok(if want_present { Step::Done } else { Step::Removed });
            }
        }
        core.reconnect_devices();
        Ok(Step::NeedsReboot)
    }

    fn download(&self, core: &Core, url: &str, dest: &Path) -> anyhow::Result<()> {
        let total = content_length(url).unwrap_or(0);
        self.set(core, Step::Downloading { done: 0, total });
        let part = dest.with_extension("part");
        let _ = std::fs::remove_file(&part);
        let mut child = Command::new(system_tool("curl.exe"))
            .args(["-L", "--fail", "-s", "--max-time", "600", "-o"])
            .arg(&part)
            .arg(url)
            .creation_flags(NO_WINDOW)
            .spawn()
            .map_err(|e| anyhow::anyhow!("curl.exe: {e}"))?;
        let status = loop {
            if let Some(st) = child.try_wait()? {
                break st;
            }
            let done = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            self.set(core, Step::Downloading { done, total });
            std::thread::sleep(Duration::from_millis(150));
        };
        anyhow::ensure!(
            status.success() && is_zip(&part),
            "{}",
            t!(
                "Download von vb-audio.com fehlgeschlagen – bitte Internetverbindung prüfen.",
                "Download from vb-audio.com failed – please check your internet connection."
            )
        );
        std::fs::rename(&part, dest)?;
        Ok(())
    }
}

pub fn system_tool(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(name)
}

pub fn windows_is_64bit() -> bool {
    if cfg!(target_pointer_width = "64") {
        return true;
    }
    let mut wow64 = windows::Win32::Foundation::BOOL(0);
    unsafe {
        let _ = windows::Win32::System::Threading::IsWow64Process(
            windows::Win32::System::Threading::GetCurrentProcess(),
            &mut wow64,
        );
    }
    wow64.as_bool()
}

fn local_package() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(PACK_PREFIX) && n.ends_with(".zip")) && is_zip(p)
    })
}

fn is_zip(p: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 2];
    std::fs::metadata(p).map(|m| m.len() > 100_000).unwrap_or(false)
        && std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)).is_ok()
        && &magic == b"PK"
}

fn resolve_url() -> String {
    let n = Command::new(system_tool("curl.exe"))
        .args(["-sL", "--max-time", "15", PAGE_URL])
        .creation_flags(NO_WINDOW)
        .output()
        .ok()
        .map(|o| latest_pack(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(None)
        .unwrap_or(FALLBACK_PACK);
    format!("{DOWNLOAD_BASE}{PACK_PREFIX}{n}.zip")
}

pub fn latest_pack(html: &str) -> Option<u32> {
    html.match_indices(PACK_PREFIX)
        .filter_map(|(i, _)| {
            let rest = &html[i + PACK_PREFIX.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            (!digits.is_empty() && rest[digits.len()..].starts_with(".zip")).then(|| digits.parse().ok()).flatten()
        })
        .max()
}

fn content_length(url: &str) -> Option<u64> {
    let out = Command::new(system_tool("curl.exe"))
        .args(["-sIL", "--max-time", "15", url])
        .creation_flags(NO_WINDOW)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let l = l.to_ascii_lowercase();
            l.strip_prefix("content-length:").and_then(|v| v.trim().parse().ok())
        })
        .last()
}

fn extract(zip: &Path, dir: &Path) -> anyhow::Result<()> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    let tar_ok = Command::new(system_tool("tar.exe"))
        .arg("-xf")
        .arg(zip)
        .arg("-C")
        .arg(dir)
        .creation_flags(NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if tar_ok {
        return Ok(());
    }
    let cmd = format!(
        "Expand-Archive -Force -LiteralPath '{}' -DestinationPath '{}'",
        zip.display().to_string().replace('\'', "''"),
        dir.display().to_string().replace('\'', "''")
    );
    let ok = Command::new(system_tool(r"WindowsPowerShell\v1.0\powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &cmd])
        .creation_flags(NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    anyhow::ensure!(ok, "{}", t!("Paket konnte nicht entpackt werden", "Package could not be extracted"));
    Ok(())
}

fn verify_signature(setup: &Path) -> anyhow::Result<String> {
    let cmd = format!(
        "$s = Get-AuthenticodeSignature -LiteralPath '{}'; $s.Status.ToString(); $s.SignerCertificate.Subject",
        setup.display().to_string().replace('\'', "''")
    );
    let out = Command::new(system_tool(r"WindowsPowerShell\v1.0\powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command", &cmd])
        .creation_flags(NO_WINDOW)
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let status = lines.next().unwrap_or_default().to_string();
    let subject = lines.next().unwrap_or_default().to_string();
    anyhow::ensure!(
        status == "Valid",
        "{} ({status})",
        t!("Die digitale Signatur des Setups ist ungültig – Installation abgebrochen", "The setup's digital signature is invalid – installation aborted")
    );
    let s = subject.to_lowercase();
    anyhow::ensure!(
        s.contains("burel") || s.contains("vb-audio"),
        "{}: {subject}",
        t!("Unerwarteter Herausgeber – Installation abgebrochen", "Unexpected publisher – installation aborted")
    );
    Ok(subject)
}

fn run_elevated(exe: &Path, args: &str) -> anyhow::Result<u32> {
    let file: Vec<u16> = exe.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let params: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();
    let dir: Vec<u16> =
        exe.parent().map(|p| p.display().to_string()).unwrap_or_default().encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            lpDirectory: PCWSTR(dir.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        if ShellExecuteExW(&mut info).is_err() {
            anyhow::bail!(
                "{}",
                t!("Die Administrator-Bestätigung wurde abgebrochen.", "The administrator confirmation was canceled.")
            );
        }
        let mut code = 0u32;
        let process = info.hProcess;
        if !process.is_invalid() {
            WaitForSingleObject(process, INFINITE);
            let _ = GetExitCodeProcess(process, &mut code);
            let _ = CloseHandle(process);
        }
        Ok(code)
    }
}

fn restore_defaults(prev_out: Option<crate::audio::winaudio::Endpoint>, prev_in: Option<crate::audio::winaudio::Endpoint>) {
    use crate::audio::winaudio::{default_capture, default_render, is_virtual_capture, is_virtual_render, set_default_endpoint};
    std::thread::sleep(Duration::from_millis(800));
    if let (Some(cur), Some(prev)) = (default_render(), prev_out) {
        if is_virtual_render(&cur.name) && !is_virtual_render(&prev.name) {
            match set_default_endpoint(&prev.id) {
                Ok(()) => log::info!("Standard-Ausgabe zurückgestellt auf „{}“", prev.name),
                Err(e) => log::warn!("Standard-Ausgabe konnte nicht zurückgestellt werden: {e}"),
            }
        }
    }
    if let (Some(cur), Some(prev)) = (default_capture(), prev_in) {
        if is_virtual_capture(&cur.name) && !is_virtual_capture(&prev.name) {
            match set_default_endpoint(&prev.id) {
                Ok(()) => log::info!("Standard-Mikrofon zurückgestellt auf „{}“", prev.name),
                Err(e) => log::warn!("Standard-Mikrofon konnte nicht zurückgestellt werden: {e}"),
            }
        }
    }
}

pub fn virtual_present(core: &Core) -> bool {
    core.engine.list_devices().outputs.iter().any(|n| {
        let n = n.to_lowercase();
        n.contains("cable input") || n.contains("vb-audio")
    })
}

pub fn use_as_windows_default(core: &Arc<Core>) -> anyhow::Result<String> {
    use crate::audio::winaudio;
    let endpoints = winaudio::capture_endpoints();
    let cable = endpoints
        .iter()
        .find(|e| e.name.to_lowercase().contains("cable output"))
        .or_else(|| endpoints.iter().find(|e| winaudio::is_virtual_capture(&e.name)))
        .ok_or_else(|| anyhow::anyhow!("{}", t!("Kein virtuelles Mikrofon gefunden", "No virtual microphone found")))?
        .clone();
    let mut s = core.settings.read().clone();
    if let Some(cur) = winaudio::default_capture().filter(|c| !winaudio::is_virtual_capture(&c.name)) {
        s.previous_default_mic = Some(cur.id.clone());
        if s.mic_in_device.is_none() {
            s.mic_in_device = Some(cur.name.clone());
        }
    }
    s.win_default_mic_switched = true;
    core.apply_settings(s);
    winaudio::set_default_capture(&cable.id)?;
    core.reconnect_devices();
    Ok(cable.name)
}

pub fn restore_windows_default(core: &Arc<Core>) -> anyhow::Result<()> {
    let mut s = core.settings.read().clone();
    if let Some(id) = s.previous_default_mic.clone() {
        crate::audio::winaudio::set_default_capture(&id)?;
    }
    s.win_default_mic_switched = false;
    core.apply_settings(s);
    Ok(())
}
