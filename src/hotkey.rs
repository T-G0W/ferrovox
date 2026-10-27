use serde::{Deserialize, Serialize};

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_WIN: u8 = 8;

pub const SIDE_CTRL: u8 = 1;
pub const SIDE_ALT: u8 = 2;
pub const SIDE_SHIFT: u8 = 4;
pub const SIDE_WIN: u8 = 8;

pub const VK_MBUTTON: u16 = 0x04;
pub const VK_XBUTTON1: u16 = 0x05;
pub const VK_XBUTTON2: u16 = 0x06;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hotkey {
    pub vk: u16,
    #[serde(default)]
    pub mods: u8,
    #[serde(default)]
    pub side: u8,
}

impl Hotkey {
    pub fn new(vk: u16, mods: u8, side: u8) -> Self {
        Hotkey { vk, mods, side: side & mods }
    }

    pub fn match_key(&self, distinguish_sides: bool) -> u32 {
        let side = if distinguish_sides { self.side & self.mods } else { 0 };
        self.vk as u32 | (self.mods as u32) << 16 | (side as u32) << 24
    }

    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let side = |bit: u8| if self.side & bit != 0 { "R" } else { "" };
        if self.mods & MOD_CTRL != 0 {
            parts.push(format!("{}{}", side(SIDE_CTRL), crate::t!("Strg", "Ctrl")));
        }
        if self.mods & MOD_ALT != 0 {
            parts.push(format!("{}Alt", side(SIDE_ALT)));
        }
        if self.mods & MOD_SHIFT != 0 {
            parts.push(format!("{}{}", side(SIDE_SHIFT), crate::t!("Umschalt", "Shift")));
        }
        if self.mods & MOD_WIN != 0 {
            parts.push(format!("{}Win", side(SIDE_WIN)));
        }
        parts.push(vk_name(self.vk));
        parts.join("+")
    }
}

pub fn mods_label(mods: u8) -> String {
    if mods == 0 {
        return crate::t!("(keiner)", "(none)").to_string();
    }
    let mut parts = Vec::new();
    if mods & MOD_CTRL != 0 {
        parts.push(crate::t!("Strg", "Ctrl"));
    }
    if mods & MOD_ALT != 0 {
        parts.push("Alt");
    }
    if mods & MOD_SHIFT != 0 {
        parts.push(crate::t!("Umschalt", "Shift"));
    }
    if mods & MOD_WIN != 0 {
        parts.push("Win");
    }
    parts.join("+")
}

pub fn is_modifier_vk(vk: u16) -> bool {
    matches!(vk, 0x10..=0x12 | 0xA0..=0xA5 | 0x5B | 0x5C)
}

pub fn digit_of(vk: u16) -> Option<char> {
    match vk {
        0x30..=0x39 => Some((b'0' + (vk - 0x30) as u8) as char),
        0x60..=0x69 => Some((b'0' + (vk - 0x60) as u8) as char),
        _ => None,
    }
}

pub fn vk_name(vk: u16) -> String {
    let fixed = match vk {
        0x01 => crate::t!("Maus links", "Mouse left"),
        0x02 => crate::t!("Maus rechts", "Mouse right"),
        VK_MBUTTON => crate::t!("Maus Mitte", "Mouse middle"),
        VK_XBUTTON1 => crate::t!("Maus 4", "Mouse 4"),
        VK_XBUTTON2 => crate::t!("Maus 5", "Mouse 5"),
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Esc",
        0x20 => crate::t!("Leertaste", "Space"),
        0x21 => crate::t!("Bild↑", "PgUp"),
        0x22 => crate::t!("Bild↓", "PgDn"),
        0x23 => crate::t!("Ende", "End"),
        0x24 => crate::t!("Pos1", "Home"),
        0x25 => "←",
        0x26 => "↑",
        0x27 => "→",
        0x28 => "↓",
        0x2C => crate::t!("Druck", "PrtSc"),
        0x2D => crate::t!("Einfg", "Ins"),
        0x2E => crate::t!("Entf", "Del"),
        0x5D => crate::t!("Menü", "Menu"),
        0x6A => "Num *",
        0x6B => "Num +",
        0x6D => "Num -",
        0x6E => "Num ,",
        0x6F => "Num /",
        0x90 => "NumLock",
        0x91 => crate::t!("Rollen", "ScrLk"),
        0xAD => crate::t!("Stumm", "Mute"),
        0xAE => crate::t!("Leiser", "Vol-"),
        0xAF => crate::t!("Lauter", "Vol+"),
        0xB0 => crate::t!("Nächster Titel", "Next track"),
        0xB1 => crate::t!("Vorheriger Titel", "Prev track"),
        0xB2 => crate::t!("Medien Stopp", "Media stop"),
        0xB3 => crate::t!("Play/Pause", "Play/Pause"),
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => key_name_from_os(vk).unwrap_or_else(|| format!("VK{vk:02X}")),
    }
}

fn key_name_from_os(vk: u16) -> Option<String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyNameTextW, MapVirtualKeyW, MAPVK_VK_TO_VSC};
    unsafe {
        let scan = MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC);
        if scan == 0 {
            return None;
        }
        let mut buf = [0u16; 64];
        let len = GetKeyNameTextW((scan << 16) as i32, &mut buf);
        (len > 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}
