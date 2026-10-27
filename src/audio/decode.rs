use super::buffer::SoundBuffer;
use anyhow::{anyhow, Context};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::{MetadataOptions, StandardTagKey};
use symphonia::core::probe::Hint;

pub const SUPPORTED_EXT: &[&str] = &[
    "mp3", "wav", "wave", "flac", "ogg", "oga", "m4a", "aac", "alac", "aif", "aiff", "caf", "mp4", "m4v",
    "mov", "mkv", "webm", "mka",
];

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Default, Debug, Clone)]
pub struct Meta {
    pub duration_ms: u64,
    pub title: Option<String>,
    pub artist: Option<String>,
}

fn open(source: Box<dyn MediaSource>, ext: Option<&str>) -> anyhow::Result<Box<dyn FormatReader>> {
    let mss = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = ext {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
        .context(crate::t!("Format nicht erkannt", "Format not recognized"))?;
    Ok(probed.format)
}

fn read_tags(format: &mut Box<dyn FormatReader>, meta: &mut Meta) {
    if let Some(rev) = format.metadata().current() {
        for tag in rev.tags() {
            match tag.std_key {
                Some(StandardTagKey::TrackTitle) if meta.title.is_none() => meta.title = Some(tag.value.to_string()),
                Some(StandardTagKey::Artist) if meta.artist.is_none() => meta.artist = Some(tag.value.to_string()),
                _ => {}
            }
        }
    }
}

pub fn probe_meta(path: &Path) -> anyhow::Result<Meta> {
    let file = std::fs::File::open(path).with_context(|| format!("{} öffnen", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str());
    let mut format = open(Box::new(file), ext)?;
    let mut meta = Meta::default();
    read_tags(&mut format, &mut meta);
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("{}", crate::t!("Keine Audiospur", "No audio track")))?;
    let params = &track.codec_params;
    if let (Some(frames), Some(rate)) = (params.n_frames, params.sample_rate) {
        meta.duration_ms = frames * 1000 / rate.max(1) as u64;
    } else if let (Some(frames), Some(tb)) = (params.n_frames, params.time_base) {
        let t = tb.calc_time(frames);
        meta.duration_ms = t.seconds * 1000 + (t.frac * 1000.0) as u64;
    } else {
        meta.duration_ms = decode_file(path)?.duration_ms();
    }
    Ok(meta)
}

pub fn decode_file(path: &Path) -> anyhow::Result<SoundBuffer> {
    let file = std::fs::File::open(path).with_context(|| format!("{} öffnen", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_string());
    decode_source(Box::new(file), ext.as_deref())
}

pub fn decode_bytes(bytes: Vec<u8>, ext: Option<&str>) -> anyhow::Result<SoundBuffer> {
    decode_source(Box::new(std::io::Cursor::new(bytes)), ext)
}

fn decode_source(source: Box<dyn MediaSource>, ext: Option<&str>) -> anyhow::Result<SoundBuffer> {
    let mut format = open(source, ext)?;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("{}", crate::t!("Keine Audiospur gefunden", "No audio track found")))?;
    let track_id = track.id;
    let mut rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context(crate::t!("Codec wird nicht unterstützt", "Codec not supported"))?;

    let mut out: Vec<f32> = Vec::new();
    if let Some(frames) = track.codec_params.n_frames {
        out.reserve((frames as usize).min(48_000 * 60 * 20) * 2);
    }
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    let mut sbuf_frames = 0usize;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => {
                if out.is_empty() {
                    return Err(anyhow!("{}: {e}", crate::t!("Lesefehler", "Read error")));
                }
                break;
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        let audio = match decoder.decode(&packet) {
            Ok(a) => a,
            Err(SymError::DecodeError(_)) => continue,
            Err(SymError::IoError(_)) => break,
            Err(e) => return Err(anyhow!("{}: {e}", crate::t!("Dekodierfehler", "Decoding error"))),
        };
        let spec = *audio.spec();
        rate = spec.rate;
        let ch = spec.channels.count().max(1);
        let cap = audio.capacity();
        if sbuf.is_none() || sbuf_frames < cap {
            sbuf = Some(SampleBuffer::<f32>::new(cap as u64, spec));
            sbuf_frames = cap;
        }
        let buf = sbuf.as_mut().unwrap();
        buf.copy_interleaved_ref(audio);
        let s = buf.samples();
        match ch {
            1 => s.iter().for_each(|&v| {
                out.push(v);
                out.push(v);
            }),
            2 => out.extend_from_slice(s),
            _ => {
                for f in s.chunks_exact(ch) {
                    let c = f[2] * 0.707;
                    out.push((f[0] + c).clamp(-1.0, 1.0));
                    out.push((f[1] + c).clamp(-1.0, 1.0));
                }
            }
        }
    }
    if out.is_empty() {
        return Err(anyhow!("{}", crate::t!("Datei enthält keine Audiodaten", "File contains no audio data")));
    }
    Ok(SoundBuffer::new(out, rate))
}
