use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, s16};
use crate::clock_state::{ClockState, Colour};
use crate::frame_head::{AlphaMode, HeadDraw, HeadState, Part, Rect, Source, Target, head_rectangle};
use crate::matrix::{Mat4, Matrix, Vec4};
use crate::menu_types::CubeState;
use crate::rod_emitters::RodEmitters;
use crate::rods::{RodFace, RodFaceDraw, RodMesh, RodPiece, RodRecord, Rods, RodsInput};

// References/model/clock_cubes.mjs, HDD OSD 1.10U (the ring of six).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CubeSend {
    #[default]
    RefractedFar,
    GrainOffsetFar,
    GrainPlainFar,
    EdgeColour,
    Depth,
    HalfBuffer,
    RefractedNear,
    GrainOffsetNear,
    GrainPlainNear,
    LayerClear,
    LayerReflection,
    LayerAlpha,
    Added,
    ChainShrink,
    ChainStretch,
    ToDisplay,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CubeDraw {
    pub send: CubeSend,
    pub label: String,
    pub faces: Vec<RodFaceDraw>,
    pub rectangle: Option<HeadDraw>,
    pub clear: Colour,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CubeFrameInputs {
    pub width: i32,
    pub height: i32,
    pub field: i32,
    pub body: i32,
}

impl Default for CubeFrameInputs {
    fn default() -> Self {
        Self { width: 640, height: 224, field: 0, body: 0 }
    }
}

const STEP: i32 = 3000;
const TURN: i32 = 60 * STEP;

// clock_cubes.mjs wrap: x mod m in 0..m-1.
fn wrap(x: i32, m: i32) -> i32 {
    let lifted = if x >= 0 { x } else { x.wrapping_add(1).wrapping_sub(m) };
    x.wrapping_sub(lifted.wrapping_sub(lifted % m))
}

// HDD module_clock_230600: A and B mixed with weight w of 128, cut toward zero.
fn mix(a: &Colour, b: &Colour, w: i32) -> Colour {
    let mut out = [0i32; 4];
    for i in 0..4 {
        let sum = a[i].wrapping_mul(w).wrapping_add(b[i].wrapping_mul(128 - w));
        out[i] = (if sum > -1 { sum } else { sum + 127 }) >> 7;
    }
    out
}

// HDD module_clock_230550: the list position moving to its target, and the pulse dying down.
fn ring_step<A: Arithmetic>(c: &mut CubeState) {
    let l = &mut c.list;
    let left = l.left;
    let mut speed = l.speed;
    let movement;
    if !(speed < left.wrapping_abs()) {
        movement = left;
        speed = 0;
    } else {
        let slower = speed.wrapping_sub(l.slowing);
        movement = if left > -1 { speed } else { speed.wrapping_neg() };
        speed = if slower > 0 { slower } else { 1 };
    }
    l.speed = speed;
    l.left = left.wrapping_sub(movement);
    l.position = wrap(l.position.wrapping_add(movement), TURN);
    l.pulse = A::mul(l.pulse, c.constants[1]);
}

// HDD module_clock_2306B0: the record's scale and matrix for a list place. The fourth word of the position is 0.
fn place<A: Arithmetic>(c: &mut CubeState, index: i32, position: i32, scale: f32, spin: i32) {
    type M<A> = Matrix<A>;
    let size = if index == c.list.pulsed { A::add(scale, c.list.pulse) } else { scale };
    let record = &mut c.record;
    record.sx = size;
    record.sy = size;
    record.sz = size;
    let k = &c.constants;
    let raw = A::add(A::div(A::mul(A::cut(f64::from(wrap(index.wrapping_mul(STEP).wrapping_sub(position), TURN))), k[2]), k[3]), k[4]);
    let turns = A::div(A::sub(raw, k[5]), k[2]).floor();
    let angle = A::sub(raw, A::mul(turns, k[2]));
    let at: Vec4 = [A::add(if index & 1 != 0 { 60.0 } else { 70.0 }, -80.0), A::add(A::mul(A::cosf(angle), 60.0), 0.0), 47.5, 0.0];
    let mut moved = M::<A>::identity();
    moved[3] = M::<A>::apply(&moved, &at);
    let turn = s16(spin.wrapping_add(index.wrapping_mul(7000)));
    record.local = M::<A>::rotate_z(&M::<A>::rotate_y(&M::<A>::rotate_x(&moved, turn), turn), turn);
}

const EXPECTED_DRAWS: usize = 96;

struct Pass<'a, A> {
    view: Mat4,
    screen: Mat4,
    mesh: &'a RodMesh,
    input: &'a RodsInput,
    emit: RodEmitters<'a, A>,
}

