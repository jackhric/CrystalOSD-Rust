use std::path::Path;

use crate::{Error, Result};

// facts/text.md section 1: HDD OSD 1.10U's font file FNTOSD, expanded with the OSDSYS scheme and walked as
// scePFontGetGlyph (0x002905A0) walks it (References/scripts/extract_font.mjs readFont, glyphOf, pixel).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FontRange {
    pub start: i32,
    pub end: i32,
    pub first: i32,
    pub add: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontBlock {
    pub at: u32,
    pub flags: u32,
    pub depth: i32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub width: i32,
    pub height: i32,
    pub ascent: i32,
    pub descent: i32,
    pub glyphs: u32,
    pub pictures: u32,
    pub picture_size: u32,
    pub index: u32,
    pub metrics: u32,
    pub table: u32,
    pub per_glyph: bool,
    pub ranges: Vec<FontRange>,
}

impl Default for FontBlock {
    fn default() -> Self {
        Self {
            at: 0,
            flags: 0,
            depth: 4,
            scale_x: 1.0,
            scale_y: 1.0,
            width: 0,
            height: 0,
            ascent: 0,
            descent: 0,
            glyphs: 0,
            pictures: 0,
            picture_size: 0,
            index: 0,
            metrics: 0,
            table: 0,
            per_glyph: false,
            ranges: Vec::new(),
        }
    }
}

// Metrics: originX, baseline, left, right, top, bottom, advance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph<'a> {
    pub block: &'a FontBlock,
    pub index: i32,
    pub picture: u32,
    pub metrics: [i32; 7],
}

pub type Rgba = [u8; 4];

fn past_the_end() -> Error {
    Error("font: read past the end".into())
}

fn u32_at(d: &[u8], at: usize) -> Result<u32> {
    at.checked_add(4).and_then(|end| d.get(at..end)).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or_else(past_the_end)
}

fn s32_at(d: &[u8], at: usize) -> Result<i32> {
    u32_at(d, at).map(|v| v as i32)
}

fn s16_at(d: &[u8], at: usize) -> Result<i32> {
    at.checked_add(2).and_then(|end| d.get(at..end)).map(|b| i32::from(i16::from_le_bytes(b.try_into().unwrap()))).ok_or_else(past_the_end)
}

fn u16_at(d: &[u8], at: usize) -> Result<u32> {
    s16_at(d, at).map(|v| u32::from(v as u16))
}

fn f32_at(d: &[u8], at: usize) -> Result<f32> {
    u32_at(d, at).map(f32::from_bits)
}

fn fail<T>(result: Result<T>) -> T {
    result.unwrap_or_else(|e| panic!("{e}"))
}

fn byte_at(d: &[u8], at: usize) -> Result<u8> {
    d.get(at).copied().ok_or_else(|| Error("font: an index past the end".into()))
}

// extract_font.mjs expand: the OSDSYS scheme (a length, then runs of 30 literal or back-reference items).
pub fn expand_osd(src: &[u8]) -> Result<Vec<u8>> {
    let big32 = |at: usize| -> Result<u32> {
        Ok((u32::from(byte_at(src, at)?) << 24)
            | (u32::from(byte_at(src, at + 1)?) << 16)
            | (u32::from(byte_at(src, at + 2)?) << 8)
            | u32::from(byte_at(src, at + 3)?))
    };
    let big16 = |at: usize| -> Result<u32> { Ok((u32::from(byte_at(src, at)?) << 8) | u32::from(byte_at(src, at + 1)?)) };
    let length = u32_at(src, 0)? as usize;
    let mut dst = vec![0u8; length];
    let mut s = 4usize;
    let mut d = 0usize;
    let mut run = 0i32;
    let mut shift = 0i32;
    let mut desc = 0u32;
    let mut mask = 0u32;
    while d < length {
        if run == 0 {
            run = 30;
            desc = big32(s)?;
            s += 4;
            let n = desc & 3;
            shift = 14 - n as i32;
            mask = 0x3fff >> n;
        }
        if (desc & (1u32 << (run + 1))) == 0 {
            dst[d] = byte_at(src, s)?;
            d += 1;
            s += 1;
        } else {
            let h = big16(s)?;
            s += 2;
            let back = (h & mask) as usize + 1;
            if back > d {
                return Err(Error("font: a reference before the start".into()));
            }
            let mut from = d - back;
            let mut i = 0u32;
            while i < 3 + (h >> shift) && d < length {
                dst[d] = dst[from];
                d += 1;
                from += 1;
                i += 1;
            }
        }
        run -= 1;
    }
    Ok(dst)
}

#[derive(Debug, Clone)]
pub struct Font {
    data: Vec<u8>,
    blocks: Vec<FontBlock>,
}

