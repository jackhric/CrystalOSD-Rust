use std::collections::HashMap;

use scene::font::Font;
use scene::frame::GlyphCache;

use crate::outlines::OutlineFont;
use crate::picture::sharp_picture;

// A rectangle of the atlas, in its pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

// The glyph cache's texture `scale` times larger (scene::text::glyph_cache_image, each texel `scale` x `scale`): each
// cell holds its code's sharp picture where the cache holds the picture, one texel in, with its clear margin; the rest
// of the cell is clear. It follows the scene's cells: a cell is drawn again when its code changes.
pub struct SharpAtlas {
    pub scale: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    layout: (i32, i32, i32, i32, i32),
    cells: Vec<i32>,
    pictures: HashMap<i32, (u32, u32, Vec<u8>)>,
}

impl SharpAtlas {
    pub fn new(scale: u32) -> Self {
        Self { scale: scale.max(1), width: 0, height: 0, rgba: Vec::new(), layout: (0, 0, 0, 0, 0), cells: Vec::new(), pictures: HashMap::new() }
    }

    // The atlas made to hold `cache`; returns the rectangles that changed (all of it when the layout changed).
    pub fn update(&mut self, font: &Font, outlines: &OutlineFont, cache: &GlyphCache) -> Vec<Rect> {
        let k = self.scale;
        let layout = (cache.cell_width, cache.cell_height, cache.width, cache.log_width, cache.log_height);
        let mut changed = Vec::new();
        let full = layout != self.layout || self.rgba.is_empty();
        if full {
            self.layout = layout;
            self.width = (1u32 << cache.log_width.clamp(0, 16)) * k;
            self.height = (1u32 << cache.log_height.clamp(0, 16)) * k;
            self.rgba = vec![0; self.width as usize * self.height as usize * 4];
            self.cells.clear();
            changed.push(Rect { x: 0, y: 0, width: self.width, height: self.height });
        }
        if cache.cell_width <= 0 || cache.cell_height <= 0 {
            return changed;
        }
        let columns = cache.width / cache.cell_width;
        self.cells.resize(cache.cells.len(), 0);
        for (n, &code) in cache.cells.iter().enumerate() {
            if self.cells[n] == code && !full {
                continue;
            }
            self.cells[n] = code;
            let cell = Rect {
                x: (n as i32 % columns * cache.cell_width) as u32 * k,
                y: (n as i32 / columns * cache.cell_height) as u32 * k,
                width: cache.cell_width as u32 * k,
                height: cache.cell_height as u32 * k,
            };
            self.clear(cell);
            if code != 0 {
                if !self.pictures.contains_key(&code)
                    && let Some(p) = sharp_picture(font, code, outlines.glyphs.get(&code), k)
                {
                    self.pictures.insert(code, p);
                }
                if let Some((pw, ph, picture)) = self.pictures.get(&code) {
                    let (pw, ph) = ((*pw).min(cell.width), (*ph).min(cell.height));
                    let full = self.pictures[&code].0 as usize;
                    for y in 0..ph as usize {
                        let from = y * full * 4;
                        let to = ((cell.y as usize + y) * self.width as usize + cell.x as usize) * 4;
                        self.rgba[to..to + pw as usize * 4].copy_from_slice(&picture[from..from + pw as usize * 4]);
                    }
                }
            }
            if !full {
                changed.push(cell);
            }
        }
        changed
    }

    fn clear(&mut self, r: Rect) {
        for y in r.y..(r.y + r.height).min(self.height) {
            let at = (y as usize * self.width as usize + r.x as usize) * 4;
            let end = at + (r.width.min(self.width - r.x) as usize) * 4;
            self.rgba[at..end].fill(0);
        }
    }

    // Draws the pictures of `codes` now, on `threads` threads, so that their entering the cache later costs a copy only.
    pub fn prepare(&mut self, font: &Font, outlines: &OutlineFont, codes: &[i32], threads: usize) {
        let k = self.scale;
        let codes: Vec<i32> = codes.iter().copied().filter(|c| !self.pictures.contains_key(c)).collect();
        let chunk = codes.len().div_ceil(threads.max(1)).max(1);
        std::thread::scope(|scope| {
            let handles: Vec<_> = codes
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || part.iter().filter_map(|&c| sharp_picture(font, c, outlines.glyphs.get(&c), k).map(|p| (c, p))).collect::<Vec<_>>())
                })
                .collect();
            for h in handles {
                self.pictures.extend(h.join().expect("a drawing thread"));
            }
        });
    }

    pub fn largest_alpha(&self) -> u8 {
        self.rgba.iter().skip(3).step_by(4).copied().max().unwrap_or(0)
    }
}
