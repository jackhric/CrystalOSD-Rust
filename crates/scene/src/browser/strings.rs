// The Browser's text (HDD OSD 1.10U NTSC, tools/browser/browser_text.md): each caller's font calls, in the console's order, through
// the program's font code and the library (text.rs, facts/text.md); the glyphs as passes. The measurements matter as much as the
// draws: each one moves its characters to the head of the cache's list, which decides the cells later glyphs take.

use std::sync::Arc;

use super::chrome::BrowserFrame;
use crate::arithmetic::Arithmetic;
use crate::frame::{BlendOp, CoordinateKind, Material, Pass, PassTopology, Sampling, SourceKind, TargetName, Vertex, glyph_texture};
use crate::program_image::ProgramImage;
use crate::text::{Text, TextDrawKind, TextFrame};

const RATIOS: u32 = 0x002b_d140;
const COLOURS: u32 = 0x0034_8b20;
const GUIDE_X: u32 = 0x002b_d2e0;
const TITLE_RATIO: u32 = 0x0036_fe08;
const FREE_RATIO: u32 = 0x0036_fe14;
const GUIDE_RATIO: u32 = 0x0036_fe2c;
const ELLIPSIS: &[u8] = "\u{2026}".as_bytes();

// module_browser_257100 -> func_00214438: scePFontSetTexMem(the sheet allocator D_00370BD0 << 6, 0x120 << 6, the colour-table
// allocator D_00370BD8 << 6) at the thread start; after the eight TEXB sheets D_00370BD0 is 0x2A40 and D_00370BD8 is 0x3E80
// (hddosd-110U-browser-text-session2, the probe at 0x0024EAD8).
pub const CACHE_TEXTURE: u32 = 0x2a40 << 6;
pub const CACHE_MEMORY: u32 = 0x120 << 6;
pub const CACHE_TABLE: u32 = 0x3e80 << 6;

// The words the callers read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextWords {
    pub width: i32,
    pub aspect: i32,
    pub mode: i32,
    pub device: i32,
    pub page_in: i32,
    pub language: i32,
    pub vidmode: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    TitleLeft { record: Vec<u8>, free: Option<Vec<u8>> },
    TitleRight { first: Vec<u8>, second: Vec<u8>, alpha: i32 },
    Dialog { lines: Vec<Vec<u8>>, n: i32 },
    Guide { s4: i32, counter: i32, label: Vec<u8> },
    Format { lines: Vec<Vec<u8>>, heights: Vec<i32>, alpha: i32 },
    Status { text: Vec<u8>, alpha: i32 },
}

#[derive(Debug, Clone, Copy)]
struct Area {
    x0: i32,
    w: i32,
}

fn half(v: i32) -> i32 {
    v / 2
}

// facts/browser-chrome.md "Titles" (0x00250E20..0x00250E70, NTSC).
pub fn title_alpha(c: i32) -> i32 {
    if c < 0xb { 0 } else { ((c - 0xa) * 0x80 - (c - 0xa)) / (0x1a - 0xa) }
}

// facts/browser-devices.md section 4: the status line's fade, which the Format dialog's render is given too.
pub fn status_alpha(empty: i32, page_in: i32) -> i32 {
    ((100 * empty) / 60 * (page_in - 10)) / (0x1a - 10)
}

fn characters(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let n = if b < 0x80 {
            1
        } else if b >> 5 == 6 {
            2
        } else if b >> 4 == 14 {
            3
        } else {
            4
        };
        let end = (i + n).min(bytes.len());
        out.push(&bytes[i..end]);
        i = end;
    }
    out
}

pub struct BrowserText<A: Arithmetic> {
    text: Text<A>,
    program: Arc<ProgramImage>,
    out: TextFrame,
}

impl<A: Arithmetic> BrowserText<A> {
    // The library and the program's font state as the Browser's thread finds them, the cache given its memory at once.
    pub fn new(mut text: Text<A>, program: Arc<ProgramImage>) -> Self {
        text.set_texture_memory(CACHE_TEXTURE, CACHE_MEMORY, CACHE_TABLE);
        BrowserText { text, program, out: TextFrame::default() }
    }

    pub fn text(&self) -> &Text<A> {
        &self.text
    }

    fn table(&self, i: usize) -> f32 {
        self.program.single(RATIOS + 4 * i as u32)
    }

