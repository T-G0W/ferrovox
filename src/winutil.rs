use std::collections::HashSet;
use std::ffi::c_void;
use std::path::Path;
use windows::core::{w, HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
    SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL,
};

pub fn hwnd_from(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

pub fn show_window(raw: isize) {
    if raw == 0 {
        return;
    }
    let h = hwnd_from(raw);
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        } else {
            let _ = ShowWindow(h, SW_SHOW);
        }
        let _ = SetForegroundWindow(h);
    }
}

pub fn style_titlebar(raw: isize, dark: bool, caption: [u8; 3]) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
    if raw == 0 {
        return;
    }
    let h = hwnd_from(raw);
    let dark_flag: i32 = dark as i32;
    let color: u32 = caption[0] as u32 | (caption[1] as u32) << 8 | (caption[2] as u32) << 16;
    unsafe {
        let _ = DwmSetWindowAttribute(h, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark_flag as *const i32 as *const c_void, 4);
        let _ = DwmSetWindowAttribute(h, DWMWA_CAPTION_COLOR, &color as *const u32 as *const c_void, 4);
    }
}

pub fn hide_window(raw: isize) {
    if raw != 0 {
        unsafe {
            let _ = ShowWindow(hwnd_from(raw), SW_HIDE);
        }
    }
}

fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        Path::new(&full).file_name().map(|n| n.to_string_lossy().to_lowercase())
    }
}

pub fn foreground_exe() -> Option<String> {
    unsafe {
        let h = GetForegroundWindow();
        if h.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        process_name(pid)
    }
}

pub fn running_exes() -> HashSet<String> {
    let mut out = HashSet::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                out.insert(String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase());
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

pub fn running_exe_list() -> Vec<String> {
    let mut v: Vec<String> = running_exes().into_iter().filter(|n| n.ends_with(".exe")).collect();
    v.sort();
    v
}

pub fn foreground_is_fullscreen() -> bool {
    unsafe {
        let h = GetForegroundWindow();
        if h.0.is_null() || h == GetDesktopWindow() || h == GetShellWindow() {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

pub fn open_url(url: &str) {
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(url), None, None, SW_SHOWNORMAL);
    }
}

pub fn show_in_explorer(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
}

pub fn open_folder(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_NAME: windows::core::PCWSTR = w!("Ferrovox");

pub fn autostart_enabled() -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_QUERY_VALUE, &mut key).is_err() {
            return false;
        }
        let ok = RegQueryValueExW(key, RUN_NAME, None, None, None, None).is_ok();
        let _ = RegCloseKey(key);
        ok
    }
}

pub fn set_autostart(enabled: bool) -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_SET_VALUE, &mut key).is_err() {
            return false;
        }
        let ok = if enabled {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
            let value = format!("\"{exe}\" --minimized");
            let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(key, RUN_NAME, 0, REG_SZ, Some(bytes)).is_ok()
        } else {
            RegDeleteValueW(key, RUN_NAME).is_ok()
        };
        let _ = RegCloseKey(key);
        ok
    }
}

pub fn another_instance_running() -> bool {
    unsafe {
        let handle = CreateMutexW(None, true, w!("Local\\FerrovoxSingleInstance"));
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        std::mem::forget(handle);
        exists
    }
}

static INACTIVE_CLOSE: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();
static ORIGINAL_WNDPROC: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

pub fn on_close_while_inactive(raw: isize, handler: impl Fn() + Send + Sync + 'static) {
    use windows::Win32::UI::WindowsAndMessaging::GWLP_WNDPROC;
    if raw == 0 || INACTIVE_CLOSE.set(Box::new(handler)).is_err() {
        return;
    }
    let proc_addr = inactive_close_proc as *const () as usize;
    unsafe {
        #[cfg(target_pointer_width = "64")]
        let old = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd_from(raw), GWLP_WNDPROC, proc_addr as isize);
        #[cfg(target_pointer_width = "32")]
        let old = windows::Win32::UI::WindowsAndMessaging::SetWindowLongW(hwnd_from(raw), GWLP_WNDPROC, proc_addr as i32) as isize;
        ORIGINAL_WNDPROC.store(old, std::sync::atomic::Ordering::SeqCst);
    }
}

unsafe extern "system" fn inactive_close_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{CallWindowProcW, IsWindowVisible, WM_CLOSE, WNDPROC};
    if msg == WM_CLOSE && (IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool()) {
        if let Some(handler) = INACTIVE_CLOSE.get() {
            handler();
            return windows::Win32::Foundation::LRESULT(0);
        }
    }
    let original: WNDPROC = std::mem::transmute(ORIGINAL_WNDPROC.load(std::sync::atomic::Ordering::SeqCst));
    CallWindowProcW(original, hwnd, msg, wparam, lparam)
}
