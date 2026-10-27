use super::buffer::SoundBuffer;
use super::engine::EngineShared;
use rtrb::{Consumer, Producer};
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub enum MixCmd {
    Play(Box<VoiceSpec>),
    Stop { id: Option<u64>, include_notify: bool },
    Pause(bool),
    Seek { id: u64, frame: f64 },
    Master(f32),
    BlockVoice(bool),
    PassGain(f32),
}

pub struct VoiceSpec {
    pub id: u64,
    pub buf: Arc<SoundBuffer>,
    pub gain: f32,
    pub start: usize,
    pub end: usize,
    pub delay_ms: u32,
    pub looped: bool,
    pub primary: bool,
    pub notify: bool,
}

pub enum MixEvent {
    Finished { id: u64, notify: bool, _buf: Arc<SoundBuffer> },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutKind {
    Speakers,
    Mic,
}

struct Voice {
    spec: VoiceSpec,
    pos: f64,
    step: f64,
    delay: usize,
    cur_gain: f32,
    target_gain: f32,
    stopping: bool,
    pausing: bool,
    paused: bool,
}

impl Voice {
    fn render(&mut self, out: &mut [f32], ramp: f32) -> bool {
        let frames = out.len() / 2;
        let s = &self.spec.buf.samples;
        let end = self.spec.end.min(self.spec.buf.frames()) as f64 - 1.0;
        for f in 0..frames {
            if self.delay > 0 {
                self.delay -= 1;
                continue;
            }
            if self.paused {
                return true;
            }
            if self.cur_gain < self.target_gain {
                self.cur_gain = (self.cur_gain + ramp).min(self.target_gain);
            } else if self.cur_gain > self.target_gain {
                self.cur_gain = (self.cur_gain - ramp).max(self.target_gain);
            }
            if self.stopping && self.cur_gain <= 0.0 {
                return false;
            }
            if self.pausing && self.cur_gain <= 0.0 {
                self.pausing = false;
                self.paused = true;
                return true;
            }
            if self.pos >= end {
                if self.spec.looped && end > self.spec.start as f64 {
                    self.pos = self.spec.start as f64;
                } else {
                    return false;
                }
            }
            let i = self.pos as usize;
            let fr = (self.pos - i as f64) as f32;
            let l = s[i * 2] + (s[i * 2 + 2] - s[i * 2]) * fr;
            let r = s[i * 2 + 1] + (s[i * 2 + 3] - s[i * 2 + 1]) * fr;
            out[f * 2] += l * self.cur_gain;
            out[f * 2 + 1] += r * self.cur_gain;
            self.pos += self.step;
        }
        true
    }

    fn audible(&self) -> bool {
        !self.paused && self.delay == 0 && !self.stopping
    }
}

pub struct MicPass {
    cons: Consumer<f32>,
    step: f64,
    pos: f64,
    prev: [f32; 2],
    cur: [f32; 2],
    target_fill: usize,
    max_fill: usize,
}

impl MicPass {
    pub fn new(cons: Consumer<f32>, in_rate: u32, out_rate: u32) -> MicPass {
        MicPass {
            cons,
            step: in_rate as f64 / out_rate.max(1) as f64,
            pos: 1.0,
            prev: [0.0; 2],
            cur: [0.0; 2],
            target_fill: (in_rate as usize / 100).max(64),
            max_fill: (in_rate as usize / 12).max(256),
        }
    }

    fn mix_into(&mut self, out: &mut [f32], g0: f32, g1: f32) -> f32 {
        let avail = self.cons.slots() / 2;
        if avail > self.max_fill {
            let skip = (avail - self.target_fill) * 2;
            if let Ok(chunk) = self.cons.read_chunk(skip) {
                chunk.commit_all();
            }
        }
        let frames = out.len() / 2;
        let mut peak = 0.0f32;
        for f in 0..frames {
            let mut starved = false;
            while self.pos >= 1.0 {
                if self.cons.slots() >= 2 {
                    let l = self.cons.pop().unwrap_or(0.0);
                    let r = self.cons.pop().unwrap_or(0.0);
                    self.prev = self.cur;
                    self.cur = [l, r];
                    self.pos -= 1.0;
                } else {
                    starved = true;
                    break;
                }
            }
            if starved {
                continue;
            }
            let t = self.pos as f32;
            let g = g0 + (g1 - g0) * (f as f32 / frames.max(1) as f32);
            let l = (self.prev[0] + (self.cur[0] - self.prev[0]) * t) * g;
            let r = (self.prev[1] + (self.cur[1] - self.prev[1]) * t) * g;
            peak = peak.max(l.abs()).max(r.abs());
            out[f * 2] += l;
            out[f * 2 + 1] += r;
            self.pos += self.step;
        }
        peak
    }
}

pub struct Mixer {
    kind: OutKind,
    rate: f64,
    voices: Vec<Voice>,
    cmds: Consumer<MixCmd>,
    events: Producer<MixEvent>,
    shared: Arc<EngineShared>,
    master: f32,
    master_cur: f32,
    mic: Option<MicPass>,
    block_voice: bool,
    pass_gain: f32,
    pass_cur: f32,
    notify_scratch: Vec<f32>,
}

impl Mixer {
    pub fn new(
        kind: OutKind,
        rate: u32,
        cmds: Consumer<MixCmd>,
        events: Producer<MixEvent>,
        shared: Arc<EngineShared>,
        mic: Option<MicPass>,
        master: f32,
        block_voice: bool,
        pass_gain: f32,
    ) -> Mixer {
        Mixer {
            kind,
            rate: rate as f64,
            voices: Vec::with_capacity(32),
            cmds,
            events,
            shared,
            master,
            master_cur: master,
            mic,
            block_voice,
            pass_gain,
            pass_cur: pass_gain,
            notify_scratch: vec![0.0; 16384],
        }
    }

