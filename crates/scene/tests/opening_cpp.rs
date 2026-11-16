#![allow(clippy::manual_is_multiple_of, clippy::explicit_counter_loop)]

mod common;
mod opening_common;

use std::panic::{self, AssertUnwindSafe};

use common::{EDGE_FLOATS, Hash, Rng};
use opening_common::{hash_frame, program};
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic};
use scene::cold_state::advance_rand;
use scene::frame::{Material, Pass, Vertex};
use scene::matrix::{Mat4, Vec4};
use scene::opening::*;
use scene::program_image::ProgramImage;
use scene::sound::SoundCommand;

#[path = "opening_common/cpp_expected.rs"]
mod expected;

fn expect(name: &str, h: &Hash) {
    let Some((_, count, hash)) = expected::EXPECTED.iter().find(|e| e.0 == name) else { panic!("no expected value for {name}") };
    assert_eq!((h.count, h.h), (*count, *hash), "{name}: (count, hash) differ from the C++");
}

// Canonical mode hashes every NaN bit pattern as 0x7fc00000, as the probe's: when two NaNs meet, which one an SSE instruction
// returns follows the operand order the compiler picked, and the C++ itself gives different payloads at -O0, -O2 and under gcc.
thread_local! {
    static CANONICAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
fn canonical(w: u32) -> u32 {
    if CANONICAL.with(|c| c.get()) && (w & 0x7f800000) == 0x7f800000 && (w & 0x007fffff) != 0 { 0x7fc00000 } else { w }
}
fn canonically(run: impl FnOnce()) {
    CANONICAL.with(|c| c.set(true));
    run();
    CANONICAL.with(|c| c.set(false));
}
fn hf(h: &mut Hash, x: f32) {
    h.u32(canonical(x.to_bits()));
}
fn h4(h: &mut Hash, v: &Vec4) {
    for x in v {
        hf(h, *x);
    }
}
fn hm(h: &mut Hash, m: &Mat4) {
    for r in m {
        h4(h, r);
    }
}
fn hname(h: &mut Hash, s: &str) {
    h.u32(s.len() as u32);
    for c in s.bytes() {
        h.u32(u32::from(c));
    }
}
fn hvertex(h: &mut Hash, v: &Vertex) {
    hf(h, v.x);
    hf(h, v.y);
    h.u32(v.z);
    hf(h, v.u);
    hf(h, v.v);
    hf(h, v.q);
    h.u32(u32::from(v.r));
    h.u32(u32::from(v.g));
    h.u32(u32::from(v.b));
    h.u32(u32::from(v.a));
}
fn hmaterial(h: &mut Hash, m: &Material) {
    h.i32(m.source as i32);
    h.i32(m.texture);
    h.i32(m.source_target as i32);
    h.boolean(m.colour_only);
    h.i32(m.coordinates as i32);
    h.i32(m.sampling as i32);
    for x in m.region {
        h.i32(x);
    }
    h.boolean(m.bilinear);
    h.i32(m.source_height);
    h.i32(m.blend as i32);
    h.u32(u32::from(m.blend_constant));
    h.i32(m.depth_test as i32);
    h.boolean(m.depth_write);
    h.boolean(m.gouraud);
    h.boolean(m.per_pixel_alpha);
    h.boolean(m.alpha_correction);
}
fn hpasses(h: &mut Hash, passes: &[Pass]) {
    h.u32(passes.len() as u32);
    for p in passes {
        hname(h, &p.name);
        h.i32(p.target as i32);
        h.i32(p.topology as i32);
        hmaterial(h, &p.material);
        h.boolean(p.edge_smoothing);
        h.boolean(p.half_line);
        h.boolean(p.prim_blend);
        h.boolean(p.scissor.is_some());
        if let Some(s) = &p.scissor {
            for x in s {
                h.i32(*x);
            }
        }
        h.u32(p.vertices.len() as u32);
        for v in &p.vertices {
            hvertex(h, v);
        }
    }
}

fn value(r: &mut Rng, kind: i32) -> f32 {
    match kind {
        0 => f32::from_bits(r.next32()),
        1 => f32::from_bits(EDGE_FLOATS[(r.next() % EDGE_FLOATS.len() as u64) as usize]),
        _ => (r.next32() as i32) as f32 * (1.0 / 16777216.0),
    }
}
fn v4(r: &mut Rng, kind: i32) -> Vec4 {
    [value(r, kind), value(r, kind), value(r, kind), value(r, kind)]
}
fn m4(r: &mut Rng, kind: i32) -> Mat4 {
    [v4(r, kind), v4(r, kind), v4(r, kind), v4(r, kind)]
}

fn vu0<A: Arithmetic>(name: &str, kinds: &[i32]) {
    type V<A> = Vu0<A>;
    for kind in 0..3 {
        let mut r = Rng::new(0x0b0e0000 + kind as u64);
        let mut h = Hash::default();
        for _ in 0..4000 {
            let a = m4(&mut r, kind);
            let b = m4(&mut r, kind);
            let v = v4(&mut r, kind);
            let w = v4(&mut r, kind);
            let u = v4(&mut r, kind);
            hm(&mut h, &V::<A>::unit_matrix());
            h4(&mut h, &V::<A>::apply(&a, &v));
            hm(&mut h, &V::<A>::mul_matrix(&a, &b));
            hm(&mut h, &V::<A>::rot_matrix(&a, &v));
            hm(&mut h, &V::<A>::rot_matrix_unit(&w));
            hm(&mut h, &V::<A>::trans_matrix(&a, &w));
            h4(&mut h, &V::<A>::sub_vector(&v, &w));
            hm(&mut h, &V::<A>::camera_matrix(&v, &w, &u));
            let mut f = [0.0f32; 9];
            for x in &mut f {
                *x = value(&mut r, kind);
            }
            hm(&mut h, &V::<A>::view_screen_matrix(f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7], f[8]));
            hm(&mut h, &V::<A>::normal_light_matrix(&v, &w, &u));
            let count = 1 + r.next() % 8;
            let vertices: Vec<Vec4> = (0..count).map(|_| v4(&mut r, kind)).collect();
            h.boolean(V::<A>::clip_all(&v, &w, &a, &vertices));
            let lo = [1728.0, 1936.0, 0.0, 5.0];
            let hi = [2368.0, 2160.0, 0.0, 16777215.0];
            h.boolean(V::<A>::clip_all(&lo, &hi, &a, &vertices));
        }
        if kinds.contains(&kind) {
            expect(&format!("{name} vu0 {kind}"), &h);
        }
    }
}

