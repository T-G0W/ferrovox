pub struct SoundBuffer {
    pub samples: Vec<f32>,
    pub rate: u32,
    pub peak: f32,
    pub loudness_db: f32,
}

pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn lin_to_db(lin: f32) -> f32 {
    if lin <= 1e-9 {
        -180.0
    } else {
        20.0 * lin.log10()
    }
}

impl SoundBuffer {
    pub fn new(samples: Vec<f32>, rate: u32) -> SoundBuffer {
        let mut b = SoundBuffer { samples, rate: rate.max(1), peak: 0.0, loudness_db: -180.0 };
        b.analyze();
        b
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn duration_ms(&self) -> u64 {
        self.frames() as u64 * 1000 / self.rate as u64
    }

    pub fn bytes(&self) -> usize {
        self.samples.len() * 4
    }

    pub fn ms_to_frame(&self, ms: u64) -> usize {
        ((ms as u128 * self.rate as u128 / 1000) as usize).min(self.frames())
    }

    pub fn analyze(&mut self) {
        self.peak = self.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        self.loudness_db = gated_loudness(&self.samples, self.rate);
    }

    pub fn tones(rate: u32, notes: &[(f32, u32)], level: f32) -> SoundBuffer {
        let mut out = Vec::new();
        for &(freq, ms) in notes {
            let n = (rate as u64 * ms as u64 / 1000) as usize;
            let fade = (rate as usize / 200).min(n / 2).max(1);
            for i in 0..n {
                let env = if i < fade {
                    i as f32 / fade as f32
                } else if i + fade > n {
                    (n - i) as f32 / fade as f32
                } else {
                    1.0
                };
                let v = if freq > 0.0 {
                    (i as f32 / rate as f32 * freq * std::f32::consts::TAU).sin() * level * env
                } else {
                    0.0
                };
                out.push(v);
                out.push(v);
            }
        }
        SoundBuffer::new(out, rate)
    }

    pub fn activation_noise(rate: u32, ms: u32, level: f32) -> SoundBuffer {
        let n = (rate as u64 * ms as u64 / 1000) as usize;
        let mut seed: u32 = 0x1234_5678;
        let (mut b0, mut b1, mut b2) = (0.0f32, 0.0f32, 0.0f32);
        let mut out = Vec::with_capacity(n * 2);
        for _ in 0..n {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let white = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
            b0 = 0.99765 * b0 + white * 0.099_046;
            b1 = 0.963 * b1 + white * 0.296_516_4;
            b2 = 0.57 * b2 + white * 1.052_691_3;
            let pink = (b0 + b1 + b2 + white * 0.1848) * 0.2 * level;
            out.push(pink);
            out.push(pink);
        }
        SoundBuffer::new(out, rate)
    }

    pub fn wave_summary(&self, bins: usize) -> Vec<u8> {
        let frames = self.frames();
        if frames == 0 || bins == 0 {
            return Vec::new();
        }
        let peak = self.peak.max(1e-6);
        (0..bins)
            .map(|b| {
                let a = b * frames / bins;
                let e = ((b + 1) * frames / bins).max(a + 1).min(frames);
                let step = ((e - a) / 64).max(1);
                let mut m = 0.0f32;
                let mut f = a;
                while f < e {
                    m = m.max(self.samples[f * 2].abs()).max(self.samples[f * 2 + 1].abs());
                    f += step;
                }
                ((m / peak).sqrt().min(1.0) * 255.0) as u8
            })
            .collect()
    }

    pub fn peaks(&self, start: usize, end: usize, columns: usize) -> Vec<(f32, f32)> {
        let end = end.min(self.frames());
        if columns == 0 || end <= start {
            return Vec::new();
        }
        let span = (end - start) as f64 / columns as f64;
        (0..columns)
            .map(|c| {
                let a = start + (c as f64 * span) as usize;
                let b = (start + ((c + 1) as f64 * span) as usize).max(a + 1).min(end);
                let step = ((b - a) / 256).max(1);
                let (mut lo, mut hi) = (0.0f32, 0.0f32);
                let mut f = a;
                while f < b {
                    let v = (self.samples[f * 2] + self.samples[f * 2 + 1]) * 0.5;
                    lo = lo.min(v);
                    hi = hi.max(v);
                    f += step;
                }
                (lo, hi)
            })
            .collect()
    }
}

pub fn gated_loudness(samples: &[f32], rate: u32) -> f32 {
    let block = ((rate as usize / 20).max(1)) * 2;
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for chunk in samples.chunks(block) {
        let p: f64 = chunk.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / chunk.len() as f64;
        if p > 1e-6 {
            sum += p;
            count += 1;
        }
    }
    if count == 0 {
        return -180.0;
    }
    10.0 * (sum / count as f64).log10() as f32
}

pub fn trim_silence(samples: &mut Vec<f32>, threshold_db: f32, rate: u32) {
    let thr = db_to_lin(threshold_db);
    let frames = samples.len() / 2;
    let loud = |f: usize| samples[f * 2].abs().max(samples[f * 2 + 1].abs()) > thr;
    let Some(first) = (0..frames).find(|&f| loud(f)) else {
        samples.clear();
        return;
    };
    let last = (0..frames).rev().find(|&f| loud(f)).unwrap_or(first);
    let pad = rate as usize / 50;
    let a = first.saturating_sub(pad);
    let b = (last + pad).min(frames - 1);
    *samples = samples[a * 2..(b + 1) * 2].to_vec();
}

pub fn normalize_to(samples: &mut [f32], rate: u32, target_db: f32) {
    let loud = gated_loudness(samples, rate);
    if loud < -100.0 {
        return;
    }
    let mut gain = db_to_lin(target_db - loud);
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak * gain > 0.99 {
        gain = 0.99 / peak.max(1e-6);
    }
    samples.iter_mut().for_each(|s| *s *= gain);
}

pub fn apply_gain(samples: &mut [f32], a: usize, b: usize, db: f32) {
    let g = db_to_lin(db);
    for s in &mut samples[a * 2..b * 2] {
        *s = (*s * g).clamp(-1.0, 1.0);
    }
}

pub fn fade(samples: &mut [f32], a: usize, b: usize, fade_in: bool) {
    let n = (b - a).max(1) as f32;
    for f in a..b {
        let t = (f - a) as f32 / n;
        let g = if fade_in { t } else { 1.0 - t };
        samples[f * 2] *= g;
        samples[f * 2 + 1] *= g;
    }
}

pub fn reverse(samples: &mut [f32], a: usize, b: usize) {
    let region = &mut samples[a * 2..b * 2];
    let frames = region.len() / 2;
    for i in 0..frames / 2 {
        let j = frames - 1 - i;
        region.swap(i * 2, j * 2);
        region.swap(i * 2 + 1, j * 2 + 1);
    }
}
