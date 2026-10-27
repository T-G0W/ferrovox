#![allow(non_snake_case)]

use std::ffi::c_void;
use windows::core::{interface, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{eCapture, eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};

#[interface("F8679F50-850A-41CF-9C72-430F290290C8")]
unsafe trait IPolicyConfig: windows::core::IUnknown {
    fn GetMixFormat(&self, id: PCWSTR, fmt: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, id: PCWSTR, default: i32, fmt: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, def_period: *mut i64, min_period: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: i32) -> HRESULT;
    fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
}

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
}

fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

unsafe fn endpoint(dev: &IMMDevice) -> windows::core::Result<Endpoint> {
    let raw: PWSTR = dev.GetId()?;
    let id = raw.to_string().unwrap_or_default();
    CoTaskMemFree(Some(raw.0 as *const c_void));
    let store = dev.OpenPropertyStore(STGM_READ)?;
    let name = store.GetValue(&PKEY_Device_FriendlyName)?.to_string();
    Ok(Endpoint { id, name })
}

pub fn capture_endpoints() -> Vec<Endpoint> {
    init();
    let mut out = Vec::new();
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return out };
        let Ok(coll) = en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else { return out };
        for i in 0..coll.GetCount().unwrap_or(0) {
            if let Ok(dev) = coll.Item(i) {
                if let Ok(ep) = endpoint(&dev) {
                    out.push(ep);
                }
            }
        }
    }
    out
}

pub fn default_capture() -> Option<Endpoint> {
    init();
    unsafe {
        let en = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(eCapture, eConsole).ok()?;
        endpoint(&dev).ok()
    }
}

pub fn default_render() -> Option<Endpoint> {
    init();
    unsafe {
        let en = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        endpoint(&dev).ok()
    }
}

pub fn set_default_endpoint(id: &str) -> anyhow::Result<()> {
    set_default_capture(id)
}

pub fn set_default_capture(id: &str) -> anyhow::Result<()> {
    init();
    unsafe {
        let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)?;
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        for role in 0..3 {
            policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok()?;
        }
    }
    Ok(())
}

pub fn is_virtual_capture(name: &str) -> bool {
    let n = name.to_lowercase();
    ["cable output", "vb-audio", "voicemeeter out", "virtual"].iter().any(|h| n.contains(h))
}

pub fn is_virtual_render(name: &str) -> bool {
    let n = name.to_lowercase();
    ["cable input", "cable in ", "vb-audio", "voicemeeter", "virtual"].iter().any(|h| n.contains(h))
}