fn hstate(h: &mut Hash, s: &TimelineState) {
    h.i32(s.counter);
    h.i32(s.stage);
    for x in s.block {
        h.f32(x);
    }
    h4(h, &s.camera);
    h4(h, &s.direction);
    h4(h, &s.up);
    h.f32(s.roll);
    h.boolean(s.go);
    h.i32(s.pending);
    h.i32(s.scene);
    h.i32(s.snapshot);
    for l in &s.lights {
        h4(h, l);
    }
    h.i32(s.logo_state);
    h.i32(s.logo_value);
    h.i32(s.logo_step);
}
fn hsounds(h: &mut Hash, sounds: &[SoundEvent]) {
    h.u32(sounds.len() as u32);
    for e in sounds {
        h.u32(e.id);
        h.i32(e.argument);
        h.i32(e.counter);
    }
}
fn hstep(h: &mut Hash, s: &TimelineStep) {
    h.i32(s.result);
    hm(h, &s.matrices.camera);
    hm(h, &s.matrices.view_screen);
    hm(h, &s.matrices.world_to_screen);
    hm(h, &s.matrices.normal_light);
    h.i32(s.fade_alpha);
    h.i32(s.blur_level);
    h.i32(s.logo_alpha);
    h.i32(s.logo_value);
    hsounds(h, &s.sounds);
}

const LATER: [u32; 21] = [0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0xffffffff];

