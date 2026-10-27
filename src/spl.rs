use crate::hotkey::{Hotkey, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN};
use crate::library::{Category, HotbarSlot, Library, Sound};
use std::collections::HashMap;
use std::path::Path;

pub struct ImportReport {
    pub sounds: usize,
    pub categories: usize,
    pub stats_merged: usize,
    pub missing_files: usize,
}

fn parse_duration(s: &str) -> u64 {
    let mut total = 0u64;
    for part in s.split(':') {
        total = total * 60 + part.trim().parse::<u64>().unwrap_or(0);
    }
    total * 1000
}

fn win_mods_to_ours(m: u32) -> u8 {
    let mut out = 0;
    if m & 1 != 0 {
        out |= MOD_ALT;
    }
    if m & 2 != 0 {
        out |= MOD_CTRL;
    }
    if m & 4 != 0 {
        out |= MOD_SHIFT;
    }
    if m & 8 != 0 {
        out |= MOD_WIN;
    }
    out
}

fn ours_to_win_mods(m: u8) -> u32 {
    let mut out = 0;
    if m & MOD_ALT != 0 {
        out |= 1;
    }
    if m & MOD_CTRL != 0 {
        out |= 2;
    }
    if m & MOD_SHIFT != 0 {
        out |= 4;
    }
    if m & MOD_WIN != 0 {
        out |= 8;
    }
    out
}

fn attr_u32(node: &roxmltree::Node, name: &str) -> Option<u32> {
    node.attribute(name).and_then(|v| v.trim().parse().ok())
}

