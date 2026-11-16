// Whole-scene sessions: the clock started cold and stepped as the app steps it (shell.rs), with scripted pad input. The
// same definitions as the C++ probe that produced the expected hashes.
#![allow(dead_code)]

use super::shell::{Session, golden_now, host_inputs};
use super::{Hash, Rng, cold_assets, hash_frame, resources};
use scene::arithmetic::Arithmetic;
use scene::clock::Clock;
use scene::frame::Frame;
use scene::menu_types::pad;
use scene::ramp::Ramp;
use scene::text::FontState;

fn hstate(h: &mut Hash, s: &FontState) {
    for x in [s.line_height, s.fixed, s.percent, s.pitch, s.decoration, s.clip, s.blank, s.ascent, s.dirty] {
        h.i32(x);
    }
    h.f32(s.tv);
    h.f32(s.ratio);
    h.f32(s.locate[0]);
    h.f32(s.locate[1]);
    for c in s.colour {
        h.f32(c);
    }
    for r in &s.matrix {
        for &x in r {
            h.f32(x);
        }
    }
}

fn hbytes(h: &mut Hash, s: &[u8]) {
    h.u32(s.len() as u32);
    for &c in s {
        h.u32(c.into());
    }
}

fn hramp(h: &mut Hash, r: &Ramp) {
    for x in [r.length, r.counter, r.changed, r.state] {
        h.i32(x);
    }
}

// The digest of one frame: the scene hash and what else the app reads of the clock.
pub fn digest<A: Arithmetic>(c: &Clock<A>, frame: &Frame) -> u64 {
    let mut h = Hash::default();
    h.u64(hash_frame(frame));
    h.u32(c.sounds().len() as u32);
    for s in c.sounds() {
        for x in [s.id, s.a1, s.a2, s.a3] {
            h.u32(x.into());
        }
        h.boolean(s.carried_a2);
        h.boolean(s.carried_a3);
    }
    h.u32(c.notes().len() as u32);
    for n in c.notes() {
        hbytes(&mut h, n.as_bytes());
    }
    h.u32(c.strings().len() as u32);
    for r in c.strings() {
        hbytes(&mut h, &r.text);
        h.boolean(r.measuring);
        hstate(&mut h, &r.own);
    }
    for &x in c.items() {
        h.i32(x);
    }
    h.boolean(c.rtc_write().is_some());
    if let Some(w) = c.rtc_write() {
        for &x in w {
            h.i32(x);
        }
    }
    let s = c.state();
    h.i32(s.mode);
    h.i32(s.overlay_level);
    h.i32(s.time_filled);
    h.f32(s.scene.scale);
    h.f32(s.scale_target);
    h.u32(s.spin.unwrap_or(0).into());
    hramp(&mut h, &s.menu_ramp);
    hramp(&mut h, &s.vignette_ramp);
    h.i32(s.time.hours);
    h.i32(s.time.minutes);
    h.i32(s.time.seconds);
    h.f32(s.time.milliseconds);
    let m = c.menus().unwrap();
    h.i32(m.page.level);
    h.i32(m.page.selected);
    h.i32(m.page.glow);
    h.i32(m.page.title_width);
    hramp(&mut h, &m.page.ramp);
    hramp(&mut h, &m.main_menu.ramp);
    h.i32(m.main_menu.selected);
    hramp(&mut h, &m.version_ramp);
    h.i32(m.version.selected);
    h.i32(m.version.first);
    h.i32(m.version.count);
    h.i32(m.version.panel_alpha);
    h.i32(m.list_fade.first);
    h.i32(m.list_fade.second);
    h.i32(m.list_fade.second_index);
    h.i32(m.entry_active);
    h.i32(m.config_gate.unwrap_or(-99));
    for e in &m.entries {
        h.i32(e.value_index);
    }
    for f in &m.adjust_fields {
        h.i32(f.item);
        h.i32(f.lowest);
        h.i32(f.highest);
    }
    let q = c.cubes().unwrap();
    h.i32(q.list.position);
    h.i32(q.list.left);
    h.i32(q.list.speed);
    h.i32(q.list.slowing);
    h.f32(q.list.pulse);
    h.i32(q.list.pulsed);
    hramp(&mut h, &q.ramp);
    if let Some(text) = c.text() {
        h.u32(text.cache().list.len() as u32);
        for e in &text.cache().list {
            h.i32(e.code);
            h.i32(e.loaded);
            h.i32(e.cell);
            h.u32(e.block);
        }
    }
    h.h
}

pub struct Scenario {
    pub name: &'static str,
    pub pal: bool,
    pub language: i32,
    pub aspect: i32,
    pub zone: i32,
    pub summer: bool,
    pub date_format: i32,
    pub time_format: i32,
    pub wide: bool,
    pub rand: u32,
    pub start: i64,
    pub frames: u64,
    pub script: u32,
    pub seed: u64,
}

