use crate::hotkey::Hotkey;
use crate::settings::PlayMode;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub type SoundId = u64;
pub type CategoryId = u64;
pub type Rgb = [u8; 3];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Sound {
    pub id: SoundId,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub tag: Option<String>,
    pub duration_ms: u64,
    pub added_on: String,
    pub last_played: Option<String>,
    pub play_count: u32,
    pub hotkey: Option<Hotkey>,
    pub numcode: Option<u16>,
    pub color: Option<Rgb>,
    pub gain_db: f32,
    pub loudness_db: Option<f32>,
    pub play_mode: Option<PlayMode>,
    pub trim_start_ms: u64,
    pub trim_end_ms: u64,
    pub hash: String,
    pub wave: Vec<u8>,
}

impl Default for Sound {
    fn default() -> Self {
        Sound {
            id: 0,
            path: PathBuf::new(),
            title: String::new(),
            artist: String::new(),
            tag: None,
            duration_ms: 0,
            added_on: crate::paths::today(),
            last_played: None,
            play_count: 0,
            hotkey: None,
            numcode: None,
            color: None,
            gain_db: 0.0,
            loudness_db: None,
            play_mode: None,
            trim_start_ms: 0,
            trim_end_ms: 0,
            hash: String::new(),
            wave: Vec::new(),
        }
    }
}

impl Sound {
    pub fn display_name(&self) -> &str {
        if let Some(tag) = self.tag.as_deref().filter(|t| !t.is_empty()) {
            return tag;
        }
        if !self.title.is_empty() {
            return &self.title;
        }
        self.path.file_stem().and_then(|s| s.to_str()).unwrap_or("?")
    }

