//! Audio visualizer: spectrum analysis of the built-in player's output, or a
//! simulated spectrum when audio is playing on another device.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::audio::AudioShared;
use crate::config::{VisColors, VisStyle};
use crate::theme::Theme;

const FFT_N: usize = 2048;
const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const LEVELS_ASCII: [char; 8] = ['.', '.', '-', '-', '=', '=', '#', '#'];

pub struct Visualizer {
    bands: Vec<f32>,
    peaks: Vec<f32>,
    peak_vel: Vec<f32>,
    wave: Vec<f32>,
    last: Instant,
    t: f32,
    /// Whether the last update used real audio.
    pub real: bool,
    window: Vec<f32>,
}

impl Default for Visualizer {
    fn default() -> Self {
        let window = (0..FFT_N)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_N as f32 - 1.0)).cos())
            .collect();
        Self {
            bands: Vec::new(),
            peaks: Vec::new(),
            peak_vel: Vec::new(),
            wave: Vec::new(),
            last: Instant::now(),
            t: 0.0,
            real: false,
            window,
        }
    }
}

/// In-place iterative radix-2 FFT on (re, im).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

impl Visualizer {
    /// Advance the analysis. `n` is the number of bands to produce.
    pub fn update(&mut self, n: usize, audio: Option<&AudioShared>, playing: bool, sensitivity: f32, speed: f32) {
        let dt = self.last.elapsed().as_secs_f32().min(0.1);
        self.last = Instant::now();
        let n = n.max(1);
        if self.bands.len() != n {
            self.bands = vec![0.0; n];
            self.peaks = vec![0.0; n];
            self.peak_vel = vec![0.0; n];
        }
        if playing {
            self.t += dt * speed.max(0.25);
        }

        let mut target = vec![0.0f32; n];
        let real_audio = audio.filter(|a| a.playing.load(std::sync::atomic::Ordering::Relaxed));
        self.real = real_audio.is_some();
        if let Some(a) = real_audio {
            let samples = a.snapshot(FFT_N);
            let sr = a.sample_rate.load(std::sync::atomic::Ordering::Relaxed).max(8000) as f32;
            self.wave = samples[FFT_N - 512..].to_vec();
            let mut re: Vec<f32> = samples.iter().zip(&self.window).map(|(s, w)| s * w).collect();
            let mut im = vec![0.0f32; FFT_N];
            fft(&mut re, &mut im);
            let mags: Vec<f32> = (0..FFT_N / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
            let fmin = 40.0f32;
            let fmax = (sr / 2.0).min(16_000.0);
            for (b, t) in target.iter_mut().enumerate() {
                let f0 = fmin * (fmax / fmin).powf(b as f32 / n as f32);
                let f1 = fmin * (fmax / fmin).powf((b + 1) as f32 / n as f32);
                let k0 = ((f0 * FFT_N as f32 / sr) as usize).clamp(1, FFT_N / 2 - 1);
                let k1 = ((f1 * FFT_N as f32 / sr) as usize).clamp(k0 + 1, FFT_N / 2);
                let m = mags[k0..k1].iter().copied().fold(0.0f32, f32::max);
                // Gentle treble boost so the spectrum isn't bass-heavy.
                let tilt = 1.0 + (b as f32 / n as f32) * 2.5;
                let db = 20.0 * (m * tilt * sensitivity / (FFT_N as f32 / 8.0)).max(1e-6).log10();
                *t = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
            }
        } else if playing {
            // Simulated: layered oscillators with a "kick" envelope.
            let beat_phase = (self.t * 2.0).fract();
            let kick = (-beat_phase * 6.0).exp();
            for (b, t) in target.iter_mut().enumerate() {
                let x = b as f32 / n as f32;
                // Pink-ish falloff toward the treble, with per-band character.
                let base = 0.85 * (1.0 - x * 0.65);
                let h = crate::anim::hash(b as u64) as f32 / u64::MAX as f32;
                let osc = 0.5 + 0.5 * (self.t * (1.1 + h * 3.5) + h * 40.0).sin();
                let osc2 = 0.5 + 0.5 * (self.t * (0.6 + x * 2.5) + b as f32 * 1.7).sin();
                let osc3 = 0.5 + 0.5 * (self.t * 0.37 + x * 9.0).sin();
                let shaped = (osc * osc2).powf(1.4) * (0.4 + 0.6 * osc3);
                let low = if x < 0.22 { kick * (1.0 - x / 0.22) } else { 0.0 };
                let hat = if x > 0.6 { (-(((self.t * 4.0 + 0.5).fract()) * 10.0)).exp() * 0.35 } else { 0.0 };
                *t = ((base * (0.08 + 0.85 * shaped) + low * 0.55 + hat) * sensitivity.sqrt()).clamp(0.0, 1.0);
            }
            self.wave = (0..512)
                .map(|i| {
                    let p = i as f32 / 512.0 * std::f32::consts::TAU;
                    let s = (p * 3.0 + self.t * 9.0).sin() * 0.5
                        + (p * 7.0 - self.t * 5.0).sin() * 0.25
                        + (p * 13.0 + self.t * 3.0).sin() * 0.12;
                    s * (0.6 + 0.4 * kick)
                })
                .collect();
        } else {
            for w in self.wave.iter_mut() {
                *w *= 0.85;
            }
        }

        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let cur = self.bands[i];
            let tgt = target[i];
            self.bands[i] = if tgt > cur {
                cur + (tgt - cur) * (1.0 - (-dt * 30.0).exp())
            } else {
                (cur - dt * 1.6).max(tgt)
            };
            if self.bands[i] >= self.peaks[i] {
                self.peaks[i] = self.bands[i];
                self.peak_vel[i] = 0.0;
            } else {
                self.peak_vel[i] += dt * 2.2;
                self.peaks[i] = (self.peaks[i] - self.peak_vel[i] * dt).max(self.bands[i]);
            }
        }
    }

