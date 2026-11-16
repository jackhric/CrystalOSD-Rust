#![allow(dead_code)]

use render::{Device, DeviceOptions};
use scene::frame::{BlendOp, CoordinateKind, DepthTest, Frame, GlyphCache, Material, Pass, PassTopology, Sampling, SourceKind, TargetName, TextureSet, Vertex};

// A headless Device with validation requested (as the C++ tests make it), or None when this machine has no Vulkan 1.4 device.
pub fn device() -> Option<Device> {
    match Device::new(None, &DeviceOptions { validation: true, sync_validation: false }) {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("skipped: no Vulkan device ({e})");
            None
        }
    }
}

pub fn shaders() -> &'static std::path::Path {
    std::path::Path::new(render::SHADER_DIRECTORY)
}

pub fn fnv(bytes: &[u8]) -> u64 {
    assets::bytes::hash_bytes(bytes, assets::bytes::FNV_OFFSET)
}

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 448;

#[allow(clippy::too_many_arguments)]
pub fn vertex(x: f32, y: f32, z: u32, u: f32, v: f32, q: f32, r: u8, g: u8, b: u8, a: u8) -> Vertex {
    Vertex { x, y, z, u, v, q, r, g, b, a }
}

#[allow(clippy::too_many_arguments)]
pub fn sprite_at(target: TargetName, blend: BlendOp, r: u8, g: u8, b: u8, a: u8, x0: f32, y0: f32, x1: f32, y1: f32) -> Pass {
    Pass {
        name: "sprite".into(),
        target,
        topology: PassTopology::Sprites,
        material: Material { blend, ..Default::default() },
        vertices: vec![vertex(x0, y0, 0, 0.0, 0.0, 1.0, r, g, b, a), vertex(x1, y1, 0, 0.0, 0.0, 1.0, r, g, b, a)],
        ..Default::default()
    }
}

pub fn sprite(blend: BlendOp, r: u8, g: u8, b: u8, a: u8) -> Pass {
    sprite_at(TargetName::Display, blend, r, g, b, a, 0.0, 0.0, 640.0, 224.0)
}

pub fn frame_of(passes: Vec<Pass>) -> Frame {
    frame_in(passes, TextureSet::Clock)
}

pub fn frame_in(passes: Vec<Pass>, set: TextureSet) -> Frame {
    let text_at = passes.len();
    Frame { passes, text_at, texture_set: set, ..Default::default() }
}

pub fn pixel_in(rgba: &[u8], width: u32, x: u32, y: u32) -> [i32; 4] {
    let at = (y as usize * width as usize + x as usize) * 4;
    [rgba[at] as i32, rgba[at + 1] as i32, rgba[at + 2] as i32, rgba[at + 3] as i32]
}

pub fn pixel(rgba: &[u8], x: u32, y: u32) -> [i32; 4] {
    pixel_in(rgba, WIDTH, x, y)
}

pub fn near(got: [i32; 4], r: f64, g: f64, b: f64, what: &str) -> bool {
    let want = [r, g, b];
    for i in 0..3 {
        if (f64::from(got[i]) - want[i]).abs() > 1.0 {
            eprintln!("{what}: got {} {} {}, want {r:.2} {g:.2} {b:.2}", got[0], got[1], got[2]);
            return false;
        }
    }
    true
}

// The render vector stream of the probe (probes/Stream.hpp): the NativeRenderer calls of a run with every argument.
pub enum Record {
    Configure(u32, u32, u32),
    SetTexture(TextureSet, i32, u32, u32, Vec<u8>),
    GlyphCache(GlyphCache, Vec<u8>),
    Draw(Box<Frame>),
    ReadTarget(TargetName, u64),
    Label(String),
    Present(u32, u32, TargetName, f32, u64),
}

pub struct Stream {
    data: Vec<u8>,
    at: usize,
}

fn target(v: i32) -> TargetName {
    [TargetName::Display, TargetName::RefractionSource, TargetName::Work, TargetName::Store, TargetName::Extra][v as usize]
}
fn texture_set(v: i32) -> TextureSet {
    [TextureSet::Clock, TextureSet::Opening][v as usize]
}
fn topology(v: i32) -> PassTopology {
    [PassTopology::Triangles, PassTopology::Lines, PassTopology::Sprites][v as usize]
}
fn source_kind(v: i32) -> SourceKind {
    [SourceKind::None, SourceKind::Texture, SourceKind::Target][v as usize]
}
fn coordinates(v: i32) -> CoordinateKind {
    [CoordinateKind::Texel, CoordinateKind::Projective][v as usize]
}
fn sampling(v: i32) -> Sampling {
    [Sampling::Repeat, Sampling::Clamp, Sampling::ClampToRegion][v as usize]
}
fn blend(v: i32) -> BlendOp {
    [
        BlendOp::Opaque,
        BlendOp::Add,
        BlendOp::AlphaOver,
        BlendOp::Subtract,
        BlendOp::FixedOver,
        BlendOp::FixedAdd,
        BlendOp::AddDestinationAlpha,
        BlendOp::SubtractFixed,
    ][v as usize]
}
fn depth_test(v: i32) -> DepthTest {
    [DepthTest::Always, DepthTest::GreaterEqual, DepthTest::Greater][v as usize]
}

