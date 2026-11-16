mod common;

use std::path::Path;

use common::{Hash, Rng, f, resources};
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic, TruncatingArithmetic};
use scene::camera::Camera;
use scene::clock_date::{date_check, date_of, seconds_of};
use scene::clock_state::{ClockLogic, ClockState, Colour, RodState};
use scene::console_clock::{local_of, rtc_at, rtc_of, unix_of, zone_of};
use scene::elf_records::{colour_at, mat4_at, ramp_at, rect_at, rod_record_at, vec4_at};
use scene::font::Font;
use scene::frame_head::{FrameHead, HeadDraw, HeadInputs, HeadState, Rect, RingRecord, TubeConstants};
use scene::matrix::{Mat4, Matrix, Vec4};
use scene::orbs::{ORB_COUNT, OrbFrame, OrbInputs, OrbState, OrbVertex, Orbs};
use scene::program_image::ProgramImage;
use scene::ramp::{Ramp, tick_ramp};
use scene::rods::{RodLight, RodMesh, RodRecord, RodSend, RodTransform, Rods, RodsFrame, RodsInput};

// FNV-1a 64 over every output field, from the C++ at commit 039cc58 (Source/Scene, -O2 -ffp-contract=off -ffp-model=strict)
// driven exactly as below: HDD OSD 1.10U's hddosd.elf and FNTOSD, the generators of tests/common. Each world hash is
// over the per-frame hashes of Camera, FrameHead (head, overlay, tripsAfter, bars, column, state), Rods (frame and
// template), Orbs (frame and state) and the ClockState after ClockLogic::step. "truncating guarded" is
// NativeArithmetic::hours/seconds/angles/colours/scale run under EeRounding.
const EXPECTED: [(&str, u64, u64); 19] = [
    ("dates", 9200000, 0x45693bdaa7e47e57),
    ("program", 23082, 0xd1c00d1d85312a5a),
    ("font", 148188, 0x456a663f236ea518),
    ("ee matrices", 2400000, 0xb52e1154edb21cfe),
    ("native matrices", 2400000, 0xaa616ff2d529e4ce),
    ("ee guarded", 51400000, 0x53930cb61b0ca16f),
    ("truncating guarded", 51400000, 0x400de3cff798d728),
    ("ee world0", 900, 0x9debbbe6e80a0bc6),
    ("ee world1", 400, 0xab26b6a47e98fd06),
    ("ee world2", 400, 0xf7092519015dc69d),
    ("ee world3", 400, 0xd4e3deb34576b4fe),
    ("ee world4", 300, 0x1bde312bbb5394b9),
    ("ee world5", 300, 0x9b1290d8517bd486),
    ("native world0", 900, 0xd1430b429a6a7a67),
    ("native world1", 400, 0x13cc9714baeaf560),
    ("native world2", 400, 0x0bd7f90af3205b7f),
    ("native world3", 400, 0x206814bdee015013),
    ("native world4", 300, 0x991ba5218fef6a1a),
    ("native world5", 300, 0xed136b8e228b3786),
];

fn expect(name: &str, h: &Hash) {
    let (_, count, hash) = EXPECTED.iter().find(|e| e.0 == name).unwrap();
    assert_eq!((h.count, h.h), (*count, *hash), "{name}");
}

fn h4(h: &mut Hash, v: &Vec4) {
    for x in v {
        h.f32(*x);
    }
}
fn hm(h: &mut Hash, m: &Mat4) {
    for r in m {
        h4(h, r);
    }
}
fn hc(h: &mut Hash, c: &Colour) {
    for x in c {
        h.i32(*x);
    }
}
fn hr(h: &mut Hash, r: &Ramp) {
    for x in [r.length, r.counter, r.changed, r.state] {
        h.i32(x);
    }
}
fn hrect(h: &mut Hash, r: &Rect) {
    hc(h, &r.colour);
    for x in [r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured] {
        h.i32(x);
    }
}

