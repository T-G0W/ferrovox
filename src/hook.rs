use crate::actions::Action;
use crate::autokeys::INJECT_TAG;
use crate::hotkey::*;
use crossbeam_channel::Sender;
use parking_lot::{Mutex, RwLock};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Default)]
pub struct DigitCfg {
    pub index_enabled: bool,
    pub index_mods: u8,
    pub numcode_enabled: bool,
    pub numcode_mods: u8,
}

pub struct HookShared {
    map: RwLock<Arc<HashMap<u32, Action>>>,
    always: RwLock<Arc<HashSet<u32>>>,
    pub enabled: AtomicBool,
    pub pass_through: AtomicBool,
    pub distinguish: AtomicBool,
    capture: AtomicBool,
    capture_tx: Mutex<Option<Sender<Hotkey>>>,
    digits: RwLock<DigitCfg>,
    tx: Sender<Action>,
    thread_id: AtomicU32,
    mouse_wanted: AtomicBool,
}

static SHARED: OnceLock<Arc<HookShared>> = OnceLock::new();
const WM_REFRESH_MOUSE: u32 = WM_APP + 1;

impl HookShared {
    pub fn set_map(&self, map: HashMap<u32, Action>, always: HashSet<u32>, mouse: bool) {
        *self.map.write() = Arc::new(map);
        *self.always.write() = Arc::new(always);
        self.mouse_wanted.store(mouse, Ordering::Relaxed);
        self.refresh_mouse();
    }

    pub fn set_digits(&self, cfg: DigitCfg) {
        *self.digits.write() = cfg;
    }

    pub fn begin_capture(&self, tx: Sender<Hotkey>) {
        *self.capture_tx.lock() = Some(tx);
        self.capture.store(true, Ordering::SeqCst);
        self.refresh_mouse();
    }

    pub fn cancel_capture(&self) {
        self.capture.store(false, Ordering::SeqCst);
        *self.capture_tx.lock() = None;
        self.refresh_mouse();
    }