fn strips(send: CubeSend, label: &str) -> CubeDraw {
    CubeDraw { send, label: label.into(), ..Default::default() }
}

fn push(out: &mut Vec<CubeDraw>, d: CubeDraw) {
    if !d.faces.is_empty() {
        out.push(d);
    }
}

impl<A: Arithmetic> Pass<'_, A> {
    // facts/config-cubes.md: the edge quad, base x F^2.
    fn edge(&self, face: &RodFace, rod: &RodRecord) -> RodFaceDraw {
        let edge_term = Rods::<A>::edge_term(face);
        let square = A::mul(edge_term, edge_term);
        let channel = |base: i32| {
            let value = A::to_int(A::mul(square, A::cut(f64::from(base))));
            if value < 0x100 { value } else { 0xff }
        };
        let mut draw = RodFaceDraw { face: face.index, piece: RodPiece::Whole, edge_smoothing: true, ..Default::default() };
        for k in 0..4 {
            draw.strip[k] = self.emit.position(&face.vertices[k]);
            RodEmitters::<A>::paint(&mut draw.strip[k], channel(rod.base[0]), channel(rod.base[1]), channel(rod.base[2]), 0x80);
        }
        draw
    }

    // facts/config-cubes.md: black, one depth step nearer, alpha 0x20.
    fn depth(&self, face: &RodFace) -> RodFaceDraw {
        let mut draw = RodFaceDraw { face: face.index, piece: RodPiece::Whole, edge_smoothing: true, ..Default::default() };
        for k in 0..4 {
            draw.strip[k] = self.emit.position(&face.vertices[k]);
            draw.strip[k].z = face.vertices[k].fixed[2].wrapping_add(1) as f32 / 16.0;
            RodEmitters::<A>::paint(&mut draw.strip[k], 0, 0, 0, 0x20);
        }
        draw
    }

    // facts/config-cubes.md: black with the reflection's alpha; no edge smoothing above 0x40.
    fn alpha(&self, face: &RodFace, value: i32) -> RodFaceDraw {
        let mut draw = RodFaceDraw { face: face.index, piece: RodPiece::Whole, edge_smoothing: !(value > 0x40), ..Default::default() };
        for k in 0..4 {
            draw.strip[k] = self.emit.position(&face.vertices[k]);
            RodEmitters::<A>::paint(&mut draw.strip[k], 0, 0, 0, value);
        }
        draw
    }

    #[allow(clippy::too_many_arguments)]
    fn rectangle(&self, send: CubeSend, label: &str, record: &Rect, part: Part, target: Target, source: Source, mode: AlphaMode) -> CubeDraw {
        let mut rectangle = head_rectangle(record, self.input.width, self.input.height, part);
        rectangle.target = target;
        rectangle.source = source;
        rectangle.alpha = mode;
        rectangle.depth_test = 1;
        CubeDraw { send, label: label.into(), rectangle: Some(rectangle), ..Default::default() }
    }

    // HDD module_clock_237350: eight sends and the half-buffer sprite.
    fn cube(&self, c: &mut CubeState, out: &mut Vec<CubeDraw>) {
        let rod = c.record;
        if rod.sy < 0.0 {
            return;
        }
        let whole = Rods::<A>::transform(&rod, &self.view, &self.screen, self.mesh);
        let cx = A::mul(whole.cx, c.centre_factors[0]);
        let cy = A::mul(whole.cy, c.centre_factors[0]);
        let mut far: Vec<&RodFace> = Vec::with_capacity(whole.faces.len());
        let mut near: Vec<&RodFace> = Vec::with_capacity(whole.faces.len());
        for face in &whole.faces {
            if face.flag == 0 {
                far.push(face);
            } else {
                near.push(face);
            }
        }
        let bent = |out: &mut Vec<CubeDraw>, send: CubeSend, faces: &[&RodFace]| {
            let mut d = strips(send, "cube: refracted");
            d.faces.reserve(faces.len());
            for face in faces {
                d.faces.push(self.emit.refracted(face, &rod, cx, cy, 0, RodPiece::Whole));
            }
            push(out, d);
        };
        let grain = |out: &mut Vec<CubeDraw>, send: CubeSend, faces: &[&RodFace], ds: f32, dt: f32| {
            let mut d = strips(send, "cube: grain");
            d.faces.reserve(faces.len());
            for face in faces {
                d.faces.push(self.emit.textured(face, &rod.textured, ds, dt, RodPiece::Whole));
            }
            push(out, d);
        };
        bent(out, CubeSend::RefractedFar, &far);
        grain(out, CubeSend::GrainOffsetFar, &far, rod.pair[0], rod.pair[1]);
        grain(out, CubeSend::GrainPlainFar, &far, 0.0, 0.0);
        let mut edges = strips(CubeSend::EdgeColour, "cube: edge colour");
        edges.faces.reserve(far.len());
        for face in &far {
            edges.faces.push(self.edge(face, &rod));
        }
        push(out, edges);
        let mut depths = strips(CubeSend::Depth, "cube: depth");
        depths.faces.reserve(near.len());
        for face in &near {
            depths.faces.push(self.depth(face));
        }
        push(out, depths);
        let w = self.input.width;
        let h = self.input.height;
        let sign = ((w as u32) >> 31) as i32;
        c.half.x1 = (w.wrapping_add(sign) >> 1) << 4;
        c.half.y1 = h << 4;
        c.half.u1 = (w.wrapping_add(sign) << 3) | 8;
        c.half.v1 = (h << 4) + 8;
        out.push(self.rectangle(
            CubeSend::HalfBuffer,
            "cube: half of buffer 0 added to buffer 1",
            &c.half,
            Part::CubeHalf,
            Target::Work1,
            Source::Work0,
            AlphaMode::Add,
        ));
        bent(out, CubeSend::RefractedNear, &near);
        grain(out, CubeSend::GrainOffsetNear, &near, rod.pair[0], rod.pair[1]);
        grain(out, CubeSend::GrainPlainNear, &near, 0.0, 0.0);
    }

    // HDD module_clock_237860: one cube in the highlight layer, two sends over the near faces.
    fn layer(&self, c: &CubeState, out: &mut Vec<CubeDraw>) {
        let rod = c.record;
        if rod.sy < 0.0 {
            return;
        }
        let whole = Rods::<A>::transform(&rod, &self.view, &self.screen, self.mesh);
        let mut reflection = strips(CubeSend::LayerReflection, "cube layer: reflection");
        let mut shade = strips(CubeSend::LayerAlpha, "cube layer: alpha");
        reflection.faces.reserve(whole.faces.len());
        shade.faces.reserve(whole.faces.len());
        for face in &whole.faces {
            if face.flag == 0 {
                continue;
            }
            reflection.faces.push(self.emit.reflected(face, &rod.reflection, RodPiece::Whole, true));
            shade.faces.push(self.alpha(face, rod.reflection[3]));
        }
        push(out, reflection);
        push(out, shade);
    }
}