fn hclock(h: &mut Hash, s: &ClockState) {
    h.f32(s.time.milliseconds);
    h.i32(s.time.seconds);
    h.i32(s.time.minutes);
    h.i32(s.time.hours);
    h.i32(s.eased.second_hand.into());
    h.i32(s.eased.hour_hand.into());
    h.f32(s.eased.progress);
    h.f32(s.eased.fraction);
    h.i32(s.state.current_rod);
    h.i32(s.state.seconds_angle.into());
    h.i32(s.state.rod_angle.into());
    for r in &s.state.rods {
        h.f32(r.appearance);
        h.f32(r.progress);
        hc(h, &r.base);
        hc(h, &r.reflection);
    }
    hc(h, &s.state.accent);
    hc(h, &s.state.fourth);
    h.f32(s.state.progress_target);
    hc(h, &s.state.accent_frame);
    hc(h, &s.state.current_target);
    hr(h, &s.appearance);
    for c in [&s.colours.base, &s.colours.reflection, &s.colours.fourth, &s.colours.current_reflection] {
        hc(h, c);
    }
    let cc = &s.cycle_counters;
    for x in [cc.base.index, cc.base.wait, cc.accent.index, cc.accent.wait, cc.last] {
        h.i32(x);
    }
    for c in s.cycle_tables.base.iter().chain(&s.cycle_tables.accent) {
        hc(h, c);
    }
    h.f32(s.logic_constants.rod_easing);
    h.f32(s.logic_constants.seconds_easing);
    h.f32(s.logic_constants.progress_step);
    h.f32(s.scene.scale);
    h.i32(s.scene.leaving);
    h.i32(s.scene.field);
    h.f32(s.scale_target);
    h.f32(s.scale_factor);
    h.i32(s.time_filled);
    h.boolean(s.spin.is_some());
    h.u32(u32::from(s.spin.unwrap_or(0)));
    h.i32(s.mode);
    h.i32(s.level);
    h.i32(s.overlay_level);
    hr(h, &s.vignette_ramp);
    h.i32(s.vignette_length);
    hr(h, &s.menu_ramp);
    h.i32(s.tail);
    h.i32(s.counter);
    h.f32(s.proportions.ax);
    h.f32(s.proportions.ay);
    for v in [&s.position, &s.direction, &s.up, &s.rotation] {
        h4(h, v);
    }
    h.f32(s.camera_offset);
    h.f32(s.zmax);
    h.f32(s.camera_factor);
}

fn hhead(h: &mut Hash, s: &HeadState) {
    hr(h, &s.grey_ramp);
    for g in s.greys {
        h.i32(g);
    }
    hr(h, &s.vignette_ramp);
    for x in [s.ring.alpha, s.ring.cx, s.ring.cy, s.ring.rx, s.ring.ry, s.ring.z] {
        h.i32(x);
    }
    for r in [&s.tint, &s.blur, &s.copy, &s.fade, &s.bars, &s.column] {
        hrect(h, r);
    }
}

#[derive(Default, Debug)]
struct Coverage {
    strips: usize,
    vignettes: usize,
    bars: usize,
    blur_after: usize,
    rods_drawn: usize,
    split_rods: usize,
    trail_points: usize,
    moved_orbs: usize,
}

fn hdraws(h: &mut Hash, draws: &[HeadDraw], cover: &mut Coverage) {
    h.u32(draws.len() as u32);
    for d in draws {
        match d.part {
            scene::frame_head::Part::Background => cover.strips += 1,
            scene::frame_head::Part::Vignette => cover.vignettes += 1,
            scene::frame_head::Part::Bars => cover.bars += 1,
            scene::frame_head::Part::BlurAfter => cover.blur_after += 1,
            _ => {}
        }
        for x in [
            d.part as i32,
            d.target as i32,
            d.source as i32,
            d.alpha as i32,
            d.depth_test,
            d.topology as i32,
            d.textured as i32,
            d.blended as i32,
            d.gouraud as i32,
            d.coordinates as i32,
        ] {
            h.i32(x);
        }
        h.u32(d.vertices.len() as u32);
        for v in &d.vertices {
            h.f32(v.x);
            h.f32(v.y);
            h.u32(v.z);
            h.u32(v.fog.into());
            for x in [v.r, v.g, v.b, v.a, v.q, v.s, v.t, v.u, v.v] {
                h.f32(x);
            }
        }
    }
}

