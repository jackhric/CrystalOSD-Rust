mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::hashes::*;
use common::shell::{console_now, golden_now, host_inputs};
use common::{Hash, Rng, cold_assets, hash_frame, resources};
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic};
use scene::clock::Clock;
use scene::cold_start::{ColdAssets, cold_start};
use scene::menu_types::{MenuWorld, pad};
use scene::menus::{Menus, MenusOptions};
use scene::text::{ClockItems, glyph_cache_image};

// FNV-1a 64 from the C++ at commit 039cc58 driven as below. "alone": the clock screen without menus (cold start, menus
// removed), NTSC and PAL, with and without the font, 300 frames each with item 0, timeFilled, the display buffer and the
// field varied; each frame's scene hash, its glyph cache image (glyphCacheImage) and its string count. "menus": 40 cold
// starts, 400 steps each of between, menuStep, step and endOfFrame on worlds the app does not reach (externals missing
// or dirty, the disc changed, leaving, the dialog and first-run ramps up, modes set) with random pads; a function that
// throws is marked and what it left is compared. Native as the app builds it (equal to the strict build), Ee from the
// strict build.
// "nofont": the sessions ntsc-en-walk and ntsc-de-tour of scenarios.rs with menus and no font (the app without FNTOSD).
const EXPECTED: [(&str, u64, u64); 6] = [
    ("native alone", 3000, 0xe756d71e944ffa59),
    ("native menus", 15237598, 0xbbb2b58f7ab99815),
    ("native nofont", 2, 0x843081ec283c4d98),
    ("ee alone", 3000, 0xcdcd3535eb97db31),
    ("ee menus", 15237598, 0xbbb2b58f7ab99815),
    ("ee nofont", 2, 0x0ad666980078cccc),
];

fn hworld(h: &mut Hash, w: &MenuWorld<'_>) {
    hclock(h, w.clock);
    hhead(h, w.head);
    hramp(h, w.sprite_fade);
    let m = &*w.menus;
    let p = &m.page;
    for x in [p.word0, p.count, p.title_width, p.selected, p.word14, p.level, p.word2c, p.word30, p.glow] {
        h.i32(x);
    }
    hramp(h, &p.ramp);
    for e in &m.entries {
        hentry(h, e);
    }
    h.i32(m.main_menu.selected);
    for r in [&m.main_menu.ramp, &m.version_ramp, &m.dialog_ramp, &m.first_run_ramp] {
        hramp(h, r);
    }
    hversion(h, &m.version);
    hversion(h, &m.version_drawn);
    hramp(h, &m.version_ramp_drawn);
    h.boolean(m.version_drawn_valid);
    h.i32(m.entry_active);
    h.i32(m.screen_code);
    for f in &m.adjust_fields {
        h.i32(f.item);
        h.i32(f.lowest);
        h.i32(f.highest);
    }
    h.boolean(m.config_gate.is_some());
    h.i32(m.config_gate.unwrap_or(0));
    h.i32(m.list_fade.first);
    h.i32(m.list_fade.second);
    h.i32(m.list_fade.second_index);
    let q = &*w.cubes;
    hramp(h, &q.ramp);
    h.f32(q.list.pulse);
    for x in [q.list.pulsed, q.list.position, q.list.left, q.list.speed, q.list.slowing] {
        h.i32(x);
    }
    for &x in w.items.iter() {
        h.i32(x);
    }
}

fn alone<A: Arithmetic>(assets: &ColdAssets) -> Hash {
    let mut h = Hash::default();
    for pal in [false, true] {
        for with_text in [false, true] {
            let a = ColdAssets { font: if with_text { assets.font.clone() } else { None }, ..assets.clone() };
            let start = 1800000000 + if pal { 7777 } else { 0 };
            let mut input = host_inputs(pal, golden_now(start, 0), 60, true);
            input.gs_allocator = 770048;
            input.language = 2 + i32::from(pal);
            input.time_format = i32::from(pal);
            input.date_format = 2 - i32::from(pal);
            let mut cold = cold_start::<A>(&a, &input).unwrap();
            cold.clock.menus = None;
            cold.clock.pass_names = false;
            let mut clock = Clock::<A>::new(&cold.clock).unwrap();
            let mut f = cold.frame;
            f.config_items = None;
            for i in 0..300u64 {
                let c = console_now(golden_now(start, i), f.menu.mechacon_param.unwrap()[0]);
                if i > 0 {
                    f.time = scene::clock_state::ClockTime { milliseconds: c.milliseconds, seconds: c.local[5], minutes: c.local[4], hours: c.local[3] };
                    let l = c.local;
                    f.items = ClockItems { year: l[0], month: l[1], day: l[2], hour: l[3], minute: l[4], second: l[5] };
                }
                f.item0 = ((i / 100) % 3) as i32;
                if i % 50 == 0 {
                    f.time_filled = Some(((i / 50) % 2) as i32);
                } else if i % 50 == 25 {
                    f.time_filled = None;
                }
                f.display_index = (i & 1) as i32;
                f.field = ((i / 3) & 1) as i32;
                let frame = clock.frame(&f);
                h.u64(hash_frame(&frame));
                if clock.text().is_some() {
                    let image = glyph_cache_image(assets.font.as_ref().unwrap(), &frame.glyphs).unwrap();
                    let mut x = common::FrameHasher(0xcbf29ce484222325);
                    x.bytes(&image);
                    h.u64(x.0);
                }
                h.u32(clock.strings().len() as u32);
            }
        }
    }
    h
}

