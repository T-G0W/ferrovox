use crossbeam_channel::{unbounded, Sender};
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

pub enum DuckCmd {
    Duck(f32),
    Restore,
}

pub fn start() -> Sender<DuckCmd> {
    let (tx, rx) = unbounded::<DuckCmd>();
    std::thread::Builder::new()
        .name("ferrovox-duck".into())
        .spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let mut saved: Vec<(ISimpleAudioVolume, f32)> = Vec::new();
            for cmd in rx {
                match cmd {
                    DuckCmd::Duck(level) => {
                        if saved.is_empty() {
                            match unsafe { duck(level) } {
                                Ok(s) => saved = s,
                                Err(e) => log::warn!("Ducking fehlgeschlagen: {e}"),
                            }
                        }
                    }
                    DuckCmd::Restore => {
                        for (vol, v) in saved.drain(..) {
                            unsafe {
                                let _ = vol.SetMasterVolume(v, std::ptr::null());
                            }
                        }
                    }
                }
            }
        })
        .expect("Ducking-Thread");
    tx
}

unsafe fn duck(level: f32) -> windows::core::Result<Vec<(ISimpleAudioVolume, f32)>> {
    let own_pid = std::process::id();
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
    let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
    let sessions = manager.GetSessionEnumerator()?;
    let mut saved = Vec::new();
    for i in 0..sessions.GetCount()? {
        let Ok(ctl) = sessions.GetSession(i) else { continue };
        let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
        let pid = ctl2.GetProcessId().unwrap_or(0);
        if pid == 0 || pid == own_pid {
            continue;
        }
        let Ok(vol) = ctl.cast::<ISimpleAudioVolume>() else { continue };
        let Ok(cur) = vol.GetMasterVolume() else { continue };
        let _ = vol.SetMasterVolume(cur * level, std::ptr::null());
        saved.push((vol, cur));
    }
    Ok(saved)
}