    pub fn matches(&self, needle_lower: &str) -> bool {
        if needle_lower.is_empty() {
            return true;
        }
        let hay = format!(
            "{} {} {} {}",
            self.display_name(),
            self.title,
            self.artist,
            self.path.to_string_lossy()
        )
        .to_lowercase();
        needle_lower.split_whitespace().all(|w| hay.contains(w))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub icon: String,
    pub color: Option<Rgb>,
    pub hidden: bool,
    pub index: Option<u8>,
    pub sounds: Vec<SoundId>,
    pub random_hotkey: Option<Hotkey>,
}

impl Default for Category {
    fn default() -> Self {
        Category {
            id: 0,
            name: String::new(),
            icon: "📁".into(),
            color: None,
            hidden: false,
            index: None,
            sounds: Vec::new(),
            random_hotkey: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HotbarSlot {
    pub sound: SoundId,
    #[serde(default)]
    pub color: Option<Rgb>,
    #[serde(default)]
    pub hotkey: Option<Hotkey>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HotbarPage {
    pub title: String,
    pub color: Option<Rgb>,
    pub slots: Vec<Option<HotbarSlot>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotbar {
    pub pages: Vec<HotbarPage>,
    pub selected: usize,
}

impl Default for Hotbar {
    fn default() -> Self {
        Hotbar { pages: vec![HotbarPage::default()], selected: 0 }
    }
}

impl Hotbar {
    pub fn page_mut(&mut self, index: usize, slots: usize) -> &mut HotbarPage {
        while self.pages.len() <= index {
            self.pages.push(HotbarPage::default());
        }
        let page = &mut self.pages[index];
        if page.slots.len() < slots {
            page.slots.resize(slots, None);
        }
        page
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub version: u32,
    pub sounds: Vec<Sound>,
    pub categories: Vec<Category>,
    pub hotbar: Hotbar,
    pub next_id: u64,
}

impl Default for Library {
    fn default() -> Self {
        Library {
            version: 1,
            sounds: Vec::new(),
            categories: vec![Category {
                id: 1,
                name: crate::t!("Meine Sounds", "My Sounds").into(),
                icon: "⭐".into(),
                ..Default::default()
            }],
            hotbar: Hotbar::default(),
            next_id: 2,
        }
    }
}

#[derive(Clone)]
pub struct Snapshot {
    pub sounds: Vec<Sound>,
    pub categories: Vec<Category>,
    pub hotbar: Hotbar,
}

impl Library {
    pub fn load(path: &Path) -> Library {
        match std::fs::read_to_string(path) {
            Ok(txt) => match serde_json::from_str::<Library>(&txt) {
                Ok(mut lib) => {
                    lib.fix_ids();
                    lib
                }
                Err(e) => {
                    log::error!("Soundliste {path:?} beschädigt: {e}");
                    let _ = std::fs::copy(path, path.with_extension("broken.json"));
                    Library::default()
                }
            },
            Err(_) => Library::default(),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let txt = serde_json::to_string_pretty(self)?;
        write_atomic(path, txt.as_bytes())
    }

    pub fn backup_daily(&self) {
        let dir = crate::paths::backups_dir();
        let file = dir.join(format!("library-{}.json", crate::paths::today()));
        if !file.exists() {
            let _ = self.save(&file);
        }
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            while files.len() > 14 {
                let _ = std::fs::remove_file(files.remove(0));
            }
        }
    }

    fn fix_ids(&mut self) {
        let max_sound = self.sounds.iter().map(|s| s.id).max().unwrap_or(0);
        let max_cat = self.categories.iter().map(|c| c.id).max().unwrap_or(0);
        self.next_id = self.next_id.max(max_sound.max(max_cat) + 1);
        let ids: HashSet<SoundId> = self.sounds.iter().map(|s| s.id).collect();
        for c in &mut self.categories {
            c.sounds.retain(|id| ids.contains(id));
        }
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            sounds: self.sounds.clone(),
            categories: self.categories.clone(),
            hotbar: self.hotbar.clone(),
        }
    }

    pub fn restore(&mut self, snap: Snapshot) {
        self.sounds = snap.sounds;
        self.categories = snap.categories;
        self.hotbar = snap.hotbar;
        self.fix_ids();
    }

    pub fn sound(&self, id: SoundId) -> Option<&Sound> {
        self.sounds.iter().find(|s| s.id == id)
    }

    pub fn sound_mut(&mut self, id: SoundId) -> Option<&mut Sound> {
        self.sounds.iter_mut().find(|s| s.id == id)
    }

    pub fn by_index(&self, index: usize) -> Option<&Sound> {
        index.checked_sub(1).and_then(|i| self.sounds.get(i))
    }

    pub fn category(&self, id: CategoryId) -> Option<&Category> {
        self.categories.iter().find(|c| c.id == id)
    }

    pub fn category_mut(&mut self, id: CategoryId) -> Option<&mut Category> {
        self.categories.iter_mut().find(|c| c.id == id)
    }

    pub fn contains_path(&self, path: &Path) -> Option<SoundId> {
        self.sounds.iter().find(|s| s.path == path).map(|s| s.id)
    }

    pub fn add_sound(&mut self, mut sound: Sound, category: Option<CategoryId>, at_top: bool) -> SoundId {
        let id = match self.contains_path(&sound.path) {
            Some(id) => id,
            None => {
                sound.id = self.alloc_id();
                let id = sound.id;
                if at_top {
                    self.sounds.insert(0, sound);
                } else {
                    self.sounds.push(sound);
                }
                id
            }
        };
        if let Some(cat) = category.and_then(|c| self.category_mut(c)) {
            if !cat.sounds.contains(&id) {
                if at_top {
                    cat.sounds.insert(0, id);
                } else {
                    cat.sounds.push(id);
                }
            }
        }
        id
    }

    pub fn remove_sounds(&mut self, ids: &[SoundId]) {
        let set: HashSet<SoundId> = ids.iter().copied().collect();
        self.sounds.retain(|s| !set.contains(&s.id));
        for c in &mut self.categories {
            c.sounds.retain(|id| !set.contains(id));
        }
        for page in &mut self.hotbar.pages {
            for slot in &mut page.slots {
                if slot.as_ref().is_some_and(|s| set.contains(&s.sound)) {
                    *slot = None;
                }
            }
        }
    }

    pub fn remove_from_category(&mut self, cat: CategoryId, ids: &[SoundId]) {
        if let Some(c) = self.category_mut(cat) {
            c.sounds.retain(|id| !ids.contains(id));
        }
    }

    pub fn view(&self, category: Option<CategoryId>) -> Vec<SoundId> {
        match category.and_then(|c| self.category(c)) {
            Some(c) => c.sounds.clone(),
            None => self.sounds.iter().map(|s| s.id).collect(),
        }
    }

    pub fn dead_entries(&self) -> Vec<SoundId> {
        self.sounds.iter().filter(|s| !s.path.exists()).map(|s| s.id).collect()
    }

    pub fn hotkey_owner(&self, hk: &Hotkey, distinguish: bool) -> Option<String> {
        let key = hk.match_key(distinguish);
        for s in &self.sounds {
            if s.hotkey.is_some_and(|h| h.match_key(distinguish) == key) {
                return Some(format!("Sound {}", crate::i18n::q(s.display_name())));
            }
        }
        for c in &self.categories {
            if c.random_hotkey.is_some_and(|h| h.match_key(distinguish) == key) {
                return Some(format!("{} {}", crate::t!("Kategorie", "Category"), crate::i18n::q(&c.name)));
            }
        }
        for (pi, page) in self.hotbar.pages.iter().enumerate() {
            for (si, slot) in page.slots.iter().enumerate() {
                if slot.as_ref().and_then(|s| s.hotkey).is_some_and(|h| h.match_key(distinguish) == key) {
                    return Some(format!("Hotbar {}/{}", pi + 1, si + 1));
                }
            }
        }
        None
    }

    pub fn clear_hotkey(&mut self, hk: &Hotkey, distinguish: bool) {
        let key = hk.match_key(distinguish);
        let same = |h: &Option<Hotkey>| h.is_some_and(|h| h.match_key(distinguish) == key);
        for s in &mut self.sounds {
            if same(&s.hotkey) {
                s.hotkey = None;
            }
        }
        for c in &mut self.categories {
            if same(&c.random_hotkey) {
                c.random_hotkey = None;
            }
        }
        for page in &mut self.hotbar.pages {
            for slot in page.slots.iter_mut().flatten() {
                if same(&slot.hotkey) {
                    slot.hotkey = None;
                }
            }
        }
    }

    pub fn category_by_index(&self, index: u8) -> Option<&Category> {
        self.categories.iter().find(|c| c.index == Some(index))
    }
}

pub fn write_atomic(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn format_duration(ms: u64) -> String {
    let s = if ms > 0 { ((ms + 500) / 1000).max(1) } else { 0 };
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn file_hash(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().to_lowercase().hash(&mut h);
    format!("fv-{size}-{:x}", h.finish())
}
