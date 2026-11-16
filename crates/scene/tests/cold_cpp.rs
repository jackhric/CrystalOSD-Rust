mod common;

use common::hashes::*;
use common::{Hash, Rng, cold_assets, resources};
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic};
use scene::cold_font::cold_font;
use scene::cold_start::{ColdAssets, ColdStartOut, cold_start};
use scene::cold_types::{ColdInputs, RtcTime};

// FNV-1a 64 over every field of ColdStartOut (and of coldFont's output), from the C++ at commit 039cc58 driven by the same
// generator below: 20000 cold starts without the font, 64 with it (each also through coldFont). The three C++ builds give
// the same hashes: EeArithmetic (strict build), NativeArithmetic under EeRounding in the strict build and in the app's
// build (ClockSceneNative).
const EXPECTED: [(&str, u64, u64); 2] = [("cold", 20000, 0x16506d8b5d0460c0), ("cold font", 128, 0x1ea26955c384fed9)];

fn hcold(o: &ColdStartOut) -> u64 {
    let mut h = Hash::default();
    let c = &o.clock;
    hclock(&mut h, &c.state);
    hhead(&mut h, &c.head);
    hrecord(&mut h, &c.rod_template);
    for r in &c.orbs.rings {
        h.i32(r.head);
        h.i32(r.count);
        h.i32(r.full);
        for e in &r.entries {
            h.f32(e.x);
            h.f32(e.y);
            h.f32(e.z);
            hc(&mut h, &e.colour);
        }
    }
    hramp(&mut h, &c.orbs.sprite_fade);
    h.f32(c.orbs.fraction);
    hc(&mut h, &c.clear_colour);
    hc(&mut h, &c.first_display_clear);
    let t = &c.tube;
    for x in [t.near, t.turn, t.scroll, t.ripple, t.scroll_end, t.turn_end, t.far, t.radius] {
        h.f32(x);
    }
    h.f32(c.minute_factor);
    h.f32(c.fraction_easing);
    hc(&mut h, &c.orb_colour);
    h.boolean(c.has_entry_data);
    h.i32(c.wide);
    for x in c.orb_random {
        h.i32(x);
    }
    for x in &c.orb_colours {
        hc(&mut h, x);
    }
    h.i32(c.width);
    h.i32(c.height);
    h.boolean(c.font.is_some());
    h.boolean(c.program.is_some());
    hcache(&mut h, &c.text.cache);
    hstate(&mut h, &c.text.font);
    hramps(&mut h, &c.text.ramps);
    h.i32(c.text.settings.time_format);
    h.i32(c.text.settings.date_format);
    h.u32(c.text.settings.settings_word);
    h.i32(c.text.settings.video_mode);
    let mi = c.menus.as_ref().unwrap();
    let m = &mi.menus;
    let p = &m.page;
    for x in [p.word0, p.count, p.title_width, p.selected, p.word14, p.level, p.word2c, p.word30, p.glow] {
        h.i32(x);
    }
    h.u32(p.entries);
    hramp(&mut h, &p.ramp);
    for e in &m.entries {
        hentry(&mut h, e);
    }
    let mm = &m.main_menu;
    for x in [mm.word0, mm.count, mm.word0c, mm.selected, mm.word14] {
        h.i32(x);
    }
    h.u32(mm.items);
    hramp(&mut h, &mm.ramp);
    hramp(&mut h, &m.version_ramp);
    hramp(&mut h, &m.dialog_ramp);
    hramp(&mut h, &m.first_run_ramp);
    hversion(&mut h, &m.version);
    h.u32(m.version_list.len() as u32);
    for r in &m.version_list {
        hrow(&mut h, r);
    }
    hversion(&mut h, &m.version_drawn);
    hramp(&mut h, &m.version_ramp_drawn);
    h.boolean(m.version_drawn_valid);
    for x in m.page_pointers {
        h.i32(x);
    }
    h.i32(m.entry_active);
    for x in m.menu_lengths {
        h.i32(x);
    }
    let l = &m.list_constants;
    for x in [l.rate, l.divisor, l.pulse, l.standing] {
        h.f32(x);
    }
    h.i32(m.screen_code);
    for f in &m.adjust_fields {
        h.i32(f.item);
        h.i32(f.lowest);
        h.i32(f.highest);
    }
    h.boolean(m.config_gate.is_some());
    h.i32(m.config_gate.unwrap_or(0));
    for x in [m.list_fade.first, m.list_fade.second, m.list_fade.second_index, m.body, m.video_mode] {
        h.i32(x);
    }
    let q = &mi.cubes;
    hramp(&mut h, &q.ramp);
    h.f32(q.list.pulse);
    for x in [q.list.pulsed, q.list.position, q.list.left, q.list.speed, q.list.slowing] {
        h.i32(x);
    }
    hc(&mut h, &q.colours.selected);
    hc(&mut h, &q.colours.plain);
    hc(&mut h, &q.colours.live);
    hrecord(&mut h, &q.record);
    for x in q.constants {
        h.f32(x);
    }
    h.f32(q.centre_factors[0]);
    h.f32(q.centre_factors[1]);
    hm(&mut h, &q.view);
    hm(&mut h, &q.screen);
    hc(&mut h, &q.layer_clear);
    hrect(&mut h, &q.added);
    hrect(&mut h, &q.half);
    hrect(&mut h, &q.chain);
    for x in mi.items {
        h.i32(x);
    }
    h.boolean(mi.options.browser_enters);
    let f = &o.frame;
    h.f32(f.time.milliseconds);
    h.i32(f.time.seconds);
    h.i32(f.time.minutes);
    h.i32(f.time.hours);
    h.i32(f.field);
    h.i32(f.display_index);
    h.i32(f.item0);
    for x in [f.items.year, f.items.month, f.items.day, f.items.hour, f.items.minute, f.items.second] {
        h.i32(x);
    }
    h.i32(f.menu.disc);
    h.boolean(f.menu.config_dirty.is_some());
    for x in f.menu.config_dirty.unwrap() {
        h.i32(x);
    }
    for x in f.menu.rtc_mirror.unwrap() {
        h.i32(x);
    }
    let param = f.menu.mechacon_param.unwrap();
    h.u32(param[0]);
    h.u32(param[1]);
    for x in f.config_items.unwrap() {
        h.i32(x);
    }
    h.boolean(f.time_filled.is_some());
    h.boolean(f.thread_step);
    h.h
}

