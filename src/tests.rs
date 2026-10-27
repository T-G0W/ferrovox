use crate::audio::buffer::{self, SoundBuffer};
use crate::audio::engine::EngineShared;
use crate::audio::mixer::{MixCmd, MixEvent, Mixer, OutKind, VoiceSpec};
use crate::hotkey::{Hotkey, MOD_ALT, MOD_CTRL, SIDE_CTRL};
use crate::library::{Library, Sound};
use std::sync::Arc;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrovox-test-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn sine(rate: u32, ms: u32, level: f32) -> Vec<f32> {
    let n = (rate * ms / 1000) as usize;
    (0..n)
        .flat_map(|i| {
            let v = (i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * level;
            [v, v]
        })
        .collect()
}

#[test]
fn mixer_plays_voice_and_reports_finish() {
    let shared = Arc::new(EngineShared::default());
    let (mut prod, cons) = rtrb::RingBuffer::<MixCmd>::new(16);
    let (ev_prod, mut ev_cons) = rtrb::RingBuffer::<MixEvent>::new(16);
    let mut mixer = Mixer::new(OutKind::Speakers, 48_000, cons, ev_prod, shared.clone(), None, 1.0, false, 1.0);
    let buf = Arc::new(SoundBuffer::new(sine(48_000, 50, 0.5), 48_000));
    let frames = buf.frames();
    assert!(prod
        .push(MixCmd::Play(Box::new(VoiceSpec {
            id: 7,
            buf,
            gain: 1.0,
            start: 0,
            end: frames,
            delay_ms: 0,
            looped: false,
            primary: true,
            notify: false
        })))
        .is_ok());
    let mut out = vec![0.0f32; 1024 * 2];
    mixer.render(&mut out);
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.3, "Mixer gibt keinen Ton aus (peak={peak})");
    assert_eq!(shared.pos_voice.load(std::sync::atomic::Ordering::Relaxed), 7);
    for _ in 0..10 {
        mixer.render(&mut out);
    }
    match ev_cons.pop() {
        Ok(MixEvent::Finished { id, notify, .. }) => {
            assert_eq!(id, 7);
            assert!(!notify);
        }
        _ => panic!("Kein Finished-Ereignis"),
    }
}

#[test]
fn mixer_resamples_and_respects_master_and_delay() {
    let shared = Arc::new(EngineShared::default());
    let (mut prod, cons) = rtrb::RingBuffer::<MixCmd>::new(16);
    let (ev_prod, _ev) = rtrb::RingBuffer::<MixEvent>::new(16);
    let mut mixer = Mixer::new(OutKind::Mic, 48_000, cons, ev_prod, shared, None, 0.0, false, 1.0);
    let buf = Arc::new(SoundBuffer::new(sine(44_100, 200, 0.5), 44_100));
    let frames = buf.frames();
    let _ = prod.push(MixCmd::Play(Box::new(VoiceSpec {
        id: 1,
        buf,
        gain: 1.0,
        start: 0,
        end: frames,
        delay_ms: 10,
        looped: false,
        primary: true,
        notify: false,
    })));
    let mut out = vec![0.0f32; 480 * 2];
    mixer.render(&mut out);
    assert!(out.iter().all(|s| *s == 0.0), "Master 0 muss stumm sein");
    let _ = prod.push(MixCmd::Master(1.0));
    let mut out = vec![0.0f32; 4800 * 2];
    mixer.render(&mut out);
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.3);
}