fn timeline_options() -> Vec<BootOptions> {
    let mut out = Vec::new();
    for first in [0x65, 0x6a] {
        for later in LATER {
            out.push(BootOptions { disc: DiscSchedule { first, later }, ..Default::default() });
        }
    }
    for forced in [false, true] {
        for ready in [false, true] {
            for exec in 0..3 {
                for later in [0x64, 0x6a, 0x6c, 0x72] {
                    out.push(BootOptions {
                        disc: DiscSchedule { first: 0x65, later },
                        clock_forced: forced,
                        hdd_ready: ready,
                        hdd_exec: exec,
                        ..Default::default()
                    });
                }
            }
        }
    }
    out
}

fn timeline<A: Arithmetic>(name: &str) {
    let mut h = Hash::default();
    for o in timeline_options() {
        let mut t = Timeline::<A>::new(&o, &Timeline::<A>::initial_state(1));
        hstate(&mut h, t.state());
        for i in 0..4000 {
            let s = t.step(if i == 0 { o.disc.first } else { o.disc.later });
            hstep(&mut h, &s);
            hstate(&mut h, t.state());
            if i == 0 {
                t.scene_set_up();
            }
            if s.result != 0 {
                break;
            }
        }
    }
    expect(&format!("{name} timeline"), &h);
    let mut g = Hash::default();
    for c in [-5, 0, 1, 2, 77, 1000000] {
        hstate(&mut g, &Timeline::<A>::initial_state(c));
    }
    expect(&format!("{name} timeline initial"), &g);
}

fn intro_matrices<A: Arithmetic>() -> Vec<(Matrices, Vec4)> {
    let mut out = Vec::new();
    let o = BootOptions::default();
    let mut t = Timeline::<A>::new(&o, &Timeline::<A>::initial_state(1));
    for i in 0..400 {
        let s = t.step(if i == 0 { o.disc.first } else { o.disc.later });
        if i == 0 {
            t.scene_set_up();
        }
        out.push((s.matrices, t.state().camera));
        if s.result != 0 {
            break;
        }
    }
    out
}

fn hwork(h: &mut Hash, w: &CubeWork) {
    h.i32(w.index);
    h4(h, &w.angles);
    hm(h, &w.turned);
    hm(h, &w.to_world);
    hm(h, &w.to_screen);
    h4(h, &w.centre);
    for c in &w.corners {
        h4(h, &c.world);
        h4(h, &c.eye);
        h4(h, &c.screen);
        h.f32(c.q);
        for x in c.ints {
            h.i32(x);
        }
    }
    for f in &w.faces {
        for x in f.corners {
            h.i32(x);
        }
        h4(h, &f.object);
        h4(h, &f.world);
        h4(h, &f.view);
        h.f32(f.facing);
        h4(h, &f.colour);
        for x in f.edge {
            h.f32(x);
        }
    }
}

fn cubes<A: Arithmetic>(name: &str, program: &ProgramImage) {
    let mut h = Hash::default();
    for s in cube_pass_setups() {
        h.boolean(s.away);
        h.i32(s.texture);
        h.i32(s.mode);
        h.u32(u32::from(s.fix));
    }
    let intro = intro_matrices::<A>();
    for field in 0..2 {
        let mut c = Cubes::<A>::new(program);
        let mut counter = 1i32;
        for (m, camera) in &intro {
            let mut out = Vec::new();
            let mut work = Vec::new();
            c.draw(m, camera, &mut out, Some(&mut work), if field == 0 { 0 } else { counter & 1 });
            hpasses(&mut h, &out);
            h.u32(work.len() as u32);
            for w in &work {
                hwork(&mut h, w);
            }
            counter += 1;
        }
    }
    expect(&format!("{name} cubes intro"), &h);
    let mut g = Hash::default();
    let mut c = Cubes::<A>::new(program);
    let mut r = Rng::new(0xc0be0000);
    for n in 0..600 {
        let (m0, camera0) = &intro[(r.next() % intro.len() as u64) as usize];
        let mut m = *m0;
        let mut camera = *camera0;
        for row in &mut m.world_to_screen {
            for x in row {
                *x = A::mul(*x, 1.0 + value(&mut r, 2) * 0.002);
            }
        }
        for x in &mut camera {
            *x += value(&mut r, 2) * 0.1;
        }
        if n % 7 == 0 {
            m = Matrices { camera: m4(&mut r, 2), view_screen: m4(&mut r, 2), world_to_screen: m4(&mut r, 2), normal_light: m4(&mut r, 2) };
        }
        let mut out = Vec::new();
        let mut work = Vec::new();
        let field = (r.next() % 3) as i32;
        c.draw(&m, &camera, &mut out, if n % 3 == 0 { None } else { Some(&mut work) }, field);
        hpasses(&mut g, &out);
        g.u32(work.len() as u32);
        for w in &work {
            hwork(&mut g, w);
        }
    }
    expect(&format!("{name} cubes random"), &g);
}

