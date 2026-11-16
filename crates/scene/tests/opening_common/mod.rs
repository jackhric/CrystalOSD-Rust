#![allow(dead_code)]

use std::path::PathBuf;

use scene::frame::{Frame, Material};
use scene::program_image::ProgramImage;

pub const FNV_OFFSET: u64 = 0xcbf29ce484222325;

// FNV-1a 64 over bytes, as Source/App/Golden.cpp's Hasher.
#[derive(Debug, Clone, Copy)]
pub struct Bytes(pub u64);

impl Default for Bytes {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Bytes {
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    pub fn bool(&mut self, v: bool) {
        self.bytes(&[u8::from(v)]);
    }
    pub fn size(&mut self, v: usize) {
        self.u64(v as u64);
    }
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    pub fn i32s(&mut self, values: &[i32]) {
        for &v in values {
            self.i32(v);
        }
    }
}

fn material(x: &mut Bytes, m: &Material) {
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

// Source/App/Golden.cpp hashFrame at commit 039cc58, field for field (enums as 32-bit, bool as one byte, sizes as 64-bit).
pub fn hash_frame(f: &Frame) -> u64 {
    let mut x = Bytes::default();
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

pub fn program() -> Option<ProgramImage> {
    let folder =
        std::env::var_os("CRYSTALOSD_RESOURCES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Resources"));
    let path = folder.join("hddosd.elf");
    if !path.is_file() {
        eprintln!("skipped: no hddosd.elf in {}", folder.display());
        return None;
    }
    Some(ProgramImage::load(&path).unwrap())
}

pub fn golden_scenes() -> Option<Vec<(u64, String, u64)>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden/cpp-native.golden");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("skipped: no golden file at {}", path.display());
        return None;
    };
    Some(
        text.lines()
            .map(|line| {
                let parts: Vec<&str> = line.split(' ').collect();
                let scene = parts[3].strip_prefix("scene=").unwrap();
                (parts[0].parse().unwrap(), parts[1].to_owned(), u64::from_str_radix(scene, 16).unwrap())
            })
            .collect(),
    )
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    pub fn next32(&mut self) -> u32 {
        (self.next() >> 32) as u32
    }
}
