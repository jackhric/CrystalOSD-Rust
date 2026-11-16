use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, as_float};
use crate::frame::{BlendOp, CoordinateKind, DepthTest, Material, Pass, PassTopology, Sampling, SourceKind, TargetName, Vertex};
use crate::matrix::{Mat4, Vec4};
use crate::opening::timeline::Matrices;
use crate::opening::vu0::Vu0;
use crate::program_image::ProgramImage;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CubeCorner {
    pub world: Vec4,
    pub eye: Vec4,
    pub screen: Vec4,
    pub q: f32,
    pub ints: [i32; 4],
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CubeFace {
    pub corners: [i32; 4],
    pub object: Vec4,
    pub world: Vec4,
    pub view: Vec4,
    pub facing: f32,
    pub colour: Vec4,
    pub edge: [f32; 4],
}

// facts/opening.md 4.1: the work record func_00224EB8 and func_00224BC0 leave for a cube that is drawn (verify_opening_cubes_v2.mjs transform).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CubeWork {
    pub index: i32,
    pub angles: Vec4,
    pub turned: Mat4,
    pub to_world: Mat4,
    pub to_screen: Mat4,
    pub centre: Vec4,
    pub corners: [CubeCorner; 8],
    pub faces: [CubeFace; 6],
}

// One row of the table of ten passes (facts/opening.md 4.1): `mode` is the row of the OSD's ALPHA table (4: (Cs - Cd) x As + Cd,
// 5: Cs x As + Cd, 8: Cs x Ad + Cd), `fix` its fixed alpha; texture 0 stands for the frame buffer (pass 0) or the extra buffer (pass 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CubePassSetup {
    pub away: bool,
    pub texture: i32,
    pub mode: i32,
    pub fix: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Make {
    Refract,
    Fixed,
    Mirror,
}

#[derive(Debug, Clone, Copy)]
struct Row {
    edge: bool,
    screen: bool,
    colour: i32,
    make: Make,
    value: i32,
}

// A generator's argument: an index into the constants D_0036FA1C..48 or one of the literals.
const PULL: i32 = 9;
const FIXED_A: i32 = 7;
const FIXED_B: i32 = 8;
const FIXED_C: i32 = 10;
const FIXED_D: i32 = 11;
const ZERO: i32 = -1;
const MIRROR_FAR: i32 = -2;
const MIRROR_NEAR: i32 = -3;

const fn setup(away: bool, texture: i32, mode: i32, fix: u8) -> CubePassSetup {
    CubePassSetup { away, texture, mode, fix }
}

const fn row(edge: bool, screen: bool, colour: i32, make: Make, value: i32) -> Row {
    Row { edge, screen, colour, make, value }
}

// facts/opening.md 4.1, the table of ten passes (descriptors built at 0x00220450..0x0022076C, verify_opening_cubes_v2.mjs PASSES).
const SETUPS: [CubePassSetup; 10] = [
    setup(true, 0, 4, 0x7a),
    setup(true, 12, 5, 0x80),
    setup(true, 10, 8, 0x2a),
    setup(true, 11, 5, 0x80),
    setup(true, 10, 8, 0x2a),
    setup(false, 0, 4, 0xf0),
    setup(false, 12, 5, 0x80),
    setup(false, 10, 8, 0x40),
    setup(false, 11, 5, 0x80),
    setup(false, 10, 8, 0x40),
];
const ROWS: [Row; 10] = [
    row(true, true, 2, Make::Refract, ZERO),
    row(false, false, 0, Make::Fixed, FIXED_A),
    row(false, false, 3, Make::Mirror, MIRROR_FAR),
    row(false, false, 0, Make::Fixed, FIXED_B),
    row(false, false, 3, Make::Mirror, MIRROR_FAR),
    row(true, true, 2, Make::Refract, PULL),
    row(false, false, 0, Make::Fixed, FIXED_C),
    row(false, false, 3, Make::Mirror, MIRROR_NEAR),
    row(false, false, 0, Make::Fixed, FIXED_D),
    row(false, false, 3, Make::Mirror, MIRROR_NEAR),
];

