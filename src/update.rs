use crate::core::{Core, ToastKind};
use crate::t;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub const PUBLIC_KEY_HEX: &str = "ebed6bdcda2575c635ac22688814112731846d307adce100f1e585cc97be876f";
const REPO: Option<&str> = option_env!("FERROVOX_UPDATE_REPO");
const NO_WINDOW: u32 = 0x0800_0000;
const FIRST_CHECK: Duration = Duration::from_secs(10);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub setup: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub notes_de: String,
    #[serde(default)]
    pub notes_en: String,
}

impl Manifest {
    pub fn notes(&self) -> &str {
        if crate::i18n::is_en() && !self.notes_en.is_empty() || self.notes_de.is_empty() {
            &self.notes_en
        } else {
            &self.notes_de
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    NotConfigured,
    Checking,
    UpToDate,
    Available(Manifest),
    Downloading { manifest: Manifest, done: u64 },
    Ready { manifest: Manifest, setup: PathBuf },
    Failed(String),
}

impl State {
    pub fn label(&self) -> String {
        match self {
            State::Idle => String::new(),
            State::NotConfigured => t!(
                "Dieser Build hat keine Update-Quelle (nur offizielle Releases aktualisieren sich selbst).",
                "This build has no update source (only official releases update themselves)."
            )
            .into(),
            State::Checking => t!("Suche nach Updates …", "Checking for updates …").into(),
            State::UpToDate => format!("{} ({})", t!("Ferrovox ist aktuell", "Ferrovox is up to date"), crate::paths::VERSION),
            State::Available(m) => format!("{} {}", t!("Neue Version verfügbar:", "New version available:"), m.version),
            State::Downloading { manifest, done } => format!(
                "{} {} – {:.1} / {:.1} MB",
                t!("Lade Version", "Downloading version"),
                manifest.version,
                *done as f64 / 1e6,
                manifest.size as f64 / 1e6
            ),
            State::Ready { manifest, .. } => format!("{} {}", t!("Bereit zur Installation: Version", "Ready to install: version"), manifest.version),
            State::Failed(e) => e.clone(),
        }
    }

    pub fn pending_version(&self) -> Option<&str> {
        match self {
            State::Available(m) | State::Downloading { manifest: m, .. } | State::Ready { manifest: m, .. } => Some(&m.version),
            _ => None,
        }
    }
}

pub struct Updater {
    state: Mutex<State>,
    busy: AtomicBool,
    install_now: Mutex<Option<PathBuf>>,
    install_on_exit: AtomicBool,
}

pub fn updater() -> &'static Arc<Updater> {
    static U: OnceLock<Arc<Updater>> = OnceLock::new();
    U.get_or_init(|| {
        Arc::new(Updater {
            state: Mutex::new(if base_url().is_some() { State::Idle } else { State::NotConfigured }),
            busy: AtomicBool::new(false),
            install_now: Mutex::new(None),
            install_on_exit: AtomicBool::new(false),
        })
    })
}

impl Updater {
    pub fn state(&self) -> State {
        self.state.lock().clone()
    }

    fn set(&self, core: &Core, s: State) {
        *self.state.lock() = s;
        core.repaint();
    }

