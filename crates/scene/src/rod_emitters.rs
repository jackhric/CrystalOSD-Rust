use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, s16};
use crate::matrix::Matrix;
use crate::rods::{Colour, RodFace, RodFaceDraw, RodFaceVertex, RodPiece, RodRecord, RodVertex, Rods, RodsInput};

// The rods' emitters (facts/clock-rod-draw.md), shared with the cubes (facts/config-cubes.md).
pub struct RodEmitters<'a, A> {
    pub input: &'a RodsInput,
    arithmetic: PhantomData<A>,
}

impl<'a, A: Arithmetic> RodEmitters<'a, A> {
    pub const TENTH: f32 = 0.1;
    pub const NINE_TENTHS: f32 = 0.9;
    pub const ALMOST_ONE: f32 = 0.99;
    pub const INWARD: f32 = 0.95;

    pub fn new(input: &'a RodsInput) -> Self {
        Self { input, arithmetic: PhantomData }
    }

    #[allow(clippy::eq_op)]
    pub fn truthy(x: f32) -> bool {
        x == x && x != 0.0
    }

    pub fn position(&self, v: &RodFaceVertex) -> RodVertex {
        RodVertex {
            x: v.fixed[0].wrapping_sub((0x800 - (self.input.width >> 1)) << 4) as f32 / 16.0,
            y: v.fixed[1].wrapping_sub((0x800 - (self.input.height >> 1)) << 4) as f32 / 16.0,
            z: v.fixed[2] as f32 / 16.0,
            ..Default::default()
        }
    }

    pub fn paint(v: &mut RodVertex, r: i32, g: i32, b: i32, a: i32) {
        v.r = r as f32 / 128.0;
        v.g = g as f32 / 128.0;
        v.b = b as f32 / 128.0;
        v.a = a as f32 / 128.0;
    }

    // facts/clock-rod-draw.md, "Refracted emitter"; HDD OSD clamps the coordinates to the screen.
    pub fn refracted(&self, face: &RodFace, rod: &RodRecord, cx: f32, cy: f32, extra: i32, piece: RodPiece) -> RodFaceDraw {
        let edge = Rods::<A>::edge_term(face);
        let mut bright = A::to_int(A::mul(A::mul(A::mul(A::mul(A::mul(rod.strength, 10.0), edge), edge), edge), edge));
        if Self::NINE_TENTHS < edge {
            let angle = s16(A::to_int(A::div(A::mul(A::sub(1.0, edge), 32768.0), Self::TENTH)));
            bright = A::to_int(A::mul(A::cut(f64::from(bright)), A::mul(A::sub(1.0, A::cos16(angle)), 0.5)));
        }
        let channel = |base: i32| {
            let value = bright.wrapping_add(base).wrapping_add(extra);
            if value < 0x100 { value } else { 0xff }
        };
        let clamp_to = |value: i32, top: i32| {
            if value < 0 {
                0
            } else if value > top {
                top
            } else {
                value
            }
        };
        let mut draw = RodFaceDraw { face: face.index, piece, edge_smoothing: !(Self::ALMOST_ONE < edge), strip: Default::default() };
        let half_w = A::cut(f64::from(self.input.width / 2));
        let half_h = A::cut(f64::from(self.input.height / 2));
        let half = A::mul(A::cut(f64::from(self.input.field)), 0.5);
        for k in 0..4 {
            let vertex = &face.vertices[k];
            let push = A::mul(A::mul(A::mul(face.normal[0], 1000.0), vertex.q), rod.refraction);
            let u = A::sub(A::add(A::mul(A::sub(A::sub(vertex.screen[0], 2048.0), cx), Self::INWARD), cx), push);
            let lift = A::mul(A::mul(A::mul(face.normal[1], 500.0), vertex.q), rod.refraction);
            let v = A::sub(A::add(A::mul(A::sub(A::sub(vertex.screen[1], 2048.0), cy), Self::INWARD), cy), lift);
            let ui = clamp_to(A::to_int(A::mul(A::add(half_w, u), 16.0)), self.input.width.wrapping_mul(16));
            let vi = clamp_to(A::to_int(A::mul(A::sub(A::add(half_h, v), half), 16.0)), self.input.height.wrapping_mul(16));
            let out = &mut draw.strip[k];
            *out = self.position(vertex);
            out.u = ui as f32 / 16.0;
            out.v = vi as f32 / 16.0;
            out.q = 1.0;
            Self::paint(out, channel(rod.base[0]), channel(rod.base[1]), channel(rod.base[2]), 0x80);
        }
        draw
    }

    // facts/clock-rod-draw.md, "Textured emitter".
    pub fn textured(&self, face: &RodFace, colour: &Colour, ds: f32, dt: f32, piece: RodPiece) -> RodFaceDraw {
        let mut draw = RodFaceDraw { face: face.index, piece, edge_smoothing: false, strip: Default::default() };
        for k in 0..4 {
            let vertex = &face.vertices[k];
            let out = &mut draw.strip[k];
            *out = self.position(vertex);
            out.u = A::mul(A::add(vertex.s, ds), vertex.q);
            out.v = A::mul(A::add(vertex.t, dt), vertex.q);
            out.q = vertex.q;
            Self::paint(out, colour[0], colour[1], colour[2], colour[3]);
        }
        draw
    }

    // facts/clock-extra-passes.md, "The reflection emitter".
    pub fn reflected(&self, face: &RodFace, colour: &Colour, piece: RodPiece, edge_smoothing: bool) -> RodFaceDraw {
        let mut draw = RodFaceDraw { face: face.index, piece, edge_smoothing, strip: Default::default() };
        let normal = [face.normal[0], face.normal[1], face.normal[2]];
        for k in 0..4 {
            let vertex = &face.vertices[k];
            let eye = Matrix::<A>::normalize(&[vertex.eye[0], vertex.eye[1], vertex.eye[2]]);
            let d = Matrix::<A>::dot(&eye, &normal);
            let twice = A::add(d, d);
            let amount = if twice < 0.0 { -twice } else { twice };
            let r0 = A::add(eye[0], A::mul(face.normal[0], amount));
            let r1 = A::add(eye[1], A::mul(face.normal[1], amount));
            let out = &mut draw.strip[k];
            *out = self.position(vertex);
            out.u = A::to_unsigned(A::mul(A::add(r0, 1.0), 512.0)) as f32 / 16.0;
            out.v = A::to_unsigned(A::mul(A::add(r1, 1.0), 256.0)) as f32 / 16.0;
            out.q = 1.0;
            Self::paint(out, colour[0], colour[1], colour[2], colour[3]);
        }
        draw
    }

    // facts/clock-rod-draw.md, "Texture offsets handed to the textured emitter".
    pub fn offsets(rod: &RodRecord, face: &RodFace, lift: f32, paired: bool) -> (f32, f32) {
        let tenth = f64::from(Self::TENTH);
        let stepped = A::add(A::cut(f64::from(rod.number) * tenth), A::cut(f64::from(face.index) * tenth));
        if !paired {
            return (stepped, if Self::truthy(lift) { A::add(stepped, lift) } else { stepped });
        }
        (A::add(stepped, rod.pair[0]), if Self::truthy(lift) { A::add(A::add(stepped, rod.pair[1]), lift) } else { A::add(stepped, rod.pair[1]) })
    }
}
