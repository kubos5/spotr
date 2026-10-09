//! Cover art: downloading, caching and rendering (terminal graphics or text).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use image::{DynamicImage, GenericImageView, RgbImage, imageops::FilterType};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};
use ratatui::widgets::StatefulWidget;

use crate::config::TextArt;

const MAX_IMAGES: usize = 48;
const MAX_PROTOCOLS: usize = 12;

type TextKey = (String, u16, u16, TextArt, u32);

pub struct ImageStore {
    images: HashMap<String, Arc<DynamicImage>>,
    order: VecDeque<String>,
    pub pending: HashSet<String>,
    pub failed: HashSet<String>,
    protocols: HashMap<String, StatefulProtocol>,
    proto_order: VecDeque<String>,
    text_cache: HashMap<TextKey, Arc<RgbImage>>,
    pub picker: Picker,
    /// True when the terminal supports a real graphics protocol.
    pub graphics: bool,
    disk_dir: PathBuf,
}

impl ImageStore {
    pub fn new(picker: Picker, disk_dir: PathBuf) -> Self {
        let graphics = picker.protocol_type() != ProtocolType::Halfblocks;
        let _ = std::fs::create_dir_all(&disk_dir);
        Self {
            images: HashMap::new(),
            order: VecDeque::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
            protocols: HashMap::new(),
            proto_order: VecDeque::new(),
            text_cache: HashMap::new(),
            picker,
            graphics,
            disk_dir,
        }
    }

    pub fn protocol_name(&self) -> &'static str {
        match self.picker.protocol_type() {
            ProtocolType::Kitty => "Kitty graphics",
            ProtocolType::Iterm2 => "iTerm2 inline images",
            ProtocolType::Sixel => "Sixel",
            ProtocolType::Halfblocks => "none (text only)",
        }
    }

    /// Terminal cell aspect ratio (height / width), used to keep covers square.
    pub fn cell_aspect(&self) -> f32 {
        let fs = self.picker.font_size();
        if fs.width == 0 || fs.height == 0 { 2.0 } else { fs.height as f32 / fs.width as f32 }
    }

    pub fn get(&self, url: &str) -> Option<Arc<DynamicImage>> {
        self.images.get(url).cloned()
    }

    /// Returns true if a download should be started for `url`.
    pub fn want(&mut self, url: &str) -> bool {
        if url.is_empty() || self.images.contains_key(url) || self.pending.contains(url) || self.failed.contains(url) {
            return false;
        }
        self.pending.insert(url.to_string());
        true
    }

    pub fn insert(&mut self, url: String, img: DynamicImage) {
        self.pending.remove(&url);
        self.images.insert(url.clone(), Arc::new(img));
        self.order.push_back(url);
        while self.order.len() > MAX_IMAGES {
            if let Some(old) = self.order.pop_front() {
                self.images.remove(&old);
                self.protocols.remove(&old);
                self.text_cache.retain(|k, _| k.0 != old);
            }
        }
    }

    pub fn fail(&mut self, url: String) {
        self.pending.remove(&url);
        self.failed.insert(url);
    }

    pub fn disk_path(&self, url: &str) -> PathBuf {
        let name = crate::spotify::models::uri_id(url).replace(|c: char| !c.is_ascii_alphanumeric(), "_");
        self.disk_dir.join(format!("{name}.img"))
    }

    /// Square-ish rect (in cells) for a cover that fits in `avail`, centered.
    pub fn cover_rect(&self, avail: Rect) -> Rect {
        let aspect = self.cell_aspect();
        let mut h = avail.height;
        let mut w = (h as f32 * aspect).round() as u16;
        if w > avail.width {
            w = avail.width;
            h = (w as f32 / aspect).round() as u16;
        }
        Rect {
            x: avail.x + (avail.width - w) / 2,
            y: avail.y + (avail.height.saturating_sub(h)) / 2,
            width: w,
            height: h.max(1).min(avail.height),
        }
    }

    /// Draw with the terminal's graphics protocol.
    pub fn render_graphics(&mut self, url: &str, area: Rect, buf: &mut Buffer) -> bool {
        let Some(img) = self.images.get(url).cloned() else { return false };
        if !self.protocols.contains_key(url) {
            let proto = self.picker.new_resize_protocol((*img).clone());
            self.protocols.insert(url.to_string(), proto);
            self.proto_order.push_back(url.to_string());
            while self.proto_order.len() > MAX_PROTOCOLS {
                if let Some(old) = self.proto_order.pop_front() {
                    self.protocols.remove(&old);
                }
            }
        }
        let proto = self.protocols.get_mut(url).unwrap();
        StatefulImage::<StatefulProtocol>::default()
            .resize(Resize::Scale(Some(FilterType::Triangle)))
            .render(area, buf, proto);
        true
    }

    /// Draw a pixelated / ASCII rendition using text cells.
    pub fn render_text(&mut self, url: &str, area: Rect, buf: &mut Buffer, art: TextArt, pixelation: u32, dim: bool) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }
        let Some(img) = self.images.get(url).cloned() else { return false };
        let (pw, ph) = match art {
            TextArt::Blocks => (area.width as u32, area.height as u32 * 2),
            TextArt::Braille => (area.width as u32 * 2, area.height as u32 * 4),
            TextArt::Ascii | TextArt::AsciiMono => (area.width as u32, area.height as u32),
        };
        let key = (url.to_string(), area.width, area.height, art, pixelation);
        let grid = match self.text_cache.get(&key) {
            Some(g) => g.clone(),
            None => {
                let small = if pixelation > 0 {
                    // Downsample to a coarse grid first, then blow it up with nearest-neighbour
                    // so the pixels stay crisp and blocky.
                    let p = img.resize_exact(pixelation, pixelation, FilterType::Triangle);
                    p.resize_exact(pw, ph, FilterType::Nearest).to_rgb8()
                } else {
                    img.resize_exact(pw, ph, FilterType::Triangle).to_rgb8()
                };
                let g = Arc::new(small);
                if self.text_cache.len() > 64 {
                    self.text_cache.clear();
                }
                self.text_cache.insert(key, g.clone());
                g
            }
        };
        draw_grid(&grid, area, buf, art, dim);
        true
    }

    pub fn clear_protocols(&mut self) {
        self.protocols.clear();
        self.proto_order.clear();
    }
}

