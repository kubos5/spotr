//! State shared between the built-in player's audio thread and the UI.
//! Always compiled (the visualizer reads it); only filled by `local-player`.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const TAP_LEN: usize = 4096;

pub struct AudioShared {
    /// Most recent mono samples actually sent to the speakers.
    pub tap: Mutex<VecDeque<f32>>,
    pub sample_rate: AtomicU32,
    speed_bits: AtomicU32,
    /// True while audio frames are flowing.
    pub playing: AtomicBool,
    /// Requested output device name ("" = default).
    pub device: Mutex<String>,
    pub reopen: AtomicBool,
}

impl Default for AudioShared {
    fn default() -> Self {
        Self {
            tap: Mutex::new(VecDeque::with_capacity(TAP_LEN)),
            sample_rate: AtomicU32::new(44_100),
            speed_bits: AtomicU32::new(1.0f32.to_bits()),
            playing: AtomicBool::new(false),
            device: Mutex::new(String::new()),
            reopen: AtomicBool::new(false),
        }
    }
}

// The writers live in the `local-player` feature.
#[cfg_attr(not(feature = "local-player"), allow(dead_code))]
impl AudioShared {
    pub fn speed(&self) -> f32 {
        f32::from_bits(self.speed_bits.load(Ordering::Relaxed))
    }
    pub fn set_speed(&self, s: f32) {
        self.speed_bits.store(s.to_bits(), Ordering::Relaxed);
    }

    pub fn push_tap(&self, samples: &[f32]) {
        if let Ok(mut tap) = self.tap.lock() {
            for &s in samples {
                if tap.len() >= TAP_LEN {
                    tap.pop_front();
                }
                tap.push_back(s);
            }
        }
    }

    /// Copy of the last `n` samples (zero-padded at the front).
    pub fn snapshot(&self, n: usize) -> Vec<f32> {
        let tap = self.tap.lock().map(|t| t.clone()).unwrap_or_default();
        let mut out = vec![0.0; n.saturating_sub(tap.len())];
        out.extend(tap.iter().skip(tap.len().saturating_sub(n)));
        out
    }

    pub fn set_device(&self, name: &str) {
        *self.device.lock().unwrap() = name.to_string();
        self.reopen.store(true, Ordering::Relaxed);
    }
}