#[test]
fn wav_roundtrip_and_decode() {
    let dir = temp_dir("wav");
    let path = dir.join("test.wav");
    let samples = sine(44_100, 500, 0.5);
    crate::audio::wav::write_wav(&path, &samples, 44_100).unwrap();
    let buf = crate::audio::decode::decode_file(&path).unwrap();
    assert_eq!(buf.rate, 44_100);
    assert!((buf.duration_ms() as i64 - 500).abs() <= 2);
    assert!((buf.loudness_db + 9.03).abs() < 0.6, "loudness {}", buf.loudness_db);
    let meta = crate::audio::decode::probe_meta(&path).unwrap();
    assert!((meta.duration_ms as i64 - 500).abs() <= 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dsp_trim_normalize_reverse() {
    let rate = 48_000;
    let mut s = vec![0.0f32; 4800 * 2];
    s.extend(sine(rate, 100, 0.25));
    s.extend(vec![0.0f32; 4800 * 2]);
    buffer::trim_silence(&mut s, -50.0, rate);
    let frames = s.len() / 2;
    assert!(frames < 4800 + 3000 && frames > 4000, "trim lieferte {frames} Frames");
    buffer::normalize_to(&mut s, rate, -12.0);
    let loud = buffer::gated_loudness(&s, rate);
    assert!((loud + 12.0).abs() < 1.0, "normalize {loud}");
    let mut r = vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0];
    buffer::reverse(&mut r, 0, 3);
    assert_eq!(r, vec![3.0, 3.0, 2.0, 2.0, 1.0, 1.0]);
}

#[test]
fn hotkey_matching_and_labels() {
    let a = Hotkey::new(0x70, MOD_CTRL, SIDE_CTRL);
    let b = Hotkey::new(0x70, MOD_CTRL, 0);
    assert_eq!(a.match_key(false), b.match_key(false));
    assert_ne!(a.match_key(true), b.match_key(true));
    let _lang = crate::i18n::TEST_LOCK.lock();
    crate::i18n::set_language(crate::i18n::Language::De);
    assert_eq!(Hotkey::new(0x70, MOD_CTRL | MOD_ALT, 0).label(), "Strg+Alt+F1");
    assert_eq!(Hotkey::new(0x05, 0, 0).label(), "Maus 4");
}

#[test]
fn library_add_remove_hotkeys() {
    let mut lib = Library::default();
    let cat = lib.categories[0].id;
    let id = lib.add_sound(Sound { path: "C:/x/a.mp3".into(), title: "A".into(), ..Default::default() }, Some(cat), false);
    let id2 = lib.add_sound(Sound { path: "C:/x/b.mp3".into(), title: "B".into(), ..Default::default() }, None, false);
    assert_eq!(lib.add_sound(Sound { path: "C:/x/a.mp3".into(), ..Default::default() }, None, false), id);
    assert_eq!(lib.by_index(2).map(|s| s.id), Some(id2));
    let hk = Hotkey::new(0x41, MOD_CTRL, 0);
    lib.sound_mut(id).unwrap().hotkey = Some(hk);
    assert!(lib.hotkey_owner(&hk, false).is_some());
    lib.clear_hotkey(&hk, false);
    assert!(lib.hotkey_owner(&hk, false).is_none());
    lib.remove_sounds(&[id]);
    assert!(lib.category(cat).unwrap().sounds.is_empty());
    assert_eq!(lib.sounds.len(), 1);
}

#[test]
fn spl_import_export_roundtrip() {
    let dir = temp_dir("spl");
    let audio = dir.join("a.wav");
    crate::audio::wav::write_wav(&audio, &sine(48_000, 100, 0.3), 48_000).unwrap();
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Soundlist>
  <Sound hash="h1" url="{}" artist="Art" title="Erster" duration="0:12" addedOn="2026-08-29" hotkeyKey="112" hotkeyKeyModifiers="2"/>
  <Sound hash="h2" url="C:\gibt\es\nicht.mp3" artist="" title="Zweiter" duration="1:02" addedOn="2026-09-10"/>
  <Categories>
    <Category type="1" icon="stock_icon_cat_list" hidden="true"/>
    <Category name="Meine Sounds" icon="stock_icon_cat_home"><Sound id="0"/><Sound id="1"/></Category>
    <Category name="Neu" icon="gen_N" index="5"><Sound id="1"/></Category>
  </Categories>
  <Hotbar><HotbarPage/></Hotbar>
</Soundlist>"#,
        audio.display()
    );
    std::fs::write(dir.join("soundlist.spl"), xml).unwrap();
    std::fs::write(
        dir.join("stats.spl"),
        r#"<?xml version="1.0" encoding="UTF-8"?><Stats><Sound hash="h1" playCount="18" lastPlayedOn="2026-10-25"/></Stats>"#,
    )
    .unwrap();
    let (lib, report) = crate::spl::import(&dir.join("soundlist.spl")).unwrap();
    assert_eq!(report.sounds, 2);
    assert_eq!(report.categories, 2);
    assert_eq!(report.stats_merged, 1);
    assert_eq!(report.missing_files, 1);
    let first = &lib.sounds[0];
    assert_eq!(first.title, "Erster");
    assert_eq!(first.play_count, 18);
    assert_eq!(first.duration_ms, 12_000);
    assert_eq!(first.hotkey, Some(Hotkey::new(112, MOD_CTRL, 0)));
    assert_eq!(lib.sounds[1].duration_ms, 62_000);
    assert_eq!(lib.categories[1].index, Some(5));
    assert_eq!(lib.categories[0].sounds.len(), 2);

    let out = crate::spl::to_xml(&lib, false, None);
    std::fs::write(dir.join("export.spl"), &out).unwrap();
    let (again, _) = crate::spl::import(&dir.join("export.spl")).unwrap();
    assert_eq!(again.sounds.len(), 2);
    assert_eq!(again.sounds[0].hotkey, first.hotkey);
    assert_eq!(again.categories.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn notification_tones_have_audio() {
    let b = SoundBuffer::tones(48_000, &[(440.0, 100), (0.0, 50), (880.0, 100)], 0.25);
    assert_eq!(b.duration_ms(), 250);
    assert!(b.peak > 0.2 && b.peak <= 0.26);
    let n = SoundBuffer::activation_noise(48_000, 200, 0.35);
    assert!(n.peak > 0.0 && n.peak < 1.0);
}

#[test]
fn vbcable_package_detection() {
    let html = r#"<a href="https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack43.zip">old</a>
        <a href="https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip">Download</a>
        VBCABLE_Driver_PackXY.zip VBCABLE_Driver_Pack46.txt"#;
    assert_eq!(crate::vmic::latest_pack(html), Some(45));
    assert_eq!(crate::vmic::latest_pack("nichts"), None);
}

#[test]
fn virtual_capture_names_are_detected() {
    use crate::audio::winaudio::is_virtual_capture;
    assert!(is_virtual_capture("CABLE Output (VB-Audio Virtual Cable)"));
    assert!(is_virtual_capture("VoiceMeeter Output (VB-Audio VoiceMeeter VAIO)"));
    assert!(!is_virtual_capture("Mikrofon (Realtek(R) Audio)"));

    use crate::audio::winaudio::is_virtual_render;
    assert!(is_virtual_render("CABLE Input (VB-Audio Virtual Cable)"));
    assert!(is_virtual_render("CABLE In 16ch (VB-Audio Virtual Cable)"));
    assert!(!is_virtual_render("Lautsprecher (Realtek(R) Audio)"));
    assert!(!is_virtual_render("Realtek HD Audio 2nd output (Realtek(R) Audio)"));
}

#[test]
fn update_version_compare_and_signature() {
    use crate::update::{is_newer, verify_signature};
    assert!(is_newer("1.0.1", "1.0.0"));
    assert!(is_newer("v1.2.10", "1.2.9"));
    assert!(is_newer("2.0", "1.9.9"));
    assert!(!is_newer("1.0.0", "1.0.0"));
    assert!(!is_newer("0.9.9", "1.0.0"));
    assert!(!is_newer("1.0.0-beta", "1.0.0"));
    assert!(verify_signature(b"{\"version\":\"9.9.9\"}", &"00".repeat(64)).is_err());
    assert!(verify_signature(b"{}", "kaputt").is_err());
    assert!(verify_signature(b"{}", "").is_err());
}

#[test]
fn language_quotes() {
    let _lang = crate::i18n::TEST_LOCK.lock();
    crate::i18n::set_language(crate::i18n::Language::En);
    assert_eq!(crate::i18n::q("Airhorn"), "\"Airhorn\"");
    crate::i18n::set_language(crate::i18n::Language::De);
    assert_eq!(crate::i18n::q("Airhorn"), "„Airhorn“");
}