    // func_002128C8: Font_SetColor(the colour table 0x00348B20 [index], alpha).
    fn colour(&mut self, index: u32, alpha: i32) {
        let p = &self.program;
        let at = COLOURS + 12 * index;
        let (r, g, b) = (p.integer(at), p.integer(at + 4), p.integer(at + 8));
        self.text.colour(r, g, b, alpha);
    }

    fn measure(&mut self, text: &[u8], ratio: f32) -> i32 {
        self.text.ratio(ratio);
        self.text.measure(text, &mut self.out)
    }

    fn put(&mut self, text: &[u8], ratio: f32, x: i32, y: i32) {
        self.text.ratio(ratio);
        self.text.locate(x, y);
        self.text.put(text, &mut self.out);
    }

    // func_002433C8 (dst, src, limit, n), the model's `truncate`.
    fn truncate(&mut self, src: &[u8], limit: i32, n: i32, ratio: f32) -> Vec<u8> {
        if src.is_empty() || n < 5 {
            return src.to_vec();
        }
        let room = limit - self.measure(ELLIPSIS, ratio);
        if room < 0 {
            return src.to_vec();
        }
        let chars = characters(src);
        let mut left = n - 1;
        let mut skip = 0;
        let mut saved = 0usize;
        let mut out: Vec<&[u8]> = Vec::new();
        let mut first = true;
        let mut k = 0;
        loop {
            if !first {
                left -= 1;
            }
            first = false;
            if k >= chars.len() {
                return out.concat();
            }
            out.push(chars[k]);
            k += 1;
            if left == 0 {
                return [out[..saved].concat(), ELLIPSIS.to_vec()].concat();
            }
            if skip != 0 {
                skip -= 1;
                saved = out.len();
                continue;
            }
            let width = self.measure(&out.concat(), ratio);
            if width <= room {
                skip = ((room - width) as u32 >> 5) as i32;
                saved = out.len();
                continue;
            }
            if width > limit {
                return [out[..saved].concat(), ELLIPSIS.to_vec()].concat();
            }
        }
    }

    // func_0024D3E0 / func_0024D468, the model's `shrink`.
    fn shrink(&mut self, start: usize, strings: &[&[u8]], area: Area) -> f32 {
        for i in start..8 {
            let ratio = self.table(i);
            let mut fits = true;
            for s in strings {
                if self.measure(s, ratio) >= area.w {
                    fits = false;
                    break;
                }
            }
            if fits {
                return ratio;
            }
        }
        self.table(7)
    }

    fn centred(&mut self, s: &TextWords, text: &[u8], ratio: f32, y: i32) {
        let area = Area { x0: 0x18, w: s.width - 0x30 };
        let width = self.measure(text, ratio);
        self.put(text, ratio, area.x0 + half(area.w) - half(width), y);
    }

    fn place(&mut self, s: &TextWords, place: &Place) {
        match place {
            Place::TitleLeft { record, free } => {
                let s1 = if s.device != 0 { 0x38 } else { 0 };
                let s2 = if s.aspect == 1 { -14 } else { 0 };
                self.colour(if s.mode == 2 { 1 } else { 0 }, title_alpha(s.page_in));
                let area = Area { x0: s1 + 0x18, w: half(s.width) - 4 - (s1 + 0x18) };
                let title_ratio = self.program.single(TITLE_RATIO);
                let text = self.truncate(record, half(s.width) - (s1 + 0x1c), 0x30, title_ratio);
                let ratio = self.shrink(3, &[&text], area);
                self.put(&text, ratio, area.x0, s2 + 0x1f);
                if let Some(free) = free {
                    let free_ratio = self.program.single(FREE_RATIO);
                    self.put(free, free_ratio, s1 + 0x18, s2 + 0x2b);
                }
            }
            Place::TitleRight { first, second, alpha } => {
                let s3 = if s.aspect == 1 { -14 } else { 0 };
                let area = Area { x0: half(s.width) + 8, w: s.width - 0x18 - (half(s.width) + 8) };
                self.colour(1, *alpha);
                let ratio = self.shrink(2, &[first, second], area);
                for (text, y) in [(first, s3 + 0x1f), (second, s3 + 0x2d)] {
                    let width = self.measure(text, ratio);
                    self.put(text, ratio, area.x0 + area.w - width, y);
                }
            }
            Place::Dialog { lines, n } => {
                self.colour(0, (n << 7) / 0x1e);
                let shown: Vec<&Vec<u8>> = lines.iter().take_while(|l| !l.is_empty()).collect();
                let count = shown.len() as i32;
                for (i, line) in shown.iter().enumerate() {
                    self.centred(s, line, 1.0, 16 * i as i32 + 0x68 - 8 * (count - 1));
                }
            }
            Place::Guide { s4, counter, label } => {
                let ratio = self.program.single(GUIDE_RATIO);
                let y = if s.aspect == 2 { 0xb7 } else { 0xc9 };
                let s2 = match (s.vidmode, s4) {
                    (true, 1) => 2,
                    (true, 2) => 1,
                    _ => *s4,
                };
                let x =
                    if *s4 == 3 { s.width - self.measure(label, ratio) - 0x34 } else { self.program.integer(GUIDE_X + 16 * s.language as u32 + 4 * s2 as u32) };
                self.colour(0, (counter * 8).min(0x80));
                self.put(label, ratio, x + 0x1c, y);
            }
            Place::Format { lines, heights, alpha } => {
                assert!(lines.len() >= 2, "a one-line Format dialog state is not modelled");
                self.colour(0, *alpha);
                let area = Area { x0: 0x18, w: s.width - 0x30 };
                let ratio = self.shrink(0, &[&lines[0], &lines[1]], area);
                self.centred(s, &lines[0], ratio, heights[0]);
                self.centred(s, &lines[1], ratio, heights[1]);
                if lines.len() > 2 {
                    self.centred(s, &lines[2], 1.0, heights[2]);
                }
            }
            Place::Status { text, alpha } => {
                self.colour(0, *alpha);
                self.centred(s, text, 1.0, 0x68);
            }
        }
    }

