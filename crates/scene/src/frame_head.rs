use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, as_float, float_bits, s16};
use crate::matrix::{Mat4, Matrix, Vec4};
use crate::ramp::{Ramp, tick_ramp};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub colour: [i32; 4],
    pub x0: i32,
    pub y0: i32,
    pub u0: i32,
    pub v0: i32,
    pub x1: i32,
    pub y1: i32,
    pub u1: i32,
    pub v1: i32,
    pub z: i32,
    pub blend: i32,
    pub textured: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RingRecord {
    pub alpha: i32,
    pub cx: i32,
    pub cy: i32,
    pub rx: i32,
    pub ry: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TubeConstants {
    pub near: f32,
    pub turn: f32,
    pub scroll: f32,
    pub ripple: f32,
    pub scroll_end: f32,
    pub turn_end: f32,
    pub far: f32,
    pub radius: f32,
}

// The head's own state: what facts/clock-frame-rest.md says the head, overlay, bars and column keep between frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeadState {
    pub grey_ramp: Ramp,
    pub greys: [i32; 3],
    pub vignette_ramp: Ramp,
    pub ring: RingRecord,
    pub tint: Rect,
    pub blur: Rect,
    pub copy: Rect,
    pub fade: Rect,
    pub bars: Rect,
    pub column: Rect,
}

// Read-only inputs, owned by the state lane and the menus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadInputs {
    pub width: i32,
    pub height: i32,
    pub mode: i32,
    pub overlay_level: i32,
    pub level: u32,
    pub counter: i32,
    pub item0: i32,
    pub proportion_x: f32,
    pub proportion_y: f32,
    pub clear_colour: [i32; 4],
    pub tube: TubeConstants,
}

