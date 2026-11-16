use std::marker::PhantomData;
use std::path::Path;

use crate::arithmetic::{Arithmetic, as_float, s16};
use crate::matrix::{Mat4, Matrix, Vec4};
use crate::rod_emitters::RodEmitters;
use crate::{Error, Result};

pub type Colour = [i32; 4];

// The rod's mesh (facts/data/rod-mesh.json): each face a triangle strip of four vertices.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RodMesh {
    pub positions: Vec<Vec4>,
    pub normals: Vec<Vec4>,
    pub coordinates: Vec<Vec4>,
}

// The rod record (facts/clock-rod-draw.md, "The rod record").
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RodRecord {
    pub number: i32,
    pub faces: i32,
    pub local: Mat4,
    pub sx: f32,
    pub sy: f32,
    pub sz: f32,
    pub base: Colour,
    pub strength: f32,
    pub textured: Colour,
    pub pair: [f32; 2],
    pub refraction: f32,
    pub reflection: Colour,
    pub extra: Colour,
}

// What the rods read of the clock's state (facts/clock-state.md, facts/clock-scene.md).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RodLight {
    pub appearance: f32,
    pub progress: f32,
    pub base: Colour,
    pub reflection: Colour,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RodsInput {
    pub current_rod: i32,
    pub seconds_angle: i32,
    pub rod_angle: i32,
    pub rods: [RodLight; 12],
    pub accent: Colour,
    pub fourth: Colour,
    pub width: i32,
    pub height: i32,
    pub field: i32,
}

impl Default for RodsInput {
    fn default() -> Self {
        Self {
            current_rod: 0,
            seconds_angle: 0,
            rod_angle: 0,
            rods: [RodLight::default(); 12],
            accent: [0; 4],
            fourth: [0; 4],
            width: 640,
            height: 224,
            field: 0,
        }
    }
}

// One face through the transform (facts/clock-rod-draw.md, "Transform"). `fixed` is the
// screen position in the OSD's 4-bit fixed point (sceVu0FTOI4Vector).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RodFaceVertex {
    pub eye: Vec4,
    pub screen: Vec4,
    pub q: f32,
    pub s: f32,
    pub t: f32,
    pub fixed: [i32; 3],
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RodFace {
    pub index: i32,
    pub normal: Vec4,
    pub vertices: [RodFaceVertex; 4],
    pub flag: i32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RodTransform {
    pub cx: f32,
    pub cy: f32,
    pub cz: f32,
    pub faces: Vec<RodFace>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RodPiece {
    #[default]
    Whole,
    A,
    B,
}

// Native units: x, y in pixels from the screen's top-left, z the depth (screen matrix units),
// colour with 1.0 = the OSD's 0x80. Texel sends: u, v in texels of the source, q = 1.
// Perspective sends: u, v, q are the S, T, Q the OSD computes (divide by q to sample).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RodVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub q: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RodFaceDraw {
    pub face: i32,
    pub piece: RodPiece,
    pub edge_smoothing: bool,
    pub strip: [RodVertex; 4],
}

// The five sends of a rod and the two of an extra pass (facts/clock-rod-draw.md, facts/clock-extra-passes.md).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RodSendKind {
    #[default]
    RefractedFar,
    GrainSubtracted,
    GrainAdded,
    RefractedNearToFrame,
    RefractedNearToRefraction,
    Reflection,
    Grain,
}

