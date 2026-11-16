use std::marker::PhantomData;

use crate::arithmetic::Arithmetic;
use crate::frame::{BlendOp, CoordinateKind, DepthTest, Pass, PassTopology, Sampling, SourceKind, TargetName};
use crate::matrix::{Mat4, Vec4};
use crate::opening::tower_vu1::{TOWER_CHAIN_ADDRESS, TowerChain, TowerVu1};
use crate::opening::types::{History, HistoryEntry};
use crate::opening::vu0::Vu0;
use crate::program_image::ProgramImage;

pub const TOWER_COLUMNS: usize = 14;
pub const TOWER_ROWS: usize = 9;

// The scratch block's matrices (0x70000060: +0x00 the base, +0xC0 world-to-screen, +0x200 the light matrix).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TowerMatrices {
    pub base: Mat4,
    pub world_to_screen: Mat4,
    pub light: Mat4,
}

// facts/opening.md 5.2: the tables OpeningInitTowersFog leaves, [column][row].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TowerTables {
    pub flag: [[i32; TOWER_ROWS]; TOWER_COLUMNS],
    pub fade: [[i32; TOWER_ROWS]; TOWER_COLUMNS],
    pub sway: [[f32; TOWER_ROWS]; TOWER_COLUMNS],
    pub tall: [[f32; TOWER_ROWS]; TOWER_COLUMNS],
    pub height: [[f32; TOWER_ROWS]; TOWER_COLUMNS],
    pub place: [[Vec4; TOWER_ROWS]; TOWER_COLUMNS],
}

const CONSTANTS: u32 = 0x0036FA74;
const STATICS: u32 = 0x002B1BE0;
const FACES: u32 = 0x002B1380;
const SIGNS: u32 = 0x002B1B80;
const CURVES: u32 = 0x002B1C10;
const CELLS: u32 = 0x002B0F90;
const SOURCE: u32 = 0x002B13A0;
const EMPTY_NAME: u32 = 0x003700D0;
const LENGTH: usize = 0x840;
const TOWER_TEXTURE: i32 = 6;

fn put_bytes(chain: &mut TowerChain, address: u32, bytes: &[u8]) {
    let at = address.wrapping_sub(TOWER_CHAIN_ADDRESS) as usize;
    chain.bytes[at..at + bytes.len()].copy_from_slice(bytes);
}
fn put_word(chain: &mut TowerChain, address: u32, value: u32) {
    put_bytes(chain, address, &value.to_le_bytes());
}
fn put_qword(chain: &mut TowerChain, address: u32, value: u64) {
    put_bytes(chain, address, &value.to_le_bytes());
}
fn put_float(chain: &mut TowerChain, address: u32, value: f32) {
    put_bytes(chain, address, &value.to_bits().to_le_bytes());
}
fn put_matrix(chain: &mut TowerChain, address: u32, m: &Mat4) {
    for (r, row) in (0u32..).zip(m) {
        for (c, x) in (0u32..).zip(row) {
            put_float(chain, address + (r * 4 + c) * 4, *x);
        }
    }
}

fn name_of(entry: &HistoryEntry) -> &[u8] {
    let mut n = 0;
    while n < entry.name.len() && entry.name[n] != 0 {
        n += 1;
    }
    &entry.name[..n]
}

