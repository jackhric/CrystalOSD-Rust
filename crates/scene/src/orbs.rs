use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, as_float, s16};
use crate::matrix::{Mat4, Matrix};
use crate::ramp::{Ramp, tick_ramp};

pub const ORB_COUNT: usize = 7;
pub const RING_LENGTH: i32 = 50;

// One ring entry: the orb's screen position relative to the screen centre (x, y) and its depth,
// then its colour as the OSD holds it (0..255 ints, alpha 0x80 = 1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrbEntry {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub colour: [i32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbRing {
    pub head: i32,
    pub count: i32,
    pub full: i32,
    pub entries: [OrbEntry; RING_LENGTH as usize],
}

impl Default for OrbRing {
    fn default() -> Self {
        Self { head: 0, count: 0, full: 0, entries: [OrbEntry::default(); RING_LENGTH as usize] }
    }
}

// What the orbs carry from frame to frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrbState {
    pub rings: [OrbRing; ORB_COUNT],
    pub sprite_fade: Ramp,
    pub fraction: f32,
}

// What the orbs read of the rest of the clock at the frame's start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbInputs {
    pub milliseconds: f32,
    pub seconds: i32,
    pub minutes: i32,
    pub second_hand: i32,
    pub hour_hand: i32,
    pub progress: f32,
    pub minute_factor: f32,
    pub fraction_easing: f32,
    pub scene_scale: f32,
    pub colour: [i32; 4],
    pub width: i32,
    pub height: i32,
    pub mode: i32,
    pub overlay_level: i32,
    pub has_entry_data: bool,
    pub wide: i32,
    pub random: [i32; ORB_COUNT],
    pub colours: [[i32; 4]; ORB_COUNT],
}

impl Default for OrbInputs {
    fn default() -> Self {
        Self {
            milliseconds: 0.0,
            seconds: 0,
            minutes: 0,
            second_hand: 0,
            hour_hand: 0,
            progress: 0.0,
            minute_factor: 0.0,
            fraction_easing: 0.0,
            scene_scale: 0.0,
            colour: [0; 4],
            width: 640,
            height: 224,
            mode: 0,
            overlay_level: 0,
            has_entry_data: false,
            wide: 0,
            random: [0; ORB_COUNT],
            colours: [[0; 4]; ORB_COUNT],
        }
    }
}

// Native units: x, y in pixels from the screen's top-left corner (the GS value less the screen
// corner's offset, over 16); z in the screen matrix's depth units (the GS z over 16); u, v
// normalized; trail colour 0..1 (over 255), sprite colour a texture multiplier (over 128), alpha
// over 128.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbVertex {
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

