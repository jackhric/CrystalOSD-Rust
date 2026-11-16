// Phase 1 comparison sheets: real menu strings drawn with each reconstruction, side by side, into out/text-compare/.
// cargo run --release -p glyph_enhance --example text_compare [FNTOSD]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use glyph_enhance::enhance::{Prepared, Technique, prepare, render};
use glyph_enhance::field::Field;
use glyph_enhance::layers::{Layers, combine};
use glyph_enhance::trace::TraceParameters;
use scene::font::Font;

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Source,
    Technique(Technique),
    Potrace,
}

impl Row {
    fn label(self) -> String {
        match self {
            Row::Source => "source pixels (nearest)".into(),
            Row::Technique(t) => t.name().into(),
            Row::Potrace => "potrace (8x threshold, reference)".into(),
        }
    }
}

const ROWS: [Row; 8] = [
    Row::Source,
    Row::Technique(Technique::Bilinear),
    Row::Technique(Technique::DistanceField),
    Row::Technique(Technique::DistanceFieldBicubic),
    Row::Technique(Technique::LevelSet),
    Row::Technique(Technique::Traced),
    Row::Technique(Technique::Fitted),
    Row::Potrace,
];

struct Image {
    width: usize,
    height: usize,
    rgb: Vec<[f32; 3]>,
}

impl Image {
    fn new(width: usize, height: usize, background: [f32; 3]) -> Self {
        Self { width, height, rgb: vec![background; width * height] }
    }

    fn save(&self, path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let bytes: Vec<u8> = self.rgb.iter().flat_map(|c| c.map(|v| v.round().clamp(0.0, 255.0) as u8)).collect();
        writer.write_image_data(&bytes).unwrap();
    }
}

struct Potraced {
    core: Vec<f32>,
    outer: Vec<f32>,
    width: usize,
    height: usize,
}

const POTRACE_IN: usize = 8;
const POTRACE_OUT: usize = 16;

