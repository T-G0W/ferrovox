use super::buffer::SoundBuffer;
use windows::core::HSTRING;
use windows::Media::SpeechSynthesis::SpeechSynthesizer;
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

pub fn voices() -> Vec<String> {
    init();
    let mut out = Vec::new();
    if let Ok(all) = SpeechSynthesizer::AllVoices() {
        for v in all {
            if let Ok(name) = v.DisplayName() {
                out.push(name.to_string());
            }
        }
    }
    out
}

pub fn synthesize(text: &str, voice: Option<&str>, rate: f32, pitch: f32) -> anyhow::Result<SoundBuffer> {
    init();
    let synth = SpeechSynthesizer::new()?;
    if let Some(name) = voice {
        for v in SpeechSynthesizer::AllVoices()? {
            if v.DisplayName()?.to_string() == name {
                synth.SetVoice(&v)?;
                break;
            }
        }
    }
    if let Ok(opts) = synth.Options() {
        let _ = opts.SetSpeakingRate(rate.clamp(0.5, 3.0) as f64);
        let _ = opts.SetAudioPitch(pitch.clamp(0.0, 2.0) as f64);
    }
    let stream = synth.SynthesizeTextToStreamAsync(&HSTRING::from(text))?.get()?;
    let size = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    reader.LoadAsync(size)?.get()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    super::decode::decode_bytes(bytes, Some("wav"))
}