pub fn perspective(kind: RodSendKind) -> bool {
    kind == RodSendKind::GrainSubtracted || kind == RodSendKind::GrainAdded || kind == RodSendKind::Grain
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RodSend {
    pub kind: RodSendKind,
    pub faces: Vec<RodFaceDraw>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RodPieceRecord {
    pub piece: RodPiece,
    pub record: RodRecord,
    pub transform: RodTransform,
}

impl Default for RodPieceRecord {
    fn default() -> Self {
        Self { piece: RodPiece::A, record: RodRecord::default(), transform: RodTransform::default() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rod {
    pub placement: i32,
    pub record: RodRecord,
    pub t: f32,
    pub key: f32,
    pub matrix: Mat4,
    pub drawn: bool,
    pub whole: RodTransform,
    pub cx: f32,
    pub cy: f32,
    pub pieces: Vec<RodPieceRecord>,
    pub sends: Vec<RodSend>,
}

impl Default for Rod {
    fn default() -> Self {
        Self {
            placement: 0,
            record: RodRecord::default(),
            t: -1.0,
            key: 0.0,
            matrix: [[0.0; 4]; 4],
            drawn: false,
            whole: RodTransform::default(),
            cx: 0.0,
            cy: 0.0,
            pieces: Vec::new(),
            sends: Vec::new(),
        }
    }
}

// One rod in an extra pass; `grainTexture` is the clock texture the grain samples (2 or 3).
#[derive(Debug, Clone, PartialEq)]
pub struct RodExtraDraw {
    pub rod: usize,
    pub split: bool,
    pub grain_texture: i32,
    pub reflection: RodSend,
    pub grain: RodSend,
}

impl Default for RodExtraDraw {
    fn default() -> Self {
        Self { rod: 0, split: false, grain_texture: 2, reflection: RodSend::default(), grain: RodSend::default() }
    }
}

// The draw list (facts/clock-scene.md, "The draw list"): deepest first; a new node goes in
// front of the first one that is not deeper than it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DepthNode {
    pub orb: bool,
    pub index: i32,
    pub key: f32,
}

pub fn insert_by_depth(list: &mut Vec<DepthNode>, node: &DepthNode) {
    let mut at = 0;
    while at < list.len() && node.key < list[at].key {
        at += 1;
    }
    list.insert(at, *node);
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RodsFrame {
    pub list: Vec<DepthNode>,
    pub rods: Vec<Rod>,
    pub extra_passes: [Vec<RodExtraDraw>; 2],
}

mod json {
    use crate::{Error, Result};

    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(String),
        String(String),
        Array(Vec<Value>),
        Object(Vec<(String, Value)>),
    }

    impl Value {
        pub fn at(&self, key: &str) -> Result<&Value> {
            match self {
                Value::Object(members) => {
                    members.iter().find(|(k, _)| k == key).map(|(_, v)| v).ok_or_else(|| Error(format!("rod mesh: key '{key}' not found")))
                }
                _ => Err(Error(format!("rod mesh: cannot use at() with a non-object for '{key}'"))),
            }
        }
    }

    struct Parser<'a> {
        text: &'a [u8],
        at: usize,
    }

    fn malformed<T>() -> Result<T> {
        Err(Error("rod mesh: not JSON".into()))
    }

    impl Parser<'_> {
        fn skip(&mut self) {
            while self.at < self.text.len() && matches!(self.text[self.at], b' ' | b'\t' | b'\n' | b'\r') {
                self.at += 1;
            }
        }

        fn eat(&mut self, literal: &[u8]) -> bool {
            if self.text[self.at..].starts_with(literal) {
                self.at += literal.len();
                true
            } else {
                false
            }
        }

        fn value(&mut self) -> Result<Value> {
            self.skip();
            let Some(&c) = self.text.get(self.at) else { return malformed() };
            match c {
                b'{' => {
                    self.at += 1;
                    let mut members = Vec::new();
                    self.skip();
                    if self.eat(b"}") {
                        return Ok(Value::Object(members));
                    }
                    loop {
                        self.skip();
                        let Value::String(key) = self.value()? else { return malformed() };
                        self.skip();
                        if !self.eat(b":") {
                            return malformed();
                        }
                        let value = self.value()?;
                        match members.iter_mut().find(|(k, _): &&mut (String, Value)| *k == key) {
                            Some(member) => member.1 = value,
                            None => members.push((key, value)),
                        }
                        self.skip();
                        if self.eat(b"}") {
                            return Ok(Value::Object(members));
                        }
                        if !self.eat(b",") {
                            return malformed();
                        }
                    }
                }
                b'[' => {
                    self.at += 1;
                    let mut items = Vec::new();
                    self.skip();
                    if self.eat(b"]") {
                        return Ok(Value::Array(items));
                    }
                    loop {
                        items.push(self.value()?);
                        self.skip();
                        if self.eat(b"]") {
                            return Ok(Value::Array(items));
                        }
                        if !self.eat(b",") {
                            return malformed();
                        }
                    }
                }
                b'"' => {
                    self.at += 1;
                    let mut out = Vec::new();
                    loop {
                        let Some(&c) = self.text.get(self.at) else { return malformed() };
                        self.at += 1;
                        match c {
                            b'"' => break,
                            b'\\' => {
                                let Some(&e) = self.text.get(self.at) else { return malformed() };
                                self.at += 1;
                                match e {
                                    b'"' | b'\\' | b'/' => out.push(e),
                                    b'b' => out.push(8),
                                    b'f' => out.push(12),
                                    b'n' => out.push(b'\n'),
                                    b'r' => out.push(b'\r'),
                                    b't' => out.push(b'\t'),
                                    b'u' => {
                                        let digits = self.text.get(self.at..self.at + 4).and_then(|d| std::str::from_utf8(d).ok());
                                        let Some(code) = digits.and_then(|d| u32::from_str_radix(d, 16).ok()) else { return malformed() };
                                        self.at += 4;
                                        let Some(ch) = char::from_u32(code) else { return malformed() };
                                        out.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
                                    }
                                    _ => return malformed(),
                                }
                            }
                            _ => out.push(c),
                        }
                    }
                    String::from_utf8(out).map(Value::String).or_else(|_| malformed())
                }
                b't' if self.eat(b"true") => Ok(Value::Bool(true)),
                b'f' if self.eat(b"false") => Ok(Value::Bool(false)),
                b'n' if self.eat(b"null") => Ok(Value::Null),
                b'-' | b'0'..=b'9' => {
                    let start = self.at;
                    while self.at < self.text.len() && matches!(self.text[self.at], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                        self.at += 1;
                    }
                    let number = String::from_utf8_lossy(&self.text[start..self.at]).into_owned();
                    if number.parse::<f64>().is_err() {
                        return malformed();
                    }
                    Ok(Value::Number(number))
                }
                _ => malformed(),
            }
        }
    }

    pub fn parse(text: &[u8]) -> Result<Value> {
        let mut parser = Parser { text, at: 0 };
        let value = parser.value()?;
        parser.skip();
        if parser.at != text.len() {
            return malformed();
        }
        Ok(value)
    }
}

fn hex_word(text: &str) -> Result<u32> {
    let invalid = || Error("rod mesh: stoul: no conversion".into());
    let mut rest = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let negative = rest.starts_with('-');
    if rest.starts_with('-') || rest.starts_with('+') {
        rest = &rest[1..];
    }
    if (rest.starts_with("0x") || rest.starts_with("0X")) && rest[2..].starts_with(|c: char| c.is_ascii_hexdigit()) {
        rest = &rest[2..];
    }
    let digits: &str = &rest[..rest.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(rest.len())];
    if digits.is_empty() {
        return Err(invalid());
    }
    let value = u64::from_str_radix(digits, 16).map_err(|_| Error("rod mesh: stoul: out of range".into()))?;
    Ok((if negative { value.wrapping_neg() } else { value }) as u32)
}

fn vectors(part: &json::Value) -> Result<Vec<Vec4>> {
    let json::Value::Array(bits) = part.at("bits")? else { return Err(Error("rod mesh: 'bits' is not an array".into())) };
    if bits.len() % 4 != 0 {
        return Err(Error("rod mesh: a part is not whole vectors".into()));
    }
    let mut out = vec![[0.0f32; 4]; bits.len() / 4];
    for (i, bit) in bits.iter().enumerate() {
        let json::Value::String(text) = bit else { return Err(Error("rod mesh: a bit pattern is not a string".into())) };
        out[i / 4][i % 4] = as_float(hex_word(text)?);
    }
    Ok(out)
}

pub fn load_rod_mesh(file: &Path) -> Result<RodMesh> {
    let text = std::fs::read(file).map_err(|_| Error(format!("cannot open {}", file.display())))?;
    let j = json::parse(&text)?;
    let mesh = RodMesh { positions: vectors(j.at("positions")?)?, normals: vectors(j.at("normals")?)?, coordinates: vectors(j.at("coordinates")?)? };
    let faces = match j.at("faces")? {
        json::Value::Number(n) => match n.parse::<i64>() {
            Ok(v) => v as usize,
            Err(_) => n.parse::<f64>().map(|v| v as usize).unwrap_or(0),
        },
        _ => return Err(Error("rod mesh: 'faces' is not a number".into())),
    };
    if mesh.normals.len() < faces || mesh.positions.len() < faces * 4 || mesh.coordinates.len() < faces * 4 {
        return Err(Error("rod mesh: fewer vectors than faces".into()));
    }
    Ok(mesh)
}

const RODS_SHOWN_ABOVE: f32 = 0.05;

struct Set<'r> {
    piece: RodPiece,
    rod: RodRecord,
    faces: Vec<&'r RodFace>,
    lift: f32,
}

fn side_of<'s, 'r>(sets: &'s [Set<'r>], side: bool) -> Vec<(&'s Set<'r>, &'r RodFace)> {
    let mut faces = Vec::new();
    for set in sets {
        for &face in &set.faces {
            if (face.flag != 0) == side {
                faces.push((set, face));
            }
        }
    }
    faces
}