    pub fn start_background(self: &Arc<Self>, core: Arc<Core>) {
        let _ = std::fs::remove_dir_all(download_dir());
        if base_url().is_none() {
            return;
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("ferrovox-update".into())
            .spawn(move || {
                std::thread::sleep(FIRST_CHECK);
                loop {
                    if core.settings.read().update_check {
                        me.check(core.clone(), false);
                    }
                    std::thread::sleep(CHECK_INTERVAL);
                }
            })
            .ok();
    }

    pub fn check(self: &Arc<Self>, core: Arc<Core>, manual: bool) {
        if matches!(self.state(), State::Ready { .. }) {
            return;
        }
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("ferrovox-update-check".into())
            .spawn(move || {
                me.run_check(&core, manual);
                me.busy.store(false, Ordering::SeqCst);
            })
            .ok();
    }

    fn run_check(&self, core: &Arc<Core>, manual: bool) {
        let Some(base) = base_url() else {
            self.set(core, State::NotConfigured);
            return;
        };
        self.set(core, State::Checking);
        let manifest = match fetch_manifest(&base) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("Update-Prüfung fehlgeschlagen: {e}");
                let msg = format!("{}: {e}", t!("Update-Prüfung fehlgeschlagen", "Update check failed"));
                self.set(core, if manual { State::Failed(msg.clone()) } else { State::Idle });
                if manual {
                    core.toast(msg, ToastKind::Error);
                }
                return;
            }
        };
        if !is_newer(&manifest.version, crate::paths::VERSION) {
            self.set(core, State::UpToDate);
            if manual {
                core.toast(State::UpToDate.label(), ToastKind::Ok);
            }
            return;
        }
        log::info!("Update verfügbar: {}", manifest.version);
        self.set(core, State::Available(manifest.clone()));
        if manual || core.settings.read().update_auto_install {
            self.download(core, manifest);
        } else {
            core.toast(State::Available(manifest).label(), ToastKind::Info);
        }
    }

    fn download(&self, core: &Arc<Core>, manifest: Manifest) {
        match self.fetch_setup(core, &manifest) {
            Ok(setup) => {
                log::info!("Update {} geladen und geprüft", manifest.version);
                core.toast(
                    format!(
                        "{} {} – {}",
                        t!("Update", "Update"),
                        manifest.version,
                        t!("wird beim Beenden installiert", "will be installed when you exit")
                    ),
                    ToastKind::Ok,
                );
                self.set(core, State::Ready { manifest, setup });
            }
            Err(e) => {
                log::warn!("Update-Download fehlgeschlagen: {e}");
                let msg = format!("{}: {e}", t!("Update-Download fehlgeschlagen", "Update download failed"));
                core.toast(msg.clone(), ToastKind::Error);
                self.set(core, State::Failed(msg));
            }
        }
    }

    fn fetch_setup(&self, core: &Core, m: &Manifest) -> anyhow::Result<PathBuf> {
        let base = base_url().ok_or_else(|| anyhow::anyhow!("no update source"))?;
        let dir = download_dir();
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join(&m.setup);
        let part = dest.with_extension("part");
        let _ = std::fs::remove_file(&part);
        self.set(core, State::Downloading { manifest: m.clone(), done: 0 });
        let mut child = Command::new(crate::vmic::system_tool("curl.exe"))
            .args(["-L", "--fail", "-s", "--max-time", "900", "-A", user_agent().as_str(), "-o"])
            .arg(&part)
            .arg(format!("{base}{}", m.setup))
            .creation_flags(NO_WINDOW)
            .spawn()
            .map_err(|e| anyhow::anyhow!("curl.exe: {e}"))?;
        let status = loop {
            if let Some(st) = child.try_wait()? {
                break st;
            }
            let done = std::fs::metadata(&part).map(|x| x.len()).unwrap_or(0);
            self.set(core, State::Downloading { manifest: m.clone(), done });
            std::thread::sleep(Duration::from_millis(200));
        };
        anyhow::ensure!(status.success(), "{}", t!("Download abgebrochen", "Download aborted"));
        let size = std::fs::metadata(&part)?.len();
        anyhow::ensure!(size == m.size, "{} ({size} ≠ {})", t!("Falsche Dateigröße", "Wrong file size"), m.size);
        let hash = sha256_file(&part)?;
        if !hash.eq_ignore_ascii_case(&m.sha256) {
            let _ = std::fs::remove_file(&part);
            anyhow::bail!("{}", t!("Prüfsumme stimmt nicht – Update verworfen", "Checksum mismatch – update discarded"));
        }
        std::fs::rename(&part, &dest)?;
        Ok(dest)
    }

