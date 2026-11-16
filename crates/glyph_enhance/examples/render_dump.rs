// Glyph pictures rendered by two techniques at one scale, as flat records for outside comparison: per glyph the code,
// width and height (u32), then per pixel the traced grey and alpha and the bilinear grey and alpha (f32), origin at the
// picture's top left.
// cargo run --release -p glyph_enhance --example render_dump -- <FNTOSD> <out.bin> <scale> <text>
use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;

use glyph_enhance::enhance::{Technique, prepare, render};
use glyph_enhance::layers::Layers;
use glyph_enhance::trace::TraceParameters;
use scene::font::Font;

fn main() {
    let mut args = std::env::args().skip(1);
    let font = Font::load(&PathBuf::from(args.next().expect("FNTOSD path"))).expect("FNTOSD");
    let out_path = PathBuf::from(args.next().expect("output path"));
    let scale: f64 = args.next().expect("scale").parse().expect("scale");
    let codes: BTreeSet<i32> = args.next().expect("text").chars().map(|c| c as i32).collect();
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path).expect("output"));
    for code in codes {
        let Some(glyph) = font.glyph(code) else { continue };
        let p = prepare(&Layers::of_glyph(&font, &glyph), &TraceParameters::default());
        let w = (p.width as f64 * scale).ceil() as usize;
        let h = (p.height as f64 * scale).ceil() as usize;
        let traced = render(&p, Technique::Traced, scale, [0.0, 0.0], w, h);
        let bilinear = render(&p, Technique::Bilinear, scale, [0.0, 0.0], w, h);
        for v in [code as u32, w as u32, h as u32] {
            out.write_all(&v.to_le_bytes()).unwrap();
        }
        for (t, b) in traced.iter().zip(&bilinear) {
            for v in [t.0, t.1, b.0, b.1] {
                out.write_all(&v.to_le_bytes()).unwrap();
            }
        }
    }
    println!("wrote {}", out_path.display());
}