fn menus<A: Arithmetic>(assets: &ColdAssets) -> Hash {
    let mut h = Hash::default();
    let mut r = Rng::new(0x3e95);
    let mut u = |n: u32| r.next32() % n;
    for _ in 0..40 {
        let pal = u(2) != 0;
        let when = golden_now(1700000000 + i64::from(u(100000000)), 0);
        let zone = u(1000) as i32 - 500;
        let summer = u(2) != 0;
        let mut input = host_inputs(pal, when, zone, summer);
        input.date_format = u(3) as i32;
        input.aspect = u(3) as i32;
        let cold = cold_start::<A>(&ColdAssets { font: None, ..assets.clone() }, &input).unwrap();
        let menus_in = cold.clock.menus.as_ref().unwrap();
        let mut clock = cold.clock.state;
        let mut head = cold.clock.head;
        let mut fade = cold.clock.orbs.sprite_fade;
        let mut m = menus_in.menus.clone();
        let mut q = menus_in.cubes;
        let mut items = menus_in.items;
        let mut sounds = Vec::new();
        let mut rtc = None;
        let menu = Menus::new(MenusOptions { browser_enters: u(2) != 0 });
        let bits = [pad::UP, pad::DOWN, pad::LEFT, pad::RIGHT, pad::CROSS, pad::CIRCLE, pad::SQUARE, pad::TRIANGLE];
        for _ in 0..400 {
            let mut ext = cold.frame.menu;
            if u(20) == 0 {
                ext.config_dirty = None;
            } else if u(5) == 0 {
                ext.config_dirty.as_mut().unwrap()[u(3) as usize] = 1;
            }
            if u(30) == 0 {
                ext.rtc_mirror = None;
            }
            if u(40) == 0 {
                ext.mechacon_param = None;
            }
            if u(50) == 0 {
                ext.disc = u(0x80) as i32;
            }
            ext.pad.pressed = if u(3) == 0 { bits[u(8) as usize] } else { 0 };
            ext.pad.repeating = if u(4) == 0 { bits[u(8) as usize] } else { ext.pad.pressed };
            if u(200) == 0 {
                clock.scene.leaving = 1;
            }
            if u(150) == 0 {
                Menus::show(&mut m.dialog_ramp);
            }
            if u(150) == 0 {
                Menus::hide(&mut m.dialog_ramp);
            }
            if u(150) == 0 {
                Menus::show(&mut m.first_run_ramp);
            }
            if u(100) == 0 {
                let mode = u(5) as i32;
                let mut w = MenuWorld {
                    clock: &mut clock,
                    head: &mut head,
                    sprite_fade: &mut fade,
                    menus: &mut m,
                    cubes: &mut q,
                    items: &mut items,
                    width: 640,
                    height: cold.clock.height,
                    sounds: Some(&mut sounds),
                    rtc_write: Some(&mut rtc),
                };
                Menus::set_mode(&mut w, mode);
            }
            if u(60) == 0 {
                clock.mode = u(4) as i32;
            }
            if u(60) == 0 {
                clock.overlay_level = if u(2) != 0 { 0x80 - clock.tail } else { 0x80 };
            }
            sounds.clear();
            rtc = None;
            let mut notes = Vec::new();
            let mut w = MenuWorld {
                clock: &mut clock,
                head: &mut head,
                sprite_fade: &mut fade,
                menus: &mut m,
                cubes: &mut q,
                items: &mut items,
                width: 640,
                height: cold.clock.height,
                sounds: Some(&mut sounds),
                rtc_write: Some(&mut rtc),
            };
            let steps: [(u32, u32); 4] = [(1, 0xe1), (2, 0xe2), (3, 0xe3), (4, 0xe4)];
            for (ok, failed) in steps {
                let result = catch_unwind(AssertUnwindSafe(|| match ok {
                    1 => Menus::between(&mut w, &ext, &mut notes),
                    2 => Menus::menu_step(&mut w, &ext),
                    3 => menu.step(&mut w, &mut ext, &mut notes),
                    _ => Menus::end_of_frame(&mut w, &ext),
                }));
                h.u32(if result.is_ok() { ok } else { failed });
            }
            hworld(&mut h, &w);
            h.u32(sounds.len() as u32);
            for s in &sounds {
                for x in [s.id, s.a1, s.a2, s.a3] {
                    h.u32(x.into());
                }
                h.boolean(s.carried_a2);
                h.boolean(s.carried_a3);
            }
            h.boolean(rtc.is_some());
            if let Some(words) = rtc {
                for x in words {
                    h.i32(x);
                }
            }
            h.u32(notes.len() as u32);
            for n in &notes {
                hbytes(&mut h, n.as_bytes());
            }
            if let Some(d) = ext.config_dirty {
                for x in d {
                    h.i32(x);
                }
            }
        }
    }
    h
}

fn run<A: Arithmetic>(tag: &str) {
    let Some(folder) = resources() else { return };
    let assets = cold_assets(&folder);
    let mut nofont = Hash::default();
    let without = ColdAssets { font: None, ..assets.clone() };
    for i in [0, 2] {
        nofont.u64(common::scenarios::run_assets::<A>(&without, &common::scenarios::SCENARIOS[i], &mut Default::default()).h);
    }
    for (name, h) in [("alone", alone::<A>(&assets)), ("menus", menus::<A>(&assets)), ("nofont", nofont)] {
        let name = format!("{tag} {name}");
        let want = EXPECTED.iter().find(|e| e.0 == name).unwrap();
        assert_eq!((h.count, h.h), (want.1, want.2), "{name}");
    }
}

#[test]
fn native_extra() {
    run::<NativeArithmetic>("native");
}

#[test]
fn ee_extra() {
    run::<EeArithmetic>("ee");
}