// facts/opening.md 5.2 verify_opening_towers_ee.mjs setup, brightness and tower. The program supplies the static
// tables and the chain's template (HDD OSD 1.10U).
#[derive(Debug, Clone)]
pub struct Towers<A> {
    tables: TowerTables,
    brightness: [[f32; 20]; 20],
    template: TowerChain,
    faces: [u32; 6],
    signs: [i32; 24],
    origin: Vec4,
    rotation: Vec4,
    degree: f32,
    quarter: f32,
    side: f32,
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> Towers<A> {
    // OpeningInitTowersFog 0x00221D30: the cells from the play history, the heights and places, the fade counts;
    // func_00220D60: the 20 x 20 brightness table.
    pub fn new(history: &History, program: &ProgramImage) -> Self {
        let mut template = TowerChain::default();
        for at in (0..LENGTH).step_by(4) {
            let w = program.word(TOWER_CHAIN_ADDRESS + at as u32);
            template.bytes[at..at + 4].copy_from_slice(&w.to_le_bytes());
        }
        let mut faces = [0u32; 6];
        for (i, face) in (0u32..).zip(faces.iter_mut()) {
            *face = program.word(FACES + i * 4);
        }
        let mut signs = [0i32; 24];
        for (i, sign) in (0u32..).zip(signs.iter_mut()) {
            *sign = program.integer(SIGNS + i * 4);
        }
        let mut origin = [0.0f32; 4];
        let mut rotation = [0.0f32; 4];
        for i in 0..4u32 {
            origin[i as usize] = program.single(STATICS + 0x10 + i * 4);
            rotation[i as usize] = program.single(STATICS + 0x20 + i * 4);
        }
        let k = |i: u32| program.single(CONSTANTS + i * 4);
        let side = k(6);
        let degree = k(7);
        let quarter = k(8);
        let shift = k(10);
        let empty = program.string(EMPTY_NAME);

        // facts/opening.md 5.2 verify_opening_towers_ee.mjs setup (OpeningInitTowersFog 0x00221D30).
        let mut sways = [0.0f32; 14];
        let mut heights = [0.0f32; 14];
        for i in 0..14u32 {
            sways[i as usize] = program.single(CURVES + i * 4);
            heights[i as usize] = program.single(CURVES + 0x38 + i * 4);
        }
        let mut t = TowerTables::default();
        for (e, entry) in (0u32..).zip(history) {
            if name_of(entry) == empty.as_slice() {
                continue;
            }
            for cell in 0..6u32 {
                let column = program.integer(CELLS + e * 0x30 + cell * 8);
                let row = program.integer(CELLS + e * 0x30 + cell * 8 + 4);
                if column < 0 || column >= TOWER_COLUMNS as i32 || row < 0 || row >= TOWER_ROWS as i32 {
                    panic!("towers: a cell outside the grid");
                }
                let (c, r) = (column as usize, row as usize);
                if cell == u32::from(entry.main_cell) {
                    let count = i32::from(entry.count);
                    let index = if count < 14 { count } else { ((count - 14) % 10) + 4 } as usize;
                    t.sway[c][r] = sways[index];
                    t.tall[c][r] = heights[index];
                    t.flag[c][r] = 1;
                } else if ((entry.mask >> cell) & 1) != 0 {
                    t.flag[c][r] = 1;
                    t.tall[c][r] = 1.0;
                    t.sway[c][r] = 1.0;
                }
            }
        }
        for c in 0..TOWER_COLUMNS {
            for r in 0..TOWER_ROWS {
                let at = SOURCE + c as u32 * 0x90 + r as u32 * 0x10;
                let s: Vec4 = [program.single(at), program.single(at + 4), program.single(at + 8), program.single(at + 12)];
                let mut p: Vec4 = [
                    A::cut(f64::from(A::cut(f64::from(s[0]) + f64::from(shift))) * 4.0),
                    A::cut(f64::from(A::cut(f64::from(s[1]) - 6.5)) * 4.0),
                    A::cut(f64::from(A::cut(f64::from(A::cut(f64::from(s[2]) + 4.0)) * 12.0)) + 150.0),
                    s[3],
                ];
                let mut h = A::cut(f64::from(t.tall[c][r]) * 30.0);
                if h < 3.0 {
                    h = 3.0;
                }
                let factor = t.sway[c][r];
                p[2] = A::cut(f64::from(p[2]) + f64::from(A::cut(f64::from(A::cut(f64::from(factor) * 30.0)) - f64::from(h))));
                if 1.0 <= factor {
                    t.sway[c][r] = 1.0;
                    t.fade[c][r] = 0;
                } else {
                    t.fade[c][r] = A::to_int(A::cut(f64::from(A::cut(1.0 - f64::from(factor))) * 128.0));
                }
                t.place[c][r] = p;
                t.height[c][r] = h;
            }
        }

        // facts/opening.md 5.2 verify_opening_towers_ee.mjs brightness (func_00220D60).
        let radius2 = k(0);
        let step = k(1);
        let base = k(2);
        let a = k(3);
        let b = k(4);
        let scale = k(5);
        let f = |x: f64| A::cut(x);
        let d64 = f64::from;
        let big_r = A::root_exact(radius2);
        let mut brightness = [[0.0f32; 20]; 20];
        for i in 0..20i32 {
            let s6 = 2 * i - 20;
            let row = f(d64(f(f64::from(s6) * d64(step))) * 0.5);
            for j in 0..20i32 {
                let s4 = 2 * j - 20;
                let y = f(d64(row) + d64(base));
                let dy = f(0.0 - d64(y));
                let x = f(d64(f(d64(f(f64::from(s4) * d64(step))) * 0.5)) + d64(base));
                let dx = f(d64(a) - d64(x));
                let mut d = A::root_exact(f(d64(f(d64(dx) * d64(dx))) + d64(f(d64(dy) * d64(dy)))));
                d = f(d64(d) + d64(d));
                let mut first = A::quotient_exact(f(d64(f(d64(big_r) - d64(d))) * 255.0), big_r);
                first = if first < 32.0 {
                    32.0
                } else if 255.0 < first {
                    255.0
                } else {
                    first
                };
                let y2 = f(d64(f(d64(f(f64::from(s6) * d64(step))) * 0.5)) + d64(base));
                let x2 = f(d64(f(d64(f(f64::from(s4) * d64(step))) * 0.5)) + d64(base));
                let ey = f(d64(step) - d64(y2));
                let ex = f(d64(b) - d64(x2));
                let d2 = f(d64(A::root_exact(f(d64(f(d64(ex) * d64(ex))) + d64(f(d64(ey) * d64(ey)))))) * 4.0);
                let second = f(d64(A::quotient_exact(f(d64(f(d64(big_r) - d64(d2))) * 255.0), big_r)) * 0.5);
                let mut total = if second < 32.0 {
                    f(d64(first) + 32.0)
                } else if 255.0 < second {
                    f(d64(first) + 255.0)
                } else {
                    f(d64(first) + d64(second))
                };
                let wobble = (((j + i) * j / (i + 1)) % 11 - 5) * 10;
                total = f(d64(f(d64(total) * d64(scale))) - f64::from(wobble));
                brightness[j as usize][i as usize] = if total < 32.0 {
                    32.0
                } else if 220.0 < total {
                    220.0
                } else {
                    total
                };
            }
        }
        Self { tables: t, brightness, template, faces, signs, origin, rotation, degree, quarter, side, arithmetic: PhantomData }
    }

    pub fn tables(&self) -> &TowerTables {
        &self.tables
    }

    pub fn brightness(&self) -> &[[f32; 20]; 20] {
        &self.brightness
    }

    // func_002214F8's sine: sinf(((counter % 360) - 180) x degree).
    pub fn swing(&self, counter: i32) -> f32 {
        let angle = A::cut(f64::from((counter % 360) - 180) * f64::from(self.degree));
        A::sinf(angle)
    }

    // The chain of one cell as func_002214F8 leaves it.
    // facts/opening.md 5.2 verify_opening_towers_ee.mjs tower (func_002214F8 and the functions it calls).
    pub fn chain(&self, column: i32, row: i32, sine: f32, matrices: &TowerMatrices) -> TowerChain {
        let f = |x: f64| A::cut(x);
        let d64 = f64::from;
        let (cu, ru) = (column as usize, row as usize);
        let s0 = column + 3;
        let s2 = row + 6;
        let s5 = row + 7;
        let s1 = s0 + s2;
        let turn = (s1 * s0 / s5) % 4;
        let swing_angle = f(d64(f(d64(sine) * 10.0)) * d64(self.degree));
        let turned = f(f64::from(turn) * d64(self.quarter));
        let twist = if self.tables.sway[cu][ru] != 1.0 { f(d64(swing_angle) + d64(turned)) } else { turned };
        let rot: Vec4 = [f(d64(self.rotation[0]) + 0.0), f(d64(self.rotation[1]) + 0.0), f(d64(self.rotation[2]) + d64(twist)), f(d64(self.rotation[3]) + 0.0)];
        let rotated = Vu0::<A>::rot_matrix(&matrices.base, &rot);
        let place = &self.tables.place[cu][ru];
        let trans: Vec4 = [
            f(d64(self.origin[0]) + d64(place[0])),
            f(d64(self.origin[1]) + d64(place[1])),
            f(d64(self.origin[2]) + d64(place[2])),
            f(d64(self.origin[3]) + 0.0),
        ];
        let local = Vu0::<A>::trans_matrix(&rotated, &trans);
        let normals = Vu0::<A>::mul_matrix(&matrices.light, &rotated);

        let mut chain = self.template.clone();
        put_qword(&mut chain, 0x2a5010, 0x1000000000008002);
        put_qword(&mut chain, 0x2a5018, 0xe);
        put_qword(&mut chain, 0x2a5020, 0x8000000044);
        put_qword(&mut chain, 0x2a5028, 0x42);
        put_qword(&mut chain, 0x2a5030, 0);
        put_qword(&mut chain, 0x2a5038, 0x49);
        put_qword(&mut chain, 0x2a5040, 0);
        put_qword(&mut chain, 0x2a5048, 0);
        put_word(&mut chain, TOWER_CHAIN_ADDRESS + 4, TOWER_CHAIN_ADDRESS + 0x180);
        for &pointer in &self.faces {
            put_word(&mut chain, pointer, 0x8004);
            put_word(&mut chain, pointer + 4, 0x304E4000);
            put_word(&mut chain, pointer + 8, 0x412);
            put_word(&mut chain, pointer + 12, 0);
        }
        let count = self.tables.fade[cu][ru];
        let h = self.tables.height[cu][ru];
        let mut light = self.brightness[s0 as usize][s2 as usize];
        light = if count == 0 { f(d64(light) * d64(A::quotient_exact(h, 30.0))) } else { f(d64(light) * d64(f(f64::from(count) * 0.0078125))) };
        let side = f(d64(light) * d64(self.side));
        for face in 0..6usize {
            let base = self.faces[face];
            for v in 0..4u32 {
                let z = if self.signs[face * 4 + v as usize] != 0 { h } else { f(0.0 - d64(h)) };
                put_float(&mut chain, base + 0x10 + v * 16 + 8, z);
                let colour = base + 0x90 + v * 16;
                if 0.0 < z {
                    put_word(&mut chain, colour, 0);
                    put_word(&mut chain, colour + 4, 0);
                    put_word(&mut chain, colour + 8, 0);
                } else {
                    for o in [0u32, 4, 8] {
                        put_float(&mut chain, colour + o, if face == 0 { light } else { side });
                    }
                }
                put_float(&mut chain, colour + 12, 128.0);
            }
        }
        let picked = (s1 * (s0 + 5)) / (s2 + 1) + (s1 * (s0 + 4)) / (s2 + 3);
        let v = if picked != 0 { f(f64::from(picked) * 0.00390625) } else { 0.0 };
        let low = f(d64(v) + 0.0);
        let high = f(d64(v) + 1.0);
        let corners = [[low, low], [high, low], [low, high], [high, high]];
        for &pointer in &self.faces {
            for (n, corner) in (0u32..).zip(&corners) {
                let at = pointer + 0xd0 + n * 16;
                put_float(&mut chain, at, corner[0]);
                put_float(&mut chain, at + 4, corner[1]);
                put_float(&mut chain, at + 8, 1.0);
                put_word(&mut chain, at + 12, 0);
            }
        }
        put_matrix(&mut chain, 0x2a4ef0, &matrices.world_to_screen);
        put_matrix(&mut chain, 0x2a4f30, &local);
        put_matrix(&mut chain, 0x2a4fb0, &normals);
        chain
    }

    // func_002214F8: one pass over the grid, column by column, row by row, no sorting. The towers' triangles are
    // appended to `out` as one pass; `probe` receives each chain as it is started.
    pub fn draw(&self, counter: i32, matrices: &TowerMatrices, camera: &Vec4, out: &mut Vec<Pass>, mut probe: Option<&mut Vec<TowerChain>>) {
        let sine = self.swing(counter);
        let mut pass = Pass {
            name: "towers".into(),
            target: TargetName::Display,
            topology: PassTopology::Triangles,
            edge_smoothing: true,
            half_line: true,
            ..Default::default()
        };
        let m = &mut pass.material;
        m.source = SourceKind::Texture;
        m.texture = TOWER_TEXTURE;
        m.coordinates = CoordinateKind::Projective;
        m.sampling = Sampling::Repeat;
        m.bilinear = true;
        m.blend = BlendOp::AlphaOver;
        m.blend_constant = 128;
        m.depth_test = DepthTest::GreaterEqual;
        m.depth_write = true;
        m.gouraud = true;
        for c in 0..TOWER_COLUMNS {
            for r in 0..TOWER_ROWS {
                let d = Vu0::<A>::sub_vector(&self.tables.place[c][r], camera);
                let key = if d[1] < 0.0 { A::cut(f64::from(d[0]).abs() - f64::from(d[1])) } else { A::cut(f64::from(d[0]).abs() + f64::from(d[1])) };
                if !(0.0 < key) || self.tables.flag[c][r] == 0 {
                    continue;
                }
                let built = self.chain(c as i32, r as i32, sine, matrices);
                if let Some(probe) = probe.as_deref_mut() {
                    probe.push(built.clone());
                }
                let v = TowerVu1::<A>::run(&built);
                for face in 0..6 {
                    for i in 2..4 {
                        if !v.kicked[face * 4 + i] {
                            continue;
                        }
                        for k in i - 2..=i {
                            pass.vertices.push(v.vertices[face * 4 + k]);
                        }
                    }
                }
            }
        }
        if !pass.vertices.is_empty() {
            out.push(pass);
        }
    }
}