fn potrace_layer(field: &Field, w: usize, h: usize, scratch: &Path, name: &str) -> Vec<f32> {
    let (bw, bh) = (w * POTRACE_IN, h * POTRACE_IN);
    let mut pbm = format!("P1\n{bw} {bh}\n").into_bytes();
    for y in 0..bh {
        for x in 0..bw {
            let u = (x as f64 + 0.5) / POTRACE_IN as f64;
            let v = (y as f64 + 0.5) / POTRACE_IN as f64;
            pbm.push(if field.bicubic(u, v) >= 0.5 { b'1' } else { b'0' });
            pbm.push(b' ');
        }
        pbm.push(b'\n');
    }
    let input = scratch.join(format!("{name}.pbm"));
    let output = scratch.join(format!("{name}.pgm"));
    std::fs::write(&input, pbm).unwrap();
    let status = Command::new("potrace")
        .args(["-g", "-G", "1", "-x"])
        .arg(format!("{}", POTRACE_OUT as f64 / POTRACE_IN as f64))
        .arg("-o")
        .arg(&output)
        .arg(&input)
        .status()
        .expect("potrace");
    assert!(status.success());
    let data = std::fs::read(&output).unwrap();
    let mut fields = Vec::new();
    let mut at = 0;
    while fields.len() < 4 {
        while data[at].is_ascii_whitespace() {
            at += 1;
        }
        if data[at] == b'#' {
            while data[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        let start = at;
        while !data[at].is_ascii_whitespace() {
            at += 1;
        }
        fields.push(String::from_utf8_lossy(&data[start..at]).into_owned());
    }
    at += 1;
    let (pw, ph): (usize, usize) = (fields[1].parse().unwrap(), fields[2].parse().unwrap());
    let (ow, oh) = (w * POTRACE_OUT, h * POTRACE_OUT);
    let mut out = vec![0.0f32; ow * oh];
    for y in 0..oh.min(ph) {
        for x in 0..ow.min(pw) {
            out[y * ow + x] = 1.0 - f32::from(data[at + y * pw + x]) / 255.0;
        }
    }
    out
}

fn potraced(p: &Prepared, scratch: &Path, code: i32) -> Potraced {
    let core = potrace_layer(&p.core, p.width, p.height, scratch, &format!("c{code:x}"));
    let outer = potrace_layer(&p.outer, p.width, p.height, scratch, &format!("o{code:x}"));
    Potraced { core, outer, width: p.width * POTRACE_OUT, height: p.height * POTRACE_OUT }
}

// The box average of a fine grid over the output pixel [x0, x1) x [y0, y1) in fine units.
fn area(fine: &[f32], width: usize, height: usize, x0: f64, x1: f64, y0: f64, y1: f64) -> f32 {
    let mut sum = 0.0f64;
    let (fx0, fx1) = (x0.floor().max(0.0) as usize, (x1.ceil() as usize).min(width));
    let (fy0, fy1) = (y0.floor().max(0.0) as usize, (y1.ceil() as usize).min(height));
    for y in fy0..fy1 {
        let wy = ((y + 1) as f64).min(y1) - (y as f64).max(y0);
        for x in fx0..fx1 {
            let wx = ((x + 1) as f64).min(x1) - (x as f64).max(x0);
            sum += wx * wy * f64::from(fine[y * width + x]);
        }
    }
    (sum / ((x1 - x0) * (y1 - y0))) as f32
}

struct Glyphs<'f> {
    font: &'f Font,
    prepared: HashMap<i32, Prepared>,
    potraced: HashMap<i32, Potraced>,
    scratch: PathBuf,
}

impl Glyphs<'_> {
    fn get(&mut self, code: i32) -> i32 {
        let code = if self.font.glyph(code).is_some() { code } else { 0xd818 };
        if !self.prepared.contains_key(&code) {
            let g = self.font.glyph(code).unwrap();
            self.prepared.insert(code, prepare(&Layers::of_glyph(self.font, &g), &TraceParameters::default()));
        }
        code
    }

    fn draw(&mut self, row: Row, code: i32, scale: f64, origin: [f64; 2], colour: [f32; 4], image: &mut Image) {
        let code = self.get(code);
        let p = &self.prepared[&code];
        let (x0, y0) = (origin[0].floor() as isize, origin[1].floor() as isize);
        let w = (p.width as f64 * scale).ceil() as usize + 2;
        let h = (p.height as f64 * scale).ceil() as usize + 2;
        let local = [origin[0] - x0 as f64, origin[1] - y0 as f64];
        let pixels: Vec<(f32, f32)> = match row {
            Row::Technique(t) => render(p, t, scale, local, w, h),
            Row::Source => {
                let mut out = Vec::with_capacity(w * h);
                let original = p.layers.recombined();
                for y in 0..h {
                    for x in 0..w {
                        let u = ((x as f64 + 0.5 - local[0]) / scale).floor();
                        let v = ((y as f64 + 0.5 - local[1]) / scale).floor();
                        let inside = u >= 0.0 && v >= 0.0 && (u as usize) < p.width && (v as usize) < p.height;
                        out.push(if inside { original[v as usize * p.width + u as usize] } else { (0.0, 0.0) });
                    }
                }
                out
            }
            Row::Potrace => {
                if !self.potraced.contains_key(&code) {
                    let t = potraced(p, &self.scratch, code);
                    self.potraced.insert(code, t);
                }
                let t = &self.potraced[&code];
                let k = POTRACE_OUT as f64 / scale;
                let mut out = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        let fx0 = (x as f64 - local[0]) * k;
                        let fy0 = (y as f64 - local[1]) * k;
                        let c = area(&t.core, t.width, t.height, fx0, fx0 + k, fy0, fy0 + k);
                        let o = area(&t.outer, t.width, t.height, fx0, fx0 + k, fy0, fy0 + k);
                        let (u, v) = ((x as f64 + 0.5 - local[0]) / scale, (y as f64 + 0.5 - local[1]) / scale);
                        let dc = f64::from(p.core_distance.bilinear(u, v)) - glyph_enhance::enhance::STEP_OFFSET;
                        let step = (0.5 - dc * scale).clamp(0.0, 1.0) as f32;
                        out.push(combine(c, o, step));
                    }
                }
                out
            }
        };
        for y in 0..h {
            for x in 0..w {
                let (ix, iy) = (x0 + x as isize, y0 + y as isize);
                if ix < 0 || iy < 0 || ix as usize >= image.width || iy as usize >= image.height {
                    continue;
                }
                let (grey, alpha) = pixels[y * w + x];
                // Native.frag: texel x vertex / 128, alpha byte x vertex alpha / 128, then over.
                let a = (alpha * colour[3] / 128.0 / 128.0).min(1.0);
                let d = &mut image.rgb[iy as usize * image.width + ix as usize];
                for c in 0..3 {
                    let s = (grey * 255.0 * colour[c] / 128.0).min(255.0);
                    d[c] = s * a + d[c] * (1.0 - a);
                }
            }
        }
    }
}