// D_0036FA1C..48 (the graded colour's scale, slopes and bases, then the shifts and the pull) and D_0036FB80 (the facing limit).
const CONSTANT_BITS: [u32; 12] =
    [0x3ecccccd, 0x3f19999a, 0x3e4ccccd, 0x3f19999a, 0x3e4ccccd, 0x3f19999a, 0x3e4ccccd, 0xbb75c28f, 0x3b75c28f, 0xbdac0831, 0xbbf5c28f, 0x3bf5c28f];
const FACING_LIMIT_BITS: u32 = 0x3ba3d70a;

// D_002B1D10 faces, D_002B1DC0 mirror directions, the fixed pattern's corners (func_00224B10).
const FACES: [[i32; 4]; 6] = [[0, 1, 2, 3], [5, 4, 7, 6], [4, 0, 6, 2], [2, 3, 6, 7], [1, 5, 3, 7], [4, 5, 0, 1]];
const MIRROR_BITS: [[u32; 4]; 6] = [
    [0x0, 0x0, 0xbf800000, 0x3f800000],
    [0x0, 0x0, 0x3f800000, 0x3f800000],
    [0xbf800000, 0x0, 0x0, 0x3f800000],
    [0x0, 0x3f800000, 0x0, 0x3f800000],
    [0x3f800000, 0x0, 0x0, 0x3f800000],
    [0x0, 0xbf800000, 0x0, 0x3f800000],
];
const FIXED_PAIRS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

const PI: f32 = std::f32::consts::PI;
const TWO_PI: f32 = std::f32::consts::TAU;
const WIDTH: i32 = 640;
const HEIGHT: i32 = 224;
const OFFSET_X: i32 = 1728 * 16;
const OFFSET_Y: i32 = 1936 * 16;

struct Lib<A>(PhantomData<A>);

impl<A: Arithmetic> Lib<A> {
    fn normalize(v: &Vec4) -> Vec4 {
        let length = A::root_exact(A::add_exact(A::add_exact(A::mul(v[0], v[0]), A::mul(v[1], v[1])), A::mul(v[2], v[2])));
        let q = A::quotient_exact(1.0, length);
        [A::mul(v[0], q), A::mul(v[1], q), A::mul(v[2], q), 0.0]
    }
    fn outer(a: &Vec4, b: &Vec4) -> Vec4 {
        [
            A::sub_exact(A::mul(a[1], b[2]), A::mul(b[1], a[2])),
            A::sub_exact(A::mul(a[2], b[0]), A::mul(b[2], a[0])),
            A::sub_exact(A::mul(a[0], b[1]), A::mul(b[0], a[1])),
            0.0,
        ]
    }
    fn inner(a: &Vec4, b: &Vec4) -> f32 {
        A::add_exact(A::add_exact(A::mul(a[0], b[0]), A::mul(a[1], b[1])), A::mul(a[2], b[2]))
    }
    fn absolute(d: f32) -> f32 {
        if d < 0.0 { -d } else { d }
    }
}

// A run of faces under one GS state is one pass: the next face joins the last pass when its state is the same.
fn append(out: &mut Vec<Pass>, header: &Pass, vertices: &[Vertex]) {
    let joins = out.last().is_some_and(|last| {
        last.name == header.name && last.target == header.target && last.material == header.material && last.edge_smoothing == header.edge_smoothing
    });
    if !joins {
        out.push(header.clone());
    }
    out.last_mut().unwrap().vertices.extend_from_slice(vertices);
}

pub fn cube_pass_setups() -> &'static [CubePassSetup; 10] {
    &SETUPS
}

#[derive(Debug, Clone, Copy, Default)]
struct State {
    place: Vec4,
    angles: Vec4,
    rates: Vec4,
}

// facts/opening.md 4.1, InitLightsCubes 0x002209E0: five cubes, the same every run.
#[derive(Debug, Clone)]
pub struct Cubes<A> {
    cubes: [State; 5],
    arithmetic: PhantomData<A>,
}