fn hrecord(h: &mut Hash, r: &RodRecord) {
    h.i32(r.number);
    h.i32(r.faces);
    hm(h, &r.local);
    h.f32(r.sx);
    h.f32(r.sy);
    h.f32(r.sz);
    hc(h, &r.base);
    h.f32(r.strength);
    hc(h, &r.textured);
    h.f32(r.pair[0]);
    h.f32(r.pair[1]);
    h.f32(r.refraction);
    hc(h, &r.reflection);
    hc(h, &r.extra);
}

fn htransform(h: &mut Hash, t: &RodTransform) {
    h.f32(t.cx);
    h.f32(t.cy);
    h.f32(t.cz);
    h.u32(t.faces.len() as u32);
    for face in &t.faces {
        h.i32(face.index);
        h4(h, &face.normal);
        h.i32(face.flag);
        for v in &face.vertices {
            h4(h, &v.eye);
            h4(h, &v.screen);
            h.f32(v.q);
            h.f32(v.s);
            h.f32(v.t);
            for x in v.fixed {
                h.i32(x);
            }
        }
    }
}

fn hsend(h: &mut Hash, s: &RodSend) {
    h.i32(s.kind as i32);
    h.u32(s.faces.len() as u32);
    for d in &s.faces {
        h.i32(d.face);
        h.i32(d.piece as i32);
        h.boolean(d.edge_smoothing);
        for v in &d.strip {
            for x in [v.x, v.y, v.z, v.u, v.v, v.q, v.r, v.g, v.b, v.a] {
                h.f32(x);
            }
        }
    }
}

fn hrods(h: &mut Hash, frame: &RodsFrame, cover: &mut Coverage) {
    h.u32(frame.list.len() as u32);
    for n in &frame.list {
        h.boolean(n.orb);
        h.i32(n.index);
        h.f32(n.key);
    }
    h.u32(frame.rods.len() as u32);
    for r in &frame.rods {
        cover.rods_drawn += usize::from(r.drawn);
        cover.split_rods += usize::from(!r.pieces.is_empty());
        h.i32(r.placement);
        hrecord(h, &r.record);
        h.f32(r.t);
        h.f32(r.key);
        hm(h, &r.matrix);
        h.boolean(r.drawn);
        htransform(h, &r.whole);
        h.f32(r.cx);
        h.f32(r.cy);
        h.u32(r.pieces.len() as u32);
        for p in &r.pieces {
            h.i32(p.piece as i32);
            hrecord(h, &p.record);
            htransform(h, &p.transform);
        }
        h.u32(r.sends.len() as u32);
        for s in &r.sends {
            hsend(h, s);
        }
    }
    for pass in &frame.extra_passes {
        h.u32(pass.len() as u32);
        for d in pass {
            h.u64(d.rod as u64);
            h.boolean(d.split);
            h.i32(d.grain_texture);
            hsend(h, &d.reflection);
            hsend(h, &d.grain);
        }
    }
}

fn hov(h: &mut Hash, v: &OrbVertex) {
    for x in [v.x, v.y, v.z, v.u, v.v, v.q, v.r, v.g, v.b, v.a] {
        h.f32(x);
    }
}

