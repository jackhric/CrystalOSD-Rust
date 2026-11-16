use std::marker::PhantomData;

use crate::arithmetic::Arithmetic;
use crate::frame::Vertex;
use crate::matrix::{Mat4, Vec4};
use crate::opening::vu0::Vu0;

// facts/opening.md 5.1: the chain func_002214F8 starts for one tower, 0x840 bytes at 0x002A4EE0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TowerChain {
    pub bytes: [u8; 0x840],
}

impl Default for TowerChain {
    fn default() -> Self {
        Self { bytes: [0; 0x840] }
    }
}

pub const TOWER_CHAIN_ADDRESS: u32 = 0x002A4EE0;
// The GS offset the tower draws run under, in pixels.
pub const TOWER_OFFSET_X: f32 = 1728.0;
pub const TOWER_OFFSET_Y: f32 = 1936.0;

// Vertices as the GS receives them (z is the packed XYZ2's Z, bits 4 up); six faces of four vertices as strips; kicked[i] is false when the microprogram wrote the vertex without the
// drawing kick (ADC); alpha is the ALPHA register value of the blend packet it kicked first.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TowerVertices {
    pub vertices: [Vertex; 24],
    pub kicked: [bool; 24],
    pub alpha: u64,
}

fn read32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

// facts/opening.md 5.1 verify_opening_vu1_v2.mjs vifWords: the words VIF1 receives, the tags' upper halves included.
fn vif_words(chain: &TowerChain) -> Vec<u32> {
    let bytes = &chain.bytes;
    let mut words = Vec::new();
    let mut at = 0usize;
    for _ in 0..64 {
        if at + 16 > bytes.len() {
            panic!("tower chain: tag past the end");
        }
        let tag = read32(bytes, at);
        let address = read32(bytes, at + 4);
        let count = tag & 0xffff;
        let id = (tag >> 28) & 7;
        if at + 16 + count as usize * 16 > bytes.len() {
            panic!("tower chain: data past the end");
        }
        words.push(read32(bytes, at + 8));
        words.push(read32(bytes, at + 12));
        for i in 0..count * 4 {
            words.push(read32(bytes, at + 16 + i as usize * 4));
        }
        if id == 7 || id == 0 {
            return words;
        }
        if id == 2 {
            at = address.wrapping_sub(TOWER_CHAIN_ADDRESS) as usize;
        } else if id == 1 {
            at += (16 + count * 16) as usize;
        } else {
            panic!("tower chain: tag not handled");
        }
    }
    panic!("tower chain: no end");
}

