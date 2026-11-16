// Real strings at a 4K-class scale, laid out by the console's metrics, each drawn three ways: the console's pictures
// magnified bilinearly (--text original), the traced outlines (--text sharp before Heros) and the chosen sources (--text
// sharp now). Each glyph's picture is drawn at the atlas's integer scale and sampled bilinearly, as the renderer does.
// cargo run --release -p glyph_enhance --example heros_compare -- [FNTOSD] [out.png] [scale] [heros-fit.txt]
use std::path::PathBuf;

use glyph_enhance::heros_table::{BUNDLED, FitTable, heros_for};
use glyph_enhance::outlines::{block_zero_codes, build, font_hash};
use glyph_enhance::picture::{SharpFont, sharp_picture};
use scene::font::Font;

const BACKGROUND: [f32; 3] = [16.0, 26.0, 66.0];
const LABEL: [f32; 3] = [235.0, 190.0, 90.0];

struct Image {
    width: usize,
    height: usize,
    rgb: Vec<[f32; 3]>,
}

impl Image {
    fn save(&self, path: &std::path::Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let bytes: Vec<u8> = self.rgb.iter().flat_map(|c| c.map(|v| v.round().clamp(0.0, 255.0) as u8)).collect();
        encoder.write_header().unwrap().write_image_data(&bytes).unwrap();
    }
}

type Picture = (u32, u32, Vec<u8>);

struct Drawer<'f> {
    font: &'f Font,
    pictures: std::collections::HashMap<(usize, i32, u32), Picture>,
}

impl Drawer<'_> {
    // One string at `scale` output pixels per console pixel, pen at `at` (x, baseline), glyphs from `shapes` (None: pictures).
    fn string(&mut self, image: &mut Image, set: usize, shapes: Option<&SharpFont>, text: &str, at: [f64; 2], (scale, tint): (f64, [f32; 3])) -> f64 {
        let [x, baseline] = at;
        let mut pen = x;
        let k = scale.ceil().max(1.0) as u32;
        for ch in text.chars() {
            let code = if self.font.glyph(ch as i32).is_some() { ch as i32 } else { 0xd818 };
            let glyph = self.font.glyph(code).unwrap();
            let [origin_x, base, left, right, top, bottom, advance] = glyph.metrics;
            let font = self.font;
            let (pw, ph, rgba) =
                self.pictures.entry((set, code, k)).or_insert_with(|| sharp_picture(font, code, shapes.and_then(|s| s.glyphs.get(&code)), k).unwrap());
            let (pw, ph) = (*pw as usize, *ph as usize);
            let px = |u: f64| pen + (u - f64::from(origin_x)) * scale;
            let py = |v: f64| baseline + (v - f64::from(base)) * scale;
            let (x0, x1) = (px(f64::from(origin_x + left - 1)), px(f64::from(origin_x + right + 1)));
            let (y0, y1) = (py(f64::from(base - top - 1)), py(f64::from(base - bottom + 1)));
            for y in (y0.floor().max(0.0) as usize)..(y1.ceil() as usize).min(image.height) {
                for xo in (x0.floor().max(0.0) as usize)..(x1.ceil() as usize).min(image.width) {
                    let (cx, cy) = (xo as f64 + 0.5, y as f64 + 0.5);
                    if cx < x0 || cx >= x1 || cy < y0 || cy >= y1 {
                        continue;
                    }
                    let u = (cx - pen) / scale + f64::from(origin_x);
                    let v = (cy - baseline) / scale + f64::from(base);
                    let (tx, ty) = ((u + 1.0) * f64::from(k) - 0.5, (v + 1.0) * f64::from(k) - 0.5);
                    let (ix, iy) = (tx.floor(), ty.floor());
                    let (fx, fy) = ((tx - ix) as f32, (ty - iy) as f32);
                    let texel = |x: f64, y: f64, c: usize| -> f32 {
                        let (x, y) = ((x as isize).clamp(0, pw as isize - 1) as usize, (y as isize).clamp(0, ph as isize - 1) as usize);
                        f32::from(rgba[(y * pw + x) * 4 + c])
                    };
                    let sample = |c: usize| {
                        let top = texel(ix, iy, c) * (1.0 - fx) + texel(ix + 1.0, iy, c) * fx;
                        let bottom = texel(ix, iy + 1.0, c) * (1.0 - fx) + texel(ix + 1.0, iy + 1.0, c) * fx;
                        top * (1.0 - fy) + bottom * fy
                    };
                    let (grey, alpha) = (sample(0) / 255.0, (sample(3) / 128.0).min(1.0));
                    let out = &mut image.rgb[y * image.width + xo];
                    for c in 0..3 {
                        out[c] = grey * tint[c] * alpha + out[c] * (1.0 - alpha);
                    }
                }
            }
            pen += f64::from(advance) * scale;
        }
        pen - x
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let font_path = PathBuf::from(args.get(1).map_or("Resources/FNTOSD", String::as_str));
    let out = PathBuf::from(args.get(2).map_or("heros-compare.png", String::as_str));
    let scale: f64 = args.get(3).map_or(4.8, |s| s.parse().unwrap());
    let file = std::fs::read(&font_path).unwrap();
    let font = Font::new(&file).unwrap();
    let table = match args.get(4) {
        Some(p) => FitTable::from_text(&std::fs::read_to_string(p).unwrap()).unwrap(),
        None => FitTable::from_text(BUNDLED).unwrap(),
    };
    let traced = build(&font, font_hash(&file), &block_zero_codes(&font), 8);
    let heros = heros_for(&font);
    let sets = [None, Some(SharpFont::traced(&traced)), Some(table.sharp_font(&font, &heros, &traced))];
    let names = ["console bilinear", "traced (before)", "Heros (now)"];
    let lines: Vec<(&str, f64)> = vec![
        ("Browser", 1.0),
        ("System Configuration", 1.0),
        ("Version Information", 1.0),
        ("Clock Adjustment", 1.0),
        ("2026/11/15 22:22:12", 1.0),
        ("Enter  Back  Options  Display  Version", 1.0),
        ("The quick brown fox jumps over the lazy dog.", 1.0),
        ("!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~", 1.0),
        ("0123456789  2026/11/15 22:22:12", 0.8),
        ("0123456789  2026/11/15 22:22:12", 0.83),
        ("Configuraci\u{f3}n  Gr\u{f6}\u{df}e  Fran\u{e7}ais  \u{c5}ngstr\u{f6}m", 1.0),
        ("info@example 100% ~ #1", 1.0),
    ];
    let row = 40.0 * scale * 0.95;
    let label = 16.0;
    let width = (640.0 * scale) as usize;
    let height = (lines.iter().map(|(_, s)| (row * s + label * 1.6) * 3.0 + 18.0).sum::<f64>()) as usize;
    let mut image = Image { width, height, rgb: vec![BACKGROUND; width * height] };
    let mut drawer = Drawer { font: &font, pictures: Default::default() };
    let mut y = 0.0;
    for (text, size) in &lines {
        for (set, shapes) in sets.iter().enumerate() {
            drawer.string(&mut image, 3, sets[1].as_ref(), names[set], [12.0, y + label * 1.2], (label / 33.0 * 1.4, LABEL));
            y += label * 1.6;
            drawer.string(&mut image, set, shapes.as_ref(), text, [20.0, y + 33.0 * scale * size * 0.95], (scale * size, [255.0; 3]));
            y += row * size;
        }
        y += 18.0;
    }
    image.save(&out);
    println!("wrote {} ({}x{})", out.display(), image.width, image.height);
}
