use std::sync::Arc;

use crate::Result;
use crate::arithmetic::Arithmetic;
use crate::font::Font;
use crate::program_image::ProgramImage;
use crate::text::{FontCache, FontCacheEntry, FontState, Text, TextInputs};

const LANGUAGE_TABLES: u32 = 0x2AD200;
const ZONE_TABLE: u32 = 0x2AD988;
const ZONE_COUNT: u32 = 0x2AE5A0;
const PAL_TV_RATIO: u32 = 0x36FB9C;
const LANGUAGES: i32 = 8;
const CACHE_BYTES: i32 = 0x1000;
const CACHE_HEAD: i32 = 0x3A0;
const CACHE_ENTRY: i32 = 0x20;
const TEXTURE_BLOCKS: i32 = 4;
const CACHE_BLOCKS: i32 = 0x120;
const BLOCK_SHIFT: i32 = 6;
const HALF: f32 = 0.5;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColdFontOut {
    pub state: FontState,
    pub cache: FontCache,
    pub widths: [i32; 8],
}

// HDD OSD 1.10U 0x226B10, 0x214228, 0x22CB50 verify_cold_font.mjs
pub fn cold_font<A: Arithmetic>(program: Arc<ProgramImage>, font: Arc<Font>, pal: bool, gs_allocator: u32) -> Result<ColdFontOut> {
    cold_font_guarded::<A::UnderEeRounding>(program, font, pal, gs_allocator)
}

fn cold_font_guarded<A: Arithmetic>(program: Arc<ProgramImage>, font: Arc<Font>, pal: bool, gs_allocator: u32) -> Result<ColdFontOut> {
    let mut inputs = TextInputs::default();
    let s = &mut inputs.font;
    s.ratio = 1.0;
    s.tv = A::mul(if pal { program.single(PAL_TV_RATIO) } else { 1.0 }, HALF);
    s.colour = [1.0, 1.0, 1.0, 1.0];
    s.blank = font.header16(0x54);
    s.ascent = font.header16(0x50);
    s.dirty = 1;
    s.matrix = [[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

    let cache = &mut inputs.cache;
    cache.list = vec![FontCacheEntry::default(); ((CACHE_BYTES - CACHE_HEAD) / CACHE_ENTRY) as usize];
    cache.memory = (CACHE_BLOCKS << BLOCK_SHIFT) as u32;
    cache.table = gs_allocator;
    cache.texture = gs_allocator.wrapping_add((TEXTURE_BLOCKS << BLOCK_SHIFT) as u32);

    let cold = inputs.cache.clone();
    let mut laid_out = cold.clone();
    laid_out.cells = 1;
    laid_out.cell_w = 8;
    laid_out.cell_h = 4;
    inputs.cache = laid_out;

    let mut text = Text::<A>::new(Arc::clone(&font), Arc::clone(&program), &inputs)?;
    let mut out = ColdFontOut::default();
    let zones = program.integer(ZONE_COUNT);
    for language in 0..LANGUAGES {
        let table = program.word(LANGUAGE_TABLES + 4 * language as u32);
        let mut widest = 0;
        for i in 0..zones {
            for field in [0xCu32, 0x4u32] {
                let id = program.word(ZONE_TABLE.wrapping_add((i as u32).wrapping_mul(0x18)).wrapping_add(field));
                let width = text.string_width(&program.string(program.word(table.wrapping_add(4u32.wrapping_mul(id)))));
                if width > widest {
                    widest = width;
                }
            }
        }
        out.widths[language as usize] = widest;
    }
    out.state = *text.font();
    out.cache = text.cache().clone();
    out.cache.cells = cold.cells;
    out.cache.cell_w = cold.cell_w;
    out.cache.cell_h = cold.cell_h;
    Ok(out)
}
