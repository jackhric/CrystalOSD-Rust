// The Browser's icon draw on VU1, HDD OSD 1.10U: the one-shape microprogram (0x002A6B10, 119 instructions) as tools/browser/
// verify_browser_icons.mjs runs it, per vertex. VU1 data memory: qwords 0..3 matrix B (0x002A7F38, func_0023A478), 4..7 matrix A
// (the slot block, func_0023A898), 8..11 the light colours and 12..15 the light vectors (0x002A7F88, 0x002A7FD0, func_0023A580),
// 24 the .ico header float (the slot block + 0x90). The pipeline registers vf26, vf29 and vf30 carry from one chunk, and one draw,
// to the next.

use std::marker::PhantomData;

use crate::arithmetic::Arithmetic;
use crate::frame::Vertex;
use crate::matrix::{Mat4, Vec4};
use crate::opening::vu0::Vu0;

pub const ICON_OFFSET_X: f32 = 1728.0;
pub const ICON_OFFSET_Y: f32 = 1936.0;

// module_browser_23B130: vertices per chunk, 3 x floor(floor(0x1F0 / (S + 6)) / 3).
pub fn chunk_vertices(shapes: u32) -> usize {
    3 * ((0x1f0 / (shapes as usize + 6)) / 3)
}

// One vertex as the vertex packet carries it (0x0023B4F0..0x0023B6A0): the position x 2^-10, the normal x 2^-12 with w 1, the
// texture coordinates x 2^-12 as (u, v, 1, 0), the colour bytes as floats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconVertexIn {
    pub position: Vec4,
    pub normal: Vec4,
    pub uv: Vec4,
    pub colour: Vec4,
}

// A one-shape .ico vertex record: position, normal, texture coordinates (halfwords), colour (bytes).
pub type IconRecord = ([i16; 3], [i16; 3], [i16; 2], [u8; 4]);