    pub fn install_and_restart(&self) -> bool {
        match self.state() {
            State::Ready { setup, .. } => {
                *self.install_now.lock() = Some(setup);
                true
            }
            _ => false,
        }
    }

    pub fn install_on_exit(&self) {
        self.install_on_exit.store(true, Ordering::SeqCst);
    }

    pub fn on_exit(&self, auto_install: bool) {
        let wanted = auto_install || self.install_on_exit.load(Ordering::SeqCst);
        let (setup, relaunch) = match (self.install_now.lock().take(), self.state()) {
            (Some(setup), _) => (setup, true),
            (None, State::Ready { setup, .. }) if wanted => (setup, false),
            _ => return,
        };
        if !setup.exists() {
            return;
        }
        let mut cmd = Command::new(&setup);
        cmd.args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"]);
        if relaunch {
            cmd.arg("/RELAUNCH");
        }
        match cmd.spawn() {
            Ok(_) => log::info!("Update-Setup gestartet ({})", setup.display()),
            Err(e) => log::error!("Update-Setup konnte nicht gestartet werden: {e}"),
        }
    }
}

fn base_url() -> Option<String> {
    let url = std::env::var("FERROVOX_UPDATE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .or_else(|| REPO.filter(|r| !r.is_empty()).map(|r| format!("https://github.com/{r}/releases/latest/download/")))?;
    Some(if url.ends_with('/') { url } else { format!("{url}/") })
}

fn download_dir() -> PathBuf {
    std::env::temp_dir().join("Ferrovox-Update")
}

fn user_agent() -> String {
    format!("Ferrovox/{}", crate::paths::VERSION)
}

fn curl_get(url: &str) -> anyhow::Result<Vec<u8>> {
    let out = Command::new(crate::vmic::system_tool("curl.exe"))
        .args(["-sL", "--fail", "--max-time", "30", "-A", user_agent().as_str(), url])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| anyhow::anyhow!("curl.exe: {e}"))?;
    anyhow::ensure!(
        out.status.success(),
        "{}",
        t!("Update-Server nicht erreichbar", "Update server not reachable")
    );
    Ok(out.stdout)
}

fn fetch_manifest(base: &str) -> anyhow::Result<Manifest> {
    let body = curl_get(&format!("{base}latest.json"))?;
    let sig = curl_get(&format!("{base}latest.json.sig"))?;
    verify_signature(&body, std::str::from_utf8(&sig).unwrap_or_default())?;
    let m: Manifest = serde_json::from_slice(&body)?;
    let plain_name = !m.setup.contains(['/', '\\', ':']) && m.setup.to_ascii_lowercase().ends_with(".exe");
    anyhow::ensure!(plain_name && m.sha256.len() == 64 && m.size > 0, "{}", t!("Ungültige Update-Beschreibung", "Invalid update description"));
    Ok(m)
}

pub fn verify_signature(message: &[u8], sig_hex: &str) -> anyhow::Result<()> {
    let invalid = || anyhow::anyhow!("{}", t!("Update-Signatur ungültig – Update verworfen", "Invalid update signature – update discarded"));
    let pk = ed25519_compact::PublicKey::from_slice(&hex_decode(PUBLIC_KEY_HEX).ok_or_else(invalid)?).map_err(|_| invalid())?;
    let sig = ed25519_compact::Signature::from_slice(&hex_decode(sig_hex.trim()).ok_or_else(invalid)?).map_err(|_| invalid())?;
    pk.verify(message, &sig).map_err(|_| invalid())
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> [u64; 3] {
        let mut out = [0u64; 3];
        let core = v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next().unwrap_or_default();
        for (slot, p) in out.iter_mut().zip(core.split('.')) {
            *slot = p.parse().unwrap_or(0);
        }
        out
    }
    parts(candidate) > parts(current)
}