fn horbs(h: &mut Hash, frame: &OrbFrame, s: &OrbState, cover: &mut Coverage) {
    for o in &frame.orbs {
        h.i32(o.k);
        h.i32(o.drawn);
        h.f32(o.depth_key);
        hm(h, &o.local);
        h.f32(o.cx);
        h.f32(o.cy);
        h.f32(o.cz);
        for send in &o.sends {
            cover.trail_points += send.trail.points.len();
            h.u32(send.trail.points.len() as u32);
            for v in &send.trail.points {
                hov(h, v);
            }
            h.f32(send.trail.header_alpha);
            for v in [&send.glow.first, &send.glow.second, &send.disc.first, &send.disc.second] {
                hov(h, v);
            }
        }
    }
    for x in frame.order {
        h.i32(x);
    }
    for r in &s.rings {
        h.i32(r.head);
        h.i32(r.count);
        h.i32(r.full);
        for e in &r.entries {
            h.f32(e.x);
            h.f32(e.y);
            h.f32(e.z);
            hc(h, &e.colour);
        }
    }
    hr(h, &s.sprite_fade);
    h.f32(s.fraction);
}

fn program(folder: &Path) -> ProgramImage {
    ProgramImage::load(&folder.join("hddosd.elf")).unwrap()
}

fn mesh_of(p: &ProgramImage) -> RodMesh {
    RodMesh {
        positions: (0..64).map(|i| vec4_at(p, 0x002b4b90 + 16 * i)).collect(),
        normals: (0..16).map(|i| vec4_at(p, 0x002b5390 + 16 * i)).collect(),
        coordinates: (0..64).map(|i| vec4_at(p, 0x002b4f90 + 16 * i)).collect(),
    }
}

struct Scenario {
    mode: i32,
    wide: i32,
    item0: i32,
    pal: bool,
    seed: u64,
    frames: i32,
    jumps: bool,
}

const SCENARIOS: [Scenario; 6] = [
    Scenario { mode: 0, wide: 0, item0: 0, pal: false, seed: 1, frames: 900, jumps: false },
    Scenario { mode: 2, wide: 0, item0: 2, pal: false, seed: 2, frames: 400, jumps: false },
    Scenario { mode: 2, wide: 1, item0: 0, pal: true, seed: 3, frames: 400, jumps: false },
    Scenario { mode: 3, wide: 0, item0: 1, pal: false, seed: 4, frames: 400, jumps: false },
    Scenario { mode: 1, wide: 0, item0: 0, pal: true, seed: 5, frames: 300, jumps: true },
    Scenario { mode: 0, wide: 1, item0: 2, pal: false, seed: 6, frames: 300, jumps: true },
];

fn clock_of(p: &ProgramImage, sc: &Scenario, r: &mut Rng) -> ClockState {
    let mut s = ClockState::default();
    s.logic_constants.rod_easing = p.single(0x36FBCC);
    s.logic_constants.seconds_easing = p.single(0x36FBCC + 4);
    s.logic_constants.progress_step = p.single(0x36FBCC + 8);
    for i in 0..6 {
        s.cycle_tables.base[i] = colour_at(p, 0x2B5570 + 16 * i as u32);
    }
    for i in 0..3 {
        s.cycle_tables.accent[i] = colour_at(p, 0x2B5570 + 0x60 + 16 * i as u32);
    }
    s.colours.base = colour_at(p, 0x2B5600);
    s.colours.reflection = colour_at(p, 0x2B5610);
    s.colours.fourth = colour_at(p, 0x2B5620);
    s.colours.current_reflection = colour_at(p, 0x2B5630);
    s.eased.second_hand = r.next32() as i16;
    s.eased.hour_hand = r.next32() as i16;
    s.eased.progress = (r.next32() % 1024) as f32 / 1024.0;
    s.eased.fraction = (r.next32() % 1024) as f32 / 1024.0;
    for rod in s.state.rods.iter_mut() {
        *rod = RodState { appearance: 0.0, progress: 0.0, base: colour_at(p, 0x2B5650), reflection: colour_at(p, 0x2B5650) };
    }
    s.state.seconds_angle = r.next32() as i16;
    s.state.rod_angle = r.next32() as i16;
    s.state.accent = colour_at(p, 0x2B5660);
    s.state.fourth = colour_at(p, 0x2B5660);
    s.appearance = Ramp { length: 60, counter: 0, changed: 0, state: 1 };
    s.scale_target = p.single(0x370294);
    s.scale_factor = p.single(0x36FC10);
    s.time_filled = 1;
    s.spin = Some(r.next32() as u16);
    s.mode = sc.mode;
    s.overlay_level = if sc.mode == 3 { 0x80 } else { 0 };
    s.vignette_ramp = Ramp { length: 30, counter: 0, changed: 0, state: 0 };
    s.vignette_length = 30;
    s.menu_ramp = Ramp { length: 40, counter: 0, changed: 0, state: 1 };
    s.tail = 10;
    s.proportions.ax = 1.0;
    s.proportions.ay = p.single(if sc.pal { 0x36FB84 } else { 0x36FB88 });
    s.position = vec4_at(p, 0x2B2190);
    s.direction = vec4_at(p, 0x2B21A0);
    s.up = vec4_at(p, 0x2B21B0);
    s.rotation = vec4_at(p, 0x2B21C0);
    s.camera_offset = f(0xC2C80000);
    s.zmax = p.single(0x36FB8C);
    s.camera_factor = p.single(0x36FB90);
    s
}