fn fog<A: Arithmetic>(name: &str) {
    let mut h = Hash::default();
    let mut f = Fog::<A>::new();
    for v in f.mesh() {
        h4(&mut h, v);
    }
    for b in f.brightness() {
        for x in b {
            h.i32(*x);
        }
    }
    for x in f.offsets() {
        h.f32(*x);
    }
    let intro = intro_matrices::<A>();
    for (m, _) in &intro {
        let mut out = Vec::new();
        f.draw(&m.world_to_screen, &mut out);
        hpasses(&mut h, &out);
        for x in f.offsets() {
            h.f32(*x);
        }
        h.i32(f.stats().drawn);
        h.i32(f.stats().left_out);
    }
    expect(&format!("{name} fog intro"), &h);
    let mut g = Hash::default();
    let mut r = Rng::new(0xf0900000);
    for n in 0..300 {
        let mut m = intro[(r.next() % intro.len() as u64) as usize].0.world_to_screen;
        for row in &mut m {
            for x in row {
                *x = A::mul(*x, 1.0 + value(&mut r, 2) * 0.004);
            }
        }
        if n % 5 == 0 {
            m = m4(&mut r, if n % 10 == 0 { 0 } else { 2 });
        }
        let mut out = Vec::new();
        f.draw(&m, &mut out);
        hpasses(&mut g, &out);
        for x in f.offsets() {
            g.f32(*x);
        }
        g.i32(f.stats().drawn);
        g.i32(f.stats().left_out);
    }
    expect(&format!("{name} fog random"), &g);
}

fn hlights<A: Arithmetic>(h: &mut Hash, l: &Lights<A>) {
    h.i32(l.head());
    h.i32(l.tail());
    for light in l.matrices() {
        for m in light {
            hm(h, m);
        }
    }
    for light in l.ring() {
        for e in light {
            for x in e {
                h.i32(*x);
            }
        }
    }
    for c in l.centres() {
        h4(h, c);
    }
    for x in l.angles() {
        h.f32(*x);
    }
    for x in l.cosines() {
        h.f32(*x);
    }
    for x in l.sines() {
        h.f32(*x);
    }
    let s = l.stats();
    for x in [s.sprite_packets, s.quads_drawn, s.quads_left_out, s.trail_packets, s.trail_hidden] {
        h.i32(x);
    }
}

fn lights<A: Arithmetic>(name: &str) {
    let mut h = Hash::default();
    let intro = intro_matrices::<A>();
    for phase in [0, 1, 0xD80, 0x1000, 0x16A8, 4000, 77777] {
        let mut l = Lights::<A>::new(phase);
        hlights(&mut h, &l);
        let mut counter = 1;
        for _ in 0..2 {
            for (m, _) in &intro {
                let mut out = Vec::new();
                l.draw(counter, &m.world_to_screen, &mut out);
                counter += 1;
                hpasses(&mut h, &out);
                hlights(&mut h, &l);
            }
        }
    }
    expect(&format!("{name} lights intro"), &h);
    let mut g = Hash::default();
    let mut r = Rng::new(0x11900000);
    let mut l = Lights::<A>::new(0x1234);
    for n in 0..400 {
        let mut m = intro[(r.next() % intro.len() as u64) as usize].0.world_to_screen;
        for row in &mut m {
            for x in row {
                *x = A::mul(*x, 1.0 + value(&mut r, 2) * 0.004);
            }
        }
        if n % 9 == 0 {
            m = m4(&mut r, 2);
        }
        let mut out = Vec::new();
        let counter = (r.next() % 5000) as i32;
        l.draw(counter, &m, &mut out);
        hpasses(&mut g, &out);
        hlights(&mut g, &l);
    }
    expect(&format!("{name} lights random"), &g);
}