impl Default for HeadInputs {
    fn default() -> Self {
        Self {
            width: 640,
            height: 224,
            mode: 0,
            overlay_level: 0,
            level: 0,
            counter: 0,
            item0: 0,
            proportion_x: 1.0,
            proportion_y: 1.0,
            clear_colour: [0; 4],
            tube: TubeConstants::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Part {
    #[default]
    Background,
    Blur,
    Copy,
    Tint,
    Vignette,
    Fade,
    BlurAfter,
    Bars,
    Column,
    CubeHalf,
    CubeAdded,
    CubeChainShrink,
    CubeChainStretch,
    CubeToDisplay,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Target {
    #[default]
    Display,
    Work0,
    Work1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Source {
    #[default]
    None,
    Background,
    Frame,
    Work0,
    Work1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    Add,
    #[default]
    AlphaOver,
    Subtract,
    Constant,
    Keep,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Topology {
    TriangleStrip,
    #[default]
    Sprite,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Coordinates {
    #[default]
    None,
    St,
    Uv,
}

// Native units: x, y in framebuffer pixels (the GS offset removed, the field not applied); z the 24-bit
// depth value, fog its top byte; colour bytes over 128 (1.0 is 0x80; background colours exceed 1);
// s, t, q as the registers hold them; u, v in texels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HeadVertex {
    pub x: f32,
    pub y: f32,
    pub z: u32,
    pub fog: u8,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub q: f32,
    pub s: f32,
    pub t: f32,
    pub u: f32,
    pub v: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadDraw {
    pub part: Part,
    pub target: Target,
    pub source: Source,
    pub alpha: AlphaMode,
    pub depth_test: i32,
    pub topology: Topology,
    pub textured: bool,
    pub blended: bool,
    pub gouraud: bool,
    pub coordinates: Coordinates,
    pub vertices: Vec<HeadVertex>,
}

impl Default for HeadDraw {
    fn default() -> Self {
        Self {
            part: Part::Background,
            target: Target::Display,
            source: Source::None,
            alpha: AlphaMode::AlphaOver,
            depth_test: 1,
            topology: Topology::Sprite,
            textured: false,
            blended: false,
            gouraud: false,
            coordinates: Coordinates::None,
            vertices: Vec::new(),
        }
    }
}

const BLUR_X: i32 = 0x13f4;
const BLUR_Y: i32 = 0x954;
const STRETCH_Y: i32 = 0x95c;

fn wide(v: i32) -> u64 {
    i64::from(v) as u64
}

fn pair(low: u32, high: u32) -> u64 {
    u64::from(low) | (u64::from(high) << 32)
}

#[derive(Debug, Clone, Copy)]
struct Origin {
    x: f32,
    y: f32,
}

fn origin_of(width: i32, height: i32) -> Origin {
    Origin { x: (0x800 - (width >> 1)) as f32, y: (0x800 - (height >> 1)) as f32 }
}

fn origin_of_inputs(input: &HeadInputs) -> Origin {
    origin_of(input.width, input.height)
}

// One vertex from the registers the OSD wrote: RGBAQ, ST or UV, XYZF2.
fn vertex_of(origin: &Origin, rgbaq: u64, coordinate: u64, kind: Coordinates, xyz: u64) -> HeadVertex {
    let mut v = HeadVertex::default();
    let colour = rgbaq as u32;
    v.r = (colour & 0xff) as f32 / 128.0;
    v.g = ((colour >> 8) & 0xff) as f32 / 128.0;
    v.b = ((colour >> 16) & 0xff) as f32 / 128.0;
    v.a = (colour >> 24) as f32 / 128.0;
    v.q = as_float((rgbaq >> 32) as u32);
    if kind == Coordinates::St {
        v.s = as_float(coordinate as u32);
        v.t = as_float((coordinate >> 32) as u32);
    } else if kind == Coordinates::Uv {
        let uv = coordinate as u32;
        v.u = (uv & 0xffff) as f32 / 16.0;
        v.v = (uv >> 16) as f32 / 16.0;
    }
    let low = xyz as u32;
    let high = (xyz >> 32) as u32;
    v.x = (low & 0xffff) as f32 / 16.0 - origin.x;
    v.y = (low >> 16) as f32 / 16.0 - origin.y;
    v.z = high & 0xffffff;
    v.fog = (high >> 24) as u8;
    v
}

fn set_prim(d: &mut HeadDraw, prim: u32) {
    d.topology = if (prim & 7) == 6 { Topology::Sprite } else { Topology::TriangleStrip };
    d.gouraud = prim & 8 != 0;
    d.textured = prim & 0x10 != 0;
    d.blended = prim & 0x40 != 0;
    d.coordinates = if (prim & 0x100) != 0 {
        Coordinates::Uv
    } else if d.textured {
        Coordinates::St
    } else {
        Coordinates::None
    };
}

fn set_state(d: &mut HeadDraw, target: Target, source: Source, alpha: AlphaMode, depth_test: i32) {
    d.target = target;
    d.source = source;
    d.alpha = alpha;
    d.depth_test = depth_test;
}

fn rectangle(r: &Rect, input: &HeadInputs, part: Part) -> HeadDraw {
    head_rectangle(r, input.width, input.height, part)
}

// facts/clock-frame-rest.md section 2 (HDD func_00236490): shrink into work buffer 1, stretch back.
fn blur_trips(s: &mut HeadState, input: &HeadInputs, trips: i64, part: Part, out: &mut Vec<HeadDraw>) {
    let mut k = 0i64;
    while k < trips {
        let x = BLUR_X - 0x20 * k as i32;
        s.blur.x1 = x;
        s.blur.y1 = BLUR_Y - 0x10 * k as i32;
        s.blur.u1 = (input.width << 4) + 8;
        s.blur.v1 = ((input.height - 1) << 4) + 8;
        let mut shrink = rectangle(&s.blur, input, part);
        set_state(&mut shrink, Target::Work1, Source::Frame, AlphaMode::AlphaOver, 1);
        out.push(shrink);
        s.blur.x1 = input.width << 4;
        s.blur.y1 = (input.height - 1) << 4;
        s.blur.u1 = x + 8;
        s.blur.v1 = STRETCH_Y - 0x10 * k as i32;
        let mut stretch = rectangle(&s.blur, input, part);
        set_state(&mut stretch, Target::Display, Source::Work1, AlphaMode::AlphaOver, 1);
        out.push(stretch);
        k += 1;
    }
}

// HDD func_00236230: what is bound, over the whole target.
fn copy(s: &mut HeadState, input: &HeadInputs, target: Target, out: &mut Vec<HeadDraw>) {
    s.copy.x1 = input.width << 4;
    s.copy.y1 = input.height << 4;
    s.copy.u1 = (input.width << 4) + 8;
    s.copy.v1 = (input.height << 4) + 8;
    s.copy.blend = 0;
    let mut d = rectangle(&s.copy, input, Part::Copy);
    set_state(&mut d, target, Source::Frame, AlphaMode::AlphaOver, 1);
    out.push(d);
}

fn project<A: Arithmetic>(screen: &Mat4, v: &Vec4) -> (Vec4, f32) {
    let mut p = Matrix::<A>::apply(screen, v);
    let q = A::div(1.0, p[3]);
    for c in p.iter_mut() {
        *c = A::mul(*c, q);
    }
    (p, q)
}

fn rgbaq_of(r: i32, g: i32, b: i32, q: f32) -> u64 {
    wide(r) | (u64::from(float_bits(q)) << 32) | (wide(b) << 16) | (wide(g) << 8) | 0x40000000
}

fn coordinate<A: Arithmetic>(u: f32, v: f32, q: f32) -> u64 {
    pair(float_bits(A::mul(A::mul(u, 3.0), q)), float_bits(A::mul(A::mul(v, 3.0), q)))
}

fn xyz_of<A: Arithmetic>(v: &Vec4) -> u64 {
    let x = A::to_int(v[0] * 16.0);
    let y = A::to_int(v[1] * 16.0);
    let z = A::to_int(v[2] * 16.0);
    wide(x) | (wide(y) << 16) | (wide(z) << 32)
}

// facts/clock-frame-rest.md section 1 (HDD func_00233110): 33 ring pairs, those in view kept, and the closing pair.
#[allow(clippy::too_many_arguments)]
fn strip<A: Arithmetic>(view: &Mat4, screen: &Mat4, angle0: i32, angle1: i32, counter: i32, greys: &[i32; 3], k: &TubeConstants, origin: &Origin) -> HeadDraw {
    type M<A> = Matrix<A>;
    let a = s16(angle0);
    let b = s16(angle1);
    let p0 = [A::mul(k.radius, A::sin16(a)), A::mul(k.radius, A::cos16(a)), 0.0, 1.0];
    let p1 = [A::mul(k.radius, A::sin16(b)), A::mul(k.radius, A::cos16(b)), 0.0, 1.0];
    let u0 = A::div(angle0 as f32, k.turn);
    let u1 = A::div(angle1 as f32, k.turn);
    let light_a = A::to_int(A::mul(A::add(A::cos16(a), 1.0), 10.0));
    let light_b = A::to_int(A::mul(A::add(A::cos16(b), 1.0), 10.0));
    let mut d = HeadDraw { part: Part::Background, vertices: Vec::with_capacity(68), ..Default::default() };
    set_prim(&mut d, 0x1c);
    set_state(&mut d, Target::Display, Source::Background, AlphaMode::Add, 2);
    let scrolled = (counter % 5000) as f32;
    let outside = |x: f32| A::sub(x, 2000.0).abs() > 1000.0;
    for i in 0..=32i32 {
        let v = A::add(A::mul(i as f32, 0.03125), A::mul(scrolled, k.scroll));
        let wobble = A::add(A::mul(A::sin16((counter as u32).wrapping_mul(100).wrapping_add((i * 0x1400) as u32) as i32), k.ripple), 1.0);
        let n = 32 - i;
        let fall = (n * n * n) >> 10;
        let c1 = greys[0] + ((0xe6 * fall) >> 5);
        let c2 = greys[1] + ((0x104 * fall) >> 5);
        let c3 = greys[2] + ((0x104 * fall) >> 5);
        let z = (i * 0x4e2 - 0x9c4) as f32;
        let v0 = M::<A>::apply(view, &[A::mul(p0[0], wobble), A::mul(p0[1], wobble), z, p0[3]]);
        let v1 = M::<A>::apply(view, &[A::mul(p1[0], wobble), A::mul(p1[1], wobble), z, p1[3]]);
        if v0[2] < k.near || v1[2] < k.near {
            continue;
        }
        let (s0, q0) = project::<A>(screen, &v0);
        let (s1, q1) = project::<A>(screen, &v1);
        if outside(s0[0]) || outside(s1[0]) || outside(s0[1]) || outside(s1[1]) {
            continue;
        }
        d.vertices.push(vertex_of(
            origin,
            rgbaq_of(c1.wrapping_add(light_a), c2.wrapping_add(light_a), c3.wrapping_add(light_a), q0),
            coordinate::<A>(u0, v, q0),
            Coordinates::St,
            xyz_of::<A>(&s0),
        ));
        d.vertices.push(vertex_of(
            origin,
            rgbaq_of(c1.wrapping_add(light_b), c2.wrapping_add(light_b), c3.wrapping_add(light_b), q1),
            coordinate::<A>(u1, v, q1),
            Coordinates::St,
            xyz_of::<A>(&s1),
        ));
    }
    let v_end = A::add(A::mul(scrolled, k.scroll_end), 1.0);
    let (e0_point, e0) = project::<A>(screen, &M::<A>::apply(view, &[0.0, 0.0, k.far, 1.0]));
    let (e1_point, e1) = project::<A>(screen, &M::<A>::apply(view, &[0.0, 0.0, k.far, 1.0]));
    d.vertices.push(vertex_of(
        origin,
        rgbaq_of(greys[0], greys[1], greys[2], e0),
        coordinate::<A>(A::div(angle0 as f32, k.turn_end), v_end, e0),
        Coordinates::St,
        xyz_of::<A>(&e0_point),
    ));
    d.vertices.push(vertex_of(
        origin,
        rgbaq_of(greys[0], greys[1], greys[2], e0),
        coordinate::<A>(A::div(angle1 as f32, k.turn_end), v_end, e1),
        Coordinates::St,
        xyz_of::<A>(&e1_point),
    ));
    d
}

// HDD func_00233D00: the vignette's sixteen sector draws (its PRIM packet draws nothing).
fn vignette<A: Arithmetic>(r: &RingRecord, input: &HeadInputs, out: &mut Vec<HeadDraw>) {
    let left = (0x800 - (input.width >> 1)) << 4;
    let top = (0x800 - (input.height >> 1)) << 4;
    let x_at = |angle: i32, scale: f32| A::to_int(A::add(A::add(r.cx as f32, A::mul(A::mul(A::sin16(angle), r.rx as f32), scale)), left as f32));
    let y_at = |angle: i32, scale: f32| A::to_int(A::add(A::add(r.cy as f32, A::mul(A::mul(A::cos16(angle), r.ry as f32), scale)), top as f32));
    let clamp = |value: i32, low: i32, high: i32| {
        if value < low {
            low
        } else if high < value {
            high
        } else {
            value
        }
    };
    let colour = |alpha: i32| (wide(alpha) << 24) | (0x3f800000u64 << 32);
    let point = |x: i32, y: i32| wide(x) | (wide(y) << 16) | (wide(r.z) << 32);
    let origin = origin_of_inputs(input);
    let mut angle = 0i32;
    while angle <= 0xffff {
        let next = angle + 0x1000;
        let mut d = HeadDraw { part: Part::Vignette, vertices: Vec::with_capacity(6), ..Default::default() };
        set_prim(&mut d, 0x4c);
        set_state(&mut d, Target::Display, Source::None, AlphaMode::AlphaOver, 2);
        let mut put = |alpha: i32, xyz: u64| d.vertices.push(vertex_of(&origin, colour(alpha), 0, Coordinates::None, xyz));
        put(0, point(x_at(angle, 1.0), y_at(angle, 1.0)));
        put(0, point(x_at(next, 1.0), y_at(next, 1.0)));
        put(r.alpha, point(x_at(angle, 1.5), y_at(angle, 1.5)));
        put(r.alpha, point(x_at(next, 1.5), y_at(next, 1.5)));
        put(r.alpha, point(clamp(x_at(angle, 10.0), left, left + (input.width << 4)), clamp(y_at(angle, 10.0), top, top + (input.height << 4))));
        put(r.alpha, point(clamp(x_at(next, 10.0), left, left + (input.width << 4)), clamp(y_at(next, 10.0), top, top + (input.height << 4))));
        out.push(d);
        angle += 0x1000;
    }
}

// HDD func_00233770: the two packets of a rectangle record, nothing masked.
pub fn head_rectangle(r: &Rect, width: i32, height: i32, part: Part) -> HeadDraw {
    let ox = (0x800 - (width >> 1)) << 4;
    let oy = (0x800 - (height >> 1)) << 4;
    let z = wide(r.z) << 32;
    let prim = ((wide(r.textured) << 4) | 0x100 | (wide(r.blend) << 6) | 6) as u32 & 0x7ff;
    let rgbaq = wide(r.colour[0]) | (0x3f800000u64 << 32) | (wide(r.colour[2]) << 16) | (wide(r.colour[1]) << 8) | (wide(r.colour[3]) << 24);
    let uv0 = wide(r.u0) | (wide(r.v0) << 16);
    let uv1 = wide(r.u1) | (wide(r.v1) << 16);
    let xyz0 = wide(r.x0.wrapping_add(ox)) | (wide(r.y0.wrapping_add(oy)) << 16) | z;
    let xyz1 = wide(r.x1.wrapping_add(ox)) | (wide(r.y1.wrapping_add(oy)) << 16) | z;
    let mut d = HeadDraw { part, vertices: Vec::with_capacity(2), ..Default::default() };
    set_prim(&mut d, prim);
    let origin = origin_of(width, height);
    d.vertices.push(vertex_of(&origin, rgbaq, uv0, Coordinates::Uv, xyz0));
    d.vertices.push(vertex_of(&origin, rgbaq, uv1, Coordinates::Uv, xyz1));
    d
}

// facts/clock-frame-rest.md: head (background, blur trips, copies, tint), overlay, trips after the rods,
// bars, column. Call order of a frame: head, [rods], overlay, tripsAfter, [pages], bars, column.
// The display is cleared with inputs.clearColour before the head's draws.
#[derive(Debug, Clone)]
pub struct FrameHead<A> {
    state: HeadState,
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> FrameHead<A> {
    pub fn new(state: &HeadState) -> Self {
        Self { state: *state, arithmetic: PhantomData }
    }

    pub fn state(&self) -> &HeadState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut HeadState {
        &mut self.state
    }

    // HDD module_clock_226000 and module_clock_233338 (facts/clock-frame-rest.md sections 1 and 2).
    pub fn head(&mut self, input: &HeadInputs, view: &Mat4, screen: &Mat4) -> Vec<HeadDraw> {
        let s = &mut self.state;
        let mut out = Vec::with_capacity(if input.mode == 0 { 16 + 24 } else { 24 });
        s.tint.x1 = input.width << 4;
        s.tint.y1 = input.height << 4;
        s.tint.u1 = (input.width << 4) + 8;
        s.tint.v1 = (input.height << 4) + 8;

        tick_ramp(&mut s.grey_ramp);
        if s.grey_ramp.state != 0 {
            let grey = s.grey_ramp.counter * 0x28 / s.grey_ramp.length;
            s.greys = [grey, grey, grey];
        }
        if input.mode == 0 {
            let origin = origin_of_inputs(input);
            let mut angle = 0i32;
            while angle <= 0xffff {
                out.push(strip::<A>(view, screen, angle, angle + 0x1000, input.counter, &s.greys, &input.tube, &origin));
                angle += 0x1000;
            }
        }

        let trips = if input.level < 6 { i64::from(input.level) } else { 10 - i64::from(input.level) };
        blur_trips(s, input, trips, Part::Blur, &mut out);
        copy(s, input, Target::Work0, &mut out);
        copy(s, input, Target::Work1, &mut out);
        let mut tint = rectangle(&s.tint, input, Part::Tint);
        set_state(&mut tint, Target::Display, Source::Work0, AlphaMode::Add, 1);
        out.push(tint);
        out
    }

    // HDD module_clock_234E70: the vignette ramp, the vignette in mode 0, then the fade.
    pub fn overlay(&mut self, input: &HeadInputs) -> Vec<HeadDraw> {
        let s = &mut self.state;
        let mut out = Vec::new();
        tick_ramp(&mut s.vignette_ramp);
        if input.mode == 0 {
            s.ring.alpha = if s.vignette_ramp.length != 0 { s.vignette_ramp.counter * 0x80 / s.vignette_ramp.length } else { 0 };
            s.ring.cx = 0x10a0;
            s.ring.cy = (input.height / 2) << 4;
            if s.vignette_ramp.state != 0 {
                vignette::<A>(&s.ring, input, &mut out);
            }
        }
        s.fade.colour[3] = 0x80 - input.overlay_level;
        let mut fade = rectangle(&s.fade, input, Part::Fade);
        set_state(&mut fade, Target::Display, Source::None, AlphaMode::AlphaOver, 1);
        out.push(fade);
        out
    }

    // HDD module_clock_232438: level - 5 more trips from level 5 up.
    pub fn trips_after(&mut self, input: &HeadInputs) -> Vec<HeadDraw> {
        let mut out = Vec::new();
        if input.level >= 5 {
            blur_trips(&mut self.state, input, i64::from(input.level) - 5, Part::BlurAfter, &mut out);
        }
        out
    }

    // HDD func_002262C8 / func_00226158: the two letterbox bars when item 0 is 0 or 2.
    pub fn bars(&mut self, input: &HeadInputs) -> Vec<HeadDraw> {
        let s = &mut self.state;
        let mut out = Vec::new();
        if input.item0 != 0 && input.item0 != 2 {
            return out;
        }
        s.bars.x1 = input.width << 4;
        let picture = A::mul(A::mul(A::mul(A::div(input.width as f32, input.proportion_x), 0.0625), 9.0), input.proportion_y);
        let margin = A::mul(A::sub(input.height as f32, picture), 0.5);
        s.bars.y0 = 0;
        s.bars.y1 = A::to_int(A::mul(margin, 16.0));
        let mut top = rectangle(&s.bars, input, Part::Bars);
        set_state(&mut top, Target::Display, Source::None, AlphaMode::AlphaOver, 1);
        out.push(top);
        s.bars.y0 = A::to_int(A::mul(A::sub(input.height as f32, margin), 16.0));
        s.bars.y1 = input.height << 4;
        let mut bottom = rectangle(&s.bars, input, Part::Bars);
        set_state(&mut bottom, Target::Display, Source::None, AlphaMode::AlphaOver, 1);
        out.push(bottom);
        out
    }

    // HDD func_00226A88: two pixels at the right edge.
    pub fn column(&mut self, input: &HeadInputs) -> Vec<HeadDraw> {
        let s = &mut self.state;
        s.column.x0 = (input.width << 4) - 0x28;
        s.column.y0 = 0;
        s.column.x1 = (input.width << 4) - 8;
        s.column.y1 = input.height << 4;
        let mut d = rectangle(&s.column, input, Part::Column);
        set_state(&mut d, Target::Display, Source::None, AlphaMode::AlphaOver, 1);
        vec![d]
    }
}
