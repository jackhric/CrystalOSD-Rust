#![allow(dead_code)]

use std::path::PathBuf;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hash {
    pub h: u64,
    pub count: u64,
}

impl Default for Hash {
    fn default() -> Self {
        Self { h: 0xcbf29ce484222325, count: 0 }
    }
}

impl Hash {
    fn byte(&mut self, b: u8) {
        self.h = (self.h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    pub fn u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.byte(b);
        }
        self.count += 1;
    }
    pub fn u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.byte(b);
        }
        self.count += 1;
    }
    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    pub fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }
    pub fn boolean(&mut self, v: bool) {
        self.u32(u32::from(v));
    }
    pub fn pair(&self) -> (u64, u64) {
        (self.count, self.h)
    }
}

pub fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}

pub const EDGE_FLOATS: [u32; 71] = [
    0x00000000, 0x80000000, 0x00000001, 0x80000001, 0x007fffff, 0x807fffff, 0x00400000, 0x00800000, 0x80800000, 0x00800001, 0x00ffffff, 0x3f800000, 0xbf800000,
    0x3f800001, 0x3f7fffff, 0xbf7fffff, 0x7f7fffff, 0xff7fffff, 0x7f7ffffe, 0x7f000000, 0x7f800000, 0xff800000, 0x7fc00000, 0xffc00000, 0x7f800001, 0x7fa00000,
    0xffffffff, 0x40000000, 0x3f000000, 0x34000000, 0x33800000, 0x33000000, 0x4b800000, 0x4b7fffff, 0x4f000000, 0x4effffff, 0xcf000000, 0xcf000001, 0x4f800000,
    0x4f7fffff, 0x3fc90fdb, 0x40490fdb, 0x3f490fd8, 0x3f490fd9, 0x4016cbe3, 0x4016cbe4, 0x43490f80, 0x43490f81, 0x3fc90fd0, 0x31ffffff, 0x32000000, 0x3e999999,
    0x3e99999a, 0x3f480000, 0x3f480001, 0x0c000000, 0x1f800000, 0x5f800000, 0x447a0000, 0x42700000, 0x47800000, 0xc7000000, 0x46fffe00, 0x3dcccccd, 0xbdcccccd,
    0x3f666666, 0x3f7d70a4, 0x01000000, 0x00800002, 0x7e800000, 0xfe800000,
];

pub fn pair_set(kind: u64, n: u32) -> Vec<(u32, u32)> {
    if kind == 2 {
        return EDGE_FLOATS.iter().flat_map(|&a| EDGE_FLOATS.iter().map(move |&b| (a, b))).collect();
    }
    let mut r = Rng::new(0x5eed0000 + kind);
    (0..n)
        .map(|_| {
            if kind == 0 {
                let a = r.next32();
                (a, r.next32())
            } else {
                let x = r.next();
                let a = x as u32;
                let raw = (x >> 32) as u32;
                let eb = (((a >> 23) & 0xff) as i32 + (r.next() % 129) as i32 - 64).clamp(0, 255);
                (a, (raw & 0x807fffff) | ((eb as u32) << 23))
            }
        })
        .collect()
}

pub fn single_set(kind: u64, n: u32) -> Vec<u32> {
    if kind == 2 {
        return EDGE_FLOATS.to_vec();
    }
    let mut r = Rng::new(0x51e60000 + kind);
    if kind == 0 {
        return (0..n).map(|_| r.next32()).collect();
    }
    let mut out = Vec::new();
    for e in 0..256u32 {
        for _ in 0..64 {
            let m = r.next32() & 0x7fffff;
            out.push((e << 23) | m);
            out.push(0x80000000 | (e << 23) | m);
        }
    }
    out
}

pub fn double_set(kind: u64, n: u32) -> Vec<u64> {
    if kind == 2 {
        return vec![
            0x0000000000000000,
            0x8000000000000000,
            0x0000000000000001,
            0x000fffffffffffff,
            0x0010000000000000,
            0x47efffffe0000000,
            0x47efffffe0000001,
            0x47efffffdfffffff,
            0x47efffffefffffff,
            0x47effffff0000000,
            0x47effffff0000001,
            0x47f0000000000000,
            0xc7efffffe0000000,
            0xc7effffff0000000,
            0xc7f0000000000000,
            0x7ff0000000000000,
            0xfff0000000000000,
            0x7ff8000000000000,
            0xfff8000000000000,
            0x7ff0000000000001,
            0x3810000000000000,
            0x380fffffffffffff,
            0xb80fffffffffffff,
            0x3810000000000001,
            0x36a0000000000000,
            0x3ff0000000000000,
            0x3fefffffffffffff,
            0x3ff0000000000001,
            0x41dfffffffc00000,
            0x41e0000000000000,
        ];
    }
    let mut r = Rng::new(0xd0b1e000 + kind);
    (0..n)
        .map(|_| {
            if kind == 0 {
                r.next()
            } else {
                let d = f64::from(f(r.next32()));
                let delta = (r.next() % (1 << 30)) as i64 - (1 << 29);
                d.to_bits().wrapping_add(delta as u64)
            }
        })
        .collect()
}

