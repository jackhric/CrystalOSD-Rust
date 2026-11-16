use std::fs;
use std::io::{self, Write};
use std::path::Path;

use assets::bytes::{FNV_OFFSET, hash_bytes as fnv};
use scene::frame::{Frame, Material};

// The bytes hashed are those the C++ build hashes (Source/App/Golden.cpp hashFrame): enums as 32-bit, bool as one
// byte, sizes as 64-bit, floats by their bits, all little-endian, in the same field order.
struct Hasher(u64);

impl Hasher {
    fn bytes(&mut self, bytes: &[u8]) {
        self.0 = fnv(bytes, self.0);
    }
    fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    fn bool(&mut self, v: bool) {
        self.bytes(&[u8::from(v)]);
    }
    fn size(&mut self, v: usize) {
        self.bytes(&(v as u64).to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    fn i32s(&mut self, values: &[i32]) {
        for &v in values {
            self.i32(v);
        }
    }
}

fn material(x: &mut Hasher, m: &Material) {
    x.i32(m.source as i32);
    x.i32(m.texture);
    x.i32(m.source_target as i32);
    x.bool(m.colour_only);
    x.i32(m.coordinates as i32);
    x.i32(m.sampling as i32);
    x.i32s(&m.region);
    x.bool(m.bilinear);
    x.i32(m.source_height);
    x.i32(m.blend as i32);
    x.u8(m.blend_constant);
    x.i32(m.depth_test as i32);
    x.bool(m.depth_write);
    x.bool(m.gouraud);
    x.bool(m.per_pixel_alpha);
    x.bool(m.alpha_correction);
}

pub fn hash_bytes(bytes: &[u8], seed: u64) -> u64 {
    fnv(bytes, seed)
}

pub fn hash_frame(f: &Frame) -> u64 {
    let mut x = Hasher(FNV_OFFSET);
    x.i32(f.width);
    x.i32(f.height);
    x.i32(f.field);
    x.i32(f.display_index);
    x.size(f.text_at);
    x.i32(f.texture_set as i32);
    x.i32(f.depth_bits);
    x.u32(f.glyphs.address);
    x.i32(f.glyphs.cell_width);
    x.i32(f.glyphs.cell_height);
    x.i32(f.glyphs.width);
    x.i32(f.glyphs.log_width);
    x.i32(f.glyphs.log_height);
    x.size(f.glyphs.cells.len());
    x.i32s(&f.glyphs.cells);
    for g in &f.glyph_layouts {
        x.u32(g.address);
        x.i32(g.cell_width);
        x.i32(g.cell_height);
        x.i32(g.width);
        x.i32(g.log_width);
        x.i32(g.log_height);
        x.size(g.cells.len());
        x.i32s(&g.cells);
    }
    x.size(f.passes.len());
    for p in &f.passes {
        x.i32(p.target as i32);
        x.i32(p.topology as i32);
        material(&mut x, &p.material);
        x.bool(p.edge_smoothing);
        x.bool(p.half_line);
        x.bool(p.prim_blend);
        x.bool(p.scissor.is_some());
        if let Some(scissor) = &p.scissor {
            x.i32s(scissor);
        }
        x.size(p.vertices.len());
        for v in &p.vertices {
            x.f32(v.x);
            x.f32(v.y);
            x.u32(v.z);
            x.f32(v.u);
            x.f32(v.v);
            x.f32(v.q);
            x.u8(v.r);
            x.u8(v.g);
            x.u8(v.b);
            x.u8(v.a);
        }
    }
    x.0
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoldenLine {
    pub frame: u64,
    pub phase: String,
    pub screen: String,
    pub scene: u64,
    pub pixels: u64,
    pub sound: u64,
}

fn underscored(text: &str) -> String {
    if text.is_empty() { "-".to_owned() } else { text.replace(' ', "_") }
}

pub fn format_line(line: &GoldenLine) -> String {
    format!(
        "{:06} {} {} scene={:016x} pixels={:016x} sound={:016x}",
        line.frame,
        underscored(&line.phase),
        underscored(&line.screen),
        line.scene,
        line.pixels,
        line.sound
    )
}

pub fn parse_line(text: &str) -> Option<GoldenLine> {
    let mut words = text.split_whitespace();
    let frame = words.next()?.parse().ok()?;
    let phase = words.next()?.to_owned();
    let screen = words.next()?;
    let mut hash = |prefix: &str| u64::from_str_radix(words.next()?.strip_prefix(prefix)?, 16).ok();
    Some(GoldenLine {
        frame,
        phase,
        screen: if screen == "-" { String::new() } else { screen.to_owned() },
        scene: hash("scene=")?,
        pixels: hash("pixels=")?,
        sound: hash("sound=")?,
    })
}

pub struct GoldenWriter {
    out: fs::File,
}

impl GoldenWriter {
    pub fn new(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        Ok(Self { out: fs::File::create(path)? })
    }

    pub fn add(&mut self, line: &GoldenLine) -> io::Result<()> {
        writeln!(self.out, "{}", format_line(line))
    }
}

/// Which of a line's hashes a check compares: a port that has no renderer or no sound yet checks the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compared {
    pub scene: bool,
    pub pixels: bool,
    pub sound: bool,
}

impl Compared {
    pub const ALL: Self = Self { scene: true, pixels: true, sound: true };
}

pub struct GoldenChecker {
    lines: Vec<GoldenLine>,
    report: String,
    compared: u64,
    fields: Compared,
}

impl GoldenChecker {
    pub fn new(path: &Path, fields: Compared) -> Self {
        let text = fs::read_to_string(path).unwrap_or_default();
        Self { lines: text.lines().filter_map(parse_line).collect(), report: String::new(), compared: 0, fields }
    }

    pub fn check(&mut self, line: &GoldenLine) -> bool {
        let Some(want) = self.lines.get(self.compared as usize) else {
            self.report = format!("frame {}: the golden file has only {} lines", line.frame, self.lines.len());
            return false;
        };
        let mut what = String::new();
        if self.fields.scene && want.scene != line.scene {
            what += " scene";
        }
        if self.fields.pixels && want.pixels != line.pixels {
            what += " pixels";
        }
        if self.fields.sound && want.sound != line.sound {
            what += " sound";
        }
        if want.frame != line.frame {
            what += " frame-number";
        }
        if !what.is_empty() {
            self.report = format!("frame {} ({}): differs in{what}\n  golden {}\n  now    {}", line.frame, line.screen, format_line(want), format_line(line));
            return false;
        }
        self.compared += 1;
        true
    }

    pub fn report(&self) -> &str {
        &self.report
    }

    pub fn compared(&self) -> u64 {
        self.compared
    }

    pub fn size(&self) -> u64 {
        self.lines.len() as u64
    }
}