    /// How many bands fit in `width` for the given style.
    pub fn bands_for(width: u16, style: VisStyle) -> usize {
        match style {
            VisStyle::Wave => 1,
            VisStyle::Dots => (width as usize).max(1),
            _ => (width as usize).div_ceil(2).max(1),
        }
    }

    pub fn idle(&self) -> bool {
        self.bands.iter().all(|b| *b < 0.002) && self.peaks.iter().all(|p| *p < 0.002)
    }

    fn color(&self, colors: VisColors, theme: &Theme, x: f32, level: f32, bright: bool) -> Style {
        match colors {
            VisColors::Spectrum => {
                let pal = theme.spectrum();
                let idx = ((x * 0.7 + level * 0.3) * (pal.len() - 1) as f32).round() as usize;
                Style::new().fg(pal[idx.min(pal.len() - 1)])
            }
            VisColors::Accent => Style::new().fg(if bright { theme.accent_bright } else { theme.accent }),
            VisColors::Mono => {
                if bright { Style::new().add_modifier(Modifier::BOLD) } else { Style::new() }
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, style: VisStyle, colors: VisColors, theme: &Theme, peaks: bool) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        match style {
            VisStyle::Bars => self.render_bars(area, buf, colors, theme, peaks, false),
            VisStyle::Mirror => self.render_bars(area, buf, colors, theme, peaks, true),
            VisStyle::Blocks => self.render_blocks(area, buf, colors, theme, peaks),
            VisStyle::Wave => self.render_wave(area, buf, colors, theme),
            VisStyle::Dots => self.render_dots(area, buf, colors, theme),
        }
    }

    fn render_bars(&self, area: Rect, buf: &mut Buffer, colors: VisColors, theme: &Theme, peaks: bool, mirror: bool) {
        let levels = if theme.unicode { LEVELS } else { LEVELS_ASCII };
        let n = self.bands.len();
        let used = (n * 2).min(area.width as usize) as u16;
        let x0 = area.x + (area.width - used) / 2;
        let h = area.height as f32;
        for (i, &v) in self.bands.iter().enumerate() {
            let x = x0 + (i as u16) * 2;
            if x >= area.right() {
                break;
            }
            let xf = i as f32 / n.max(1) as f32;
            if mirror {
                // Bars grow up and down from the middle line, using half blocks.
                let half = h / 2.0;
                let units = (v * half * 2.0).round() as i32; // in half-cells
                let mid = area.y as f32 + half;
                for row in 0..area.height {
                    let cy = area.y + row;
                    let center_dist = ((cy as f32 + 0.5) - mid).abs();
                    let cell_units = (center_dist * 2.0).round() as i32;
                    let ch = if cell_units < units {
                        '█'
                    } else if cell_units == units && units > 0 {
                        if (cy as f32) < mid { '▄' } else { '▀' }
                    } else {
                        continue;
                    };
                    let lvl = 1.0 - center_dist / half.max(1.0);
                    if let Some(c) = buf.cell_mut((x, cy)) {
                        c.set_char(if theme.unicode { ch } else { '#' })
                            .set_style(self.color(colors, theme, xf, 1.0 - lvl, false));
                    }
                }
                continue;
            }
            let total = v * h * 8.0;
            for row in 0..area.height {
                let cy = area.bottom() - 1 - row;
                let fill = total - row as f32 * 8.0;
                let lvl = row as f32 / h;
                if fill >= 8.0 {
                    if let Some(c) = buf.cell_mut((x, cy)) {
                        c.set_char(levels[7]).set_style(self.color(colors, theme, xf, lvl, false));
                    }
                } else if fill > 0.5
                    && let Some(c) = buf.cell_mut((x, cy)) {
                        c.set_char(levels[(fill as usize).min(7)]).set_style(self.color(colors, theme, xf, lvl, false));
                    }
            }
            if peaks {
                let p = self.peaks[i];
                let prow = (p * h).floor() as u16;
                if p > 0.02 && prow < area.height {
                    let cy = area.bottom() - 1 - prow;
                    if let Some(c) = buf.cell_mut((x, cy))
                        && c.symbol() == " " {
                            c.set_char(if theme.unicode { '▔' } else { '-' })
                                .set_style(self.color(colors, theme, xf, 1.0, true));
                        }
                }
            }
        }
    }

    fn render_blocks(&self, area: Rect, buf: &mut Buffer, colors: VisColors, theme: &Theme, peaks: bool) {
        let n = self.bands.len();
        let used = (n * 2).min(area.width as usize) as u16;
        let x0 = area.x + (area.width - used) / 2;
        let h = area.height as f32;
        for (i, &v) in self.bands.iter().enumerate() {
            let x = x0 + (i as u16) * 2;
            if x >= area.right() {
                break;
            }
            let lit = (v * h).round() as u16;
            let peak = (self.peaks[i] * h).round() as u16;
            for row in 0..area.height {
                let cy = area.bottom() - 1 - row;
                let lvl = row as f32 / h;
                let style = match colors {
                    VisColors::Spectrum => Style::new().fg(if lvl > 0.75 {
                        Color::Red
                    } else if lvl > 0.5 {
                        Color::Yellow
                    } else {
                        Color::Green
                    }),
                    _ => self.color(colors, theme, 0.0, lvl, false),
                };
                let ch = if row < lit {
                    if theme.unicode { '■' } else { '#' }
                } else if peaks && row + 1 == peak && peak > 0 {
                    if theme.unicode { '□' } else { 'o' }
                } else {
                    if theme.unicode { '·' } else { '.' }
                };
                let style = if row < lit || (peaks && row + 1 == peak) { style } else { Style::new().fg(Color::DarkGray) };
                if let Some(c) = buf.cell_mut((x, cy)) {
                    c.set_char(ch).set_style(style);
                }
            }
        }
    }

    fn render_dots(&self, area: Rect, buf: &mut Buffer, colors: VisColors, theme: &Theme) {
        let n = self.bands.len();
        let h = area.height as f32;
        for (i, &v) in self.bands.iter().enumerate() {
            let x = area.x + i as u16;
            if x >= area.right() {
                break;
            }
            let xf = i as f32 / n.max(1) as f32;
            let row = ((v * h).floor() as u16).min(area.height - 1);
            let cy = area.bottom() - 1 - row;
            if v > 0.02 {
                if let Some(c) = buf.cell_mut((x, cy)) {
                    c.set_char(if theme.unicode { '•' } else { 'o' })
                        .set_style(self.color(colors, theme, xf, v, true));
                }
                for r in 0..row {
                    let cy = area.bottom() - 1 - r;
                    if let Some(c) = buf.cell_mut((x, cy)) {
                        c.set_char(if theme.unicode { '·' } else { '.' })
                            .set_style(self.color(colors, theme, xf, r as f32 / h, false).add_modifier(Modifier::DIM));
                    }
                }
            }
        }
    }

    fn render_wave(&self, area: Rect, buf: &mut Buffer, colors: VisColors, theme: &Theme) {
        if self.wave.is_empty() {
            return;
        }
        let dots_w = area.width as usize * 2;
        let dots_h = area.height as usize * 4;
        let mut grid = vec![0u8; area.width as usize * area.height as usize];
        let mut prev: Option<usize> = None;
        for dx in 0..dots_w {
            let s = self.wave[dx * self.wave.len() / dots_w];
            let y = ((0.5 - s.clamp(-1.0, 1.0) * 0.48) * (dots_h - 1) as f32).round() as usize;
            let (lo, hi) = match prev {
                Some(p) if p < y => (p + 1, y),
                Some(p) if p > y => (y, p - 1),
                _ => (y, y),
            };
            for yy in lo..=hi {
                let cx = dx / 2;
                let cy = yy / 4;
                const BITS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
                grid[cy * area.width as usize + cx] |= BITS[yy % 4][dx % 2];
            }
            prev = Some(y);
        }
        for cy in 0..area.height {
            for cx in 0..area.width {
                let bits = grid[cy as usize * area.width as usize + cx as usize];
                if bits == 0 {
                    continue;
                }
                let ch = if theme.unicode { char::from_u32(0x2800 + bits as u32).unwrap_or(' ') } else { '*' };
                let xf = cx as f32 / area.width as f32;
                if let Some(c) = buf.cell_mut((area.x + cx, area.y + cy)) {
                    c.set_char(ch).set_style(self.color(colors, theme, xf, 0.5, false));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_finds_tone() {
        let n = 1024;
        let k = 37;
        let mut re: Vec<f32> = (0..n).map(|i| (std::f32::consts::TAU * k as f32 * i as f32 / n as f32).sin()).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let mags: Vec<f32> = (0..n / 2).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt()).collect();
        let max = mags.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        assert_eq!(max, k);
    }
}
