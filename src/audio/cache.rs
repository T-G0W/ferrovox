use super::buffer::SoundBuffer;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Entry {
    buf: Arc<SoundBuffer>,
    path: PathBuf,
    tick: u64,
}

#[derive(Default)]
struct Inner {
    map: HashMap<u64, Entry>,
    bytes: usize,
    tick: u64,
}

pub struct SoundCache {
    inner: Mutex<Inner>,
    cap: AtomicUsize,
}

impl SoundCache {
    pub fn new(cap_mb: u32) -> SoundCache {
        SoundCache { inner: Mutex::new(Inner::default()), cap: AtomicUsize::new(cap_mb as usize * 1024 * 1024) }
    }

    pub fn set_capacity(&self, cap_mb: u32) {
        self.cap.store(cap_mb.max(32) as usize * 1024 * 1024, Ordering::Relaxed);
    }

    pub fn peek(&self, id: u64) -> Option<Arc<SoundBuffer>> {
        let mut g = self.inner.lock();
        g.tick += 1;
        let tick = g.tick;
        g.map.get_mut(&id).map(|e| {
            e.tick = tick;
            e.buf.clone()
        })
    }

    pub fn get(&self, id: u64, path: &Path) -> anyhow::Result<Arc<SoundBuffer>> {
        {
            let mut g = self.inner.lock();
            g.tick += 1;
            let tick = g.tick;
            if let Some(e) = g.map.get_mut(&id) {
                if e.path == path {
                    e.tick = tick;
                    return Ok(e.buf.clone());
                }
            }
        }
        let buf = Arc::new(super::decode::decode_file(path)?);
        self.insert(id, path, buf.clone());
        Ok(buf)
    }

    pub fn insert(&self, id: u64, path: &Path, buf: Arc<SoundBuffer>) {
        let cap = self.cap.load(Ordering::Relaxed);
        let mut g = self.inner.lock();
        g.tick += 1;
        let tick = g.tick;
        if let Some(old) = g.map.insert(id, Entry { buf: buf.clone(), path: path.to_path_buf(), tick }) {
            g.bytes -= old.buf.bytes();
        }
        g.bytes += buf.bytes();
        while g.bytes > cap && g.map.len() > 1 {
            let victim = g.map.iter().filter(|(k, _)| **k != id).min_by_key(|(_, e)| e.tick).map(|(k, _)| *k);
            match victim {
                Some(v) => {
                    if let Some(e) = g.map.remove(&v) {
                        g.bytes -= e.buf.bytes();
                    }
                }
                None => break,
            }
        }
    }

    pub fn invalidate(&self, id: u64) {
        let mut g = self.inner.lock();
        if let Some(e) = g.map.remove(&id) {
            g.bytes -= e.buf.bytes();
        }
    }

    pub fn clear(&self) {
        let mut g = self.inner.lock();
        g.map.clear();
        g.bytes = 0;
    }

    pub fn usage_mb(&self) -> f32 {
        self.inner.lock().bytes as f32 / (1024.0 * 1024.0)
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map.len()
    }
}