    fn refresh_mouse(&self) {
        let tid = self.thread_id.load(Ordering::Relaxed);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_REFRESH_MOUSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub fn start(tx: Sender<Action>) -> Arc<HookShared> {
    let shared = Arc::new(HookShared {
        map: RwLock::new(Arc::new(HashMap::new())),
        always: RwLock::new(Arc::new(HashSet::new())),
        enabled: AtomicBool::new(true),
        pass_through: AtomicBool::new(false),
        distinguish: AtomicBool::new(false),
        capture: AtomicBool::new(false),
        capture_tx: Mutex::new(None),
        digits: RwLock::new(DigitCfg::default()),
        tx,
        thread_id: AtomicU32::new(0),
        mouse_wanted: AtomicBool::new(false),
    });
    let _ = SHARED.set(shared.clone());
    std::thread::Builder::new()
        .name("ferrovox-hook".into())
        .spawn(hook_thread)
        .expect("Hook-Thread");
    shared
}

fn hook_thread() {
    let sh = SHARED.get().expect("Hook nicht initialisiert").clone();
    unsafe {
        sh.thread_id.store(GetCurrentThreadId(), Ordering::Relaxed);
        let hinst: HINSTANCE = GetModuleHandleW(None).map(|h| h.into()).unwrap_or_default();
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), hinst, 0);
        if let Err(e) = &kb {
            log::error!("Tastatur-Hook konnte nicht installiert werden: {e}");
        }
        let mut mouse: Option<HHOOK> = None;
        let mut msg = MSG::default();
        let sync_mouse = |mouse: &mut Option<HHOOK>| {
            let want = sh.mouse_wanted.load(Ordering::Relaxed) || sh.capture.load(Ordering::SeqCst);
            if want && mouse.is_none() {
                *mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0).ok();
            } else if !want {
                if let Some(h) = mouse.take() {
                    let _ = UnhookWindowsHookEx(h);
                }
            }
        };
        sync_mouse(&mut mouse);
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_REFRESH_MOUSE {
                sync_mouse(&mut mouse);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

struct HookState {
    down: [bool; 256],
    swallowed: [bool; 256],
    digits: String,
    digit_mode: u8,
}

thread_local! {
    static STATE: RefCell<HookState> = const {
        RefCell::new(HookState { down: [false; 256], swallowed: [false; 256], digits: String::new(), digit_mode: 0 })
    };
}

impl HookState {
    fn mods(&self) -> (u8, u8) {
        let d = &self.down;
        let mut mods = 0;
        let mut side = 0;
        if d[0xA2] || d[0xA3] || d[0x11] {
            mods |= MOD_CTRL;
            if d[0xA3] && !d[0xA2] {
                side |= SIDE_CTRL;
            }
        }
        if d[0xA4] || d[0xA5] || d[0x12] {
            mods |= MOD_ALT;
            if d[0xA5] && !d[0xA4] {
                side |= SIDE_ALT;
            }
        }
        if d[0xA0] || d[0xA1] || d[0x10] {
            mods |= MOD_SHIFT;
            if d[0xA1] && !d[0xA0] {
                side |= SIDE_SHIFT;
            }
        }
        if d[0x5B] || d[0x5C] {
            mods |= MOD_WIN;
            if d[0x5C] && !d[0x5B] {
                side |= SIDE_WIN;
            }
        }
        (mods, side)
    }
}

fn handle_key(vk: u16, down: bool) -> bool {
    let Some(sh) = SHARED.get() else { return false };
    let vk_i = (vk & 0xFF) as usize;
    STATE.with(|st| {
        let mut st = st.borrow_mut();
        if down {
            let repeat = st.down[vk_i];
            st.down[vk_i] = true;
            if is_modifier_vk(vk) {
                return false;
            }
            if repeat {
                return st.swallowed[vk_i];
            }
            let (mods, side) = st.mods();

            if sh.capture.swap(false, Ordering::SeqCst) {
                if let Some(tx) = sh.capture_tx.lock().take() {
                    let _ = tx.send(Hotkey::new(vk, mods, side));
                }
                sh.refresh_mouse();
                st.swallowed[vk_i] = true;
                return true;
            }

            let enabled = sh.enabled.load(Ordering::Relaxed);
            if enabled && mods != 0 {
                if let Some(d) = digit_of(vk) {
                    let cfg = sh.digits.read().clone();
                    let mode = if cfg.index_enabled && mods == cfg.index_mods {
                        1
                    } else if cfg.numcode_enabled && mods == cfg.numcode_mods {
                        2
                    } else {
                        0
                    };
                    if mode != 0 && (st.digit_mode == 0 || st.digit_mode == mode) {
                        st.digit_mode = mode;
                        if st.digits.len() < 6 {
                            st.digits.push(d);
                        }
                        st.swallowed[vk_i] = true;
                        return true;
                    }
                }
            }

            let distinguish = sh.distinguish.load(Ordering::Relaxed);
            let key = Hotkey::new(vk, mods, side).match_key(distinguish);
            let action = sh.map.read().get(&key).cloned();
            if let Some(action) = action {
                if enabled || sh.always.read().contains(&key) {
                    let _ = sh.tx.send(action);
                    if !sh.pass_through.load(Ordering::Relaxed) {
                        st.swallowed[vk_i] = true;
                        return true;
                    }
                }
            }
            false
        } else {
            st.down[vk_i] = false;
            if is_modifier_vk(vk) && !st.digits.is_empty() {
                let cfg = sh.digits.read().clone();
                let need = if st.digit_mode == 1 { cfg.index_mods } else { cfg.numcode_mods };
                let (mods, _) = st.mods();
                if mods & need != need {
                    let digits = std::mem::take(&mut st.digits);
                    let action = if st.digit_mode == 1 {
                        Action::PlayCode(digits)
                    } else {
                        Action::PlayNumcode(digits.parse().unwrap_or(0))
                    };
                    st.digit_mode = 0;
                    let _ = sh.tx.send(action);
                }
            }
            if st.swallowed[vk_i] {
                st.swallowed[vk_i] = false;
                return true;
            }
            false
        }
    })
}

unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if kb.dwExtraInfo != INJECT_TAG {
            let msg = wparam.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            if (down || up) && handle_key(kb.vkCode as u16, down) {
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let msg = wparam.0 as u32;
        if msg != WM_MOUSEMOVE {
            let ms = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            if ms.dwExtraInfo != INJECT_TAG {
                let x = (ms.mouseData >> 16) as u16;
                let ev = match msg {
                    WM_MBUTTONDOWN => Some((VK_MBUTTON, true)),
                    WM_MBUTTONUP => Some((VK_MBUTTON, false)),
                    WM_XBUTTONDOWN => Some((if x == 1 { VK_XBUTTON1 } else { VK_XBUTTON2 }, true)),
                    WM_XBUTTONUP => Some((if x == 1 { VK_XBUTTON1 } else { VK_XBUTTON2 }, false)),
                    _ => None,
                };
                if let Some((vk, down)) = ev {
                    if handle_key(vk, down) {
                        return LRESULT(1);
                    }
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
