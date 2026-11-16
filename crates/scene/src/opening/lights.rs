use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, as_float};
use crate::frame::{BlendOp, CoordinateKind, DepthTest, Pass, PassTopology, Sampling, SourceKind, TargetName, Vertex};
use crate::matrix::{Mat4, Vec4};
use crate::opening::vu0::Vu0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LightStats {
    pub sprite_packets: i32,
    pub quads_drawn: i32,
    pub quads_left_out: i32,
    pub trail_packets: i32,
    pub trail_hidden: i32,
}

// verify_opening_lights_v2.mjs: the constants at 0x0036FA0C, the tables at 0x002B0E30, the clip box at 0x002B0C20
const FACTOR_BITS: [u32; 4] = [0x3c23d70a, 0x3dcccccd, 0x3ba3d70a, 0x3dcccccd];
const COLOURS: [[f32; 3]; 4] = [[32.0, 128.0, 0.0], [128.0, 32.0, 64.0], [128.0, 0.0, 0.0], [64.0, 32.0, 128.0]];
const GLOW_HALF: f32 = 0.8;
const CORE_HALF: f32 = 0.25;
const ALPHAS: [i32; 2] = [24, 12];
const CLIP_MIN: Vec4 = [1728.0, 1936.0, 0.0, 5.0];
const CLIP_MAX: Vec4 = [2368.0, 2160.0, 0.0, 16777215.0];
const CORNER_X: i32 = 1728 * 16;
const CORNER_Y: i32 = 1936 * 16;
const LIGHT_TEXTURE: i32 = 8;
// The FIX field of the lights' ALPHA register as the dump records it (the blend does not read it: C is the source alpha).
const ALPHA_FIX: u8 = 0x80;

fn mod128(x: i32) -> i32 {
    x.wrapping_sub(((if x < 0 { x.wrapping_add(127) } else { x }) >> 7) << 7)
}

fn corners_of(pair: i32) -> [Vec4; 4] {
    let h = if pair == 0 { GLOW_HALF } else { CORE_HALF };
    [[-h, -h, 0.0, 1.0], [h, -h, 0.0, 1.0], [-h, h, 0.0, 1.0], [h, h, 0.0, 1.0]]
}

const COORDINATES: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

fn sprite_pass() -> Pass {
    let mut pass = Pass { name: "light sprites".into(), target: TargetName::Display, topology: PassTopology::Triangles, ..Default::default() };
    pass.material.source = SourceKind::Texture;
    pass.material.texture = LIGHT_TEXTURE;
    pass.material.coordinates = CoordinateKind::Projective;
    pass.material.sampling = Sampling::Repeat;
    pass.material.bilinear = true;
    pass.material.blend = BlendOp::Add;
    pass.material.blend_constant = ALPHA_FIX;
    pass.material.depth_test = DepthTest::Always;
    pass.material.depth_write = false;
    pass.material.gouraud = true;
    pass.half_line = true;
    pass
}

fn trail_pass() -> Pass {
    let mut pass = Pass { name: "light trails".into(), target: TargetName::Display, topology: PassTopology::Lines, ..Default::default() };
    pass.material.blend = BlendOp::Add;
    pass.material.blend_constant = ALPHA_FIX;
    pass.material.depth_test = DepthTest::Always;
    pass.material.depth_write = false;
    pass.material.gouraud = true;
    pass.half_line = true;
    pass.edge_smoothing = true;
    pass
}

const LIGHTS: usize = 4;
const RING: usize = 128;