fn head_of(p: &ProgramImage) -> HeadState {
    HeadState {
        grey_ramp: Ramp { length: 40, counter: 0, changed: 0, state: 1 },
        greys: [p.integer(0x370AA8), p.integer(0x370AAC), p.integer(0x370AB0)],
        vignette_ramp: Ramp { length: 30, counter: 0, changed: 0, state: 1 },
        ring: RingRecord {
            alpha: p.integer(0x2B5F70),
            cx: p.integer(0x2B5F74),
            cy: p.integer(0x2B5F78),
            rx: p.integer(0x2B5F7C),
            ry: p.integer(0x2B5F80),
            z: p.integer(0x2B5F84),
        },
        tint: rect_at(p, 0x2B21E0),
        blur: rect_at(p, 0x2B6120),
        copy: rect_at(p, 0x2B6060),
        fade: rect_at(p, 0x2B5F30),
        bars: rect_at(p, 0x2B2220),
        column: rect_at(p, 0x2B2500),
    }
}

fn world<A: Arithmetic>(p: &ProgramImage, sc: &Scenario, cover: &mut Coverage) -> Hash {
    let mut r = Rng::new(sc.seed);
    let mut s = clock_of(p, sc, &mut r);
    let mut head = FrameHead::<A>::new(&head_of(p));
    let mut rods = Rods::<A>::new(mesh_of(p), &rod_record_at(p, 0x2B5490));
    let t = |o: u32| p.single(0x36FC14 + o);
    let tube = TubeConstants { near: t(0), turn: t(4), scroll: t(8), ripple: t(12), scroll_end: t(16), turn_end: t(20), far: t(24), radius: t(28) };
    let mut orb_state = OrbState { sprite_fade: Ramp { length: 256, counter: 0, changed: 0, state: 1 }, fraction: s.eased.fraction, ..Default::default() };
    let mut random = [0i32; ORB_COUNT];
    for a in random.iter_mut() {
        *a = (r.next32() % 65536) as i32;
    }
    let orb_colours: [[i32; 4]; ORB_COUNT] = std::array::from_fn(|k| colour_at(p, 0x2B5680 + 16 * k as u32));
    let width = 640;
    let height = if sc.pal { 512 } else { 448 };
    let mut micro = u64::from(r.next32()) * 1000;
    let mut all = Hash::default();
    for frame in 0..sc.frames {
        let mut h = Hash::default();
        micro += if sc.jumps { u64::from(r.next32() % 4000000000) } else { 16683 };
        let ms = micro / 1000;
        s.time.milliseconds = (micro % 1000000) as f32 / 1000.0;
        s.time.seconds = (ms / 1000 % 60) as i32;
        s.time.minutes = (ms / 60000 % 60) as i32;
        s.time.hours = (ms / 3600000 % 24) as i32;
        s.scene.field = frame & 1;
        tick_ramp(&mut s.menu_ramp);
        let cam = Camera::<A>::matrices(&mut s);
        hm(&mut h, &cam.view);
        hm(&mut h, &cam.screen);
        let input = HeadInputs {
            width,
            height,
            mode: s.mode,
            overlay_level: s.overlay_level,
            level: s.level as u32,
            counter: s.counter,
            item0: sc.item0,
            proportion_x: s.proportions.ax,
            proportion_y: s.proportions.ay,
            tube,
            ..Default::default()
        };
        hdraws(&mut h, &head.head(&input, &cam.view, &cam.screen), cover);
        let ri = RodsInput {
            current_rod: s.state.current_rod,
            seconds_angle: s.state.seconds_angle.into(),
            rod_angle: s.state.rod_angle.into(),
            rods: s.state.rods.map(|r| RodLight { appearance: r.appearance, progress: r.progress, base: r.base, reflection: r.reflection }),
            accent: s.state.accent,
            fourth: s.state.fourth,
            width,
            height,
            field: s.scene.field,
        };
        hrods(&mut h, &rods.frame(&ri, &cam.view, &cam.screen), cover);
        hrecord(&mut h, rods.rod_template());
        let oi = OrbInputs {
            milliseconds: s.time.milliseconds,
            seconds: s.time.seconds,
            minutes: s.time.minutes,
            second_hand: s.eased.second_hand.into(),
            hour_hand: s.eased.hour_hand.into(),
            progress: s.eased.progress,
            minute_factor: p.single(0x36FBDC),
            fraction_easing: p.single(0x36FBE0),
            scene_scale: s.scene.scale,
            colour: colour_at(p, 0x2B5670),
            width,
            height,
            mode: s.mode,
            overlay_level: s.overlay_level,
            has_entry_data: true,
            wide: sc.wide,
            random,
            colours: orb_colours,
        };
        let of = Orbs::<A>::frame(&mut orb_state, &oi, &cam.view, &cam.screen);
        if s.mode == 2 || s.mode == 3 {
            cover.moved_orbs += 1;
        }
        horbs(&mut h, &of, &orb_state, cover);
        hdraws(&mut h, &head.overlay(&input), cover);
        hdraws(&mut h, &head.trips_after(&input), cover);
        hdraws(&mut h, &head.bars(&input), cover);
        hdraws(&mut h, &head.column(&input), cover);
        hhead(&mut h, head.state());
        ClockLogic::<A>::step(&mut s);
        hclock(&mut h, &s);
        all.u64(h.h);
        if std::env::var_os("PROBE_FRAMES").is_some() {
            println!("frame {frame} 0x{:016x}", h.h);
        }
    }
    all
}