// The pieces drawn: the whole rod, or piece A's faces 8 and up and piece B's all but 8 and 9.
fn sets_of<'r>(rod: &'r Rod, a: &RodRecord, b: &RodRecord, lift: f32) -> Vec<Set<'r>> {
    if rod.pieces.is_empty() {
        return vec![Set { piece: RodPiece::Whole, rod: rod.record, faces: rod.whole.faces.iter().collect(), lift: 0.0 }];
    }
    let first = Set { piece: RodPiece::A, rod: *a, faces: rod.pieces[0].transform.faces.iter().filter(|face| face.index >= 8).collect(), lift: 0.0 };
    let second =
        Set { piece: RodPiece::B, rod: *b, faces: rod.pieces[1].transform.faces.iter().filter(|face| face.index != 8 && face.index != 9).collect(), lift };
    vec![first, second]
}

// The rods of a frame (References/model/clock_frame.mjs scene(), HDD OSD 1.10U). The rod
// template is state: it is written in place each frame and carried to the next.
#[derive(Debug, Clone)]
pub struct Rods<A> {
    mesh: RodMesh,
    template: RodRecord,
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> Rods<A> {
    pub fn new(mesh: RodMesh, rod_template: &RodRecord) -> Self {
        Self { mesh, template: *rod_template, arithmetic: PhantomData }
    }

    pub fn rod_template(&self) -> &RodRecord {
        &self.template
    }

    pub fn rod_template_mut(&mut self) -> &mut RodRecord {
        &mut self.template
    }

    // facts/clock-scene.md, "Submitting the rods": the rod's matrix chain.
    pub fn rod_matrix(i: i32, ring: i32, spin: i32) -> Mat4 {
        type M<A> = Matrix<A>;
        let mut m = M::<A>::rotate_y(&M::<A>::rotate_z(&M::<A>::identity(), ring), spin);
        m = M::<A>::rotate_z(&m, s16(((i << 16) / 12) - 0x8000));
        m = M::<A>::move_(&m, 0.0, 20.0, 0.0);
        M::<A>::rotate_y(&m, s16(s16(spin) << 2))
    }

    // facts/clock-rod-draw.md, "Transform".
    pub fn transform(rod: &RodRecord, view: &Mat4, screen: &Mat4, mesh: &RodMesh) -> RodTransform {
        type M<A> = Matrix<A>;
        let m = M::<A>::multiply(view, &rod.local);
        let mut centre = M::<A>::apply(screen, &M::<A>::apply(&m, &[0.0, 0.0, 0.0, 1.0]));
        let inverse = A::div(1.0, centre[3]);
        for x in centre.iter_mut() {
            *x = A::mul(*x, inverse);
        }
        let mut out =
            RodTransform { cx: A::sub(centre[0], 2048.0), cy: A::sub(centre[1], 2048.0), cz: centre[2], faces: Vec::with_capacity(rod.faces as usize) };
        for i in 0..rod.faces {
            let mut face = RodFace { index: i, normal: M::<A>::apply(&m, &mesh.normals[i as usize]), ..Default::default() };
            for k in 0..4 {
                let at = i as usize * 4 + k;
                let p = &mesh.positions[at];
                let v = &mut face.vertices[k];
                v.eye = M::<A>::apply(&m, &[A::mul(p[0], rod.sx), A::mul(p[1], rod.sy), A::mul(p[2], rod.sz), 1.0]);
                let mut on = M::<A>::apply(screen, &v.eye);
                v.q = A::div(1.0, on[3]);
                for x in on.iter_mut() {
                    *x = A::mul(*x, v.q);
                }
                v.screen = on;
                let st = &mesh.coordinates[at];
                v.s = st[0];
                v.t = A::mul(st[1], rod.sy);
                for (n, fixed) in v.fixed.iter_mut().enumerate() {
                    *fixed = A::to_int(A::mul(on[n], 16.0));
                }
            }
            let v0 = face.vertices[0].screen;
            let e2x = A::sub(face.vertices[2].screen[0], v0[0]);
            let e2y = A::sub(face.vertices[2].screen[1], v0[1]);
            let e1x = A::sub(face.vertices[1].screen[0], v0[0]);
            let e1y = A::sub(face.vertices[1].screen[1], v0[1]);
            face.flag = if A::sub(A::mul(e2x, e1y), A::mul(e2y, e1x)) > 0.0 { 0 } else { 1 };
            out.faces.push(face);
        }
        out
    }

    // facts/clock-rod-draw.md: 1 - |eye direction . normal| at the face's first vertex.
    pub fn edge_term(face: &RodFace) -> f32 {
        let eye = &face.vertices[0].eye;
        let d = Matrix::<A>::dot(&Matrix::<A>::normalize(&[eye[0], eye[1], eye[2]]), &[face.normal[0], face.normal[1], face.normal[2]]);
        A::sub(1.0, if d < 0.0 { -d } else { d })
    }

    pub fn depth_key(view: &Mat4, local: &Mat4) -> f32 {
        Matrix::<A>::apply(view, &local[3])[2]
    }

    // References/model/clock_frame.mjs scene(): the rods' part, in its order (facts/clock-scene.md).
    pub fn frame(&mut self, input: &RodsInput, view: &Mat4, screen: &Mat4) -> RodsFrame {
        let mut out = RodsFrame::default();
        if !(RODS_SHOWN_ABOVE < input.rods[0].appearance) {
            return out;
        }
        out.rods.reserve(12);
        out.list.reserve(12);

        for i in 0..12i32 {
            let number = (i as u32).wrapping_add(input.current_rod as u32) % 12;
            let light = &input.rods[number as usize];
            self.template.local = Self::rod_matrix(i, input.rod_angle, input.seconds_angle);
            self.template.number = number as i32;
            self.template.sy = light.appearance;
            self.template.base = light.base;
            self.template.reflection = light.reflection;
            self.template.strength = if i == 0 { 200.0 } else { 160.0 };
            let mut rod = Rod { placement: i, record: self.template, t: if i == 0 { light.progress } else { -1.0 }, ..Default::default() };
            rod.key = Self::depth_key(view, &rod.record.local);
            rod.matrix = Matrix::<A>::multiply(view, &rod.record.local);
            insert_by_depth(&mut out.list, &DepthNode { orb: false, index: i, key: rod.key });
            out.rods.push(rod);
        }

        let emit = RodEmitters::<A>::new(input);
        // Piece B's texture offsets move by 2 t s (facts/clock-rod-draw.md).
        let lift_of = |rod: &Rod| A::add(A::mul(rod.t, rod.record.sy), A::mul(rod.t, rod.record.sy));

        for node in &out.list {
            let rod = &mut out.rods[node.index as usize];
            if rod.record.sy < 0.0 {
                continue;
            }
            rod.drawn = true;
            rod.whole = Self::transform(&rod.record, view, screen, &self.mesh);
            rod.cx = A::mul(rod.whole.cx, RodEmitters::<A>::NINE_TENTHS);
            rod.cy = A::mul(rod.whole.cy, RodEmitters::<A>::NINE_TENTHS);
            if rod.t > 0.0 {
                let s = rod.record.sy;
                let mut a = rod.record;
                let mut b = rod.record;
                a.sy = A::mul(rod.t, s);
                a.strength = if rod.placement == 0 { 100.0 } else { 0.0 };
                a.base = input.accent;
                b.sy = A::mul(A::sub(1.0, rod.t), s);
                b.local[3] = Matrix::<A>::apply(&rod.record.local, &[0.0, A::mul(A::mul(s, 26.0), rod.t), 0.0, 1.0]);
                rod.pieces.reserve(2);
                let transform_a = Self::transform(&a, view, screen, &self.mesh);
                rod.pieces.push(RodPieceRecord { piece: RodPiece::A, record: a, transform: transform_a });
                let transform_b = Self::transform(&b, view, screen, &self.mesh);
                rod.pieces.push(RodPieceRecord { piece: RodPiece::B, record: b, transform: transform_b });
            }
            let drawn: &Rod = rod;
            let sets = if drawn.pieces.is_empty() {
                sets_of(drawn, &drawn.record, &drawn.record, 0.0)
            } else {
                sets_of(drawn, &drawn.pieces[0].record, &drawn.pieces[1].record, lift_of(drawn))
            };
            let bent = |kind: RodSendKind, near_side: bool, extra: i32| {
                let side = side_of(&sets, near_side);
                let mut send = RodSend { kind, faces: Vec::with_capacity(side.len()) };
                for (set, face) in side {
                    send.faces.push(emit.refracted(face, &set.rod, drawn.cx, drawn.cy, extra, set.piece));
                }
                send
            };
            let grain = |kind: RodSendKind, paired: bool| {
                let side = side_of(&sets, false);
                let mut send = RodSend { kind, faces: Vec::with_capacity(side.len()) };
                for (set, face) in side {
                    let (ds, dt) = RodEmitters::<A>::offsets(&set.rod, face, set.lift, paired);
                    send.faces.push(emit.textured(face, &set.rod.textured, ds, dt, set.piece));
                }
                send
            };
            let sends = vec![
                bent(RodSendKind::RefractedFar, false, 0),
                grain(RodSendKind::GrainSubtracted, false),
                grain(RodSendKind::GrainAdded, true),
                bent(RodSendKind::RefractedNearToFrame, true, 0),
                bent(RodSendKind::RefractedNearToRefraction, true, 0xff),
            ];
            drop(sets);
            out.rods[node.index as usize].sends = sends;
        }

        // facts/clock-extra-passes.md: two passes over the list. A split rod's pieces take the
        // fourth colour as their reflection; a whole rod keeps its own.
        for pass in 0..2 {
            for node in &out.list {
                let rod = &out.rods[node.index as usize];
                if !rod.drawn {
                    continue;
                }
                let split = !rod.pieces.is_empty();
                let sets = if split {
                    let mut a = rod.pieces[0].record;
                    let mut b = rod.pieces[1].record;
                    a.strength = rod.record.strength;
                    a.base = rod.record.base;
                    a.reflection = input.fourth;
                    b.reflection = input.fourth;
                    sets_of(rod, &a, &b, lift_of(rod))
                } else {
                    sets_of(rod, &rod.record, &rod.record, 0.0)
                };
                let mut draw = RodExtraDraw {
                    rod: node.index as usize,
                    split,
                    grain_texture: if (pass != 0) == split { 3 } else { 2 },
                    reflection: RodSend { kind: RodSendKind::Reflection, faces: Vec::new() },
                    grain: RodSend { kind: RodSendKind::Grain, faces: Vec::new() },
                };
                let side = side_of(&sets, true);
                draw.reflection.faces.reserve(side.len());
                draw.grain.faces.reserve(side.len());
                for (set, face) in &side {
                    draw.reflection.faces.push(emit.reflected(face, &set.rod.reflection, set.piece, false));
                }
                for (set, face) in &side {
                    let (ds, dt) = RodEmitters::<A>::offsets(&set.rod, face, set.lift, pass != 0);
                    draw.grain.faces.push(emit.textured(face, &set.rod.extra, ds, dt, set.piece));
                }
                out.extra_passes[pass].push(draw);
            }
        }
        out
    }
}