impl Default for OrbVertex {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, z: 0.0, u: 0.0, v: 0.0, q: 1.0, r: 0.0, g: 0.0, b: 0.0, a: 0.0 }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrbTrail {
    pub points: Vec<OrbVertex>,
    pub header_alpha: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OrbSprite {
    pub first: OrbVertex,
    pub second: OrbVertex,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrbSend {
    pub trail: OrbTrail,
    pub glow: OrbSprite,
    pub disc: OrbSprite,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrbDraw {
    pub k: i32,
    pub drawn: i32,
    pub depth_key: f32,
    pub local: Mat4,
    pub cx: f32,
    pub cy: f32,
    pub cz: f32,
    pub sends: [OrbSend; 2],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrbFrame {
    pub orbs: [OrbDraw; ORB_COUNT],
    pub order: [i32; ORB_COUNT],
}

const DEPTH_TO_SIZE: u32 = 0x36da1a93;

// JS `(value < 0 ? value + (1 << by) - 1 : value) >> by`: a division by 2^by toward zero.
fn shift(value: i32, by: i32) -> i32 {
    (if value < 0 { value + (1 << by) - 1 } else { value }) >> by
}

fn imul(a: i32, b: i32) -> i32 {
    (a as u32).wrapping_mul(b as u32) as i32
}

fn faded(ramp: &Ramp, alpha: i32) -> i32 {
    if ramp.length == 0 { 0 } else { ramp.counter.wrapping_mul(alpha) / ramp.length }
}

// facts/clock-orbs.md, facts/clock-camera.md: the orbits, the rings, the trails and the sprites.
pub struct Orbs<A>(PhantomData<A>);

impl<A: Arithmetic> Orbs<A> {
    // facts/clock-camera.md: the orbit of orb k (clock_frame.mjs orbMatrix).
    pub fn orbit_matrix(k: i32, hour_hand: i32, second_hand: i32, minute_turn: f32, seconds_turn: f32, factor: f32, radius: f32) -> Mat4 {
        type M<A> = Matrix<A>;
        let mut m = M::<A>::rotate_z(&M::<A>::rotate_y(&M::<A>::rotate_z(&M::<A>::identity(), hour_hand), second_hand), -0x8000);
        m = M::<A>::rotate_y(&m, s16(A::to_int(A::mul(minute_turn, factor))));
        m = M::<A>::rotate_x(&m, s16(A::to_int(A::mul((k + 0x15) as f32, seconds_turn))));
        m = M::<A>::move_(&m, 0.0, radius, 0.0);
        M::<A>::rotate_y(&m, 0x2000)
    }

    // facts/clock-orbs.md: the orb part of HDD module_clock_22FE98 (clock_frame.mjs scene, drawOrb).
    pub fn frame(state: &mut OrbState, input: &OrbInputs, view: &Mat4, screen: &Mat4) -> OrbFrame {
        type M<A> = Matrix<A>;
        let mut out = OrbFrame::default();

        let seconds = A::add(input.seconds as f32, A::div(input.milliseconds, 1000.0));
        let minutes = A::add(input.minutes as f32, A::div(seconds, 60.0));
        let seconds_turn = A::div(A::mul(seconds, 65536.0), 60.0);
        let minute_turn = A::div(A::mul(minutes, 65536.0), 60.0);
        let fraction = state.fraction;
        let radius = A::mul(A::add(A::mul(fraction, 7.25), 10.0), input.scene_scale);
        state.fraction = A::add(A::mul(A::sub(A::sub(1.0, input.progress), fraction), input.fraction_easing), fraction);

        for (listed, k) in (0..ORB_COUNT).enumerate() {
            let orb = &mut out.orbs[k];
            orb.k = k as i32;
            orb.local = Self::orbit_matrix(k as i32, input.hour_hand, input.second_hand, minute_turn, seconds_turn, input.minute_factor, radius);
            orb.depth_key = M::<A>::apply(view, &orb.local[3])[2];
            let key = orb.depth_key;
            let mut at = 0usize;
            while at < listed && key < out.orbs[out.order[at] as usize].depth_key {
                at += 1;
            }
            let mut n = listed;
            while n > at {
                out.order[n] = out.order[n - 1];
                n -= 1;
            }
            out.order[at] = k as i32;
        }

        let half_w = input.width / 2;
        let half_h = input.height / 2;
        let corner_x = ((0x800 - (input.width >> 1)) << 4) as f32;
        let corner_y = ((0x800 - (input.height >> 1)) << 4) as f32;
        let pixel_x = |gs: i32| (gs as f32 - corner_x) * 0.0625;
        let pixel_y = |gs: i32| (gs as f32 - corner_y) * 0.0625;

        for drawn in 0..ORB_COUNT {
            let orb = &mut out.orbs[out.order[drawn] as usize];
            orb.drawn = drawn as i32;
            tick_ramp(&mut state.sprite_fade);

            let mut placed = [[0.0f32; 4]; 4];
            for (row, local) in placed.iter_mut().zip(&orb.local) {
                *row = M::<A>::apply(view, local);
            }
            let mut centre = M::<A>::apply(screen, &M::<A>::apply(&placed, &[0.0, 0.0, 0.0, 1.0]));
            let q = A::div(1.0, centre[3]);
            for c in centre.iter_mut() {
                *c = A::mul(*c, q);
            }
            orb.cx = A::sub(centre[0], 2048.0);
            orb.cy = A::sub(centre[1], 2048.0);
            orb.cz = centre[2];

            let ring = &mut state.rings[orb.k as usize];
            let mut px = orb.cx;
            let mut py = orb.cy;
            let mut colour = input.colour;
            if input.has_entry_data && (input.mode == 2 || input.mode == 3) {
                let weight = input.overlay_level;
                let k = orb.k;
                if (input.mode == 2 && (input.wide == 1 || k == 0)) || (input.mode == 3 && k == 0) {
                    let angle = s16(A::to_int(A::mul((weight << 14) as f32, 0.0078125)));
                    let turn = s16(input.random[k as usize]);
                    let rest = A::sub(1.0, A::sin16(angle));
                    let dx = A::mul(A::mul(half_w as f32, A::cos16(turn)), rest);
                    let dy = A::mul(A::mul(half_h as f32, A::sin16(turn)), rest);
                    if input.mode == 2 {
                        px = A::add(px, dx);
                        py = A::add(py, dy);
                    } else {
                        let w = A::mul(weight as f32, 0.0078125);
                        py = A::add(A::mul(py, w), A::mul(dy, 1.5));
                        px = A::add(A::mul(px, w), A::mul(dx, 1.5));
                    }
                }
                let own = input.mode == 2 || k == 0;
                let mine = if own { 128 - weight } else { 0 };
                let other = if own { weight } else { 128 };
                let base = &input.colours[if input.mode == 3 { 0 } else { k as usize }];
                for i in 0..4 {
                    colour[i] = imul(base[i], mine).wrapping_add(imul(input.colour[i], other)) >> 7;
                }
            }
            let entry = OrbEntry { x: px, y: py, z: orb.cz, colour };
            ring.entries[ring.head as usize] = entry;
            ring.count += 1;
            if ring.count == 3 {
                ring.count = 0;
                ring.head += 1;
                if ring.head == RING_LENGTH {
                    ring.head = 0;
                    ring.full = 1;
                }
                ring.entries[ring.head as usize] = entry;
            }

            let mut trail = OrbTrail { points: Vec::new(), header_alpha: faded(&state.sprite_fade, 0x80) as f32 / 128.0 };
            let full = ring.full != 0;
            let points = if full {
                RING_LENGTH - 1
            } else if ring.head > 1 {
                ring.head - 1
            } else {
                0
            };
            trail.points.reserve(points as usize);
            for i in 0..points {
                let e = &ring.entries[(if full { (ring.head + 100 - i) % RING_LENGTH } else { ring.head - i }) as usize];
                let step = if full { i } else { (i * 50) / (ring.head - 1) };
                let mut t = A::to_int(A::sub(128.0, A::cut(f64::from(step) * 3.0)));
                if t < 0 {
                    t = 0;
                }
                let red = shift(imul(shift(imul(shift(imul(imul(e.colour[0], t), t), 7), t), 7), t), 14);
                let green = shift(imul(imul(e.colour[1], t), t), 14);
                let blue = shift(imul(e.colour[2], t), 7);
                trail.points.push(OrbVertex {
                    x: pixel_x(A::to_int(A::mul(A::add(e.x, 2048.0), 16.0))),
                    y: pixel_y(A::to_int(A::mul(A::add(e.y, 2048.0), 16.0))),
                    z: A::to_int(A::mul(e.z, 16.0)) as f32 * 0.0625,
                    r: red as f32 / 255.0,
                    g: green as f32 / 255.0,
                    b: blue as f32 / 255.0,
                    a: (t >> 1) as f32 / 128.0,
                    ..Default::default()
                });
            }

            let head = ring.entries[ring.head as usize];
            let size = A::mul(head.z, as_float(DEPTH_TO_SIZE));
            let rect = |factor: f32, c: [i32; 4]| {
                let hw = A::mul(size, factor);
                let hh = A::mul(hw, 0.5);
                let at = |centre: f32, delta: f32, middle: i32| A::to_int(A::mul(A::add(A::add(centre, delta), middle as f32), 16.0)) as f32 * 0.0625;
                let mut s = OrbSprite {
                    first: OrbVertex { x: at(head.x, -hw, half_w), y: at(head.y, -hh, half_h), z: 0.0, u: 0.0, v: 0.0, q: 1.0, ..Default::default() },
                    second: OrbVertex {
                        x: at(head.x, hw, half_w),
                        y: at(head.y, hh, half_h),
                        z: 0.0,
                        u: 63.0 / 64.0,
                        v: 63.0 / 64.0,
                        q: 1.0,
                        ..Default::default()
                    },
                };
                for v in [&mut s.first, &mut s.second] {
                    v.r = c[0] as f32 / 128.0;
                    v.g = c[1] as f32 / 128.0;
                    v.b = c[2] as f32 / 128.0;
                    v.a = c[3] as f32 / 128.0;
                }
                s
            };
            let [r, g, b, a] = head.colour;
            let shown = faded(&state.sprite_fade, 0x80);
            orb.sends[0] =
                OrbSend { trail: trail.clone(), glow: rect(30.0, [r, g, b, faded(&state.sprite_fade, a)]), disc: rect(4.5, [0x80, 0x80, 0x80, shown]) };
            orb.sends[1] = OrbSend { trail, glow: rect(30.0, [r, g, b, shown]), disc: rect(4.5, [0xff, 0xff, 0xff, shown]) };
        }
        out
    }
}