    fn handle_cmds(&mut self) {
        while let Ok(cmd) = self.cmds.pop() {
            match cmd {
                MixCmd::Play(spec) => {
                    let step = spec.buf.rate as f64 / self.rate;
                    let delay = (spec.delay_ms as f64 * self.rate / 1000.0) as usize;
                    let gain = spec.gain;
                    let start = spec.start;
                    self.voices.push(Voice {
                        pos: start as f64,
                        step,
                        delay,
                        cur_gain: if start == 0 { gain } else { 0.0 },
                        target_gain: gain,
                        stopping: false,
                        pausing: false,
                        paused: false,
                        spec: *spec,
                    });
                }
                MixCmd::Stop { id, include_notify } => {
                    for v in &mut self.voices {
                        if (include_notify || !v.spec.notify) && id.map_or(true, |i| i == v.spec.id) {
                            v.stopping = true;
                            v.target_gain = 0.0;
                            if v.paused || v.delay > 0 {
                                v.cur_gain = 0.0;
                                v.paused = false;
                                v.delay = 0;
                            }
                        }
                    }
                }
                MixCmd::Pause(p) => {
                    for v in self.voices.iter_mut().filter(|v| !v.spec.notify && !v.stopping) {
                        if p {
                            v.pausing = true;
                            v.target_gain = 0.0;
                        } else {
                            v.pausing = false;
                            v.paused = false;
                            v.target_gain = v.spec.gain;
                        }
                    }
                }
                MixCmd::Seek { id, frame } => {
                    for v in self.voices.iter_mut().filter(|v| v.spec.id == id) {
                        v.pos = frame.clamp(0.0, v.spec.buf.frames().saturating_sub(2) as f64);
                        v.delay = 0;
                    }
                }
                MixCmd::Master(g) => self.master = g,
                MixCmd::BlockVoice(b) => self.block_voice = b,
                MixCmd::PassGain(g) => self.pass_gain = g,
            }
        }
    }

    pub fn render(&mut self, out: &mut [f32]) {
        self.handle_cmds();
        out.fill(0.0);
        let frames = out.len() / 2;
        if frames == 0 {
            return;
        }
        let ramp = (1.0 / (0.008 * self.rate)) as f32;

        let mut i = 0;
        while i < self.voices.len() {
            if self.voices[i].spec.notify {
                i += 1;
                continue;
            }
            if self.voices[i].render(out, ramp) {
                i += 1;
            } else {
                let v = self.voices.swap_remove(i);
                let _ = self.events.push(MixEvent::Finished { id: v.spec.id, notify: false, _buf: v.spec.buf });
            }
        }

        let (g0, g1) = (self.master_cur, self.master);
        for f in 0..frames {
            let g = g0 + (g1 - g0) * (f as f32 / frames as f32);
            out[f * 2] *= g;
            out[f * 2 + 1] *= g;
        }
        self.master_cur = self.master;

        if let Some(mic) = self.mic.as_mut() {
            let sounds_active = self.voices.iter().any(|v| !v.spec.notify && v.audible());
            let target = if self.block_voice && sounds_active { 0.0 } else { self.pass_gain };
            let p = mic.mix_into(out, self.pass_cur, target);
            self.pass_cur = target;
            store_peak(&self.shared.mic_in_peak, p);
        }

        if self.voices.iter().any(|v| v.spec.notify) {
            if self.notify_scratch.len() < out.len() {
                self.notify_scratch.resize(out.len(), 0.0);
            }
            let scratch = &mut self.notify_scratch[..out.len()];
            scratch.fill(0.0);
            let mut i = 0;
            while i < self.voices.len() {
                if !self.voices[i].spec.notify {
                    i += 1;
                    continue;
                }
                if self.voices[i].render(scratch, ramp) {
                    i += 1;
                } else {
                    let v = self.voices.swap_remove(i);
                    let _ = self.events.push(MixEvent::Finished { id: v.spec.id, notify: true, _buf: v.spec.buf });
                }
            }
            for (o, n) in out.iter_mut().zip(scratch.iter()) {
                *o += *n;
            }
        }

        let mut peak = 0.0f32;
        for s in out.iter_mut() {
            let x = *s;
            *s = if x.abs() > 0.95 { x.signum() * (0.95 + (x.abs() - 0.95).tanh() * 0.05) } else { x };
            peak = peak.max(s.abs());
        }
        match self.kind {
            OutKind::Speakers => store_peak(&self.shared.spk_peak, peak),
            OutKind::Mic => store_peak(&self.shared.mic_peak, peak),
        }

        if let Some(v) = self.voices.iter().find(|v| v.spec.primary && !v.spec.notify && !v.stopping) {
            self.shared.pos_voice.store(v.spec.id, Ordering::Relaxed);
            self.shared.pos_frames.store(v.pos as u64, Ordering::Relaxed);
        }
    }
}

fn store_peak(cell: &std::sync::atomic::AtomicU32, value: f32) {
    let old = f32::from_bits(cell.load(Ordering::Relaxed));
    if value > old {
        cell.store(value.to_bits(), Ordering::Relaxed);
    }
}
