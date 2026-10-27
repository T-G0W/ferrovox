use super::buffer::SoundBuffer;
use std::path::Path;

pub fn write_wav(path: &Path, samples: &[f32], rate: u32) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let spec = hound::WavSpec { channels: 2, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

pub fn save_buffer(path: &Path, buf: &SoundBuffer) -> anyhow::Result<()> {
    write_wav(path, &buf.samples, buf.rate)
}