pub fn vertex_qwords<A: Arithmetic>(records: &[IconRecord]) -> Vec<IconVertexIn> {
    let scaled = |v: i16, scale: f32| A::mul(A::from_int(i32::from(v)), scale);
    let position_scale = 1.0 / 1024.0;
    let normal_scale = 1.0 / 4096.0;
    records
        .iter()
        .map(|(p, n, uv, colour)| IconVertexIn {
            position: [scaled(p[0], position_scale), scaled(p[1], position_scale), scaled(p[2], position_scale), 1.0],
            normal: [scaled(n[0], normal_scale), scaled(n[1], normal_scale), scaled(n[2], normal_scale), 1.0],
            uv: [scaled(uv[0], normal_scale), scaled(uv[1], normal_scale), 1.0, 0.0],
            colour: colour.map(f32::from),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IconPipeline {
    edge: Vec4,
    last: Vec4,
    before: Vec4,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconInputs {
    pub b: Mat4,
    pub a: Mat4,
    pub colours: Mat4,
    pub vectors: Mat4,
    pub bias: f32,
}

// A vertex as the GS receives it, and whether its XYZF2 carries ADC (culled: no drawing kick).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconVertexOut {
    pub vertex: Vertex,
    pub culled: bool,
}

pub struct IconVu1<A>(PhantomData<A>);

impl<A: Arithmetic> IconVu1<A> {
    // Instructions 91..115 (BAL from 3): qwords 16..19, each row of A through B.
    fn screen_matrix(i: &IconInputs) -> Mat4 {
        std::array::from_fn(|k| Vu0::<A>::apply(&i.b, &i.a[k]))
    }

    // Instructions 22..90 for one chunk: per vertex the transform, the lights, Q = 1 / w, ST x Q, the colour clamped to 255 and
    // truncated, XYZ x 16 truncated, and the cull: the z of (s - s'') x (s' - s''), less the bias, negative (MAC flag Sz).
    fn chunk(pipe: &mut IconPipeline, m: &Mat4, i: &IconInputs, vertices: &[IconVertexIn], out: &mut Vec<IconVertexOut>) {
        for v in vertices {
            let clip = Vu0::<A>::apply(m, &v.position);
            let factors = Vu0::<A>::apply(&i.vectors, &v.normal).map(|x| if x > 0.0 { x } else { 0.0 });
            let light = Vu0::<A>::apply(&i.colours, &factors);
            let q = A::quotient_exact(1.0, clip[3]);
            let place: Vec4 = clip.map(|x| A::mul(x, q));
            let st: Vec4 = v.uv.map(|x| A::mul(x, q));
            let colour: [i32; 4] = std::array::from_fn(|k| {
                let lit = A::mul(v.colour[k], light[k]);
                A::to_int(if lit < 255.0 { lit } else { 255.0 })
            });
            let near: Vec4 = std::array::from_fn(|k| A::sub_exact(place[k], pipe.before[k]));
            let cross = A::sub_exact(A::mul(near[0], pipe.edge[1]), A::mul(pipe.edge[0], near[1]));
            let culled = A::mul(A::sub_exact(cross, i.bias), 1.0) < 0.0;
            pipe.before = pipe.last;
            pipe.last = place;
            pipe.edge = std::array::from_fn(|k| A::sub_exact(pipe.last[k], pipe.before[k]));
            let fixed = |x: f32| A::to_int(A::mul(x, 16.0));
            out.push(IconVertexOut {
                vertex: Vertex {
                    x: ((fixed(place[0]) as u32) & 0xffff) as f32 / 16.0 - ICON_OFFSET_X,
                    y: ((fixed(place[1]) as u32) & 0xffff) as f32 / 16.0 - ICON_OFFSET_Y,
                    z: ((fixed(place[2]) as u32) >> 4) & 0x00ff_ffff,
                    u: st[0],
                    v: st[1],
                    q: st[2],
                    r: colour[0] as u8,
                    g: colour[1] as u8,
                    b: colour[2] as u8,
                    a: colour[3] as u8,
                },
                culled,
            });
        }
    }

    // One draw: the chunks in order, every vertex sent.
    pub fn draw(pipe: &mut IconPipeline, i: &IconInputs, vertices: &[IconVertexIn]) -> Vec<IconVertexOut> {
        let m = Self::screen_matrix(i);
        let mut out = Vec::with_capacity(vertices.len());
        for chunk in vertices.chunks(chunk_vertices(1)) {
            Self::chunk(pipe, &m, i, chunk, &mut out);
        }
        out
    }

    // The triangles the GS draws: every third vertex closes one, unless it carries ADC.
    pub fn triangles(sent: &[IconVertexOut]) -> Vec<Vertex> {
        sent.chunks(3).filter(|t| t.len() == 3 && !t[2].culled).flat_map(|t| t.iter().map(|v| v.vertex)).collect()
    }
}

// func_00259240 (0x00259240): the constants it and func_0024DFC8 read (gp data).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconConstants {
    pub above: f32,
    pub below: f32,
    pub period: f32,
    pub high: f32,
    pub high_less: f32,
    pub low: f32,
    pub low_more: f32,
    pub drop: f32,
    pub spin: f32,
}

impl IconConstants {
    pub fn of(program: &crate::program_image::ProgramImage) -> Self {
        let c = |a| program.single(a);
        Self {
            above: c(0x0036_fdc0),
            below: c(0x0036_fdc4),
            period: c(0x0036_fdc8),
            high: c(0x0036_fdcc),
            high_less: c(0x0036_fdd0),
            low: c(0x0036_fdd4),
            low_more: c(0x0036_fdd8),
            drop: c(0x0036_fefc),
            spin: c(0x0036_ff04),
        }
    }
}

// The item fields func_00259240 reads: the angles (+0, +4, +8), the position (+0x10), the icon kind (+0x130).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconItem {
    pub angles: [f32; 3],
    pub position: Vec4,
    pub kind: i32,
}

impl<A: Arithmetic> IconVu1<A> {
    // func_0024DFC8: whole periods off beyond D_0036FDC0 (or below -D_0036FDC4), then one period into (D_0036FDD4, D_0036FDCC].
    pub fn wrap(mut a: f32, k: &IconConstants) -> f32 {
        if (a > 0.0 && k.above < a) || (!(a > 0.0) && k.below < -a) {
            a = A::sub_exact(a, A::mul(A::from_int(A::to_int(A::div(a, k.period))), k.period));
        }
        if k.high < a {
            A::sub_exact(a, k.high_less)
        } else if a < k.low {
            A::add_exact(a, k.low_more)
        } else {
            a
        }
    }

    // The local matrix: sceVu0RotMatrix of the unit matrix by the wrapped angles, sceVu0TransMatrix by the position, rows 0..2 x
    // `scale` (item + 0x60 x func_00264498(), func_0024DCE0); kinds 1 and 0xC drop y by D_0036FEFC and take the y angle as
    // wrap(y + 0), kind 0 turns by D_0036FF04.
    pub fn local_matrix(item: &IconItem, scale: f32, k: &IconConstants) -> Mat4 {
        let mut angles = [Self::wrap(item.angles[0], k), Self::wrap(item.angles[1], k), Self::wrap(item.angles[2], k), 1.0];
        let mut position = item.position;
        if item.kind == 1 || item.kind == 0xc {
            position[1] = A::sub_exact(position[1], k.drop);
            angles[1] = Self::wrap(A::add_exact(item.angles[1], 0.0), k);
        } else if item.kind == 0 {
            angles[1] = Self::wrap(A::add_exact(angles[1], k.spin), k);
        }
        let m = Vu0::<A>::trans_matrix(&Vu0::<A>::rot_matrix_unit(&angles), &position);
        std::array::from_fn(|r| if r < 3 { m[r].map(|v| A::mul(v, scale)) } else { m[r] })
    }

    // Matrix A: sceVu0MulMatrix(item + 0x20, row + 0x30, local).
    pub fn item_matrix(row: &Mat4, local: &Mat4) -> Mat4 {
        Vu0::<A>::mul_matrix(row, local)
    }
}
