// Block 0 of FNTOSD as flat records for outside analysis: per glyph the code (u32), the seven metrics (i16) and the
// 4-bit table indices of its picture (one byte per pixel, rows top first).
// cargo run -p glyph_enhance --example font_dump -- <FNTOSD> <out.bin>
use std::io::Write;
use std::path::PathBuf;

use glyph_enhance::layers::Layers;
use scene::font::Font;

fn main() {
    let mut args = std::env::args().skip(1);
    let font_path = PathBuf::from(args.next().expect("FNTOSD path"));
    let out_path = PathBuf::from(args.next().expect("output path"));
    let font = Font::load(&font_path).expect("FNTOSD");
    let first = &font.blocks()[0];
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path).expect("output"));
    let mut count = 0;
    for code in (0x20..=0x45f).chain(0x2000..=0x2200).chain(0xd800..=0xd820) {
        let Some(glyph) = font.glyph(code) else { continue };
        if !std::ptr::eq(glyph.block, first) {
            continue;
        }
        out.write_all(&(code as u32).to_le_bytes()).unwrap();
        for m in glyph.metrics {
            out.write_all(&(m as i16).to_le_bytes()).unwrap();
        }
        out.write_all(&Layers::indices(&font, &glyph)).unwrap();
        count += 1;
    }
    println!("{count} glyphs of {}x{} written to {}", first.width, first.height, out_path.display());
}