fn cold_case(r: &mut Rng, i: u32) -> ColdInputs {
    let mut c = ColdInputs { pal: r.next32() & 1 != 0, ..Default::default() };
    c.language = (r.next32() % 9) as i32;
    if i.is_multiple_of(17) {
        c.language = (r.next32() % 32) as i32;
    }
    c.city = (r.next32() % 0x90) as i32;
    c.aspect = (r.next32() % 4) as i32;
    c.time_zone = (r.next32() % 1501) as i32 - 720;
    c.summer_time = r.next32() & 1 != 0;
    c.date_format = (r.next32() % 4) as i32;
    c.time_format = (r.next32() % 2) as i32;
    let year = (2000 + r.next32() % 100) as i32;
    let month = (1 + r.next32() % 12) as i32;
    let day = (1 + r.next32() % 28) as i32;
    let hour = (r.next32() % 24) as i32;
    let minute = (r.next32() % 60) as i32;
    let second = (r.next32() % 60) as i32;
    c.rtc = RtcTime { year, month, day, hour, minute, second };
    if i.is_multiple_of(5) {
        let year = 2000 + 99 * (r.next32() & 1) as i32;
        let month = 1 + 11 * (r.next32() & 1) as i32;
        let a = (r.next32() & 1) as i32;
        let day = 1 + 30 * a - (r.next32() & 1) as i32;
        let hour = 23 * (r.next32() & 1) as i32;
        let minute = 59 * (r.next32() & 1) as i32;
        let second = 59 * (r.next32() & 1) as i32;
        c.rtc = RtcTime { year, month, day, hour, minute, second };
    }
    c.rand_state = r.next32();
    c.screen_width = 640;
    c.screen_height = if c.pal { 0x100 } else { 0xE0 };
    c.wide = r.next32() & 1 != 0;
    c.screen_code = r.next32() % 0x80;
    c.field = r.next32() & 1;
    c.gs_allocator = r.next32() % 0x100000;
    c
}

fn run<A: Arithmetic>() {
    let Some(folder) = resources() else { return };
    let with_font = cold_assets(&folder);
    let no_font = ColdAssets { font: None, ..with_font.clone() };
    let mut all = Hash::default();
    let mut r = Rng::new(0xc01d5747);
    for i in 0..20000 {
        all.u64(hcold(&cold_start::<A>(&no_font, &cold_case(&mut r, i)).unwrap()));
    }
    let mut font = Hash::default();
    let mut rf = Rng::new(0xf0e75747);
    for i in 0..64 {
        let c = cold_case(&mut rf, i);
        font.u64(hcold(&cold_start::<A>(&with_font, &c).unwrap()));
        let fo = cold_font::<A>(with_font.program.clone(), with_font.font.clone().unwrap(), c.pal, c.gs_allocator).unwrap();
        let mut g = Hash::default();
        hcache(&mut g, &fo.cache);
        hstate(&mut g, &fo.state);
        for w in fo.widths {
            g.i32(w);
        }
        font.u64(g.h);
    }
    for (name, h) in [("cold", all), ("cold font", font)] {
        let want = EXPECTED.iter().find(|e| e.0 == name).unwrap();
        assert_eq!((h.count, h.h), (want.1, want.2), "{name}");
    }
}

#[test]
fn native_cold_start() {
    run::<NativeArithmetic>();
}

#[test]
fn ee_cold_start() {
    run::<EeArithmetic>();
}