#[allow(clippy::too_many_arguments)]
const fn sc(
    name: &'static str,
    pal: bool,
    language: i32,
    aspect: i32,
    zone: i32,
    summer: bool,
    date_format: i32,
    time_format: i32,
    wide: bool,
    rand: u32,
    start: i64,
    frames: u64,
    script: u32,
    seed: u64,
) -> Scenario {
    Scenario { name, pal, language, aspect, zone, summer, date_format, time_format, wide, rand, start, frames, script, seed }
}

pub const SCENARIOS: [Scenario; 10] = [
    sc("ntsc-en-walk", false, 1, 0, 540, false, 0, 0, false, 1, 1772953140, 1700, 1, 0),
    sc("pal-fr-walk", true, 3, 1, 60, true, 2, 1, true, 12345, 1919898570, 1700, 1, 0),
    sc("ntsc-de-tour", false, 2, 2, -480, false, 1, 1, false, 7, 4102444730, 1450, 2, 0),
    sc("pal-es-tour", true, 4, 0, 330, false, 0, 0, false, 99, 1767225590, 1450, 2, 0),
    sc("ntsc-it-random", false, 5, 1, -300, true, 2, 0, true, 3, 1800000000, 1600, 3, 11),
    sc("pal-nl-random", true, 6, 2, 0, false, 1, 1, false, 5, 2000000000, 1600, 3, 12),
    sc("ntsc-pt-random", false, 7, 0, 600, true, 0, 1, false, 9, 1700000123, 1600, 3, 13),
    sc("pal-en-random", true, 1, 1, -600, false, 2, 1, true, 77, 3000000000, 1600, 3, 14),
    sc("ntsc-ja-idle", false, 0, 0, 540, false, 0, 0, false, 1, 1767225599, 400, 4, 0),
    sc("pal-de-idle", true, 2, 2, 120, true, 1, 0, true, 2, 1711846790, 400, 4, 0),
];

pub struct Press {
    pub at: u64,
    pub bit: u32,
    pub hold: u64,
}

// Script 1: System Configuration entry by entry (the clock adjusted and confirmed, every value row entered); script 2: the
// clock adjust cancelled, the Version page, the menu hidden and shown; script 3: random presses; script 4: none.
pub fn presses_of(s: &Scenario) -> Vec<Press> {
    let mut out = Vec::new();
    let mut at = |t: u64, bit: u32| out.push(Press { at: t + 150, bit, hold: 4 });
    if s.script == 1 {
        at(100, pad::DOWN);
        at(130, pad::CROSS);
        let mut t = 280;
        at(t, pad::CROSS);
        let edits = [
            pad::UP,
            pad::UP,
            pad::DOWN,
            pad::RIGHT,
            pad::UP,
            pad::RIGHT,
            pad::DOWN,
            pad::DOWN,
            pad::RIGHT,
            pad::UP,
            pad::RIGHT,
            pad::DOWN,
            pad::RIGHT,
            pad::UP,
            pad::RIGHT,
            pad::LEFT,
            pad::LEFT,
            pad::UP,
        ];
        for e in edits {
            t += 12;
            at(t, e);
        }
        t += 30;
        at(t, pad::CROSS);
        for entry in 1..7 {
            t += 60;
            at(t, pad::DOWN);
            t += 40;
            at(t, pad::CROSS);
            t += 30;
            at(t, pad::RIGHT);
            t += 20;
            at(t, pad::LEFT);
            t += 20;
            at(t, if entry % 2 != 0 { pad::CROSS } else { pad::CIRCLE });
        }
        for (step, bit) in [(60, pad::DOWN), (30, pad::UP), (30, pad::TRIANGLE), (60, pad::SQUARE), (120, pad::SQUARE), (120, pad::CIRCLE)] {
            t += step;
            at(t, bit);
        }
    } else if s.script == 2 {
        for (t, bit) in [
            (80, pad::TRIANGLE),
            (120, pad::DOWN),
            (140, pad::DOWN),
            (160, pad::UP),
            (180, pad::TRIANGLE),
            (220, pad::CIRCLE),
            (300, pad::DOWN),
            (320, pad::UP),
            (340, pad::DOWN),
            (360, pad::CROSS),
            (500, pad::CROSS),
            (530, pad::UP),
            (545, pad::RIGHT),
            (560, pad::DOWN),
            (580, pad::CIRCLE),
            (640, pad::SQUARE),
            (760, pad::SQUARE),
            (800, pad::SQUARE),
            (830, pad::SQUARE),
            (900, pad::DOWN),
            (930, pad::CROSS),
            (970, pad::CIRCLE),
            (1050, pad::UP),
            (1080, pad::CIRCLE),
            (1200, pad::TRIANGLE),
            (1240, pad::CIRCLE),
        ] {
            at(t, bit);
        }
    } else if s.script == 3 {
        let mut r = Rng::new(s.seed);
        let table = [
            pad::UP,
            pad::DOWN,
            pad::UP,
            pad::DOWN,
            pad::LEFT,
            pad::RIGHT,
            pad::LEFT,
            pad::RIGHT,
            pad::CROSS,
            pad::CROSS,
            pad::CIRCLE,
            pad::SQUARE,
            pad::TRIANGLE,
            pad::DOWN,
            pad::CROSS,
            pad::UP,
        ];
        let mut t = 60u64;
        while t < s.frames {
            t += u64::from(6 + r.next32() % 35);
            let bit = table[(r.next32() % 16) as usize];
            let hold = u64::from(1 + r.next32() % 8);
            out.push(Press { at: t, bit, hold });
        }
    }
    out
}

