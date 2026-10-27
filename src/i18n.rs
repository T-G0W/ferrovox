use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

const DE: u8 = 0;
const EN: u8 = 1;
static LANG: AtomicU8 = AtomicU8::new(DE);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    Auto,
    De,
    En,
}

impl Language {
    pub fn label(self) -> String {
        match self {
            Language::Auto => format!("{} ({})", crate::t!("Automatisch", "Automatic"), system_language().label()),
            Language::De => "Deutsch".into(),
            Language::En => "English".into(),
        }
    }

    pub fn resolve(self) -> Language {
        match self {
            Language::Auto => system_language(),
            other => other,
        }
    }
}

pub fn system_language() -> Language {
    static SYS: OnceLock<Language> = OnceLock::new();
    *SYS.get_or_init(|| {
        let langid = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        if langid & 0x3FF == 0x07 {
            Language::De
        } else {
            Language::En
        }
    })
}

#[cfg(test)]
pub static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub fn set_language(lang: Language) {
    LANG.store(if lang.resolve() == Language::En { EN } else { DE }, Ordering::Relaxed);
}

pub fn is_en() -> bool {
    LANG.load(Ordering::Relaxed) == EN
}

pub fn q(s: &str) -> String {
    if is_en() {
        format!("\"{s}\"")
    } else {
        format!("„{s}“")
    }
}

#[macro_export]
macro_rules! t {
    ($de:expr, $en:expr) => {
        if $crate::i18n::is_en() {
            $en
        } else {
            $de
        }
    };
}
