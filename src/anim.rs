//! Small animation toolkit: easing, timelines and text effects.

use std::time::{Duration, Instant};

/// Global animation settings, shared by every effect.
#[derive(Clone, Copy)]
pub struct Motion {
    pub enabled: bool,
    pub scale: f32,
    pub text_effects: bool,
}

impl Motion {
    pub fn dur(&self, ms: u64) -> Duration {
        Duration::from_millis((ms as f32 * self.scale) as u64)
    }

    /// Linear progress 0..=1 of an animation that started at `start`.
    pub fn t(&self, start: Instant, ms: u64) -> f32 {
        if !self.enabled {
            return 1.0;
        }
        let d = self.dur(ms).as_secs_f32().max(0.001);
        (start.elapsed().as_secs_f32() / d).clamp(0.0, 1.0)
    }

    pub fn active(&self, start: Instant, ms: u64) -> bool {
        self.enabled && start.elapsed() < self.dur(ms)
    }
}

pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

pub fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Cheap deterministic hash for per-cell noise.
pub fn hash(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    x
}

const SCRAMBLE: &[char] = &['▓', '▒', '░', '#', '%', '&', '@', '$', '*', '+', '=', '?', '/', '<', '>'];
const SCRAMBLE_ASCII: &[char] = &['#', '%', '&', '@', '$', '*', '+', '=', '?', '/', '<', '>'];

/// "Decrypt" effect: characters resolve left-to-right out of noise.
pub fn scramble(text: &str, t: f32, frame: u64, unicode: bool) -> String {
    if t >= 1.0 {
        return text.to_string();
    }
    let set = if unicode { SCRAMBLE } else { SCRAMBLE_ASCII };
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len().max(1) as f32;
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let reveal = (i as f32 / n) * 0.7;
            if c == ' ' || t > reveal + 0.3 {
                c
            } else if t < reveal * 0.5 {
                ' '
            } else {
                set[(hash(i as u64 * 7919 + frame / 2) % set.len() as u64) as usize]
            }
        })
        .collect()
}

/// Typewriter: reveal the first `t * len` characters, with a cursor.
pub fn typewriter(text: &str, t: f32) -> String {
    if t >= 1.0 {
        return text.to_string();
    }
    let n = text.chars().count();
    let shown = ((n as f32) * t).ceil() as usize;
    let mut s: String = text.chars().take(shown).collect();
    if shown < n {
        s.push('▌');
    }
    s
}

/// A decaying horizontal shake offset (for errors).
pub fn shake(t: f32) -> i16 {
    if t >= 1.0 {
        return 0;
    }
    let amp = (1.0 - t) * 3.0;
    ((t * 40.0).sin() * amp).round() as i16
}

/// Sinusoidal 0..1 pulse with the given period.
pub fn pulse(period_ms: u64) -> f32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let phase = (now % period_ms) as f32 / period_ms as f32;
    0.5 - 0.5 * (phase * std::f32::consts::TAU).cos()
}

/// Seconds since process start, for continuous effects.
pub fn clock() -> f32 {
    use std::sync::OnceLock;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f32()
}
