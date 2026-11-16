use std::path::PathBuf;

use glyph_enhance::atlas::SharpAtlas;
use glyph_enhance::layers::{CONSOLE_TABLE, Layers, combine, split};
use glyph_enhance::outlines::{OutlineFont, block_zero_codes, build, font_hash, trace_glyph};
use glyph_enhance::picture::sharp_picture;
use glyph_enhance::raster::Canvas;
use scene::font::Font;
use scene::frame::GlyphCache;

fn font_file() -> Option<Vec<u8>> {
    let folder =
        std::env::var_os("CRYSTALOSD_RESOURCES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Resources"));
    match std::fs::read(folder.join("FNTOSD")) {
        Ok(file) => Some(file),
        Err(_) => {
            eprintln!("skipped: no FNTOSD in {}", folder.display());
            None
        }
    }
}

#[test]
fn layers_recombine_every_colour_exactly() {
    for colour in CONSOLE_TABLE {
        let (core, outer, step) = split(colour);
        let (grey, alpha) = combine(core, outer, step);
        assert_eq!((grey * 255.0).round() as u8, colour[0]);
        assert_eq!(alpha.round() as u8, colour[3]);
        assert_eq!(alpha, f32::from(colour[3]));
    }
}

#[test]
fn layers_of_every_block_zero_glyph_recombine_to_its_picture() {
    let Some(file) = font_file() else { return };
    let font = Font::new(&file).unwrap();
    for code in block_zero_codes(&font) {
        let glyph = font.glyph(code).unwrap();
        let table = font.table(glyph.block);
        let layers = Layers::of_glyph(&font, &glyph);
        let indices = Layers::indices(&font, &glyph);
        for (i, (grey, alpha)) in layers.recombined().into_iter().enumerate() {
            let want = table[indices[i] as usize];
            assert_eq!(((grey * 255.0).round() as u8, alpha as u8), (want[0], want[3]), "code {code:#x} pixel {i}");
        }
    }
}

#[test]
fn a_square_has_its_exact_area() {
    let mut canvas = Canvas::new(8, 8);
    canvas.fill(&[vec![[1.25, 1.5], [1.25, 4.0], [5.75, 4.0], [5.75, 1.5]]], |p| p);
    let coverage = canvas.coverage();
    let total: f32 = coverage.iter().sum();
    assert!((total - 4.5 * 2.5).abs() < 1e-4, "{total}");
    assert!((coverage[8 + 1] - 0.75 * 0.5).abs() < 1e-5);
    assert_eq!(coverage[2 * 8 + 3], 1.0);
    assert_eq!(coverage[5 * 8 + 3], 0.0);
}

#[test]
fn every_block_zero_glyph_traces_and_draws() {
    let Some(file) = font_file() else { return };
    let font = Font::new(&file).unwrap();
    let codes = block_zero_codes(&font);
    assert_eq!(codes.len(), 529);
    let outlines = build(&font, font_hash(&file), &codes, 8);
    assert!(outlines.glyphs.len() >= 515, "{}", outlines.glyphs.len());
    for c in 0x21..0x7f {
        assert!(outlines.glyphs.contains_key(&c), "ASCII {c:#x} keeps its picture");
    }
    for &code in &codes {
        let (w, h, rgba) = sharp_picture(&font, code, outlines.glyphs.get(&code), 2).unwrap();
        assert_eq!((w, h), (68, 84));
        assert!(rgba.chunks(4).all(|p| p[3] <= 125 && p[0] == p[1] && p[1] == p[2]), "code {code:#x}");
    }
}

#[test]
fn tracing_is_deterministic_and_its_file_reads_back() {
    let Some(file) = font_file() else { return };
    let font = Font::new(&file).unwrap();
    let codes: Vec<i32> = (0x20..0x80).collect();
    let a = build(&font, font_hash(&file), &codes, 1);
    let b = build(&font, font_hash(&file), &codes, 7);
    assert_eq!(a.to_text(), b.to_text());
    let read = OutlineFont::from_text(&a.to_text()).unwrap();
    assert_eq!(read.to_text(), a.to_text());
    assert_eq!(read.font_hash, font_hash(&file));
}

fn cache_of(cells: Vec<i32>) -> GlyphCache {
    GlyphCache { address: 0xbc100, cell_width: 40, cell_height: 44, width: 384, log_width: 9, log_height: 9, cells }
}