fn flat<A: Arithmetic>(name: &str) {
    type F<A> = Flat<A>;
    let mut h = Hash::default();
    h.i32(F::<A>::picture_height());
    h.i32(F::<A>::bar_rows());
    for x in PROCESS_SCISSOR {
        h.i32(x);
    }
    let mut r = Rng::new(0xf1a70000);
    for n in 0..3000 {
        let mut input = FlatInputs {
            counter: r.next32() as i32,
            display_index: (r.next() % 2) as i32,
            field: (r.next() % 2) as i32,
            logo_alpha: (r.next() % 400) as i32 - 50,
            fade_alpha: (r.next() % 400) as i32 - 50,
            blur_level: (r.next() % 7) as i32 - 1,
            fill_sprites: (r.next() % 4) as i32,
        };
        if n % 50 == 0 {
            input.logo_alpha = r.next32() as i32;
            input.fade_alpha = r.next32() as i32;
        }
        let mut out = Vec::new();
        F::<A>::scissor(&input, &mut out);
        F::<A>::ghost(&input, &mut out);
        F::<A>::copy_to_store(&input, &mut out);
        F::<A>::blur(&input, &mut out);
        F::<A>::fade(&input, &mut out);
        F::<A>::logo(&input, &mut out);
        F::<A>::bars(&input, &mut out);
        hpasses(&mut h, &out);
        h.i32(F::<A>::page(&input));
        h.i32(F::<A>::blur_which(&input));
    }
    let mut out = Vec::new();
    F::<A>::scissor_default(&mut out);
    hpasses(&mut h, &out);
    expect(&format!("{name} flat"), &h);
}

fn demo_history() -> History {
    let mut history = [HistoryEntry::default(); 21];
    let names = ["DEMO A", "DEMO B", "DEMO C", "DEMO D", "DEMO E"];
    for (i, name) in names.iter().enumerate() {
        history[i].name[..name.len()].copy_from_slice(name.as_bytes());
        history[i].count = (3 + 4 * i) as u8;
        history[i].mask = (0x0F << i) as u8;
        history[i].main_cell = i as u8;
    }
    history
}

fn random_history(seed: u64) -> History {
    let mut r = Rng::new(seed);
    let mut history = [HistoryEntry::default(); 21];
    for e in &mut history {
        if r.next() % 3 != 0 {
            let length = 1 + r.next() % 15;
            for c in 0..length as usize {
                e.name[c] = b'A' + (r.next() % 26) as u8;
            }
        }
        e.count = r.next() as u8;
        e.mask = r.next() as u8;
        e.main_cell = (r.next() % 8) as u8;
    }
    history
}

fn htables(h: &mut Hash, t: &TowerTables) {
    for c in &t.flag {
        for x in c {
            h.i32(*x);
        }
    }
    for c in &t.fade {
        for x in c {
            h.i32(*x);
        }
    }
    for table in [&t.sway, &t.tall, &t.height] {
        for c in table {
            for x in c {
                h.f32(*x);
            }
        }
    }
    for c in &t.place {
        for v in c {
            h4(h, v);
        }
    }
}
fn hchain(h: &mut Hash, c: &TowerChain) {
    for w in c.bytes.chunks(4) {
        h.u32(canonical(u32::from_le_bytes(w.try_into().unwrap())));
    }
}
fn hvertices(h: &mut Hash, v: &TowerVertices) {
    for x in &v.vertices {
        hvertex(h, x);
    }
    for k in v.kicked {
        h.boolean(k);
    }
    h.u64(v.alpha);
}

