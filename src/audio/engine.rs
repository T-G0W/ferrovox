use super::buffer::SoundBuffer;
use super::mixer::{MicPass, MixCmd, MixEvent, Mixer, OutKind, VoiceSpec};
use anyhow::{anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VIRTUAL_HINTS: &[&str] = &["cable input", "vb-audio", "voicemeeter input", "voicemeeter aux", "virtual"];

#[derive(Default)]
pub struct EngineShared {
    pub pos_voice: AtomicU64,
    pub pos_frames: AtomicU64,
    pub spk_peak: AtomicU32,
    pub mic_peak: AtomicU32,
    pub mic_in_peak: AtomicU32,
    pub rec_peak: AtomicU32,
    pub rec_frames: AtomicU64,
    pub rec_rate: AtomicU32,
    pub stream_error: AtomicBool,
    pub status: Mutex<DeviceStatus>,
}

impl EngineShared {
    pub fn take_peak(cell: &AtomicU32) -> f32 {
        f32::from_bits(cell.swap(0f32.to_bits(), Ordering::Relaxed))
    }
}

#[derive(Default, Clone, Debug)]
pub struct DeviceStatus {
    pub speakers: Option<String>,
    pub mic_out: Option<String>,
    pub mic_in: Option<String>,
    pub speakers_rate: u32,
    pub mic_rate: u32,
    pub errors: Vec<String>,
    pub virtual_found: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct DeviceConfig {
    pub speakers: Option<String>,
    pub mic_out: Option<String>,
    pub mic_in: Option<String>,
    pub passthrough: bool,
    pub pass_gain: f32,
    pub block_voice: bool,
    pub spk_master: f32,
    pub mic_master: f32,
}

#[derive(Clone, Default, Debug)]
pub struct DeviceLists {
    pub outputs: Vec<String>,
    pub inputs: Vec<String>,
    pub default_output: Option<String>,
    pub default_input: Option<String>,
}

pub struct PlayRequest {
    pub id: u64,
    pub buf: Arc<SoundBuffer>,
    pub gain: f32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub speakers: bool,
    pub mic: bool,
    pub spk_delay_ms: u32,
    pub mic_delay_ms: u32,
    pub looped: bool,
    pub notify: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecSource {
    Speakers,
    Microphone,
}

pub enum EngineCmd {
    Configure(DeviceConfig),
    Play(PlayRequest),
    Stop { id: Option<u64>, include_notify: bool },
    Pause(bool),
    Seek { id: u64, ms: u64 },
    Masters { speakers: f32, mic: f32 },
    BlockVoice(bool),
    PassGain(f32),
    StartRecording(RecSource),
    StopRecording(Sender<Option<SoundBuffer>>),
    ListDevices(Sender<DeviceLists>),
}

#[derive(Debug)]
pub enum EngineEvent {
    Finished { id: u64 },
    Error(String),
    DeviceSwitched(String),
}

pub struct AudioEngine {
    tx: Sender<EngineCmd>,
    pub shared: Arc<EngineShared>,
    pub events: Receiver<EngineEvent>,
}

impl AudioEngine {
    pub fn start() -> AudioEngine {
        let (tx, rx) = unbounded();
        let (ev_tx, ev_rx) = unbounded();
        let shared = Arc::new(EngineShared::default());
        let sh = shared.clone();
        std::thread::Builder::new()
            .name("ferrovox-audio".into())
            .spawn(move || EngineThread::new(sh, ev_tx).run(rx))
            .expect("Audio-Thread");
        AudioEngine { tx, shared, events: ev_rx }
    }

    pub fn send(&self, cmd: EngineCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn list_devices(&self) -> DeviceLists {
        let (tx, rx) = unbounded();
        self.send(EngineCmd::ListDevices(tx));
        rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
    }

    pub fn stop_recording(&self) -> Option<SoundBuffer> {
        let (tx, rx) = unbounded();
        self.send(EngineCmd::StopRecording(tx));
        rx.recv_timeout(Duration::from_secs(5)).ok().flatten()
    }
}

struct Output {
    _stream: cpal::Stream,
    prod: Producer<MixCmd>,
    events: Consumer<MixEvent>,
    rate: u32,
}

struct Recording {
    source: RecSource,
    cons: Consumer<f32>,
    _stream: Option<cpal::Stream>,
    buf: Vec<f32>,
    rate: u32,
}

struct EngineThread {
    shared: Arc<EngineShared>,
    ev_tx: Sender<EngineEvent>,
    host: cpal::Host,
    cfg: DeviceConfig,
    spk: Option<Output>,
    mic: Option<Output>,
    input: Option<cpal::Stream>,
    input_rate: u32,
    mic_rec_on: Arc<AtomicBool>,
    mic_rec_cons: Option<Consumer<f32>>,
    recording: Option<Recording>,
    pending: HashMap<u64, u8>,
    rates: HashMap<u64, u32>,
    last_error_rebuild: Instant,
    last_default_check: Instant,
    tried_out: Option<String>,
    tried_in: Option<String>,
}

impl EngineThread {
    fn new(shared: Arc<EngineShared>, ev_tx: Sender<EngineEvent>) -> Self {
        EngineThread {
            shared,
            ev_tx,
            host: cpal::default_host(),
            cfg: DeviceConfig { spk_master: 0.8, mic_master: 0.8, pass_gain: 1.0, ..Default::default() },
            spk: None,
            mic: None,
            input: None,
            input_rate: 48_000,
            mic_rec_on: Arc::new(AtomicBool::new(false)),
            mic_rec_cons: None,
            recording: None,
            pending: HashMap::new(),
            rates: HashMap::new(),
            last_error_rebuild: Instant::now(),
            last_default_check: Instant::now(),
            tried_out: None,
            tried_in: None,
        }
    }

    fn run(mut self, rx: Receiver<EngineCmd>) {
        loop {
            match rx.recv_timeout(Duration::from_millis(15)) {
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.drain_events();
            self.drain_recording();
            if self.last_default_check.elapsed() > Duration::from_millis(1500) {
                self.last_default_check = Instant::now();
                self.follow_default_devices();
            }
            if self.shared.stream_error.swap(false, Ordering::Relaxed)
                && self.last_error_rebuild.elapsed() > Duration::from_secs(2)
            {
                self.last_error_rebuild = Instant::now();
                log::warn!("Audio-Stream-Fehler – Geräte werden neu verbunden");
                let cfg = self.cfg.clone();
                self.configure(cfg);
            }
        }
    }

    fn handle(&mut self, cmd: EngineCmd) {
        match cmd {
            EngineCmd::Configure(cfg) => {
                self.tried_out = None;
                self.tried_in = None;
                self.configure(cfg)
            }
            EngineCmd::Play(req) => self.play(req),
            EngineCmd::Stop { id, include_notify } => self.broadcast(|| MixCmd::Stop { id, include_notify }),
            EngineCmd::Pause(p) => self.broadcast(|| MixCmd::Pause(p)),
            EngineCmd::Seek { id, ms } => {
                if let Some(&rate) = self.rates.get(&id) {
                    let frame = ms as f64 * rate as f64 / 1000.0;
                    self.broadcast(|| MixCmd::Seek { id, frame });
                    self.shared.pos_frames.store(frame as u64, Ordering::Relaxed);
                }
            }
            EngineCmd::Masters { speakers, mic } => {
                self.cfg.spk_master = speakers;
                self.cfg.mic_master = mic;
                if let Some(o) = self.spk.as_mut() {
                    let _ = o.prod.push(MixCmd::Master(speakers));
                }
                if let Some(o) = self.mic.as_mut() {
                    let _ = o.prod.push(MixCmd::Master(mic));
                }
            }
            EngineCmd::BlockVoice(b) => {
                self.cfg.block_voice = b;
                if let Some(o) = self.mic.as_mut() {
                    let _ = o.prod.push(MixCmd::BlockVoice(b));
                }
            }
            EngineCmd::PassGain(g) => {
                self.cfg.pass_gain = g;
                if let Some(o) = self.mic.as_mut() {
                    let _ = o.prod.push(MixCmd::PassGain(g));
                }
            }
            EngineCmd::StartRecording(src) => {
                if let Err(e) = self.start_recording(src) {
                    let _ = self.ev_tx.send(EngineEvent::Error(format!("Aufnahme: {e}")));
                }
            }
            EngineCmd::StopRecording(reply) => {
                self.drain_recording();
                let out = self.recording.take().map(|r| {
                    self.mic_rec_on.store(false, Ordering::Relaxed);
                    SoundBuffer::new(r.buf, r.rate)
                });
                self.shared.rec_frames.store(0, Ordering::Relaxed);
                if self.recording.is_none() && !self.cfg.passthrough {
                    self.input = None;
                }
                let _ = reply.send(out);
            }
            EngineCmd::ListDevices(reply) => {
                let _ = reply.send(self.list_devices());
            }
        }
    }

    fn broadcast(&mut self, mut make: impl FnMut() -> MixCmd) {
        for out in [self.spk.as_mut(), self.mic.as_mut()].into_iter().flatten() {
            let _ = out.prod.push(make());
        }
    }

    fn list_devices(&self) -> DeviceLists {
        DeviceLists {
            outputs: self
                .host
                .output_devices()
                .map(|d| d.filter_map(|x| x.name().ok()).collect())
                .unwrap_or_default(),
            inputs: self
                .host
                .input_devices()
                .map(|d| d.filter_map(|x| x.name().ok()).collect())
                .unwrap_or_default(),
            default_output: self.host.default_output_device().and_then(|d| d.name().ok()),
            default_input: self.host.default_input_device().and_then(|d| d.name().ok()),
        }
    }

    fn find_output(&self, name: &Option<String>) -> Option<cpal::Device> {
        let is_virtual = |d: &cpal::Device| d.name().map(|n| super::winaudio::is_virtual_render(&n)).unwrap_or(false);
        if let Some(n) = name {
            if let Some(d) = self.host.output_devices().ok()?.find(|d| d.name().ok().as_deref() == Some(n.as_str())) {
                return Some(d);
            }
        }
        if let Some(d) = self.host.default_output_device() {
            if !is_virtual(&d) {
                return Some(d);
            }
        }
        self.host.output_devices().ok()?.find(|d| !is_virtual(d))
    }

    fn follow_default_devices(&mut self) {
        if self.recording.is_some() {
            return;
        }
        let mut switch_to = None;
        if self.cfg.speakers.is_none() {
            let want = self.find_output(&None).and_then(|d| d.name().ok());
            let have = self.shared.status.lock().speakers.clone();
            if want.is_some() && want != have && want != self.tried_out {
                self.tried_out = want.clone();
                switch_to = want;
            }
        }
        let mut input_changed = false;
        if self.cfg.passthrough && self.cfg.mic_in.is_none() && self.input.is_some() {
            let want = self.find_input(&None).and_then(|d| d.name().ok());
            let have = self.shared.status.lock().mic_in.clone();
            if want.is_some() && want != have && want != self.tried_in {
                self.tried_in = want;
                input_changed = true;
            }
        }
        if switch_to.is_some() || input_changed {
            log::info!("Windows-Standardgerät geändert – verbinde neu ({switch_to:?})");
            let cfg = self.cfg.clone();
            self.configure(cfg);
            if let Some(name) = switch_to {
                let _ = self.ev_tx.send(EngineEvent::DeviceSwitched(name));
            }
        }
    }

    fn find_virtual_output(&self) -> Option<cpal::Device> {
        let devs: Vec<cpal::Device> = self.host.output_devices().ok()?.collect();
        for hint in VIRTUAL_HINTS {
            if let Some(d) = devs
                .iter()
                .find(|d| d.name().map(|n| n.to_lowercase().contains(hint)).unwrap_or(false))
            {
                return Some(d.clone());
            }
        }
        None
    }

    fn find_input(&self, name: &Option<String>) -> Option<cpal::Device> {
        let devs: Vec<cpal::Device> = self.host.input_devices().ok()?.collect();
        let is_virtual =
            |d: &cpal::Device| d.name().map(|n| super::winaudio::is_virtual_capture(&n)).unwrap_or(false);
        if let Some(n) = name {
            if let Some(d) = devs.iter().find(|d| d.name().ok().as_deref() == Some(n.as_str())) {
                return Some(d.clone());
            }
        }
        if let Some(d) = self.host.default_input_device() {
            if !is_virtual(&d) {
                return Some(d);
            }
        }
        devs.into_iter().find(|d| !is_virtual(d))
    }

    fn configure(&mut self, cfg: DeviceConfig) {
        self.spk = None;
        self.mic = None;
        self.input = None;
        self.mic_rec_cons = None;
        self.rates.clear();
        let pending: Vec<u64> = self.pending.drain().map(|(id, _)| id).collect();
        for id in pending {
            let _ = self.ev_tx.send(EngineEvent::Finished { id });
        }
        self.cfg = cfg.clone();
        let mut status = DeviceStatus::default();

        if let Some(dev) = self.find_output(&cfg.speakers) {
            let name = dev.name().unwrap_or_default();
            match self.build_output(&dev, OutKind::Speakers, None, cfg.spk_master) {
                Ok(out) => {
                    status.speakers_rate = out.rate;
                    status.speakers = Some(name);
                    self.spk = Some(out);
                }
                Err(e) => status.errors.push(format!("{} {}: {e}", crate::t!("Lautsprecher", "Speakers"), crate::i18n::q(&name))),
            }
        } else {
            status.errors.push(crate::t!("Kein Lautsprecher-Gerät gefunden", "No speaker device found").into());
        }

        let mic_dev = match &cfg.mic_out {
            Some(n) if n.is_empty() => None,
            Some(_) => self.find_output(&cfg.mic_out),
            None => self.find_virtual_output(),
        };
        status.virtual_found = self.find_virtual_output().is_some();
        if let Some(dev) = mic_dev {
            let name = dev.name().unwrap_or_default();
            let same_as_speakers = status.speakers.as_deref() == Some(name.as_str());
            if same_as_speakers {
                status.errors.push(format!(
                    "{} ({}) – {}",
                    crate::t!("Mikrofon-Ausgang ist dasselbe Gerät wie die Lautsprecher", "Microphone output is the same device as the speakers"),
                    crate::i18n::q(&name),
                    crate::t!("Sounds werden doppelt abgespielt.", "sounds will play twice.")
                ));
            }
            let mut mic_pass = None;
            if cfg.passthrough {
                match self.build_input(&cfg, true) {
                    Ok((stream, rate, cons, in_name)) => {
                        self.input = Some(stream);
                        self.input_rate = rate;
                        status.mic_in = Some(in_name);
                        mic_pass = cons.map(|c| (c, rate));
                    }
                    Err(e) => status.errors.push(format!("Mikrofon-Eingang: {e}")),
                }
            }
            let out_rate = dev.default_output_config().map(|c| c.sample_rate().0).unwrap_or(48_000);
            let pass = mic_pass.map(|(cons, in_rate)| MicPass::new(cons, in_rate, out_rate));
            match self.build_output(&dev, OutKind::Mic, pass, cfg.mic_master) {
                Ok(out) => {
                    status.mic_rate = out.rate;
                    status.mic_out = Some(name);
                    self.mic = Some(out);
                }
                Err(e) => status.errors.push(format!("{} {}: {e}", crate::t!("Mikrofon-Ausgang", "Microphone output"), crate::i18n::q(&name))),
            }
        }
        *self.shared.status.lock() = status;
    }

    fn build_output(
        &self,
        dev: &cpal::Device,
        kind: OutKind,
        pass: Option<MicPass>,
        master: f32,
    ) -> anyhow::Result<Output> {
        let cfg = dev.default_output_config()?;
        let rate = cfg.sample_rate().0;
        let channels = cfg.channels() as usize;
        let (prod, cmds) = RingBuffer::<MixCmd>::new(256);
        let (ev_prod, events) = RingBuffer::<MixEvent>::new(256);
        let mixer = Mixer::new(
            kind,
            rate,
            cmds,
            ev_prod,
            self.shared.clone(),
            pass,
            master,
            self.cfg.block_voice,
            self.cfg.pass_gain,
        );
        let sc: cpal::StreamConfig = cfg.clone().into();
        let stream = match cfg.sample_format() {
            SampleFormat::F32 => out_stream::<f32>(dev, &sc, channels, mixer, self.shared.clone())?,
            SampleFormat::I16 => out_stream::<i16>(dev, &sc, channels, mixer, self.shared.clone())?,
            SampleFormat::U16 => out_stream::<u16>(dev, &sc, channels, mixer, self.shared.clone())?,
            SampleFormat::I32 => out_stream::<i32>(dev, &sc, channels, mixer, self.shared.clone())?,
            other => bail!("{} {other:?}", crate::t!("Nicht unterstütztes Sampleformat", "Unsupported sample format")),
        };
        stream.play()?;
        Ok(Output { _stream: stream, prod, events, rate })
    }

    #[allow(clippy::type_complexity)]
    fn build_input(
        &mut self,
        cfg: &DeviceConfig,
        with_pass: bool,
    ) -> anyhow::Result<(cpal::Stream, u32, Option<Consumer<f32>>, String)> {
        let dev = self.find_input(&cfg.mic_in).ok_or_else(|| anyhow!("{}", crate::t!("Kein Mikrofon gefunden", "No microphone found")))?;
        let name = dev.name().unwrap_or_default();
        if with_pass && super::winaudio::is_virtual_capture(&name) {
            bail!(
                "{} {}",
                crate::i18n::q(&name),
                crate::t!(
                    "ist ein virtuelles Kabel – Durchleitung würde rückkoppeln. Bitte das echte Mikrofon wählen.",
                    "is a virtual cable – passthrough would cause feedback. Please choose your real microphone."
                )
            );
        }
        let icfg = dev.default_input_config()?;
        let rate = icfg.sample_rate().0;
        let channels = icfg.channels() as usize;
        let (pass_prod, pass_cons) = if with_pass {
            let (p, c) = RingBuffer::<f32>::new(rate as usize);
            (Some(p), Some(c))
        } else {
            (None, None)
        };
        let (rec_prod, rec_cons) = RingBuffer::<f32>::new(rate as usize * 8);
        self.mic_rec_cons = Some(rec_cons);
        let sc: cpal::StreamConfig = icfg.clone().into();
        let on = self.mic_rec_on.clone();
        let sh = self.shared.clone();
        let stream = match icfg.sample_format() {
            SampleFormat::F32 => in_stream::<f32>(&dev, &sc, channels, pass_prod, Some(rec_prod), on, sh)?,
            SampleFormat::I16 => in_stream::<i16>(&dev, &sc, channels, pass_prod, Some(rec_prod), on, sh)?,
            SampleFormat::U16 => in_stream::<u16>(&dev, &sc, channels, pass_prod, Some(rec_prod), on, sh)?,
            SampleFormat::I32 => in_stream::<i32>(&dev, &sc, channels, pass_prod, Some(rec_prod), on, sh)?,
            other => bail!("{} {other:?}", crate::t!("Nicht unterstütztes Sampleformat", "Unsupported sample format")),
        };
        stream.play()?;
        Ok((stream, rate, pass_cons, name))
    }

    fn play(&mut self, req: PlayRequest) {
        let start = req.buf.ms_to_frame(req.start_ms);
        let end = if req.end_ms == 0 { req.buf.frames() } else { req.buf.ms_to_frame(req.end_ms).max(start + 1) };
        let mut count = 0u8;
        let primary_spk = req.speakers && self.spk.is_some();
        let targets = [
            (self.spk.as_mut(), req.speakers, req.spk_delay_ms, primary_spk),
            (self.mic.as_mut(), req.mic, req.mic_delay_ms, !primary_spk),
        ];
        for (out, wanted, delay, primary) in targets {
            if let (Some(out), true) = (out, wanted) {
                let spec = VoiceSpec {
                    id: req.id,
                    buf: req.buf.clone(),
                    gain: req.gain,
                    start,
                    end,
                    delay_ms: delay,
                    looped: req.looped,
                    primary,
                    notify: req.notify,
                };
                if out.prod.push(MixCmd::Play(Box::new(spec))).is_ok() {
                    count += 1;
                }
            }
        }
        if count == 0 {
            if !req.notify {
                let _ = self.ev_tx.send(EngineEvent::Finished { id: req.id });
            }
            return;
        }
        if !req.notify {
            self.pending.insert(req.id, count);
            self.rates.insert(req.id, req.buf.rate);
            self.shared.pos_voice.store(req.id, Ordering::Relaxed);
            self.shared.pos_frames.store(start as u64, Ordering::Relaxed);
        }
    }

    fn drain_events(&mut self) {
        let mut finished = Vec::new();
        for out in [self.spk.as_mut(), self.mic.as_mut()].into_iter().flatten() {
            while let Ok(ev) = out.events.pop() {
                match ev {
                    MixEvent::Finished { id, notify, .. } => {
                        if !notify {
                            finished.push(id);
                        }
                    }
                }
            }
        }
        for id in finished {
            if let Some(c) = self.pending.get_mut(&id) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    self.pending.remove(&id);
                    self.rates.remove(&id);
                    let _ = self.ev_tx.send(EngineEvent::Finished { id });
                }
            }
        }
    }

    fn start_recording(&mut self, src: RecSource) -> anyhow::Result<()> {
        self.recording = None;
        self.mic_rec_on.store(false, Ordering::Relaxed);
        match src {
            RecSource::Microphone => {
                if self.input.is_none() {
                    let cfg = self.cfg.clone();
                    let (stream, rate, _, _) = self.build_input(&cfg, false)?;
                    self.input = Some(stream);
                    self.input_rate = rate;
                }
                let mut cons = self.mic_rec_cons.take().ok_or_else(|| anyhow!("{}", crate::t!("Mikrofon nicht verfügbar", "Microphone not available")))?;
                let n = cons.slots();
                if let Ok(c) = cons.read_chunk(n) {
                    c.commit_all();
                }
                self.mic_rec_on.store(true, Ordering::Relaxed);
                self.shared.rec_rate.store(self.input_rate, Ordering::Relaxed);
                self.recording =
                    Some(Recording { source: src, cons, _stream: None, buf: Vec::new(), rate: self.input_rate });
            }
            RecSource::Speakers => {
                let dev = self.find_output(&self.cfg.speakers).ok_or_else(|| anyhow!("{}", crate::t!("Kein Lautsprecher", "No speakers")))?;
                let ocfg = dev.default_output_config()?;
                let rate = ocfg.sample_rate().0;
                let channels = ocfg.channels() as usize;
                let (prod, cons) = RingBuffer::<f32>::new(rate as usize * 8);
                let sc: cpal::StreamConfig = ocfg.clone().into();
                let on = Arc::new(AtomicBool::new(true));
                let sh = self.shared.clone();
                let stream = match ocfg.sample_format() {
                    SampleFormat::F32 => in_stream::<f32>(&dev, &sc, channels, None, Some(prod), on, sh)?,
                    SampleFormat::I16 => in_stream::<i16>(&dev, &sc, channels, None, Some(prod), on, sh)?,
                    SampleFormat::U16 => in_stream::<u16>(&dev, &sc, channels, None, Some(prod), on, sh)?,
                    SampleFormat::I32 => in_stream::<i32>(&dev, &sc, channels, None, Some(prod), on, sh)?,
                    other => bail!("{} {other:?}", crate::t!("Nicht unterstütztes Sampleformat", "Unsupported sample format")),
                };
                stream.play()?;
                self.shared.rec_rate.store(rate, Ordering::Relaxed);
                self.recording = Some(Recording { source: src, cons, _stream: Some(stream), buf: Vec::new(), rate });
            }
        }
        self.shared.rec_frames.store(0, Ordering::Relaxed);
        Ok(())
    }

    fn drain_recording(&mut self) {
        let Some(rec) = self.recording.as_mut() else { return };
        let n = rec.cons.slots();
        if n == 0 {
            return;
        }
        if let Ok(chunk) = rec.cons.read_chunk(n) {
            let (a, b) = chunk.as_slices();
            rec.buf.extend_from_slice(a);
            rec.buf.extend_from_slice(b);
            chunk.commit_all();
        }
        self.shared.rec_frames.store((rec.buf.len() / 2) as u64, Ordering::Relaxed);
        let _ = rec.source;
    }
}

fn out_stream<T>(
    dev: &cpal::Device,
    sc: &cpal::StreamConfig,
    channels: usize,
    mut mixer: Mixer,
    shared: Arc<EngineShared>,
) -> anyhow::Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let mut scratch = vec![0f32; 4096 * 2];
    let stream = dev.build_output_stream(
        sc,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / channels.max(1);
            let mut done = 0;
            while done < frames {
                let n = (frames - done).min(scratch.len() / 2);
                let buf = &mut scratch[..n * 2];
                mixer.render(buf);
                for f in 0..n {
                    let (l, r) = (buf[f * 2], buf[f * 2 + 1]);
                    let frame = &mut data[(done + f) * channels..(done + f + 1) * channels];
                    if channels == 1 {
                        frame[0] = T::from_sample((l + r) * 0.5);
                    } else {
                        frame[0] = T::from_sample(l);
                        frame[1] = T::from_sample(r);
                        for c in frame[2..].iter_mut() {
                            *c = T::from_sample(0.0f32);
                        }
                    }
                }
                done += n;
            }
        },
        move |e| {
            log::error!("Ausgabe-Stream: {e}");
            shared.stream_error.store(true, Ordering::Relaxed);
        },
        None,
    )?;
    Ok(stream)
}

fn in_stream<T>(
    dev: &cpal::Device,
    sc: &cpal::StreamConfig,
    channels: usize,
    mut pass: Option<Producer<f32>>,
    mut rec: Option<Producer<f32>>,
    rec_on: Arc<AtomicBool>,
    shared: Arc<EngineShared>,
) -> anyhow::Result<cpal::Stream>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let sh = shared.clone();
    let stream = dev.build_input_stream(
        sc,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let recording = rec_on.load(Ordering::Relaxed);
            let mut peak = 0.0f32;
            for frame in data.chunks(channels.max(1)) {
                let l = f32::from_sample(frame[0]);
                let r = if channels > 1 { f32::from_sample(frame[1]) } else { l };
                peak = peak.max(l.abs()).max(r.abs());
                if let Some(p) = pass.as_mut() {
                    if p.slots() >= 2 {
                        let _ = p.push(l);
                        let _ = p.push(r);
                    }
                }
                if recording {
                    if let Some(p) = rec.as_mut() {
                        if p.slots() >= 2 {
                            let _ = p.push(l);
                            let _ = p.push(r);
                        }
                    }
                }
            }
            let old = f32::from_bits(sh.rec_peak.load(Ordering::Relaxed));
            if peak > old {
                sh.rec_peak.store(peak.to_bits(), Ordering::Relaxed);
            }
        },
        move |e| {
            log::error!("Eingabe-Stream: {e}");
            shared.stream_error.store(true, Ordering::Relaxed);
        },
        None,
    )?;
    Ok(stream)
}
