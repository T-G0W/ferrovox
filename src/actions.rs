use crate::library::{CategoryId, SoundId};
use crate::settings::{PlayMode, SpecialAction};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Action {
    PlaySound { id: SoundId, mode: Option<PlayMode> },
    PlayIndex { index: usize, mode: Option<PlayMode> },
    PlayCode(String),
    PlayNumcode(u16),
    PlayHotbarSlot { page: usize, slot: usize },
    RandomInCategory { cat: Option<CategoryId>, mode: Option<PlayMode> },
    Special(SpecialAction),
    PlaySelected { mode: Option<PlayMode> },
    PlayTts { text: Option<String>, mode: Option<PlayMode> },
    Stop,
    TogglePause,
    SeekMs(u64),
    SetVolume(f32),
    ToggleMute,
    SetPlayMode(PlayMode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Hotkey,
    Ui,
    Remote,
    Internal,
}