const fn vertex(x: f32, y: f32, z: u32, u: f32, v: f32, q: f32, r: u8, g: u8, b: u8, a: u8) -> Vertex {
    Vertex { x, y, z, u, v, q, r, g, b, a }
}

impl<A: Arithmetic> Cubes<A> {
    // facts/opening.md 4.1, InitLightsCubes 0x002209E0: the table D_002B0F40 and the constants D_0036FA50..68 are read from the program.
    pub fn new(program: &ProgramImage) -> Self {
        let scale = program.single(0x0036fa50);
        let spread = program.single(0x0036fa54);
        let base = program.single(0x0036fa58);
        let speed = program.single(0x0036fa5c);
        let tilt = program.single(0x0036fa60);
        let drift = program.single(0x0036fa64);
        let fallback = program.single(0x0036fa68);
        let mut cubes = [State::default(); 5];
        for (n, s) in (0i32..).zip(cubes.iter_mut()) {
            let at = 0x002b0f40u32 + (n as u32) * 16;
            s.place = [A::mul(3.5, program.single(at)), A::mul(3.5, program.single(at + 4)), A::sub_exact(150.0, A::mul(15.0, program.single(at + 8))), 0.0];
            let mut u = A::mul((n - 2) as f32, scale);
            if u == 0.0 {
                u = fallback;
            }
            let angle = A::add_exact(A::mul(A::mul(u, (n % 3) as f32), spread), base);
            s.angles = [angle, angle, angle, angle];
            s.rates = [A::quotient_exact(speed, u), A::mul(u, tilt), A::add_exact(A::quotient_exact(u, 1000.0), drift), 0.0];
        }
        Self { cubes, arithmetic: PhantomData }
    }