impl Stream {
    pub fn open(path: &std::path::Path) -> Stream {
        let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(&data[..4], b"CRSV", "{} is not a render vector stream", path.display());
        let mut s = Stream { data, at: 4 };
        assert_eq!(s.u32(), 1, "stream version");
        s
    }
    fn take(&mut self, n: usize) -> &[u8] {
        let s = &self.data[self.at..self.at + n];
        self.at += n;
        s
    }
    fn u8(&mut self) -> u8 {
        self.take(1)[0]
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn i32(&mut self) -> i32 {
        self.u32() as i32
    }
    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take(8).try_into().unwrap())
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.u32())
    }
    fn b(&mut self) -> bool {
        self.u8() != 0
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        self.take(n).to_vec()
    }
    fn string(&mut self) -> String {
        let n = self.u64() as usize;
        String::from_utf8_lossy(self.take(n)).into_owned()
    }
    fn glyphs(&mut self) -> GlyphCache {
        let mut g = GlyphCache {
            address: self.u32(),
            cell_width: self.i32(),
            cell_height: self.i32(),
            width: self.i32(),
            log_width: self.i32(),
            log_height: self.i32(),
            cells: Vec::new(),
        };
        let n = self.u64() as usize;
        g.cells = (0..n).map(|_| self.i32()).collect();
        g
    }
    fn frame(&mut self) -> Frame {
        let mut f = Frame {
            width: self.i32(),
            height: self.i32(),
            field: self.i32(),
            display_index: self.i32(),
            text_at: self.u64() as usize,
            texture_set: texture_set(self.i32()),
            depth_bits: self.i32(),
            ..Default::default()
        };
        f.glyphs = self.glyphs();
        let passes = self.u64() as usize;
        for _ in 0..passes {
            let mut p = Pass { name: self.string(), target: target(self.i32()), topology: topology(self.i32()), ..Default::default() };
            let m = &mut p.material;
            m.source = source_kind(self.i32());
            m.texture = self.i32();
            m.source_target = target(self.i32());
            m.colour_only = self.b();
            m.coordinates = coordinates(self.i32());
            m.sampling = sampling(self.i32());
            for k in 0..4 {
                m.region[k] = self.i32();
            }
            m.bilinear = self.b();
            m.source_height = self.i32();
            m.blend = blend(self.i32());
            m.blend_constant = self.u8();
            m.depth_test = depth_test(self.i32());
            m.depth_write = self.b();
            m.gouraud = self.b();
            m.per_pixel_alpha = self.b();
            m.alpha_correction = self.b();
            p.edge_smoothing = self.b();
            p.half_line = self.b();
            p.prim_blend = self.b();
            let scissor = self.b();
            let s = [self.i32(), self.i32(), self.i32(), self.i32()];
            p.scissor = scissor.then_some(s);
            let n = self.u64() as usize;
            for _ in 0..n {
                p.vertices.push(Vertex {
                    x: self.f32(),
                    y: self.f32(),
                    z: self.u32(),
                    u: self.f32(),
                    v: self.f32(),
                    q: self.f32(),
                    r: self.u8(),
                    g: self.u8(),
                    b: self.u8(),
                    a: self.u8(),
                });
            }
            f.passes.push(p);
        }
        f
    }
}

impl Iterator for Stream {
    type Item = Record;
    fn next(&mut self) -> Option<Record> {
        if self.at >= self.data.len() {
            return None;
        }
        Some(match self.u8() {
            1 => Record::Configure(self.u32(), self.u32(), self.u32()),
            2 => {
                let set = texture_set(self.i32());
                let number = self.i32();
                let (w, h) = (self.u32(), self.u32());
                Record::SetTexture(set, number, w, h, self.bytes(w as usize * h as usize * 4))
            }
            3 => {
                let g = self.glyphs();
                let n = self.u64() as usize;
                Record::GlyphCache(g, self.bytes(n))
            }
            4 => Record::Draw(Box::new(self.frame())),
            5 => Record::ReadTarget(target(self.i32()), self.u64()),
            6 => Record::Label(self.string()),
            7 => Record::Present(self.u32(), self.u32(), target(self.i32()), self.f32(), self.u64()),
            t => panic!("unknown record {t}"),
        })
    }
}