// facts/opening.md 4.2 (OpeningDrawLights 0x0021F5F8): verify_opening_lights_v2.mjs expected.
// draw takes the world-to-screen matrix; the matrix block's first matrix (the lights' base) is the unit matrix.
// A sprite packet or trail with nothing to draw adds no pass; the stats count every packet the OSD sends.
#[derive(Debug, Clone)]
pub struct Lights<A> {
    phase: i32,
    head: i32,
    tail: i32,
    matrices: [[Mat4; 4]; LIGHTS],
    ring: [[[i32; 4]; RING]; LIGHTS],
    centres: [Vec4; LIGHTS],
    angles: [f32; LIGHTS * 2],
    cosines: [f32; LIGHTS],
    sines: [f32; LIGHTS],
    stats: LightStats,
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> Lights<A> {
    pub const LIGHTS: usize = LIGHTS;
    pub const RING: usize = RING;

    pub fn new(phase: u32) -> Self {
        Self {
            phase: phase as i32,
            head: 0,
            tail: 0,
            matrices: [[Vu0::<A>::unit_matrix(); 4]; LIGHTS],
            ring: [[[0; 4]; RING]; LIGHTS],
            centres: [[0.0; 4]; LIGHTS],
            angles: [0.0; LIGHTS * 2],
            cosines: [0.0; LIGHTS],
            sines: [0.0; LIGHTS],
            stats: LightStats::default(),
            arithmetic: PhantomData,
        }
    }

    pub fn head(&self) -> i32 {
        self.head
    }
    pub fn tail(&self) -> i32 {
        self.tail
    }
    pub fn matrices(&self) -> &[[Mat4; 4]; LIGHTS] {
        &self.matrices
    }
    pub fn ring(&self) -> &[[[i32; 4]; RING]; LIGHTS] {
        &self.ring
    }
    pub fn centres(&self) -> &[Vec4; LIGHTS] {
        &self.centres
    }
    pub fn angles(&self) -> &[f32; LIGHTS * 2] {
        &self.angles
    }
    pub fn cosines(&self) -> &[f32; LIGHTS] {
        &self.cosines
    }
    pub fn sines(&self) -> &[f32; LIGHTS] {
        &self.sines
    }
    pub fn stats(&self) -> &LightStats {
        &self.stats
    }

    // verify_opening_lights_v2.mjs expected
    pub fn draw(&mut self, counter: i32, world_to_screen: &Mat4, out: &mut Vec<Pass>) {
        self.stats = LightStats::default();
        type V<A> = Vu0<A>;
        let ka = as_float(FACTOR_BITS[0]);
        let kb = as_float(FACTOR_BITS[1]);
        let kc = as_float(FACTOR_BITS[2]);
        let kd = as_float(FACTOR_BITS[3]);
        let base = V::<A>::unit_matrix();
        let origin: Vec4 = [0.0, 0.0, 0.0, 1.0];

        let transform = |m: &Mat4, v: &Vec4| -> ([i32; 3], f32) {
            let p = V::<A>::apply(m, v);
            let q = A::quotient_exact(1.0, p[3]);
            ([A::to_int(A::mul(A::mul(p[0], q), 16.0)), A::to_int(A::mul(A::mul(p[1], q), 16.0)), A::to_int(A::mul(A::mul(p[2], q), 16.0)) >> 4], q)
        };

        let mut sprites = sprite_pass();
        for i in 0..LIGHTS {
            let fi = i as i32;
            let shifted = counter.wrapping_add(self.phase) as f32;
            let a = A::mul(A::mul(A::mul(A::add_exact(shifted, (17 * fi) as f32), ka), A::add_exact(fi as f32, 10.0)), kb);
            let b = A::mul(A::mul(A::mul(A::add_exact(shifted, (15 * fi) as f32), kc), A::add_exact(fi as f32, 10.0)), kd);
            self.angles[i * 2] = a;
            self.angles[i * 2 + 1] = b;
            let cosine = A::cosf(a);
            let sine = A::sinf(b);
            self.cosines[i] = cosine;
            self.sines[i] = sine;
            for n in 0..4 {
                let m;
                if n == 3 {
                    let centre: Vec4 = [
                        A::mul(A::sub_exact(10.0, fi as f32), cosine),
                        A::mul(A::add_exact(fi as f32, 3.0), sine),
                        A::add_exact(A::mul(cosine, 12.0), 88.0),
                        0.0,
                    ];
                    self.centres[i] = centre;
                    let moved: Mat4 = [
                        base[0],
                        base[1],
                        base[2],
                        [A::add_exact(base[3][0], centre[0]), A::add_exact(base[3][1], centre[1]), A::add_exact(base[3][2], centre[2]), base[3][3]],
                    ];
                    m = V::<A>::mul_matrix(world_to_screen, &moved);
                    self.matrices[i][3] = m;
                } else {
                    self.matrices[i][n] = self.matrices[i][n + 1];
                    m = self.matrices[i][n];
                }
                let (at, _) = transform(&m, &origin);
                self.ring[i][self.head as usize] = [at[0], at[1], at[2], 16];

                self.stats.sprite_packets += 1;
                for pair in 0..2i32 {
                    let quad = corners_of(pair);
                    if V::<A>::clip_all(&CLIP_MIN, &CLIP_MAX, &m, &quad) {
                        self.stats.quads_left_out += 1;
                        continue;
                    }
                    self.stats.quads_drawn += 1;
                    let alpha = ALPHAS[pair as usize] * (n as i32 + 1) / 5;
                    let mut v = [Vertex::default(); 4];
                    for (k, vertex) in v.iter_mut().enumerate() {
                        let (s, w) = transform(&m, &quad[k]);
                        vertex.x = s[0].wrapping_sub(CORNER_X) as f32 * 0.0625;
                        vertex.y = s[1].wrapping_sub(CORNER_Y) as f32 * 0.0625;
                        vertex.z = (s[2] as u32) & 0xffffff;
                        vertex.u = A::mul(COORDINATES[k][0], w);
                        vertex.v = A::mul(COORDINATES[k][1], w);
                        vertex.q = w;
                        if pair == 0 {
                            vertex.r = A::to_unsigned(A::mul(COLOURS[i % 4][0], 0.5)) as u8;
                            vertex.g = A::to_unsigned(A::mul(COLOURS[i % 4][1], 0.5)) as u8;
                            vertex.b = A::to_unsigned(A::mul(COLOURS[i % 4][2], 0.5)) as u8;
                        } else {
                            vertex.r = 0x80;
                            vertex.g = 0x80;
                            vertex.b = 0x80;
                        }
                        vertex.a = alpha as u8;
                    }
                    sprites.vertices.extend_from_slice(&[v[0], v[1], v[2], v[1], v[2], v[3]]);
                }
            }
        }
        if !sprites.vertices.is_empty() {
            out.push(sprites);
        }

        self.head = mod128(self.head.wrapping_add(1));
        if self.head == self.tail {
            self.tail = mod128(self.tail.wrapping_add(1));
        }

        let mut trails = trail_pass();
        for i in 0..LIGHTS {
            self.stats.trail_packets += 1;
            let end = mod128(self.tail + 1);
            let mut at = mod128(self.head + 127);
            let length = mod128(self.head - (end - 127));
            let mut previous_y = 0i32;
            let mut strip: Vec<Vertex> = Vec::new();
            let mut kicked: Vec<bool> = Vec::new();
            let mut step = 0i32;
            while at != end {
                let level = ((length - step) << 6) / length;
                if (step & 7) == 0 {
                    let level_value = level as f32;
                    let channel = |c: f32| A::to_int(A::mul(A::mul(c, level_value), 0.0078125));
                    let entry = &self.ring[i][at as usize];
                    let before = previous_y;
                    let sx = (entry[0] >> 4).wrapping_sub(0x6c0);
                    let sy = (entry[1] >> 4).wrapping_sub(0x790);
                    previous_y = sy;
                    let hidden = entry[2] < 5 || sx < 0 || 640 < sx || sy < 0 || 224 < sy || before < 0 || 640 < before;
                    let v = Vertex {
                        x: entry[0].wrapping_sub(CORNER_X) as f32 * 0.0625,
                        y: entry[1].wrapping_sub(CORNER_Y) as f32 * 0.0625,
                        z: (entry[2] as u32) & 0xffffff,
                        r: channel(COLOURS[i][0]) as u8,
                        g: channel(COLOURS[i][1]) as u8,
                        b: channel(COLOURS[i][2]) as u8,
                        a: (level << 1) as u8,
                        ..Default::default()
                    };
                    strip.push(v);
                    kicked.push(!hidden);
                    if hidden {
                        self.stats.trail_hidden += 1;
                    }
                }
                step += 1;
                at = mod128(at + 127);
            }
            for j in 1..strip.len() {
                if kicked[j] {
                    trails.vertices.extend_from_slice(&[strip[j - 1], strip[j]]);
                }
            }
        }
        if !trails.vertices.is_empty() {
            out.push(trails);
        }
    }
}