impl Font {
    pub fn new(file: &[u8]) -> Result<Self> {
        let data = expand_osd(file)?;
        let d = &data;
        if u32_at(d, 0)? != 0 || u32_at(d, 4)? != 0 {
            return Err(Error("not font data: the first two words are not zero".into()));
        }
        let at = |base: u32, offset: u32| base.wrapping_add(offset) as usize;
        let count = u32_at(d, 0x58)?;
        let mut blocks = Vec::new();
        for i in 0..count {
            let mut b = FontBlock { at: u32_at(d, 0x5cu32.wrapping_add(i.wrapping_mul(4)) as usize)?, ..Default::default() };
            b.flags = u32_at(d, at(b.at, 0x10))?;
            b.depth = if (b.flags & 7) == 0 { 4 } else { ((b.flags & 7) << 3) as i32 };
            b.scale_x = f32_at(d, at(b.at, 0x14))?;
            b.scale_y = f32_at(d, at(b.at, 0x18))?;
            b.width = s16_at(d, at(b.at, 0x1c))?;
            b.height = s16_at(d, at(b.at, 0x1e))?;
            b.ascent = s16_at(d, at(b.at, 0x20))?;
            b.descent = s16_at(d, at(b.at, 0x22))?;
            b.glyphs = u32_at(d, at(b.at, 0x28))?;
            b.pictures = b.at.wrapping_add(u32_at(d, at(b.at, 0x2c))?);
            let bits = b.width.wrapping_mul(b.height).wrapping_mul(b.depth);
            b.picture_size = ((((if bits < 0 { bits.wrapping_add(7) } else { bits }) >> 3).wrapping_add(15)) & !15) as u32;
            b.index = b.at.wrapping_add(u32_at(d, at(b.at, 0x3c))?);
            b.metrics = b.at.wrapping_add(u32_at(d, at(b.at, 0x44))?);
            b.per_glyph = ((b.flags >> 3) & 1) != 0;
            b.table = b.at.wrapping_add(u32_at(d, at(b.at, 0x54))?);
            let ranges = u32_at(d, at(b.at, 0x30))?;
            for r in 0..ranges {
                let o = b.at.wrapping_add(u32_at(d, at(b.at, 0x34))?).wrapping_add(r.wrapping_mul(16)) as usize;
                b.ranges.push(FontRange { start: s32_at(d, o)?, end: s32_at(d, o + 4)?, first: s32_at(d, o + 8)?, add: s32_at(d, o + 12)? });
            }
            blocks.push(b);
        }
        Ok(Self { data, blocks })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::read(path).map_err(|_| Error(format!("cannot open the font {}", path.display())))?;
        Self::new(&file)
    }

    // scePFontGetGlyph: the first block whose ranges, searched by bisection, give the code an index entry.
    pub fn glyph(&self, code: i32) -> Option<Glyph<'_>> {
        for block in &self.blocks {
            let ranges = &block.ranges;
            if ranges.is_empty() || code < ranges[0].start || ranges[ranges.len() - 1].end < code {
                continue;
            }
            let mut low = -1i32;
            let mut high = ranges.len() as i32;
            let mut found = None;
            while low + 1 != high {
                let middle = (low + high) >> 1;
                let range = &ranges[middle as usize];
                if code < range.start {
                    high = middle;
                } else if range.end < code {
                    low = middle;
                } else {
                    found = Some(range);
                    break;
                }
            }
            let Some(found) = found else { continue };
            let entry =
                fail(u16_at(&self.data, block.index.wrapping_add(2u32.wrapping_mul(found.first.wrapping_add(code).wrapping_sub(found.start) as u32)) as usize));
            if entry == 0xffff {
                continue;
            }
            let index = found.add.wrapping_add(entry as i32);
            let picture = block.pictures.wrapping_add(block.picture_size.wrapping_mul(index as u32));
            let m = block.metrics.wrapping_add(if block.per_glyph { 16u32.wrapping_mul(index as u32) } else { 0 });
            let mut metrics = [0i32; 7];
            for (i, metric) in metrics.iter_mut().enumerate() {
                *metric = fail(s16_at(&self.data, m.wrapping_add(2 * i as u32) as usize));
            }
            return Some(Glyph { block, index, picture, metrics });
        }
        None
    }

    pub fn pixel(&self, glyph: &Glyph<'_>, x: i32, y: i32) -> i32 {
        let n = y.wrapping_mul(glyph.block.width).wrapping_add(x);
        if glyph.block.depth == 4 {
            return (i32::from(fail(byte_at(&self.data, glyph.picture.wrapping_add((n >> 1) as u32) as usize))) >> ((n & 1) * 4)) & 0xf;
        }
        if glyph.block.depth == 8 {
            return i32::from(fail(byte_at(&self.data, glyph.picture.wrapping_add(n as u32) as usize)));
        }
        panic!("font: a pixel format of {} bits is not read", glyph.block.depth)
    }

    // The block's sixteen colours as R, G, B, A.
    pub fn table(&self, block: &FontBlock) -> [Rgba; 16] {
        let mut out = [[0u8; 4]; 16];
        for (i, colour) in out.iter_mut().enumerate() {
            for (c, channel) in colour.iter_mut().enumerate() {
                *channel = fail(byte_at(&self.data, block.table as usize + 4 * i + c));
            }
        }
        out
    }

    pub fn block_at(&self, at: u32) -> Option<&FontBlock> {
        self.blocks.iter().find(|block| block.at == at)
    }

    pub fn blocks(&self) -> &[FontBlock] {
        &self.blocks
    }

    // The s16 at `offset` of the expanded file's header (+0x50 ascent, +0x52 descent, +0x54 the blank's width).
    pub fn header16(&self, offset: u32) -> i32 {
        fail(s16_at(&self.data, offset as usize))
    }
}