    // The frame's text: every run's calls in order, each run's glyphs placed as passes before the chrome pass the run stands before.
    pub fn draw(&mut self, frame: &mut BrowserFrame, words: &TextWords) {
        self.text.begin_frame(frame.frame.width, frame.frame.height);
        self.out = TextFrame::default();
        let mut glyphs_of: Vec<(usize, Vec<Pass>)> = Vec::new();
        for run in &frame.texts {
            let Some(place) = &run.place else { continue };
            let before = self.out.draws.len();
            self.place(words, place);
            let passes = self.out.draws[before..].iter().filter(|d| d.kind == TextDrawKind::Glyph).map(glyph_pass).collect();
            glyphs_of.push((run.before_pass, passes));
        }
        self.text.end_frame(&mut self.out);
        let chrome = std::mem::take(&mut frame.frame.passes);
        let mut passes = Vec::with_capacity(chrome.len() + 64);
        let mut runs = glyphs_of.into_iter().peekable();
        for (k, pass) in chrome.into_iter().enumerate() {
            while let Some((_, glyphs)) = runs.next_if(|(at, _)| *at == k) {
                passes.extend(glyphs);
            }
            passes.push(pass);
        }
        for (_, glyphs) in runs {
            passes.extend(glyphs);
        }
        frame.frame.passes = passes;
        frame.frame.glyphs = self.out.glyphs.clone();
        frame.frame.glyph_layouts = self.out.glyph_layouts.clone();
    }
}

// facts/text.md section 3: a glyph is a fan of twelve vertices (PRIM 0x5D: Gouraud, textured, blended, ST) of the glyph cache,
// region clamp to its cell, bilinear, blend (Cs - Cd) x As + Cd, depth test always, drawn with the field's half line.
fn glyph_pass(d: &crate::text::TextDraw) -> Pass {
    let material = Material {
        source: SourceKind::Texture,
        texture: glyph_texture(d.layout),
        coordinates: CoordinateKind::Projective,
        sampling: Sampling::ClampToRegion,
        region: d.glyph.region,
        bilinear: true,
        blend: BlendOp::AlphaOver,
        gouraud: true,
        ..Material::default()
    };
    let fan: Vec<Vertex> = d
        .glyph
        .fan
        .iter()
        .map(|g| Vertex { x: g.x, y: g.y, z: 0, u: g.s, v: g.t, q: g.q, r: g.colour[0], g: g.colour[1], b: g.colour[2], a: g.colour[3] })
        .collect();
    let mut vertices = Vec::with_capacity(30);
    for k in 1..fan.len() - 1 {
        vertices.extend([fan[0], fan[k], fan[k + 1]]);
    }
    Pass { name: "browser text".into(), target: TargetName::Display, topology: PassTopology::Triangles, material, half_line: true, vertices, ..Pass::default() }
}
