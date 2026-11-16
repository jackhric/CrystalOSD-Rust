mod common;

use common::{Hash, Rng, cold_assets, resources};
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic};
use scene::cold_start::cold_start;
use scene::cold_types::{ColdInputs, RtcTime};
use scene::cubes::{CubeFrameInputs, Cubes};
use scene::frame_head::{HeadDraw, Rect};
use scene::ramp::Ramp;

// FNV-1a 64 from the C++ at commit 039cc58 driven as below: 60 cold starts (NTSC or PAL), each cube state then set at random
// (list position, distance left, speed, slowing, pulse, pulsed place, ramp, spin) and stepped 30 frames with random blur
// levels and fields and occasional new moves. Native as the app builds it (equal to the strict build), Ee from the strict build.
const EXPECTED: [(&str, u64, u64); 2] = [("native cubes", 11425763, 0xb3a1eb93cb6b9dea), ("ee cubes", 11425591, 0x8f49b8ce7d498bd1)];

fn hrect(h: &mut Hash, r: &Rect) {
    for x in r.colour {
        h.i32(x);
    }
    for x in [r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured] {
        h.i32(x);
    }
}

fn hdraw(h: &mut Hash, d: &HeadDraw) {
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

fn hramp(h: &mut Hash, r: &Ramp) {
    for x in [r.length, r.counter, r.changed, r.state] {
        h.i32(x);
    }
}

fn run<A: Arithmetic>(tag: &str) {
    let Some(folder) = resources() else { return };
    let assets = cold_assets(&folder);
    let mut h = Hash::default();
    let mut r = Rng::new(0xc0be5000);
    let mut u = |n: u32| r.next32() % n;
    for _ in 0..60 {
        let pal = u(2) != 0;
        let input = ColdInputs {
            pal,
            screen_height: if pal { 0x100 } else { 0xE0 },
            rtc: RtcTime { year: 2026, month: 1, day: 1, ..Default::default() },
            ..Default::default()
        };
        let cold = cold_start::<A>(&assets, &input).unwrap();
        let menus = cold.clock.menus.as_ref().unwrap();
        let mut q = menus.cubes;
        let mut head = cold.clock.head;
        let mut clock = cold.clock.state;
        let cubes = Cubes::<A>::new(menus.cube_mesh.clone());
        q.list.position = u(180000) as i32;
        q.list.left = u(40000) as i32 - 20000;
        q.list.speed = u(5000) as i32;
        q.list.slowing = u(300) as i32;
        q.list.pulse = if u(2) != 0 { menus.menus.list_constants.pulse } else { 0.0 };
        q.list.pulsed = u(60) as i32;
        let body = menus.menus.body;
        q.ramp.state = u(4) as i32;
        q.ramp.counter = if q.ramp.state == 0 {
            0
        } else if q.ramp.state == 2 {
            q.ramp.length
        } else {
            u(q.ramp.length as u32 + 1) as i32
        };
        clock.spin = Some(u(65536) as u16);
        for _ in 0..30 {
            clock.level = u(7) as i32;
            if u(10) == 0 {
                q.list.left += u(12000) as i32 - 6000;
                q.list.speed = u(4000) as i32;
            }
            clock.spin = Some(clock.spin.unwrap().wrapping_add(0x1e));
            let field = u(2) as i32;
            let draws = cubes.frame(&mut q, &mut head, &clock, &CubeFrameInputs { width: 640, height: cold.clock.height, field, body });
            h.u32(draws.len() as u32);
            for d in &draws {
                h.i32(d.send as i32);
                h.u32(d.label.len() as u32);
                for &c in d.label.as_bytes() {
                    h.u32(c.into());
                }
                h.u32(d.faces.len() as u32);
                for face in &d.faces {
                    h.i32(face.face);
                    h.i32(face.piece as i32);
                    h.boolean(face.edge_smoothing);
                    for v in &face.strip {
                        for x in [v.x, v.y, v.z, v.u, v.v, v.q, v.r, v.g, v.b, v.a] {
                            h.f32(x);
                        }
                    }
                }
                h.boolean(d.rectangle.is_some());
                if let Some(rect) = &d.rectangle {
                    hdraw(&mut h, rect);
                }
                for x in d.clear {
                    h.i32(x);
                }
            }
            h.i32(q.list.position);
            h.i32(q.list.left);
            h.i32(q.list.speed);
            h.f32(q.list.pulse);
            hramp(&mut h, &q.ramp);
            for row in &q.record.local {
                for &x in row {
                    h.f32(x);
                }
            }
            h.f32(q.record.sx);
            for x in q.record.base.iter().chain(&q.record.reflection) {
                h.i32(*x);
            }
            for rect in [&q.added, &q.half, &q.chain, &head.copy] {
                hrect(&mut h, rect);
            }
        }
    }
    let name = format!("{tag} cubes");
    let want = EXPECTED.iter().find(|e| e.0 == name).unwrap();
    assert_eq!((h.count, h.h), (want.1, want.2), "{name}");
}

#[test]
fn native_cubes() {
    run::<NativeArithmetic>("native");
}

#[test]
fn ee_cubes() {
    run::<EeArithmetic>("ee");
}