fn worlds<A: Arithmetic>(tag: &str) {
    let Some(folder) = resources() else { return };
    let p = program(&folder);
    let mut cover = Coverage::default();
    for (i, sc) in SCENARIOS.iter().enumerate() {
        expect(&format!("{tag} world{i}"), &world::<A>(&p, sc, &mut cover));
    }
    assert!(cover.strips > 0 && cover.vignettes > 0 && cover.bars > 0 && cover.blur_after > 0, "{cover:?}");
    assert!(cover.rods_drawn > 0 && cover.split_rods > 0 && cover.trail_points > 0 && cover.moved_orbs > 0, "{cover:?}");
}

#[test]
fn ee_worlds() {
    worlds::<EeArithmetic>("ee");
}

#[test]
fn native_worlds() {
    worlds::<NativeArithmetic>("native");
}

struct Values(Rng);

impl Values {
    fn value(&mut self) -> f32 {
        let low = self.0.next32() & 0x807fffff;
        f(low | ((100 + self.0.next32() % 54) << 23))
    }
    fn vec(&mut self) -> Vec4 {
        std::array::from_fn(|_| self.value())
    }
    fn mat(&mut self) -> Mat4 {
        std::array::from_fn(|_| self.vec())
    }
    fn radians(&mut self) -> f32 {
        ((self.0.next32() % 8001) as i32 - 4000) as f32 / 1000.0
    }
}