struct Vu1<A> {
    memory: Vec<u8>,
    light: Mat4,
    normal: Mat4,
    matrix: Mat4,
    low: Vec4,
    high: Vec4,
    colour: [i32; 4],
    alpha: u64,
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> Vu1<A> {
    fn new() -> Self {
        Self {
            memory: vec![0; 0x4000],
            light: [[0.0; 4]; 4],
            normal: [[0.0; 4]; 4],
            matrix: [[0.0; 4]; 4],
            low: [0.0; 4],
            high: [0.0; 4],
            colour: [0; 4],
            alpha: 0,
            arithmetic: PhantomData,
        }
    }

    fn write(&mut self, address: u32, words: &[u32], count: u32) {
        for k in 0..count {
            let at = ((address << 4).wrapping_add(k.wrapping_mul(4)) & 0x3fff) as usize;
            self.memory[at..at + 4].copy_from_slice(&words[k as usize].to_le_bytes());
        }
    }

    // Instructions 0..25: world-to-screen x the tower's matrix, the registers, the blend packet.
    fn setup(&mut self) {
        let mut a = [[0.0f32; 4]; 4];
        for (r, row) in (0u32..).zip(a.iter_mut()) {
            *row = self.vector(r);
        }
        for i in 0..4 {
            let v = Vu0::<A>::apply(&a, &self.vector(4 + i));
            self.store(22 + i, &v);
        }
        for i in 0..4u32 {
            self.light[i as usize] = self.vector(8 + i);
        }
        for i in 0..4u32 {
            self.normal[i as usize] = self.vector(12 + i);
        }
        self.low = self.vector(16);
        self.high = self.vector(17);
        self.alpha = self.qword(19);
        if (self.word(21, 0) & 0xffff) != 0 {
            panic!("tower chain: entry 21 is not zero");
        }
        for i in 0..4u32 {
            self.matrix[i as usize] = self.vector(22 + i);
        }
    }

    // Instructions 33..96: one face.
    fn strip(&mut self, top: u32, out: &mut TowerVertices, face: usize) {
        let count = self.word(top, 0) & 0x7fff;
        if count != 4 {
            panic!("tower chain: a face without four vertices");
        }
        let mut wait = 0i32;
        for i in 0..count {
            let p = Vu0::<A>::apply(&self.matrix, &self.vector(top.wrapping_add(1).wrapping_add(i)));
            let mut n = Vu0::<A>::apply(&self.normal, &self.vector(top.wrapping_add(1).wrapping_add(count).wrapping_add(i)));
            let tint = self.vector(top.wrapping_add(1).wrapping_add(2 * count).wrapping_add(i));
            let q = A::quotient_exact(1.0, p[3]);
            let place: Vec4 = [A::mul(p[0], q), A::mul(p[1], q), A::mul(p[2], q), p[3]];
            let mut st = self.vector(top.wrapping_add(1).wrapping_add(3 * count).wrapping_add(i));
            for x in &mut st {
                *x = A::mul(*x, q);
            }
            for x in &mut n {
                *x = if *x > 0.0 { *x } else { 0.0 };
            }
            let mut inside = true;
            for k in [0usize, 1, 3] {
                inside = inside && A::sub_exact(self.low[k], place[k]) < 0.0 && A::sub_exact(place[k], self.high[k]) < 0.0;
            }
            let mut drawn = false;
            if inside {
                drawn = wait <= 0;
                wait -= 1;
            } else {
                wait = 3;
            }
            if inside {
                let lit = Vu0::<A>::apply(&self.light, &n);
                for k in 0..4 {
                    self.colour[k] = A::to_int(A::mul(tint[k], lit[k]));
                }
            }
            let v = &mut out.vertices[face * 4 + i as usize];
            v.u = st[0];
            v.v = st[1];
            v.q = st[2];
            v.r = self.colour[0] as u8;
            v.g = self.colour[1] as u8;
            v.b = self.colour[2] as u8;
            v.a = self.colour[3] as u8;
            v.x = ((A::to_int(A::mul(place[0], 16.0)) as u32) & 0xffff) as f32 / 16.0 - TOWER_OFFSET_X;
            v.y = ((A::to_int(A::mul(place[1], 16.0)) as u32) & 0xffff) as f32 / 16.0 - TOWER_OFFSET_Y;
            v.z = (A::to_int(A::mul(place[2], 16.0)) as u32) >> 4;
            out.kicked[face * 4 + i as usize] = drawn;
        }
    }

    fn word(&self, n: u32, k: usize) -> u32 {
        read32(&self.memory, (((n & 0x3ff) << 4) as usize) + k * 4)
    }

    fn qword(&self, n: u32) -> u64 {
        let at = ((n & 0x3ff) << 4) as usize;
        u64::from_le_bytes(self.memory[at..at + 8].try_into().unwrap())
    }

    fn vector(&self, n: u32) -> Vec4 {
        let mut v = [0.0f32; 4];
        for (k, x) in v.iter_mut().enumerate() {
            *x = f32::from_bits(self.word(n, k));
        }
        v
    }

    fn store(&mut self, n: u32, v: &Vec4) {
        for (k, x) in v.iter().enumerate() {
            let at = (((n & 0x3ff) << 4) as usize) + k * 4;
            self.memory[at..at + 4].copy_from_slice(&x.to_bits().to_le_bytes());
        }
    }
}

// facts/opening.md 5.1 verify_opening_vu1_v2.mjs run: the three parts of the microprogram, 0..25, 33..96 and the
// vertex rule of instructions 77 and 142.
pub struct TowerVu1<A>(PhantomData<A>);

impl<A: Arithmetic> TowerVu1<A> {
    // facts/opening.md 5.1 verify_opening_vu1_v2.mjs run: what VIF1 does with each code, as far as the chain uses them.
    pub fn run(chain: &TowerChain) -> TowerVertices {
        let words = vif_words(chain);
        let mut vu = Vu1::<A>::new();
        let mut out = TowerVertices::default();
        let mut base = 0u32;
        let mut offset = 0u32;
        let mut tops = 0u32;
        let mut top = 0u32;
        let mut flip = 0u32;
        let mut faces = 0usize;
        let start = |top: &mut u32, tops: &mut u32, flip: &mut u32, base: u32, offset: u32| {
            *top = *tops;
            *tops = if *flip == 0 { base.wrapping_add(offset) } else { base };
            *flip ^= 1;
        };
        let mut i = 0usize;
        while i < words.len() {
            let code = words[i];
            let command = (code >> 24) & 0x7f;
            let number = (code >> 16) & 0xff;
            let immediate = code & 0xffff;
            match command {
                0x00 | 0x01 | 0x10 | 0x11 | 0x13 => {}
                0x03 => base = immediate & 0x3ff,
                0x02 => {
                    offset = immediate & 0x3ff;
                    flip = 0;
                    tops = base;
                }
                0x14 => {
                    if immediate != 0 {
                        panic!("tower chain: MSCAL not 0");
                    }
                    start(&mut top, &mut tops, &mut flip, base, offset);
                    vu.setup();
                    out.alpha = vu.alpha;
                }
                0x17 => {
                    start(&mut top, &mut tops, &mut flip, base, offset);
                    if faces >= 6 {
                        panic!("tower chain: more than six faces");
                    }
                    vu.strip(top, &mut out, faces);
                    faces += 1;
                }
                0x6c => {
                    let address = (immediate & 0x3ff).wrapping_add(if (immediate & 0x8000) != 0 { tops } else { 0 });
                    let entries = if number == 0 { 256 } else { number };
                    if i + entries as usize * 4 >= words.len() {
                        panic!("tower chain: unpack past the end");
                    }
                    vu.write(address, &words[i + 1..], entries * 4);
                    i += entries as usize * 4;
                }
                _ => panic!("tower chain: VIF code not handled"),
            }
            i += 1;
        }
        if faces != 6 {
            panic!("tower chain: not six faces");
        }
        out
    }
}
