use crate::hotkey::{VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub const INJECT_TAG: usize = 0x4645_5252;

pub fn is_dangerous(vk: u16) -> bool {
    crate::hotkey::is_modifier_vk(vk)
}

fn is_extended(vk: u16) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0x5C | 0x5D | 0x6F | 0x90 | 0xA3 | 0xA5)
}

pub fn send_key(vk: u16, down: bool) {
    let input = match vk {
        VK_MBUTTON | VK_XBUTTON1 | VK_XBUTTON2 => {
            let (flags, data) = match (vk, down) {
                (VK_MBUTTON, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                (VK_MBUTTON, false) => (MOUSEEVENTF_MIDDLEUP, 0),
                (VK_XBUTTON1, true) => (MOUSEEVENTF_XDOWN, 1),
                (VK_XBUTTON1, false) => (MOUSEEVENTF_XUP, 1),
                (_, true) => (MOUSEEVENTF_XDOWN, 2),
                (_, false) => (MOUSEEVENTF_XUP, 2),
            };
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: INJECT_TAG },
                },
            }
        }
        _ => {
            let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
            let mut flags = if scan != 0 { KEYEVENTF_SCANCODE } else { KEYBD_EVENT_FLAGS(0) };
            if is_extended(vk) {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if !down {
                flags |= KEYEVENTF_KEYUP;
            }
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: scan,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: INJECT_TAG,
                    },
                },
            }
        }
    };
    unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn press_all(keys: &[u16], down: bool) {
    if down {
        keys.iter().for_each(|&k| send_key(k, true));
    } else {
        keys.iter().rev().for_each(|&k| send_key(k, false));
    }
}

pub fn tap_all(keys: &[u16]) {
    press_all(keys, true);
    std::thread::sleep(std::time::Duration::from_millis(30));
    press_all(keys, false);
}