struct Line {
    text: Vec<i32>,
    size: f64,
    colour: [f32; 4],
}

fn codes(s: &str) -> Vec<i32> {
    s.chars().map(|c| c as i32).collect()
}

fn line(s: &str, size: f64) -> Line {
    Line { text: codes(s), size, colour: [128.0; 4] }
}

const LABEL: usize = 46;
const MARGIN: usize = 24;

fn line_width(font: &Font, l: &Line) -> f64 {
    l.text.iter().map(|&c| font.glyph(c).or_else(|| font.glyph(0xd818)).map_or(0.0, |g| f64::from(g.metrics[6]) * l.size)).sum()
}

fn sheet(glyphs: &mut Glyphs, lines: &[Line], scale: f64, rows: &[Row], path: &Path, zoom: usize) {
    let font = glyphs.font;
    let width = lines.iter().map(|l| line_width(font, l)).fold(0.0, f64::max) * scale + 2.0 * MARGIN as f64;
    let line_heights: Vec<f64> = lines.iter().map(|l| 44.0 * l.size * scale).collect();
    let block = line_heights.iter().sum::<f64>() + LABEL as f64;
    let mut image = Image::new(width.ceil() as usize, (block * rows.len() as f64).ceil() as usize + MARGIN, [18.0, 28.0, 58.0]);
    let mut labels = Vec::new();
    for (r, &row) in rows.iter().enumerate() {
        let top = r as f64 * block;
        labels.push((row.label(), top as usize + 8));
        let mut y = top + LABEL as f64;
        for (l, h) in lines.iter().zip(&line_heights) {
            let mut pen = MARGIN as f64;
            let baseline = y + 33.0 * l.size * scale;
            for &c in &l.text {
                let code = glyphs.get(c);
                let g = font.glyph(code).unwrap();
                let [origin_x, base, _, _, _, _, advance] = g.metrics;
                let s = scale * l.size;
                let origin = [pen - f64::from(origin_x) * s, baseline - f64::from(base) * s];
                glyphs.draw(row, c, s, origin, l.colour, &mut image);
                pen += f64::from(advance) * s;
            }
            y += h;
        }
    }
    let image = if zoom > 1 {
        let mut z = Image::new(image.width * zoom, image.height * zoom, [0.0; 3]);
        for y in 0..z.height {
            for x in 0..z.width {
                z.rgb[y * z.width + x] = image.rgb[(y / zoom) * image.width + x / zoom];
            }
        }
        labels = labels.into_iter().map(|(s, y)| (s, y * zoom)).collect();
        z
    } else {
        image
    };
    image.save(path);
    let mut args: Vec<String> = vec![path.display().to_string(), "-fill".into(), "#ffd060".into(), "-font".into(), "Adwaita-Sans".into()];
    args.push("-pointsize".into());
    args.push(format!("{}", 26 * zoom.min(2)));
    for (text, y) in labels {
        args.push("-annotate".into());
        args.push(format!("+{}+{}", MARGIN, y + 26 * zoom.min(2)));
        args.push(text);
    }
    args.push(path.display().to_string());
    let status = Command::new("magick").args(&args).status().expect("magick");
    assert!(status.success());
    println!("wrote {}", path.display());
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let font_path = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| root.join("Resources/FNTOSD"));
    let font = Font::load(&font_path).expect("FNTOSD");
    let out = root.join("out/text-compare");
    std::fs::create_dir_all(&out).unwrap();
    let scratch = std::env::temp_dir().join("glyph_enhance_potrace");
    std::fs::create_dir_all(&scratch).unwrap();
    let mut glyphs = Glyphs { font: &font, prepared: HashMap::new(), potraced: HashMap::new(), scratch };
    let k = 4.8;
    let blue = [80.0, 112.0, 128.0, 128.0];
    let menu = [
        Line { text: codes("Browser   System Configuration"), size: 1.0, colour: [128.0; 4] },
        Line { text: codes("Clock Adjustment   Version Information"), size: 1.0, colour: blue },
    ];
    sheet(&mut glyphs, &menu, k, &ROWS, &out.join("compare-4k-menu-items.png"), 1);
    sheet(&mut glyphs, &menu, 2.0, &ROWS, &out.join("compare-2x-menu-items.png"), 1);
    sheet(&mut glyphs, &menu, 3.0, &ROWS, &out.join("compare-3x-menu-items.png"), 1);
    let ascii: String = (0x21u8..0x7f).map(char::from).collect();
    let ascii_lines = [line(&ascii[..32], 1.0), line(&ascii[32..64], 1.0), line(&ascii[64..], 1.0)];
    sheet(&mut glyphs, &ascii_lines, k, &ROWS, &out.join("compare-4k-ascii.png"), 1);
    let pangrams = [line("The quick brown fox jumps over the lazy dog.", 1.0), line("SPHINX OF BLACK QUARTZ, JUDGE MY VOW! 0123456789", 1.0)];
    sheet(&mut glyphs, &pangrams, k, &ROWS, &out.join("compare-4k-pangrams.png"), 1);
    let marks: Vec<i32> = (0xd803..=0xd818).collect();
    let mut hint = codes(" Enter  ");
    hint.insert(0, 0xd804);
    hint.push(0xd803);
    hint.extend(codes(" Back"));
    let small = [
        line("2026/11/15 22:22:12  0123456789", 0.83),
        Line { text: hint, size: 0.8, colour: [128.0; 4] },
        Line { text: marks.clone(), size: 0.8, colour: [128.0; 4] },
        line("Version Information  Clock Adjustment", 0.7),
    ];
    sheet(&mut glyphs, &small, k, &ROWS, &out.join("compare-4k-small-sizes.png"), 1);
    sheet(&mut glyphs, &[Line { text: marks, size: 1.0, colour: [128.0; 4] }], k, &ROWS, &out.join("compare-4k-marks.png"), 1);
    let hard = [line("aegswMW&@%8#", 1.0)];
    let zoom_rows = [ROWS[0], ROWS[1], ROWS[3], ROWS[5], ROWS[6], ROWS[7]];
    sheet(&mut glyphs, &[line("aegsw&@", 1.0)], k, &zoom_rows, &out.join("compare-zoom-corners-1.png"), 2);
    sheet(&mut glyphs, &[line("MW%8#", 1.0)], k, &zoom_rows, &out.join("compare-zoom-corners-2.png"), 2);
    sheet(&mut glyphs, &hard, k, &ROWS, &out.join("compare-4k-hard-cases.png"), 1);
    let other = [line("ΑΒΓΔ αβγδεζ ЖЗИЙ жзий", 1.0), line("時計設定 ブラウザ", 1.0)];
    sheet(&mut glyphs, &other, k, &ROWS, &out.join("compare-4k-non-latin.png"), 1);
}