// What a session reached: Source/App/Screens.cpp screenOf per frame, the entries entered, and the events.
#[derive(Debug, Default)]
pub struct Coverage {
    pub screens: std::collections::BTreeSet<&'static str>,
    pub entered: std::collections::BTreeSet<i32>,
    pub rtc_writes: u32,
    pub sounds: u32,
    pub notes: std::collections::BTreeSet<String>,
    pub pulsed: u32,
    pub version_moves: u32,
    pub glyphs: u32,
    pub lines: u32,
    pub trail_red: u32,
    pub cube_layer: u32,
    pub max_red: u8,
    pub max_alpha: u8,
}

fn screen_of(m: &scene::menu_types::MenusState, menu_ramp: &Ramp) -> &'static str {
    if m.version_ramp.state != 0 {
        return "version";
    }
    match m.page.ramp.state {
        0 => return "main menu",
        1 => return "opening System Configuration",
        3 => return "closing System Configuration",
        _ => {}
    }
    match menu_ramp.state {
        1 => "hiding the menu",
        2 => "clock alone",
        3 => "showing the menu",
        _ => {
            if m.page.level == 1 {
                "inside an entry"
            } else {
                "System Configuration"
            }
        }
    }
}

pub fn run_scenario<A: Arithmetic>(s: &Scenario) -> Option<Hash> {
    run_covered::<A>(s, &mut Coverage::default())
}

pub fn run_covered<A: Arithmetic>(s: &Scenario, cover: &mut Coverage) -> Option<Hash> {
    let folder = resources()?;
    Some(run_assets::<A>(&cold_assets(&folder), s, cover))
}

pub fn run_assets<A: Arithmetic>(assets: &scene::cold_start::ColdAssets, s: &Scenario, cover: &mut Coverage) -> Hash {
    let mut cold = host_inputs(s.pal, golden_now(s.start, 0), s.zone, s.summer);
    cold.language = s.language;
    cold.aspect = s.aspect;
    cold.date_format = s.date_format;
    cold.time_format = s.time_format;
    cold.wide = s.wide;
    cold.rand_state = s.rand;
    cold.gs_allocator = 770048;
    let mut session = Session::<A>::start(assets, &cold);
    let presses = presses_of(s);
    let mut all = Hash::default();
    let per_frame = std::env::var_os("PROBE_FRAMES").is_some();
    for i in 0..s.frames {
        for p in &presses {
            if p.at == i {
                session.reader.down(p.bit);
            }
            if p.at + p.hold == i {
                session.reader.up(p.bit);
            }
        }
        let frame = session.step(golden_now(s.start, i));
        let d = digest(&session.clock, &frame);
        let c = &session.clock;
        let m = c.menus().unwrap();
        cover.screens.insert(screen_of(m, &c.state().menu_ramp));
        if m.page.level == 1 {
            cover.entered.insert(m.page.selected);
        }
        cover.rtc_writes += u32::from(c.rtc_write().is_some());
        cover.sounds += c.sounds().len() as u32;
        cover.notes.extend(c.notes().iter().cloned());
        cover.pulsed += u32::from(c.cubes().unwrap().list.pulse != 0.0);
        cover.version_moves += u32::from(m.version.selected != 0);
        cover.glyphs += frame.passes.iter().filter(|p| p.material.texture == scene::frame::GLYPH_TEXTURE).count() as u32;
        for p in &frame.passes {
            if p.topology == scene::frame::PassTopology::Lines {
                cover.lines += p.vertices.len() as u32;
                cover.trail_red += p.vertices.iter().filter(|v| v.r != 0).count() as u32;
                cover.max_red = cover.max_red.max(p.vertices.iter().map(|v| v.r).max().unwrap_or(0));
                cover.max_alpha = cover.max_alpha.max(p.vertices.iter().map(|v| v.a).max().unwrap_or(0));
            }
            if p.material.texture == 5 && p.material.source == scene::frame::SourceKind::Texture {
                cover.cube_layer += p.vertices.len() as u32;
            }
        }
        if per_frame {
            println!("{} {} {:016x} {:016x}", s.name, i, d, hash_frame(&frame));
        }
        all.u64(d);
    }
    all
}