pub fn import(path: &Path) -> anyhow::Result<(Library, ImportReport)> {
    let raw = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    anyhow::ensure!(root.has_tag_name("Soundlist"), "{}", crate::t!("Keine Soundpad-Soundliste", "Not a Soundpad sound list"));

    let mut lib = Library { categories: Vec::new(), ..Default::default() };
    let mut order: Vec<u64> = Vec::new();
    let mut missing = 0;

    for node in root.children().filter(|n| n.has_tag_name("Sound")) {
        let url = node.attribute("url").unwrap_or_default();
        if url.is_empty() {
            continue;
        }
        let path = std::path::PathBuf::from(url);
        if !path.exists() {
            missing += 1;
        }
        let hotkey = attr_u32(&node, "hotkeyKey").filter(|k| *k != 0).map(|vk| {
            Hotkey::new(vk as u16, win_mods_to_ours(attr_u32(&node, "hotkeyKeyModifiers").unwrap_or(0)), 0)
        });
        let sound = Sound {
            title: node.attribute("title").unwrap_or_default().to_string(),
            artist: node.attribute("artist").unwrap_or_default().to_string(),
            tag: node.attribute("tag").map(str::to_string),
            duration_ms: node.attribute("duration").map(parse_duration).unwrap_or(0),
            added_on: node.attribute("addedOn").map(str::to_string).unwrap_or_else(crate::paths::today),
            last_played: node.attribute("lastPlayedOn").map(str::to_string),
            play_count: attr_u32(&node, "playCount").unwrap_or(0),
            numcode: attr_u32(&node, "hotkeyNumCode").map(|n| n as u16),
            hash: node.attribute("hash").unwrap_or_default().to_string(),
            hotkey,
            path,
            ..Default::default()
        };
        let id = lib.add_sound(sound, None, false);
        order.push(id);
    }

    fn walk(node: roxmltree::Node, lib: &mut Library, order: &[u64], prefix: &str) {
        for cat in node.children().filter(|n| n.has_tag_name("Category")) {
            if cat.attribute("type") == Some("1") {
                continue;
            }
            let name = cat.attribute("name").unwrap_or(crate::t!("Kategorie", "Category"));
            let full = if prefix.is_empty() { name.to_string() } else { format!("{prefix} / {name}") };
            let id = lib.alloc_id();
            let sounds = cat
                .children()
                .filter(|n| n.has_tag_name("Sound"))
                .filter_map(|n| attr_u32(&n, "id"))
                .filter_map(|i| order.get(i as usize).copied())
                .collect();
            let icon = match cat.attribute("icon") {
                Some(i) if i.contains("home") => "⭐",
                _ => "📁",
            };
            lib.categories.push(Category {
                id,
                name: full.clone(),
                icon: icon.into(),
                hidden: cat.attribute("hidden") == Some("true"),
                index: attr_u32(&cat, "index").map(|i| i as u8),
                sounds,
                ..Default::default()
            });
            walk(cat, lib, order, &full);
        }
    }
    if let Some(cats) = root.children().find(|n| n.has_tag_name("Categories")) {
        walk(cats, &mut lib, &order, "");
    }

    if let Some(hotbar) = root.children().find(|n| n.has_tag_name("Hotbar")) {
        let mut pages = Vec::new();
        for page in hotbar.children().filter(|n| n.is_element()) {
            let mut p = crate::library::HotbarPage {
                title: page.attribute("title").unwrap_or_default().to_string(),
                ..Default::default()
            };
            for item in page.children().filter(|n| n.is_element()) {
                let slot = attr_u32(&item, "id")
                    .and_then(|i| order.get(i as usize).copied())
                    .map(|sound| HotbarSlot { sound, color: None, hotkey: None });
                p.slots.push(slot);
            }
            pages.push(p);
        }
        if !pages.is_empty() {
            lib.hotbar.pages = pages;
        }
    }

    if lib.categories.is_empty() {
        lib.categories = Library::default().categories;
        lib.next_id = lib.next_id.max(2);
    }

    let mut merged = 0;
    let stats_path = path.with_file_name("stats.spl");
    if let Ok(raw) = std::fs::read(&stats_path) {
        let text = String::from_utf8_lossy(&raw).into_owned();
        if let Ok(doc) = roxmltree::Document::parse(&text) {
            let mut stats: HashMap<String, (u32, Option<String>)> = HashMap::new();
            for n in doc.root_element().children().filter(|n| n.has_tag_name("Sound")) {
                if let Some(hash) = n.attribute("hash") {
                    stats.insert(
                        hash.to_string(),
                        (attr_u32(&n, "playCount").unwrap_or(0), n.attribute("lastPlayedOn").map(str::to_string)),
                    );
                }
            }
            for s in &mut lib.sounds {
                if let Some((count, last)) = stats.get(&s.hash) {
                    s.play_count = s.play_count.max(*count);
                    if last.is_some() {
                        s.last_played = last.clone();
                    }
                    merged += 1;
                }
            }
        }
    }

    let report = ImportReport {
        sounds: lib.sounds.len(),
        categories: lib.categories.len(),
        stats_merged: merged,
        missing_files: missing,
    };
    Ok((lib, report))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn sound_xml(s: &Sound, index: Option<usize>) -> String {
    let mut x = String::from("  <Sound");
    if let Some(i) = index {
        x += &format!(" index=\"{i}\"");
    }
    x += &format!(
        " url=\"{}\" artist=\"{}\" title=\"{}\" duration=\"{}\" addedOn=\"{}\"",
        esc(&s.path.to_string_lossy()),
        esc(&s.artist),
        esc(s.display_name()),
        crate::library::format_duration(s.duration_ms),
        esc(&s.added_on)
    );
    if !s.hash.is_empty() {
        x += &format!(" hash=\"{}\"", esc(&s.hash));
    }
    if let Some(t) = &s.tag {
        x += &format!(" tag=\"{}\"", esc(t));
    }
    if let Some(l) = &s.last_played {
        x += &format!(" lastPlayedOn=\"{}\"", esc(l));
    }
    if s.play_count > 0 {
        x += &format!(" playCount=\"{}\"", s.play_count);
    }
    if let Some(hk) = s.hotkey {
        x += &format!(" hotkeyKey=\"{}\" hotkeyKeyModifiers=\"{}\"", hk.vk, ours_to_win_mods(hk.mods));
    }
    if let Some(n) = s.numcode {
        x += &format!(" hotkeyNumCode=\"{n}\"");
    }
    x += "/>\n";
    x
}

pub fn to_xml(lib: &Library, with_index: bool, range: Option<(usize, usize)>) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Soundlist>\n");
    let (from, to) = range.unwrap_or((1, lib.sounds.len()));
    for (i, s) in lib.sounds.iter().enumerate() {
        let idx = i + 1;
        if idx < from || idx > to {
            continue;
        }
        out += &sound_xml(s, with_index.then_some(idx));
    }
    if range.is_none() {
        let pos: HashMap<u64, usize> = lib.sounds.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        out += "  <Categories>\n";
        for c in &lib.categories {
            out += &format!(
                "    <Category name=\"{}\"{}{}>\n",
                esc(&c.name),
                if c.hidden { " hidden=\"true\"" } else { "" },
                c.index.map(|i| format!(" index=\"{i}\"")).unwrap_or_default()
            );
            for id in &c.sounds {
                if let Some(p) = pos.get(id) {
                    out += &format!("      <Sound id=\"{p}\"/>\n");
                }
            }
            out += "    </Category>\n";
        }
        out += "  </Categories>\n";
    }
    out += "</Soundlist>\n";
    out
}

pub fn categories_xml(lib: &Library, only: Option<usize>) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Categories>\n");
    for (i, c) in lib.categories.iter().enumerate() {
        if only.is_some_and(|o| o != i + 1) {
            continue;
        }
        out += &format!(
            "  <Category index=\"{}\" name=\"{}\" hidden=\"{}\" sounds=\"{}\"/>\n",
            i + 1,
            esc(&c.name),
            c.hidden,
            c.sounds.len()
        );
    }
    out += "</Categories>\n";
    out
}