fn matrices<A: Arithmetic>(tag: &str) {
    type M<A> = Matrix<A>;
    let mut g = Values(Rng::new(0x3a7a1c00));
    let mut h = Hash::default();
    for _ in 0..20000 {
        let a = g.mat();
        let b = g.mat();
        let v = g.vec();
        hm(&mut h, &M::<A>::multiply(&a, &b));
        h4(&mut h, &M::<A>::apply(&a, &v));
        let angle = g.0.next32() as i32;
        hm(&mut h, &M::<A>::rotate_x(&a, angle));
        hm(&mut h, &M::<A>::rotate_y(&a, angle));
        hm(&mut h, &M::<A>::rotate_z(&a, angle));
        hm(&mut h, &M::<A>::move_(&a, v[0], v[1], v[2]));
        let n = M::<A>::normalize(&[v[0], v[1], v[2]]);
        for x in n {
            h.f32(x);
        }
        h.f32(M::<A>::dot(&n, &[v[1], v[2], v[3]]));
        let s: [f32; 9] = std::array::from_fn(|_| g.value());
        hm(&mut h, &M::<A>::view_screen(s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8]));
        let rotation = [g.radians(), g.radians(), g.radians(), 0.0];
        let (q, w, e) = (g.vec(), g.vec(), g.vec());
        hm(&mut h, &M::<A>::view_matrix(&q, &w, &e, &rotation));
    }
    expect(&format!("{tag} matrices"), &h);
}

#[test]
fn ee_matrices() {
    matrices::<EeArithmetic>("ee");
}

#[test]
fn native_matrices() {
    matrices::<NativeArithmetic>("native");
}

fn guarded<A: Arithmetic>(tag: &str) {
    let Some(folder) = resources() else { return };
    let p = program(&folder);
    let mut r = Rng::new(0x9a4ded);
    let mut h = Hash::default();
    let sc = Scenario { mode: 0, wide: 0, item0: 0, pal: false, seed: 7, frames: 0, jumps: false };
    let mut s = clock_of(&p, &sc, &mut r);
    for i in 0..200000 {
        s.time.milliseconds = (r.next32() % 1000000) as f32 / 1000.0;
        s.time.seconds = (r.next32() % 60) as i32;
        s.time.minutes = (r.next32() % 60) as i32;
        s.time.hours = (r.next32() % 24) as i32;
        if i % 1000 == 0 {
            s.time.milliseconds = f(r.next32());
            s.time.seconds = r.next32() as i32;
            s.time.minutes = r.next32() as i32;
            s.time.hours = r.next32() as i32;
        }
        if i % 7 == 0 {
            s.state.seconds_angle = r.next32() as i16;
            s.eased.hour_hand = r.next32() as i16;
        }
        let hours = ClockLogic::<A>::hours(&s.time);
        let seconds = ClockLogic::<A>::seconds(&s.time);
        ClockLogic::<A>::angles(&mut s);
        ClockLogic::<A>::colours(&mut s);
        ClockLogic::<A>::scale(&mut s);
        h.f32(hours);
        h.f32(seconds);
        hclock(&mut h, &s);
    }
    expect(&format!("{tag} guarded"), &h);
}

#[test]
fn ee_guarded() {
    guarded::<EeArithmetic>("ee");
}

// NativeArithmetic under EeRounding is <NativeArithmetic as Arithmetic>::UnderEeRounding.
#[test]
fn truncating_guarded() {
    guarded::<<NativeArithmetic as Arithmetic>::UnderEeRounding>("truncating");
    guarded::<TruncatingArithmetic>("truncating");
}