#[derive(Debug, Clone)]
pub struct Cubes<A> {
    mesh: RodMesh,
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> Cubes<A> {
    pub fn new(mesh: RodMesh) -> Self {
        Self { mesh, arithmetic: PhantomData }
    }

    // References/model/clock_cubes.mjs cubes(): the ramp's tick, one step of the list, then the pass.
    pub fn frame(&self, c: &mut CubeState, head: &mut HeadState, clock: &ClockState, input: &CubeFrameInputs) -> Vec<CubeDraw> {
        let mut out = Vec::with_capacity(EXPECTED_DRAWS);
        crate::ramp::tick_ramp(&mut c.ramp);
        ring_step::<A>(c);

        let scale = A::div(A::cut(f64::from(c.ramp.counter)), A::cut(f64::from(input.body)));
        if c.ramp.state == 0 {
            return out;
        }

        let rods_input = RodsInput { width: input.width, height: input.height, field: input.field, ..Default::default() };
        let pass = Pass::<A> { view: c.view, screen: c.screen, mesh: &self.mesh, input: &rods_input, emit: RodEmitters::<A>::new(&rods_input) };
        let w = input.width;
        let h = input.height;
        let position = c.list.position;
        let whole = position / STEP;
        let part = position % STEP;
        let spin = i32::from(clock.spin.unwrap_or(0));

        let weight = (part << 7) / STEP;
        for slot in 0..6 {
            c.record.base = if slot == 2 {
                mix(&c.colours.selected, &c.colours.plain, 128 - weight)
            } else if slot == 3 {
                mix(&c.colours.selected, &c.colours.plain, weight)
            } else {
                c.colours.plain
            };
            let list_position = c.list.position;
            place::<A>(c, wrap(whole + slot - 2, 60), list_position, scale, spin);
            pass.cube(c, &mut out);
        }
        out.push(CubeDraw { send: CubeSend::LayerClear, label: "draw to a work buffer".into(), clear: c.layer_clear, ..Default::default() });
        for slot in 0..6 {
            let fade = (((-2 * STEP + slot * STEP - part).wrapping_abs() - 2 * STEP) << 7) / STEP;
            if !(fade < 128) {
                continue;
            }
            c.record.reflection[3] = 128 - (if fade > -1 { fade } else { 0 });
            place::<A>(c, wrap(whole + slot - 2, 60), position, scale, spin);
            pass.layer(c, &mut out);
        }

        c.added.x1 = w << 4;
        c.added.y1 = h << 4;
        c.added.u1 = (w << 4) + 8;
        c.added.v1 = (h << 4) + 8;
        out.push(pass.rectangle(CubeSend::Added, "cubes: buffer 1 added to buffer 0", &c.added, Part::CubeAdded, Target::Work0, Source::Work1, AlphaMode::Add));
        let level = clock.level as u32;
        let trips = 5u32.saturating_sub(level);
        for k in 0..trips {
            let x = 0x13f4 - 0x20 * k as i32;
            c.chain.x1 = x;
            c.chain.y1 = 0x954 - 0x10 * k as i32;
            c.chain.u1 = (w << 4) + 8;
            c.chain.v1 = ((h - 1) << 4) + 8;
            out.push(pass.rectangle(
                CubeSend::ChainShrink,
                "cubes: blur chain, shrink",
                &c.chain,
                Part::CubeChainShrink,
                Target::Work1,
                Source::Work0,
                AlphaMode::AlphaOver,
            ));
            c.chain.x1 = w << 4;
            c.chain.y1 = (h - 1) << 4;
            c.chain.u1 = x + 8;
            c.chain.v1 = 0x95c - 0x10 * k as i32;
            out.push(pass.rectangle(
                CubeSend::ChainStretch,
                "cubes: blur chain, stretch",
                &c.chain,
                Part::CubeChainStretch,
                Target::Work0,
                Source::Work1,
                AlphaMode::AlphaOver,
            ));
        }
        head.copy.x1 = w << 4;
        head.copy.y1 = h << 4;
        head.copy.u1 = (w << 4) + 8;
        head.copy.v1 = (h << 4) + 8;
        head.copy.blend = 1;
        out.push(pass.rectangle(
            CubeSend::ToDisplay,
            "cubes: buffer 0 onto the display",
            &head.copy,
            Part::CubeToDisplay,
            Target::Display,
            Source::Work0,
            AlphaMode::AlphaOver,
        ));
        out
    }
}