#[test]
fn the_atlas_puts_each_picture_in_its_cell_and_nothing_outside_it() {
    let Some(file) = font_file() else { return };
    let font = Font::new(&file).unwrap();
    let outlines = build(&font, font_hash(&file), &[0x41, 0x57, 0x40], 2);
    let k = 3u32;
    let mut atlas = SharpAtlas::new(k);
    let mut cells = vec![0; 72];
    cells[0] = 0x41;
    cells[10] = 0x57;
    cells[71] = 0x40;
    let changed = atlas.update(&font, &outlines, &cache_of(cells.clone()));
    assert_eq!((atlas.width, atlas.height), (512 * k, 512 * k));
    assert_eq!(changed.len(), 1, "the first update is the whole texture");
    let alpha = |x: u32, y: u32| atlas.rgba[((y * atlas.width + x) * 4 + 3) as usize];
    for (n, &code) in cells.iter().enumerate() {
        let (cx, cy) = ((n as u32 % 9) * 40, (n as u32 / 9) * 44);
        // The region clamp: MINU = cellX, MAXU = cellX + 1 + 32, MINV = cellY, MAXV = cellY + 1 + 40, inclusive.
        let mut inside = 0u32;
        for y in cy * k..(cy + 44) * k {
            for x in cx * k..(cx + 40) * k {
                let a = alpha(x, y);
                let clamp = x >= cx * k && x < (cx + 34) * k && y >= cy * k && y < (cy + 42) * k;
                assert!(clamp || a == 0, "cell {n}: ink outside its clamp region at ({x}, {y})");
                inside += u32::from(a > 0);
            }
        }
        assert_eq!(inside > 0, code != 0, "cell {n}");
    }
    for y in 384 * k..atlas.height {
        assert!((0..atlas.width).all(|x| alpha(x, y) == 0));
    }
    cells[10] = 0x40;
    let changed = atlas.update(&font, &outlines, &cache_of(cells.clone()));
    assert_eq!(changed.len(), 1);
    assert_eq!((changed[0].x, changed[0].y, changed[0].width, changed[0].height), (40 * k, 44 * k, 40 * k, 44 * k));
    let mut same = SharpAtlas::new(k);
    same.update(&font, &outlines, &cache_of(cells));
    assert!(same.rgba == atlas.rgba, "an atlas brought up to date equals one made from the cells at once");
}

// Each glyph's picture drawn at 4 texels a pixel and averaged back to the picture's own pixels: per pixel the larger of
// the premultiplied grey's and the alpha's difference (0..1) from the picture, its mean over the pixels either inks.
fn averaged_back(font: &Font, code: i32, outline: Option<&glyph_enhance::outlines::GlyphOutline>) -> (f64, usize, f64) {
    let k = 4usize;
    let glyph = font.glyph(code).unwrap();
    let original = Layers::of_glyph(font, &glyph).recombined();
    let (pw, _, rgba) = sharp_picture(font, code, outline, k as u32).unwrap();
    let (w, h) = (glyph.block.width as usize, glyph.block.height as usize);
    let (mut sum, mut count, mut worst) = (0.0f64, 0usize, 0.0f64);
    for y in 0..h {
        for x in 0..w {
            let (mut pg, mut pa) = (0.0f64, 0.0f64);
            for j in 0..k {
                for i in 0..k {
                    let at = (((y + 1) * k + j) * pw as usize + (x + 1) * k + i) * 4;
                    pg += f64::from(rgba[at]) / 255.0 * f64::from(rgba[at + 3]) / 125.0;
                    pa += f64::from(rgba[at + 3]) / 125.0;
                }
            }
            let (g0, a0) = original[y * w + x];
            let d = (pg / (k * k) as f64 - f64::from(g0 * a0) / 125.0).abs().max((pa / (k * k) as f64 - f64::from(a0) / 125.0).abs());
            worst = worst.max(d);
            if a0 > 0.0 || pa > 0.0 {
                sum += d;
                count += 1;
            }
        }
    }
    (sum, count, worst)
}

// The faithfulness of the sharp text: every Latin glyph with outlines (ASCII, Latin-1, Latin Extended-A) averaged back
// stays within 0.12 of its picture on average over its inked pixels and within 0.65 at any pixel, 0.06 over them all;
// and over them all it is closer to the pictures than today's bilinear magnification is.
#[test]
fn sharp_latin_glyphs_average_back_to_their_pictures() {
    let Some(file) = font_file() else { return };
    let font = Font::new(&file).unwrap();
    let block = font.blocks()[0].at;
    let codes: Vec<i32> = (0x21..0x7f).chain(0xa1..0x180).filter(|&c| font.glyph(c).is_some_and(|g| g.block.at == block)).collect();
    let (mut sharp, mut today, mut count, mut worst) = (0.0f64, 0.0f64, 0usize, (0.0f64, 0));
    for &code in &codes {
        let Some(outline) = trace_glyph(&font, code) else { continue };
        let (sum, n, pixel) = averaged_back(&font, code, Some(&outline));
        let mean = sum / n.max(1) as f64;
        assert!(pixel <= 0.65, "code {code:#x}: a pixel differs by {pixel}");
        assert!(mean <= 0.12, "code {code:#x}: mean {mean}");
        if mean > worst.0 {
            worst = (mean, code);
        }
        let (bilinear, m, _) = averaged_back(&font, code, None);
        sharp += sum;
        count += n;
        today += bilinear * n as f64 / m.max(1) as f64;
    }
    let (sharp, today) = (sharp / count as f64, today / count as f64);
    eprintln!("{} Latin glyphs: sharp {sharp:.4}, bilinear {today:.4} (mean over inked pixels), worst glyph {:#x} {:.4}", codes.len(), worst.1, worst.0);
    assert!(sharp <= 0.06, "{sharp}");
    assert!(sharp < today, "sharp {sharp} against bilinear {today}");
}
