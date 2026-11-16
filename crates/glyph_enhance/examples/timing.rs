// Generation costs of the sharp text: tracing, reading the outlines back, drawing pictures and updating the atlas.
use std::time::Instant;

use glyph_enhance::atlas::SharpAtlas;
use glyph_enhance::outlines::{OutlineFont, block_zero_codes, build, font_hash};
use glyph_enhance::picture::sharp_picture;
use scene::font::Font;
use scene::frame::GlyphCache;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "Resources/FNTOSD".to_owned());
    let file = std::fs::read(&path).expect("FNTOSD");
    let font = Font::new(&file).unwrap();
    let codes = block_zero_codes(&font);
    let t = Instant::now();
    let one = build(&font, font_hash(&file), &codes, 1);
    println!("trace block 0 ({} glyphs) on 1 thread: {:.0} ms", codes.len(), t.elapsed().as_secs_f64() * 1e3);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let t = Instant::now();
    let outlines = build(&font, font_hash(&file), &codes, threads);
    println!("trace block 0 on {threads} threads: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    assert_eq!(one.to_text(), outlines.to_text());
    let text = outlines.to_text();
    let t = Instant::now();
    let read = OutlineFont::from_text(&text).unwrap();
    println!("outline file: {} bytes, read in {:.1} ms", text.len(), t.elapsed().as_secs_f64() * 1e3);
    let points: usize = read.glyphs.values().map(|g| g.core.iter().chain(&g.outer).map(Vec::len).sum::<usize>()).sum();
    println!("{} outlines, {points} points", read.glyphs.len());
    let ascii: Vec<i32> = (0x21..0x7f).collect();
    for k in [2u32, 4, 6, 8] {
        let t = Instant::now();
        for &c in &ascii {
            sharp_picture(&font, c, read.glyphs.get(&c), k).unwrap();
        }
        let per = t.elapsed().as_secs_f64() * 1e3 / ascii.len() as f64;
        let t = Instant::now();
        let mut atlas = SharpAtlas::new(k);
        let mut cells: Vec<i32> = (0..72).map(|n| 0x21 + n).collect();
        let cache =
            |cells: &Vec<i32>| GlyphCache { address: 0xbc100, cell_width: 40, cell_height: 44, width: 384, log_width: 9, log_height: 9, cells: cells.clone() };
        atlas.update(&font, &read, &cache(&cells));
        let full = t.elapsed().as_secs_f64() * 1e3;
        cells[5] = 0x7e;
        let t = Instant::now();
        atlas.update(&font, &read, &cache(&cells));
        let cell = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "scale {k}: one glyph picture {per:.2} ms; atlas {}x{} ({:.1} MB) with 72 new glyphs {full:.0} ms; one cell changed {cell:.2} ms",
            atlas.width,
            atlas.height,
            atlas.rgba.len() as f64 / 1e6
        );
    }
}