fn towers<A: Arithmetic>(name: &str, program: &ProgramImage, chains: bool) {
    let mut histories = vec![[HistoryEntry::default(); 21], demo_history()];
    for s in 0..6 {
        histories.push(random_history(0x70e40000 + s));
    }
    let intro = intro_matrices::<A>();
    let mut h = Hash::default();
    for history in &histories {
        let t = Towers::<A>::new(history, program);
        htables(&mut h, t.tables());
        for row in t.brightness() {
            for x in row {
                h.f32(*x);
            }
        }
        for c in (-400..800).step_by(7) {
            h.f32(t.swing(c));
        }
        let mut counter = 1;
        for (m, camera) in &intro {
            let mut tm = TowerMatrices::default();
            for (i, row) in tm.base.iter_mut().enumerate() {
                row[i] = 1.0;
            }
            tm.world_to_screen = m.world_to_screen;
            tm.light = m.normal_light;
            let mut out = Vec::new();
            let mut chains = Vec::new();
            t.draw(counter, &tm, camera, &mut out, Some(&mut chains));
            counter += 1;
            hpasses(&mut h, &out);
            h.u32(chains.len() as u32);
            if counter % 16 == 0 {
                for c in &chains {
                    hchain(&mut h, c);
                }
            }
        }
    }
    expect(&format!("{name} towers intro"), &h);

    let mut g = Hash::default();
    let mut r = Rng::new(0x70e50000);
    let t = Towers::<A>::new(&demo_history(), program);
    for n in 0..1500 {
        let column = (r.next() % TOWER_COLUMNS as u64) as i32;
        let row = (r.next() % TOWER_ROWS as u64) as i32;
        let sine = value(&mut r, 2) * (1.0 / 128.0);
        let (m, _) = &intro[(r.next() % intro.len() as u64) as usize];
        let mut tm = TowerMatrices { base: m4(&mut r, 2), world_to_screen: m.world_to_screen, light: m.normal_light };
        if n % 2 == 0 {
            tm.base = Vu0::<A>::unit_matrix();
            for rr in &mut tm.world_to_screen {
                for x in rr {
                    *x = A::mul(*x, 1.0 + value(&mut r, 2) * 0.004);
                }
            }
        }
        if n % 11 == 0 {
            tm = TowerMatrices { base: m4(&mut r, 0), world_to_screen: m4(&mut r, 1), light: m4(&mut r, 2) };
        }
        let chain = t.chain(column, row, sine, &tm);
        hchain(&mut g, &chain);
        hvertices(&mut g, &TowerVu1::<A>::run(&chain));
    }
    if chains {
        expect(&format!("{name} towers chains"), &g);
    }

    let mut e = Hash::default();
    let mut q = Rng::new(0x70e60000);
    let tm = TowerMatrices { base: Vu0::<A>::unit_matrix(), world_to_screen: intro[100].0.world_to_screen, light: intro[100].0.normal_light };
    let valid = t.chain(5, 4, 0.25, &tm);
    let hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    for _ in 0..3000 {
        let mut chain = valid.clone();
        let changes = 1 + q.next() % 3;
        for _ in 0..changes {
            let at = ((q.next() % (chain.bytes.len() / 4) as u64) * 4) as usize;
            let mut w = u32::from_le_bytes(chain.bytes[at..at + 4].try_into().unwrap());
            let how = q.next() % 4;
            if how == 0 {
                w = q.next32();
            } else if how == 1 {
                w ^= 1u32 << (q.next() % 32);
            } else if how == 2 {
                w = (w & 0x00ffffff) | (((q.next() % 0x80) as u32) << 24);
            } else {
                w = (w & 0xffff0000) | (q.next() % 0x10) as u32;
            }
            chain.bytes[at..at + 4].copy_from_slice(&w.to_le_bytes());
        }
        match panic::catch_unwind(AssertUnwindSafe(|| TowerVu1::<A>::run(&chain))) {
            Ok(v) => {
                e.u32(0);
                hvertices(&mut e, &v);
            }
            Err(error) => {
                e.u32(1);
                let message = error.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| error.downcast_ref::<String>().cloned()).unwrap();
                hname(&mut e, &message);
            }
        }
    }
    panic::set_hook(hook);
    expect(&format!("{name} towers vu1 mutated"), &e);
}