pub fn int_set(n: u32) -> Vec<i32> {
    let mut out = vec![0, 1, -1, 16777216, 16777217, -16777217, i32::MAX, i32::MIN, 0x7fffff80, 0x7fffffc0, 0x7fffffbf, 33554431, 33554433, -33554435];
    let mut r = Rng::new(0x1e7e9e40);
    out.extend((0..n).map(|_| r.next32() as i32));
    out
}

pub fn trig_set(kind: u32, n: u32) -> Vec<u32> {
    let mut out = Vec::new();
    match kind {
        0 => {
            for b in (0..=0x43500000u32).step_by(211) {
                out.push(b);
                out.push(b | 0x80000000);
            }
            for b in (0x43500000u32..0x7fc00000).step_by(13499) {
                out.push(b);
                out.push(b | 0x80000000);
            }
            out.extend_from_slice(&EDGE_FLOATS);
        }
        1 => out.extend((0..n).map(|i| ((-210.0 + f64::from(i) * (420.0 / f64::from(n))) as f32).to_bits())),
        2 => {
            for b in (0..=0x40a00000u32).step_by(101) {
                out.push(b);
                out.push(b | 0x80000000);
            }
            out.extend_from_slice(&EDGE_FLOATS);
        }
        _ => {
            let mut r = Rng::new(0x7819e000);
            out.extend((0..n).map(|_| r.next32()));
        }
    }
    out
}

pub fn resources() -> Option<PathBuf> {
    let folder =
        std::env::var_os("CRYSTALOSD_RESOURCES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Resources"));
    if folder.join("hddosd.elf").is_file() {
        Some(folder)
    } else {
        eprintln!("skipped: no resource files in {}", folder.display());
        None
    }
}

// Source/App/Golden.cpp hashFrame (FNV-1a 64 over the frame's bytes: enums as 32-bit, bool as one byte, sizes as
// 64-bit, floats by their bits, little-endian, in its field order); crates/app/src/golden.rs is the app's port.
pub struct FrameHasher(pub u64);

impl FrameHasher {
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
    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    pub fn bool(&mut self, v: bool) {
        self.bytes(&[u8::from(v)]);
    }
    pub fn size(&mut self, v: usize) {
        self.bytes(&(v as u64).to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
}

pub fn hash_frame(f: &scene::frame::Frame) -> u64 {
    let mut x = FrameHasher(0xcbf29ce484222325);
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
    for &c in &f.glyphs.cells {
        x.i32(c);
    }
    x.size(f.passes.len());
    for p in &f.passes {
        x.i32(p.target as i32);
        x.i32(p.topology as i32);
        let m = &p.material;
        x.i32(m.source as i32);
        x.i32(m.texture);
        x.i32(m.source_target as i32);
        x.bool(m.colour_only);
        x.i32(m.coordinates as i32);
        x.i32(m.sampling as i32);
        for &r in &m.region {
            x.i32(r);
        }
        x.bool(m.bilinear);
        x.i32(m.source_height);
        x.i32(m.blend as i32);
        x.u8(m.blend_constant);
        x.i32(m.depth_test as i32);
        x.bool(m.depth_write);
        x.bool(m.gouraud);
        x.bool(m.per_pixel_alpha);
        x.bool(m.alpha_correction);
        x.bool(p.edge_smoothing);
        x.bool(p.half_line);
        x.bool(p.prim_blend);
        x.bool(p.scissor.is_some());
        if let Some(s) = &p.scissor {
            for &v in s {
                x.i32(v);
            }
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

pub fn mesh_at(p: &scene::program_image::ProgramImage, faces: u32, positions: u32, normals: u32, coordinates: u32) -> scene::rods::RodMesh {
    use scene::elf_records::vec4_at;
    scene::rods::RodMesh {
        positions: (0..faces * 4).map(|i| vec4_at(p, positions + 16 * i)).collect(),
        normals: (0..faces).map(|i| vec4_at(p, normals + 16 * i)).collect(),
        coordinates: (0..faces * 4).map(|i| vec4_at(p, coordinates + 16 * i)).collect(),
    }
}

// Source/Assets/Resources.cpp RODMESH and CUBEMESH: HDD OSD 1.10U's meshes, read from hddosd.elf.
pub fn cold_assets(folder: &std::path::Path) -> scene::cold_start::ColdAssets {
    use std::sync::Arc;
    let program = Arc::new(scene::program_image::ProgramImage::load(&folder.join("hddosd.elf")).unwrap());
    let font = Arc::new(scene::font::Font::load(&folder.join("FNTOSD")).unwrap());
    let mesh = mesh_at(&program, 16, 0x002b4b90, 0x002b5390, 0x002b4f90);
    let cube_mesh = mesh_at(&program, 6, 0x002b5790, 0x002b5910, 0x002b5970);
    scene::cold_start::ColdAssets { program, font: Some(font), mesh, cube_mesh }
}
pub mod hashes;
pub mod scenarios;
pub mod shell;
