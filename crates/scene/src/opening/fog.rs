use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, as_float};
use crate::frame::{BlendOp, CoordinateKind, DepthTest, Pass, PassTopology, Sampling, SourceKind, TargetName, Vertex};
use crate::matrix::{Mat4, Vec4};
use crate::opening::vu0::Vu0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FogStats {
    pub drawn: i32,
    pub left_out: i32,
}

const RADIUS_SQUARED: f32 = 5202.0;
const HALF_PI: f64 = std::f64::consts::FRAC_PI_2;
const TEXTURES: [i32; 6] = [5, 3, 2, 5, 3, 2];

// verify_opening_fog_v2.mjs: the clip box at 0x002B0C20, the scroll step at 0x0036F990
const CLIP_MIN: Vec4 = [1728.0, 1936.0, 0.0, 5.0];
const CLIP_MAX: Vec4 = [2368.0, 2160.0, 0.0, 16777215.0];
const STEP_BITS: u32 = 0x38d1b717;
const CORNER_X: i32 = 1728 * 16;
const CORNER_Y: i32 = 1936 * 16;

const POINT_COUNT: usize = 17 * 17;

// facts/opening.md 4.3, 4.6 (func_0021DEA8, OpeningDrawFog 0x0021E168): verify_opening_fog_v2.mjs expected, verify_opening_inputs.mjs fog.
// draw takes the world-to-screen matrix; a layer whose quads are all clipped adds no pass.
#[derive(Debug, Clone)]
pub struct Fog<A> {
    offsets: [f32; 6],
    mesh: [Vec4; POINT_COUNT],
    brightness: [[i32; 4]; POINT_COUNT],
    stats: FogStats,
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> Default for Fog<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Arithmetic> Fog<A> {
    pub const POINTS: usize = 17;

    // verify_opening_inputs.mjs tableSine and fog; the counter is 0 at set-up
    pub fn new() -> Self {
        let table_sine = |angle: i32| {
            let mut a = if angle < 0 { -angle } else { angle };
            if a >= 0x4000 {
                a = 0x8000 - a;
            }
            let s = A::sinf((f64::from(a) * HALF_PI * (1.0 / 16384.0)) as f32);
            if angle < 0 { -s } else { s }
        };
        let sum = |a: f32, b: f32| A::cut(f64::from(a) + f64::from(b));
        let mut mesh = [[0.0f32; 4]; POINT_COUNT];
        let mut brightness = [[0i32; 4]; POINT_COUNT];
        let radius = A::root_exact(RADIUS_SQUARED);
        let centre = -5.1f32;
        let angle = 0i32;
        let cosine = table_sine(angle + 0x4000);
        let sine = table_sine(angle);
        let points = Self::POINTS as i32;
        for i in 0..points {
            for j in 0..points {
                let x = sum(sum((j * 6) as f32, -48.0), sum(cosine, cosine));
                let y = sum(sum((i * 6) as f32, -48.0), sum(sine, sine));
                let dx = A::cut(f64::from(centre) - f64::from(sum(A::mul(A::mul((2 * j - 16) as f32, 6.0), 0.5), 3.0)));
                let dy = A::cut(0.0 - f64::from(sum(A::mul(A::mul((2 * i - 16) as f32, 6.0), 0.5), 3.0)));
                let d = A::root_exact(sum(A::mul(dx, dx), A::mul(dy, dy)));
                let over = A::cut(f64::from(radius) - f64::from(A::mul(d, 4.0)));
                let mut level = sum(A::quotient_exact(A::mul(over, 96.0), radius), 0.0);
                level = if level < 0.0 {
                    0.0
                } else if 127.0 < level {
                    127.0
                } else {
                    level
                };
                let dark = A::to_int(A::mul(A::mul(level, 0.0), 0.0078125));
                let at = (j * points + i) as usize;
                mesh[at] = [x, y, 134.0, 1.0];
                brightness[at] = [dark, dark, A::to_int(A::mul(A::mul(level, 128.0), 0.0078125)), 0x80];
            }
        }
        Self { offsets: [0.0; 6], mesh, brightness, stats: FogStats::default(), arithmetic: PhantomData }
    }

    pub fn offsets(&self) -> &[f32; 6] {
        &self.offsets
    }

    pub fn mesh(&self) -> &[Vec4; POINT_COUNT] {
        &self.mesh
    }

    pub fn brightness(&self) -> &[[i32; 4]; POINT_COUNT] {
        &self.brightness
    }

    pub fn stats(&self) -> &FogStats {
        &self.stats
    }

    // verify_opening_fog_v2.mjs expected
    pub fn draw(&mut self, world_to_screen: &Mat4, out: &mut Vec<Pass>) {
        self.stats = FogStats::default();
        type V<A> = Vu0<A>;
        let points = Self::POINTS;
        let step = as_float(STEP_BITS);
        for layer in 0..6i32 {
            let mut scroll = A::add_exact(
                self.offsets[layer as usize],
                A::mul(A::mul(A::mul(A::sub_exact(14.0, layer as f32), step), A::add_exact(layer as f32, 1.0)), 0.5),
            );
            if 1.0 < scroll {
                scroll = A::sub_exact(scroll, 1.0);
            }
            self.offsets[layer as usize] = scroll;
            let drop = A::mul(layer as f32, 5.0);

            let mut pass = Pass { name: format!("fog {layer}"), target: TargetName::Display, topology: PassTopology::Triangles, ..Default::default() };
            pass.material.source = SourceKind::Texture;
            pass.material.texture = TEXTURES[layer as usize];
            pass.material.coordinates = CoordinateKind::Projective;
            pass.material.sampling = Sampling::Repeat;
            pass.material.bilinear = true;
            pass.material.blend = BlendOp::FixedAdd;
            pass.material.blend_constant = 20;
            pass.material.depth_test = DepthTest::GreaterEqual;
            pass.material.depth_write = false;
            pass.material.gouraud = true;
            pass.half_line = true;

            for row in 0..16i32 {
                for column in 0..16i32 {
                    let index = |k: i32| ((column + (k & 1)) as usize) * points + (row + (k >> 1)) as usize;
                    let mut corners = [[0.0f32; 4]; 4];
                    for (k, c) in (0i32..).zip(corners.iter_mut()) {
                        let v = &self.mesh[index(k)];
                        *c = [v[0], v[1], A::sub_exact(v[2], drop), v[3]];
                    }
                    if V::<A>::clip_all(&CLIP_MIN, &CLIP_MAX, world_to_screen, &corners) {
                        self.stats.left_out += 1;
                        continue;
                    }
                    self.stats.drawn += 1;
                    let mut quad = [Vertex::default(); 4];
                    for (k, v) in (0i32..).zip(quad.iter_mut()) {
                        let p = V::<A>::apply(world_to_screen, &corners[k as usize]);
                        let q = A::quotient_exact(1.0, p[3]);
                        let sx = A::to_int(A::mul(A::mul(p[0], q), 16.0));
                        let sy = A::to_int(A::mul(A::mul(p[1], q), 16.0));
                        let sz = A::to_int(A::mul(A::mul(p[2], q), 16.0)) >> 4;
                        let across = A::mul(A::add_exact(A::mul((k >> 1) as f32, 0.5), A::mul((row & 1) as f32, 0.5)), q);
                        let along = A::mul(A::sub_exact(A::add_exact(A::mul((k & 1) as f32, 0.5), A::mul((column & 1) as f32, 0.5)), scroll), q);
                        let level = self.brightness[index(k)][2];
                        v.x = sx.wrapping_sub(CORNER_X) as f32 * 0.0625;
                        v.y = sy.wrapping_sub(CORNER_Y) as f32 * 0.0625;
                        v.z = (sz as u32) & 0xffffff;
                        v.u = along;
                        v.v = across;
                        v.q = q;
                        v.r = (if level < 0 { (level + 3) >> 2 } else { level >> 2 }) as u8;
                        v.g = (level * 2 / 5) as u8;
                        v.b = level as u8;
                        v.a = 0x80;
                    }
                    pass.vertices.extend_from_slice(&[quad[0], quad[1], quad[2], quad[1], quad[2], quad[3]]);
                }
            }
            if !pass.vertices.is_empty() {
                out.push(pass);
            }
        }
    }
}