    // Per cube: turn, matrices, clip test, work record, then up to ten passes in the table's order (away faces into Extra, toward faces
    // into Display), the faces of a pass split wherever the antialiasing of a refracting face changes. field: the frame counter's parity.
    // facts/opening.md 4.1 (verify_opening_cubes_v2.mjs: turn, transform, pass).
    pub fn draw(&mut self, matrices: &Matrices, camera: &Vec4, out: &mut Vec<Pass>, mut probe: Option<&mut Vec<CubeWork>>, field: i32) {
        type L<A> = Lib<A>;
        type V<A> = Vu0<A>;
        const CLIP_MIN: Vec4 = [1728.0, 1936.0, 0.0, 5.0];
        const CLIP_MAX: Vec4 = [2368.0, 2160.0, 0.0, 16777215.0];
        let colour: Vec4 = [112.0, 112.0, 152.0, 128.0];
        let mut vertices = [[0.0f32; 4]; 8];
        for (i, v) in vertices.iter_mut().enumerate() {
            *v = [if i & 1 != 0 { 1.8 } else { -1.8 }, if i & 2 != 0 { 1.8 } else { -1.8 }, if i & 4 != 0 { 1.8 } else { -1.8 }, 1.0];
        }
        let mut constants = [0.0f32; 12];
        for (c, bits) in constants.iter_mut().zip(CONSTANT_BITS) {
            *c = as_float(bits);
        }
        let facing_limit = as_float(FACING_LIMIT_BITS);

        for n in 0..5i32 {
            let state = &mut self.cubes[n as usize];
            for i in 0..3 {
                let mut a = A::add_exact(state.angles[i], state.rates[i]);
                if PI < a {
                    a = A::sub_exact(a, TWO_PI);
                }
                if a < -PI {
                    a = A::add_exact(a, TWO_PI);
                }
                state.angles[i] = a;
            }
            let mut work = CubeWork { index: n, angles: state.angles, ..Default::default() };
            work.angles[3] = 0.0;
            work.turned = V::<A>::rot_matrix(&V::<A>::unit_matrix(), &state.angles);
            work.to_world = V::<A>::trans_matrix(&work.turned, &state.place);
            work.to_screen = V::<A>::mul_matrix(&matrices.world_to_screen, &work.to_world);
            if V::<A>::clip_all(&CLIP_MIN, &CLIP_MAX, &work.to_screen, &vertices) {
                continue;
            }

            let origin = V::<A>::apply(&work.to_screen, &[0.0, 0.0, 0.0, 1.0]);
            let origin_q = A::quotient_exact(1.0, origin[3]);
            work.centre = [A::mul(origin[0], origin_q), A::mul(origin[1], origin_q), A::mul(origin[2], origin_q), 1.0];

            for (c, vertex) in work.corners.iter_mut().zip(&vertices) {
                c.world = V::<A>::apply(&work.to_world, vertex);
                c.eye = L::<A>::normalize(&V::<A>::apply(&matrices.camera, &c.world));
                let p = V::<A>::apply(&work.to_screen, vertex);
                c.q = A::quotient_exact(1.0, p[3]);
                for k in 0..4 {
                    c.screen[k] = A::mul(p[k], c.q);
                }
                c.ints = [
                    A::to_int(A::mul(c.screen[0], 16.0)),
                    A::to_int(A::mul(c.screen[1], 16.0)),
                    A::to_int(A::mul(c.screen[2], 16.0)) >> 4,
                    A::to_int(A::mul(c.screen[3], 16.0)),
                ];
            }
            for f in 0..6 {
                let fc = FACES[f];
                let object = L::<A>::outer(
                    &V::<A>::sub_vector(&vertices[fc[1] as usize], &vertices[fc[0] as usize]),
                    &V::<A>::sub_vector(&vertices[fc[2] as usize], &vertices[fc[0] as usize]),
                );
                let world = V::<A>::apply(&work.to_world, &object);
                let view = V::<A>::apply(&matrices.camera, &world);
                let corners = &work.corners;
                let flat = |a: i32, b: i32| {
                    let (a, b) = (&corners[a as usize], &corners[b as usize]);
                    L::<A>::normalize(&[A::sub_exact(a.screen[0], b.screen[0]), A::sub_exact(a.screen[1], b.screen[1]), 0.0, 0.0])
                };
                let a = flat(fc[0], fc[1]);
                let b = flat(fc[0], fc[2]);
                let mut face = CubeFace { corners: fc, ..Default::default() };
                face.object = L::<A>::normalize(&object);
                face.world = L::<A>::normalize(&world);
                face.view = L::<A>::normalize(&view);
                face.facing = A::sub_exact(A::mul(a[0], b[1]), A::mul(a[1], b[0]));
                for k in 0..4 {
                    face.colour[k] = if colour[k] < 0.0 {
                        0.0
                    } else if colour[k] > 127.0 {
                        127.0
                    } else {
                        colour[k]
                    };
                }
                for k in 0..4 {
                    let d = L::<A>::inner(&face.world, &L::<A>::normalize(&V::<A>::sub_vector(&corners[fc[k] as usize].world, camera)));
                    face.edge[k] = L::<A>::absolute(d);
                }
                work.faces[f] = face;
            }

            // module_opening_225728: the clear of the extra buffer (outside the scissor) and the copy of the frame into it, before the passes.
            let mut clear =
                Pass { name: format!("cube{n} clear"), target: TargetName::Extra, topology: PassTopology::Sprites, half_line: true, ..Default::default() };
            clear.material.depth_write = false;
            clear.vertices = vec![vertex(-1728.0, -1936.0, 0, 0.0, 0.0, 1.0, 0, 0, 0, 0), vertex(-1088.0, -1712.0, 0, 0.0, 0.0, 1.0, 0, 0, 0, 0)];
            out.push(clear);
            let mut copy =
                Pass { name: format!("cube{n} copy"), target: TargetName::Extra, topology: PassTopology::Sprites, half_line: true, ..Default::default() };
            copy.material.source = SourceKind::Target;
            copy.material.source_target = TargetName::Display;
            copy.material.colour_only = true;
            copy.material.coordinates = CoordinateKind::Texel;
            copy.material.sampling = Sampling::Repeat;
            copy.material.bilinear = true;
            copy.material.per_pixel_alpha = true;
            copy.material.depth_write = false;
            copy.vertices = vec![vertex(0.0, 0.0, 0, 0.5, 0.5, 1.0, 128, 128, 128, 0), vertex(639.5, 223.5, 0, 640.0, 224.0, 1.0, 128, 128, 128, 0)];
            out.push(copy);

            // The ten passes (verify_opening_cubes_v2.mjs pass, func_002254E0 with the callback at 0x0021FEB8).
            for p in 0..10i32 {
                let setup = &SETUPS[p as usize];
                let row = &ROWS[p as usize];
                let fix = f32::from(setup.fix);
                for f in 0..6 {
                    let face = &work.faces[f];
                    if (face.facing < 0.0) != setup.away {
                        continue;
                    }

                    let mut pairs = [[0.0f32; 2]; 4];
                    if row.make == Make::Refract {
                        let pull = if row.value == ZERO { 0.0 } else { constants[row.value as usize] };
                        for (i, pair) in pairs.iter_mut().enumerate() {
                            let c = &work.corners[face.corners[i] as usize];
                            let u = A::add_exact(
                                A::add_exact(A::sub_exact(c.screen[0], 2048.0), (WIDTH / 2) as f32),
                                A::mul(A::mul(A::mul(A::mul(face.view[0], 320.0), 1.0), c.q), 4.0),
                            );
                            let v = A::add_exact(
                                A::add_exact(A::sub_exact(c.screen[1], 2048.0), (HEIGHT / 2) as f32),
                                A::mul(A::mul(A::mul(A::mul(face.view[1], 112.0), 1.0), c.q), 4.0),
                            );
                            let mut x = A::add_exact(u, A::mul(A::sub_exact(c.screen[0], work.centre[0]), pull));
                            let mut y = A::add_exact(v, A::mul(A::sub_exact(c.screen[1], work.centre[1]), pull));
                            let w1 = A::sub_exact(WIDTH as f32, 1.0);
                            let h1 = A::sub_exact(HEIGHT as f32, 1.0);
                            if w1 < x {
                                x = w1;
                            } else if x < 1.0 {
                                x = 1.0;
                            }
                            if h1 < y {
                                y = h1;
                            } else if y < 1.0 {
                                y = 1.0;
                            }
                            *pair = [x, y];
                        }
                    } else if row.make == Make::Mirror {
                        let value = if row.value == MIRROR_FAR { -0.25 } else { 0.5 };
                        let mirror: Vec4 = [as_float(MIRROR_BITS[f][0]), as_float(MIRROR_BITS[f][1]), as_float(MIRROR_BITS[f][2]), as_float(MIRROR_BITS[f][3])];
                        let d = L::<A>::inner(&face.object, &mirror);
                        let amount = A::mul(-L::<A>::absolute(d), value);
                        let moved: Vec4 = [A::mul(face.view[0], amount), A::mul(face.view[1], amount), A::mul(face.view[2], amount), face.view[3]];
                        for (i, pair) in pairs.iter_mut().enumerate() {
                            let e = L::<A>::normalize(&work.corners[face.corners[i] as usize].eye);
                            *pair = [A::add_exact(A::add_exact(moved[1], e[1]), 0.5), A::add_exact(A::add_exact(moved[0], e[0]), 0.5)];
                        }
                    } else {
                        let shift = constants[row.value as usize];
                        for (pair, fixed) in pairs.iter_mut().zip(&FIXED_PAIRS) {
                            *pair = [A::add_exact(fixed[0], shift), A::sub_exact(fixed[1], shift)];
                        }
                    }

                    let smooth = row.edge && (facing_limit < face.facing || setup.away);
                    let mut corner = [Vertex::default(); 4];
                    for i in 0..4 {
                        let c = &work.corners[face.corners[i] as usize];
                        let lift = A::mul(A::mul(A::sub_exact(1.0, face.edge[i]), A::sub_exact(1.0, face.edge[i])), 0.5);
                        let (q, s, t);
                        if row.screen {
                            let half = if !setup.away && field != 0 { 0.5 } else { 0.0 };
                            q = 1.0;
                            let s0 = A::mul(pairs[i][0], 0.0009765625);
                            let t0 = A::mul(A::sub_exact(pairs[i][1], half), 0.00390625);
                            s = if s0 < 0.0 {
                                0.0
                            } else if 0.625 < s0 {
                                0.625
                            } else {
                                s0
                            };
                            t = if t0 < 0.0 {
                                0.0
                            } else if 0.875 < t0 {
                                0.875
                            } else {
                                t0
                            };
                        } else {
                            q = c.q;
                            s = A::mul(pairs[i][0], q);
                            t = A::mul(pairs[i][1], q);
                        }
                        let mut rgb = [0i32; 3];
                        for (k, channel) in rgb.iter_mut().enumerate() {
                            if row.colour == 0 {
                                *channel = 0x80;
                            } else if row.colour == 2 {
                                *channel = A::to_int(A::mul(A::mul(A::add_exact(face.colour[k], A::mul(lift, 32.0)), fix), 0.0078125));
                            } else {
                                let scaled = A::mul(lift, constants[0]);
                                *channel = A::to_int(A::mul(
                                    A::mul(A::mul(face.colour[k], A::add_exact(A::mul(scaled, constants[1 + k * 2]), constants[2 + k * 2])), fix),
                                    0.0078125,
                                ));
                            }
                            *channel = if *channel > 0x80 {
                                0x80
                            } else if *channel < 0 {
                                0
                            } else {
                                *channel
                            };
                        }
                        let v = &mut corner[i];
                        v.x = c.ints[0].wrapping_sub(OFFSET_X) as f32 / 16.0;
                        v.y = c.ints[1].wrapping_sub(OFFSET_Y) as f32 / 16.0;
                        v.z = (c.ints[2] as u32) & 0xffffff;
                        v.u = s;
                        v.v = t;
                        v.q = q;
                        v.r = rgb[0] as u8;
                        v.g = rgb[1] as u8;
                        v.b = rgb[2] as u8;
                        v.a = 0x80;
                    }

                    let mut header = Pass {
                        name: format!("cube{n} pass{p}"),
                        target: if setup.away { TargetName::Extra } else { TargetName::Display },
                        topology: PassTopology::Triangles,
                        edge_smoothing: smooth,
                        half_line: true,
                        ..Default::default()
                    };
                    let m: &mut Material = &mut header.material;
                    m.coordinates = CoordinateKind::Projective;
                    m.sampling = Sampling::Repeat;
                    m.bilinear = true;
                    m.depth_test = DepthTest::Always;
                    m.depth_write = false;
                    if row.make == Make::Refract {
                        m.source = SourceKind::Target;
                        m.source_target = if setup.away { TargetName::Display } else { TargetName::Extra };
                        m.blend = if smooth { BlendOp::AlphaOver } else { BlendOp::Opaque };
                        m.blend_constant = if smooth { setup.fix } else { 0 };
                    } else {
                        m.source = SourceKind::Texture;
                        m.texture = setup.texture;
                        m.blend = if setup.mode == 5 { BlendOp::Add } else { BlendOp::AddDestinationAlpha };
                        m.blend_constant = setup.fix;
                    }

                    let mut triangles = Vec::with_capacity(6);
                    for i in 0..2 {
                        let mut tri = [corner[i], corner[i + 1], corner[i + 2]];
                        let last = tri[2];
                        for v in &mut tri {
                            v.r = last.r;
                            v.g = last.g;
                            v.b = last.b;
                            v.a = last.a;
                        }
                        triangles.extend_from_slice(&tri);
                    }
                    append(out, &header, &triangles);
                }
            }
            if let Some(probe) = probe.as_deref_mut() {
                probe.push(work);
            }
        }
    }
}