#[test]
fn dates() {
    let mut r = Rng::new(0xda7e0000);
    let mut h = Hash::default();
    for _ in 0..200000 {
        let y = 1600 + (r.next32() % 800) as i32;
        let mo = 1 + (r.next32() % 12) as i32;
        let d = 1 + (r.next32() % 31) as i32;
        let hh = (r.next32() % 24) as i32;
        let mi = (r.next32() % 60) as i32;
        let se = (r.next32() % 60) as i32;
        h.u64(seconds_of(y, mo, d, hh, mi, se) as u64);
        for x in date_of((r.next() % (1 << 36)) as i64) {
            h.i32(x);
        }
        let word = r.next32();
        let z = zone_of(word);
        h.i32(z.minutes);
        h.boolean(z.summer);
        let items = [2000 + (r.next32() % 100) as i32, mo, d, hh, mi, se];
        for x in local_of(&items, z) {
            h.i32(x);
        }
        for x in rtc_of(&items, z) {
            h.i32(x);
        }
        for x in rtc_at((r.next() % 8000000000) as i64 - 1000000000) {
            h.i32(x);
        }
        h.u64(unix_of(&items) as u64);
        let check = [1990 + (r.next32() % 120) as i32, mo, d, hh, mi, se];
        let c = date_check(&check, word);
        for x in c.items {
            h.i32(x);
        }
        for range in c.ranges {
            h.i32(range[0]);
            h.i32(range[1]);
        }
    }
    expect("dates", &h);
}

#[test]
fn program_image_and_records() {
    let Some(folder) = resources() else { return };
    let p = program(&folder);
    let mut h = Hash::default();
    for a in (0x2B2000u32..0x2B7000).step_by(4) {
        h.u32(p.word(a));
        h.i32(p.integer(a));
        h.f32(p.single(a));
        h.f64(p.double_at(a));
    }
    for a in (0x2AD200u32..0x2AE600).step_by(4) {
        let w = p.word(a);
        h.u32(w);
        if (0x200000..0x380000).contains(&w) {
            match std::panic::catch_unwind(|| p.string(w)) {
                Ok(s) => {
                    h.u32(s.len() as u32);
                    for c in s {
                        h.u32(c.into());
                    }
                }
                Err(_) => h.u32(0xffffffff),
            }
        }
    }
    hrecord(&mut h, &rod_record_at(&p, 0x2B5490));
    hrect(&mut h, &rect_at(&p, 0x2B21E0));
    hr(&mut h, &ramp_at(&p, 0x2B46B8));
    hm(&mut h, &mat4_at(&p, 0x2B5490 + 0x20));
    expect("program", &h);
}

#[test]
fn font() {
    let Some(folder) = resources() else { return };
    let font = Font::load(&folder.join("FNTOSD")).unwrap();
    let mut h = Hash::default();
    h.u32(font.blocks().len() as u32);
    for b in font.blocks() {
        for x in [b.at, b.flags, b.depth as u32] {
            h.u32(x);
        }
        h.f32(b.scale_x);
        h.f32(b.scale_y);
        for x in [b.width, b.height, b.ascent, b.descent] {
            h.i32(x);
        }
        for x in [b.glyphs, b.pictures, b.picture_size, b.index, b.metrics, b.table] {
            h.u32(x);
        }
        h.boolean(b.per_glyph);
        h.u32(b.ranges.len() as u32);
        for rg in &b.ranges {
            for x in [rg.start, rg.end, rg.first, rg.add] {
                h.i32(x);
            }
        }
        for c in font.table(b) {
            for x in c {
                h.u32(x.into());
            }
        }
        h.boolean(font.block_at(b.at).is_some_and(|found| std::ptr::eq(found, b)));
    }
    for o in (0x50..0x58).step_by(2) {
        h.i32(font.header16(o));
    }
    for code in -2..0x10002 {
        let g = font.glyph(code);
        h.boolean(g.is_some());
        let Some(g) = g else { continue };
        h.u32(g.block.at);
        h.i32(g.index);
        h.u32(g.picture);
        for m in g.metrics {
            h.i32(m);
        }
        let mut sum = 0u32;
        for y in 0..g.block.height {
            for x in 0..g.block.width {
                sum = sum.wrapping_mul(31).wrapping_add(font.pixel(&g, x, y) as u32);
            }
        }
        h.u32(sum);
    }
    expect("font", &h);
}
