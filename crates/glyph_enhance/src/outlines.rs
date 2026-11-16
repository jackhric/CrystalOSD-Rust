use std::collections::BTreeMap;
use std::fmt::Write as _;

use scene::font::Font;

use crate::enhance::{Technique, faithfulness, prepare_with};
use crate::layers::{Layers, is_console_table};
use crate::trace::{Contour, TraceParameters, simplify};

// The version changes whenever tracing changes, so outlines kept by an earlier build are traced again.
pub const FORMAT: &str = "crystalosd-sharp-outlines 2";

// A glyph whose outlines, drawn at 2x and averaged back, differ more than this from its picture keeps the picture: the
// grey plates of the marks 0xD80D..0xD810 are pictures, not a white core in a dark outline.
pub const FALLBACK_EDGE_ERROR: f64 = 0.06;
pub const FALLBACK_PIXEL_ERROR: f64 = 0.7;

const SIMPLIFY: f64 = 0.01;

// The traced outlines of one glyph picture, in picture coordinates (a pixel spans [i, i + 1)): the white core's and the
// dark outline's outer edge.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphOutline {
    pub width: i32,
    pub height: i32,
    pub core: Vec<Contour>,
    pub outer: Vec<Contour>,
}

// The outlines of one FNTOSD (`font_hash` of its file): the glyphs not in it keep their pictures.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutlineFont {
    pub font_hash: u64,
    pub glyphs: BTreeMap<i32, GlyphOutline>,
}

pub fn font_hash(file: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in file {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// Every code of the font's first block (Latin, Greek, Cyrillic, the marks): the blocks of Japanese keep their pictures.
pub fn block_zero_codes(font: &Font) -> Vec<i32> {
    let Some(block) = font.blocks().first() else { return Vec::new() };
    let mut codes = Vec::new();
    for range in &block.ranges {
        for code in range.start..=range.end {
            if font.glyph(code).is_some_and(|g| g.block.at == block.at) {
                codes.push(code);
            }
        }
    }
    codes
}

pub fn trace_glyph(font: &Font, code: i32) -> Option<GlyphOutline> {
    let glyph = font.glyph(code)?;
    if !is_console_table(&font.table(glyph.block)) {
        return None;
    }
    let parameters = TraceParameters::default();
    let layers = Layers::of_glyph(font, &glyph);
    let prepared = prepare_with(&layers, &parameters, false);
    let f = faithfulness(&prepared, Technique::Traced, 2);
    if f.grey_edge_mean.max(f.alpha_edge_mean) > FALLBACK_EDGE_ERROR || f.grey_max.max(f.alpha_max) > FALLBACK_PIXEL_ERROR {
        return None;
    }
    let simplified = |contours: &[Contour]| -> Vec<Contour> { contours.iter().map(|c| simplify(c, &[], SIMPLIFY)).filter(|c| c.len() >= 3).collect() };
    Some(GlyphOutline {
        width: glyph.block.width,
        height: glyph.block.height,
        core: simplified(&prepared.core_contours),
        outer: simplified(&prepared.outer_contours),
    })
}

// The outlines of `codes`, traced on `threads` threads.
pub fn build(font: &Font, file_hash: u64, codes: &[i32], threads: usize) -> OutlineFont {
    let threads = threads.max(1);
    let chunk = codes.len().div_ceil(threads).max(1);
    let mut glyphs = BTreeMap::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = codes
            .chunks(chunk)
            .map(|part| scope.spawn(move || part.iter().filter_map(|&c| trace_glyph(font, c).map(|g| (c, g))).collect::<Vec<_>>()))
            .collect();
        for h in handles {
            glyphs.extend(h.join().expect("a tracing thread"));
        }
    });
    OutlineFont { font_hash: file_hash, glyphs }
}

impl OutlineFont {
    pub fn to_text(&self) -> String {
        let mut out = format!("{FORMAT}\nfont {:016x}\n", self.font_hash);
        for (code, g) in &self.glyphs {
            let _ = writeln!(out, "glyph {code:04x} {} {} core {} outer {}", g.width, g.height, g.core.len(), g.outer.len());
            for c in g.core.iter().chain(&g.outer) {
                let _ = write!(out, "{}", c.len());
                for p in c {
                    let _ = write!(out, " {:.3} {:.3}", p[0], p[1]);
                }
                out.push('\n');
            }
        }
        out
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next() != Some(FORMAT) {
            return Err(format!("not a {FORMAT} file"));
        }
        let font = lines.next().and_then(|l| l.strip_prefix("font ")).and_then(|h| u64::from_str_radix(h, 16).ok()).ok_or("no font hash")?;
        let mut glyphs = BTreeMap::new();
        while let Some(line) = lines.next() {
            let w: Vec<&str> = line.split_whitespace().collect();
            if w.len() != 8 || w[0] != "glyph" || w[4] != "core" || w[6] != "outer" {
                return Err(format!("a bad glyph line: {line}"));
            }
            let number = |s: &str| s.parse::<i32>().map_err(|_| format!("a bad number in: {line}"));
            let code = i32::from_str_radix(w[1], 16).map_err(|_| format!("a bad code in: {line}"))?;
            let (width, height, cores, outers) = (number(w[2])?, number(w[3])?, number(w[5])? as usize, number(w[7])? as usize);
            let mut contours = Vec::with_capacity(cores + outers);
            for _ in 0..cores + outers {
                let l = lines.next().ok_or("the file ends inside a glyph")?;
                let v: Vec<f64> = l.split_whitespace().map(|s| s.parse::<f64>()).collect::<Result<_, _>>().map_err(|_| format!("a bad contour: {l}"))?;
                let n = v.first().copied().unwrap_or(0.0) as usize;
                if v.len() != 1 + 2 * n || n < 3 {
                    return Err(format!("a bad contour for {code:04x}"));
                }
                contours.push((0..n).map(|i| [v[1 + 2 * i], v[2 + 2 * i]]).collect());
            }
            let outer = contours.split_off(cores);
            glyphs.insert(code, GlyphOutline { width, height, core: contours, outer });
        }
        Ok(Self { font_hash: font, glyphs })
    }
}

// Where the outlines of the FNTOSD with `file_hash` are kept: `directory`/sharp-text/FNTOSD-<hash>.outlines.
pub fn cache_path(directory: &std::path::Path, file_hash: u64) -> std::path::PathBuf {
    directory.join("sharp-text").join(format!("FNTOSD-{file_hash:016x}.outlines"))
}

pub enum Loaded {
    Read(OutlineFont),
    Built(OutlineFont, Result<(), String>),
}

// The outlines from `path` when it holds this font's, else traced now (block 0, every thread) and written to `path`.
pub fn load_or_build(font: &Font, file: &[u8], path: &std::path::Path) -> Loaded {
    let hash = font_hash(file);
    if let Ok(text) = std::fs::read_to_string(path)
        && let Ok(found) = OutlineFont::from_text(&text)
        && found.font_hash == hash
    {
        return Loaded::Read(found);
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let built = build(font, hash, &block_zero_codes(font), threads);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| std::fs::write(path, built.to_text()))
        .map_err(|e| format!("{}: {e}", path.display()));
    Loaded::Built(built, written)
}