struct Combo {
    options: BootOptions,
    field_mode: i32,
}

fn combos() -> Vec<Combo> {
    let mut out = Vec::new();
    let histories = [None, Some(demo_history()), Some(random_history(0x0be50001)), Some(random_history(0x0be50002))];
    let phases = [None, Some(0), Some(0x1000), Some(0x16A8), Some(77777)];
    let mut n = 0usize;
    for later in LATER {
        let options = BootOptions {
            disc: DiscSchedule { first: 0x65, later },
            history: histories[n % histories.len()],
            lights_phase: phases[n % phases.len()],
            cold_start: n % 3 != 2,
            module: if n % 4 == 3 {
                4
            } else if n % 4 == 2 {
                2
            } else {
                1
            },
            ..Default::default()
        };
        out.push(Combo { options, field_mode: (n % 2) as i32 });
        n += 1;
    }
    for forced in [false, true] {
        for ready in [false, true] {
            for exec in 0..3 {
                let options = BootOptions {
                    disc: DiscSchedule { first: if n % 2 != 0 { 0x6a } else { 0x65 }, later: if n % 3 == 0 { 0x6c } else { 0x64 } },
                    clock_forced: forced,
                    hdd_ready: ready,
                    hdd_exec: exec,
                    history: histories[n % histories.len()],
                    lights_phase: phases[n % phases.len()],
                    cold_start: n % 4 != 1,
                    module: 1 + (n % 4) as i32,
                    pal: n % 5 == 0,
                };
                out.push(Combo { options, field_mode: ((n / 2) % 2) as i32 });
                n += 1;
            }
        }
    }
    out
}

fn hcommands(h: &mut Hash, commands: &[SoundCommand]) {
    h.u32(commands.len() as u32);
    for c in commands {
        h.u32(u32::from(c.id));
        h.u32(u32::from(c.a1));
        h.u32(u32::from(c.a2));
        h.u32(u32::from(c.a3));
        h.boolean(c.carried_a2);
        h.boolean(c.carried_a3);
    }
}

fn whole<A: Arithmetic>(name: &str, program: &ProgramImage) {
    let mut all = Hash::default();
    for (index, c) in combos().iter().enumerate() {
        let mut o = Opening::<A>::new(&c.options, program);
        let mut h = Hash::default();
        hcommands(&mut h, &o.start_commands());
        h.u32(o.rand_calls());
        h.u32(o.rand_state());
        h.i32(o.counter());
        h.boolean(o.hand_off().is_some());
        let mut display = 0;
        let mut frames = 0;
        while !o.ended() && frames < 5000 {
            let field = if c.field_mode != 0 { frames & 1 } else { 0 };
            let f = o.frame(display, field);
            h.u64(hash_frame(&f));
            hpasses(&mut h, &f.passes);
            hsounds(&mut h, o.sounds());
            hcommands(&mut h, o.commands());
            h.i32(o.counter());
            h.boolean(o.ended());
            h.u32(o.rand_calls());
            h.u32(o.rand_state());
            hstate(&mut h, o.timeline().state());
            h.i32(o.fog().stats().drawn);
            h.i32(o.fog().stats().left_out);
            hlights(&mut h, o.lights());
            h.u32(o.cube_work().len() as u32);
            for w in o.cube_work() {
                hwork(&mut h, w);
            }
            display ^= 1;
            frames += 1;
        }
        h.i32(frames);
        h.boolean(o.hand_off().is_some());
        if let Some(hand) = o.hand_off() {
            h.i32(hand.module);
            h.i32(hand.execute_app_type);
            h.boolean(hand.previous_was_opening);
        }
        let (_, _, want_frames, count, hash) = expected::WHOLE.iter().find(|e| e.0 == name && e.1 == index).unwrap();
        assert_eq!((frames, h.count, h.h), (*want_frames, *count, *hash), "{name} whole combo {index}");
        all.u64(h.h);
    }
    expect(&format!("{name} whole all"), &all);
}