fn rgb(p: &image::Rgb<u8>, dim: bool) -> Color {
    if dim {
        Color::Rgb(p[0] / 3, p[1] / 3, p[2] / 3)
    } else {
        Color::Rgb(p[0], p[1], p[2])
    }
}

fn luma(p: &image::Rgb<u8>) -> f32 {
    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
}

const RAMP: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

fn draw_grid(grid: &RgbImage, area: Rect, buf: &mut Buffer, art: TextArt, dim: bool) {
    for cy in 0..area.height {
        for cx in 0..area.width {
            let x = area.x + cx;
            let y = area.y + cy;
            let Some(cell) = buf.cell_mut((x, y)) else { continue };
            match art {
                TextArt::Blocks => {
                    let top = grid.get_pixel(cx as u32, cy as u32 * 2);
                    let bot = grid.get_pixel(cx as u32, cy as u32 * 2 + 1);
                    cell.set_char('▀').set_style(Style::new().fg(rgb(top, dim)).bg(rgb(bot, dim)));
                }
                TextArt::Ascii | TextArt::AsciiMono => {
                    let p = grid.get_pixel(cx as u32, cy as u32);
                    let l = luma(p);
                    let ch = RAMP[((l * (RAMP.len() - 1) as f32).round() as usize).min(RAMP.len() - 1)];
                    let style = if art == TextArt::Ascii {
                        Style::new().fg(rgb(p, dim))
                    } else if dim {
                        Style::new().add_modifier(Modifier::DIM)
                    } else {
                        Style::new()
                    };
                    cell.set_char(ch).set_style(style);
                }
                TextArt::Braille => {
                    // 2x4 dots per cell; light pixels become dots.
                    const BITS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
                    let mut bits = 0u32;
                    let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
                    for (dy, row) in BITS.iter().enumerate() {
                        for (dx, bit) in row.iter().enumerate() {
                            let p = grid.get_pixel(cx as u32 * 2 + dx as u32, cy as u32 * 4 + dy as u32);
                            if luma(p) > 0.35 {
                                bits |= bit;
                                r += p[0] as u32;
                                g += p[1] as u32;
                                b += p[2] as u32;
                                n += 1;
                            }
                        }
                    }
                    let ch = char::from_u32(0x2800 + bits).unwrap_or(' ');
                    let color = match (r.checked_div(n), g.checked_div(n), b.checked_div(n)) {
                        (Some(r), Some(g), Some(b)) => rgb(&image::Rgb([r as u8, g as u8, b as u8]), dim),
                        _ => Color::Reset,
                    };
                    cell.set_char(ch).set_style(Style::new().fg(color));
                }
            }
        }
    }
}

/// Download (or read from disk) and decode an image.
pub async fn fetch(http: reqwest::Client, url: String, disk: PathBuf) -> anyhow::Result<DynamicImage> {
    if let Some(seed) = url.strip_prefix("demo://") {
        return Ok(crate::spotify::demo::image(seed));
    }
    let bytes = match tokio::fs::read(&disk).await {
        Ok(b) => b,
        Err(_) => {
            let b = http.get(&url).send().await?.error_for_status()?.bytes().await?.to_vec();
            let _ = tokio::fs::write(&disk, &b).await;
            b
        }
    };
    let img = tokio::task::spawn_blocking(move || image::load_from_memory(&bytes)).await??;
    // Keep memory in check: covers are at most 640px.
    let img = if img.dimensions().0 > 640 { img.resize(640, 640, FilterType::Triangle) } else { img };
    Ok(img)
}

/// Draw a placeholder "vinyl" when there's no art yet.
pub fn render_placeholder(area: Rect, buf: &mut Buffer, unicode: bool, spin: f32) {
    if area.width < 3 || area.height < 2 {
        return;
    }
    let cx = area.width as f32 / 2.0;
    let cy = area.height as f32 / 2.0;
    let r = (area.height as f32 / 2.0).min(area.width as f32 / 4.0);
    for y in 0..area.height {
        for x in 0..area.width {
            let dx = (x as f32 + 0.5 - cx) / 2.0;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let ang = dy.atan2(dx) + spin;
            let ch = if d < r * 0.18 {
                if unicode { '●' } else { 'o' }
            } else if d < r {
                let groove = ((d * 3.0) as i32 % 2 == 0) ^ (ang.sin() > 0.6);
                if groove { if unicode { '░' } else { '.' } } else { ' ' }
            } else {
                continue;
            };
            if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                cell.set_char(ch).set_style(Style::new().fg(Color::DarkGray));
            }
        }
    }
}