#[test]
fn rand_matches_the_cpp() {
    let mut h = Hash::default();
    for s in [0, 1, 0x12345678, 0xffffffff] {
        for calls in [0, 1, 2, 385, 386, 27314] {
            h.u32(advance_rand(s, calls));
        }
    }
    expect("rand", &h);
}

#[test]
fn vu0_ee_matches_the_cpp() {
    vu0::<EeArithmetic>("ee", &[0, 1, 2]);
}

// NativeArithmetic: bit for bit the C++ built at -O0 on every set; the -O2 reference bit for bit on the moderate values and,
// on random bits and edges (where NaNs meet), with NaN payloads canonical.
#[test]
fn vu0_native_matches_the_cpp() {
    vu0::<NativeArithmetic>("native", &[2]);
    vu0::<NativeArithmetic>("native o0", &[0, 1, 2]);
    canonically(|| vu0::<NativeArithmetic>("native canonical", &[0, 1, 2]));
}

#[test]
fn timeline_ee_matches_the_cpp() {
    timeline::<EeArithmetic>("ee");
}

#[test]
fn timeline_native_matches_the_cpp() {
    timeline::<NativeArithmetic>("native");
}

#[test]
fn cubes_ee_matches_the_cpp() {
    let Some(p) = program() else { return };
    cubes::<EeArithmetic>("ee", &p);
}

#[test]
fn cubes_native_matches_the_cpp() {
    let Some(p) = program() else { return };
    cubes::<NativeArithmetic>("native", &p);
}

#[test]
fn fog_ee_matches_the_cpp() {
    fog::<EeArithmetic>("ee");
}

#[test]
fn fog_native_matches_the_cpp() {
    fog::<NativeArithmetic>("native");
}

#[test]
fn lights_ee_matches_the_cpp() {
    lights::<EeArithmetic>("ee");
}

#[test]
fn lights_native_matches_the_cpp() {
    lights::<NativeArithmetic>("native");
}

#[test]
fn flat_ee_matches_the_cpp() {
    flat::<EeArithmetic>("ee");
}

#[test]
fn flat_native_matches_the_cpp() {
    flat::<NativeArithmetic>("native");
}

#[test]
fn handoff_matches_the_cpp() {
    let mut h = Hash::default();
    for snapshot in 0..0x100u32 {
        for forced in [false, true] {
            for ready in [false, true] {
                for exec in [-1, 0, 1, 2] {
                    for cdda in [-1, 0, 1, 2] {
                        let o = decide(&HandOffInputs { snapshot, clock_forced: forced, hdd_ready: ready, hdd_exec: exec, cdda });
                        h.i32(o.module);
                        h.i32(o.execute_app_type);
                        h.boolean(o.previous_was_opening);
                    }
                }
            }
        }
    }
    for snapshot in [0xffffffff, 0x80000000, 0x10000] {
        let o = decide(&HandOffInputs { snapshot, ..Default::default() });
        h.i32(o.module);
        h.i32(o.execute_app_type);
        h.boolean(o.previous_was_opening);
    }
    h.i32(FRAMES_TO_CLOCK);
    expect("handoff", &h);
}

#[test]
fn towers_ee_matches_the_cpp() {
    let Some(p) = program() else { return };
    towers::<EeArithmetic>("ee", &p, true);
}

// The chains set feeds random-bit matrices every eleventh call: bit for bit it equals the C++ built at -O0, and the -O2 reference
// with NaN payloads canonical.
#[test]
fn towers_native_matches_the_cpp() {
    let Some(p) = program() else { return };
    towers::<NativeArithmetic>("native", &p, false);
    towers::<NativeArithmetic>("native o0", &p, true);
    canonically(|| towers::<NativeArithmetic>("native canonical", &p, true));
}

#[test]
fn whole_ee_matches_the_cpp() {
    let Some(p) = program() else { return };
    whole::<EeArithmetic>("ee", &p);
}

#[test]
fn whole_native_matches_the_cpp() {
    let Some(p) = program() else { return };
    whole::<NativeArithmetic>("native", &p);
}
