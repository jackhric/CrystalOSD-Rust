mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{Hash, Rng, cold_assets, resources};
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic};
use scene::cold_start::{ColdAssets, cold_start};
use scene::cold_types::{ColdInputs, RtcTime};
use scene::menu_types::{ConfigEntry, ListFade};
use scene::ramp::Ramp;
use scene::text::{ClockItems, FontState, PagesInputs, Text, TextFrame, TextFrameInputs, TextRamps};

// FNV-1a 64 from the C++ at commit 039cc58 driven as below. "strings": NTSC and PAL, languages 1 to 7, every id 0..0x240
// of the language's table whose pointer resolves, drawn through drawString (the cache carried; a string the C++ refuses,
// a cache laid out again inside it, is marked and the Text rebuilt) then measured. "text frames": 140 cold starts
// (language, video mode, time and date format, aspect, summer time drawn at random) with 24 random pages and frame inputs
// each. Native as the app builds it (equal to the strict build), Ee from the strict build.
const EXPECTED: [(&str, u64, u64); 4] = [
    ("native strings", 16107420, 0x6aac78ba8c27ec19),
    ("native text frames", 29616629, 0xace74163974b6f63),
    ("ee strings", 16107420, 0x0fcf651439629962),
    ("ee text frames", 29616629, 0xdff73bed8d5312bc),
];

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

fn htext(h: &mut Hash, f: &TextFrame) {
    h.u32(f.draws.len() as u32);
    for d in &f.draws {
        h.i32(d.kind as i32);
        h.i32(d.texture);
        for x in d.glyph.region {
            h.i32(x);
        }
        for v in &d.glyph.fan {
            for x in [v.x, v.y, v.s, v.t, v.q] {
                h.f32(x);
            }
            for c in v.colour {
                h.u32(c.into());
            }
        }
        let r = &d.icon;
        for x in r.colour {
            h.i32(x);
        }
        for x in [r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured] {
            h.i32(x);
        }
    }
    h.u32(f.strings.len() as u32);
    for s in &f.strings {
        hbytes(h, &s.text);
        h.boolean(s.measuring);
        hstate(h, &s.own);
    }
    let g = &f.glyphs;
    h.u32(g.address);
    for x in [g.cell_width, g.cell_height, g.width, g.log_width, g.log_height] {
        h.i32(x);
    }
    h.u32(g.cells.len() as u32);
    for &c in &g.cells {
        h.i32(c);
    }
}

struct Gen(Rng);

impl Gen {
    fn u(&mut self, n: u32) -> u32 {
        self.0.next32() % n
    }
    fn i(&mut self, n: u32) -> i32 {
        self.u(n) as i32
    }
    fn ramp(&mut self, length: i32) -> Ramp {
        let state = self.i(4);
        let counter = if state == 0 {
            0
        } else if state == 2 {
            length
        } else {
            self.i(length as u32 + 1)
        };
        Ramp { length, counter, changed: self.i(2), state }
    }
    fn ramps(&mut self) -> TextRamps {
        let mut t = TextRamps::default();
        let n = 1 + self.i(100);
        t.config = self.ramp(n);
        let n = 1 + self.i(20);
        t.main_menu = self.ramp(n);
        let n = 1 + self.i(20);
        t.version = self.ramp(n);
        if self.u(6) == 0 {
            let n = 1 + self.i(20);
            t.dialog_closing = self.ramp(n);
        }
        if self.u(6) == 0 {
            let n = 1 + self.i(20);
            t.first_run = self.ramp(n);
        }
        let n = 1 + self.i(20);
        t.dialog = self.ramp(n);
        t.lead = self.i(50);
        t.body = self.i(50);
        t.panel7 = i32::from(self.u(5) == 0);
        t.panel8_on = i32::from(self.u(3) == 0);
        t.panel8 = self.i(129);
        t.adjust_row = self.i(2);
        const IDS: [[i32; 4]; 4] = [[1, 1, 1, 1], [0x5e, 0x55, 0x56, 0x57], [1, 0x55, 1, 0x57], [0x5e, 1, 0x56, 1]];
        t.panel8_ids = IDS[self.u(4) as usize];
        t
    }
}

fn text_cold(language: i32, pal: bool, time_format: i32, date_format: i32, aspect: i32, summer: bool) -> ColdInputs {
    ColdInputs {
        pal,
        language,
        time_format,
        date_format,
        aspect,
        summer_time: summer,
        rtc: RtcTime { year: 2026, month: 3, day: 4, hour: 5, minute: 6, second: 7 },
        screen_height: if pal { 0x100 } else { 0xE0 },
        gs_allocator: 770048,
        ..Default::default()
    }
}

fn strings<A: Arithmetic>(assets: &ColdAssets) -> Hash {
    let program = &assets.program;
    let font = assets.font.clone().unwrap();
    let mut h = Hash::default();
    for pal in [false, true] {
        for language in 1..8 {
            let cold = cold_start::<A>(assets, &text_cold(language, pal, 0, 0, 0, false)).unwrap();
            let fresh = || Text::<A>::new(font.clone(), program.clone(), &cold.clock.text).unwrap();
            let mut text = fresh();
            let table = program.word(0x002ad200 + 4 * language as u32);
            for id in 0..0x240u32 {
                let Ok(s) = catch_unwind(|| program.string(program.word(table + 4 * id))) else {
                    h.u32(0xdead);
                    continue;
                };
                match catch_unwind(AssertUnwindSafe(|| text.draw_string(&s, 640, if pal { 256 } else { 224 }))) {
                    Ok(frame) => htext(&mut h, &frame),
                    Err(_) => {
                        h.u32(0xbad);
                        text = fresh();
                    }
                }
                h.i32(text.string_width(&s));
            }
            hstate(&mut h, text.font());
        }
    }
    h
}

fn frames<A: Arithmetic>(assets: &ColdAssets) -> Hash {
    let mut h = Hash::default();
    let mut g = Gen(Rng::new(0x7e47f4a3));
    for _ in 0..140 {
        let pal = g.u(2) != 0;
        let language = 1 + g.i(7);
        let time_format = g.i(2);
        let date_format = g.i(3);
        let aspect = g.i(3);
        let summer = g.u(2) != 0;
        let cold = cold_start::<A>(assets, &text_cold(language, pal, time_format, date_format, aspect, summer)).unwrap();
        let mut inputs = cold.clock.text.clone();
        if g.u(4) == 0 {
            inputs.ramps.config.state = 2;
        }
        let mut text = Text::<A>::new(assets.font.clone().unwrap(), assets.program.clone(), &inputs).unwrap();
        let m0 = &cold.clock.menus.as_ref().unwrap().menus;
        let height = if pal { 256 } else { 224 };
        for _ in 0..24 {
            let mut p = PagesInputs { main_menu: m0.main_menu, ..Default::default() };
            p.main_menu.selected = g.i(m0.main_menu.count as u32);
            let n = 1 + g.i(20);
            p.main_menu.ramp = g.ramp(n);
            if g.u(2) != 0 {
                p.main_menu.ramp.state = 2;
            }
            p.page = m0.page;
            p.page.selected = g.i(m0.page.count as u32);
            p.page.level = g.i(3);
            p.page.glow = g.i(2 * 0x7aa8) - 0x7aa8;
            p.entries = m0.entries;
            for e in p.entries.iter_mut() {
                let e: &mut ConfigEntry = e;
                e.value_index = if e.value_count > 0 { g.i(e.value_count as u32) } else { 0 };
            }
            let first = g.i(129);
            let second = if g.u(2) != 0 { 0 } else { g.i(129) };
            p.list_fade = ListFade { first, second, second_index: g.i(m0.page.count as u32) };
            p.list_constants = m0.list_constants;
            p.adjust_fields = m0.adjust_fields;
            p.items = cold.clock.menus.as_ref().unwrap().items;
            p.items[0] = g.i(3);
            p.items[6] = 2000 + g.i(100);
            p.items[7] = 1 + g.i(12);
            p.items[8] = 1 + g.i(31);
            p.items[9] = g.i(24);
            p.items[10] = g.i(60);
            p.items[11] = g.i(60);
            p.items[0xd] = g.i(2);
            p.items[0xe] = g.i(3);
            let n = 1 + g.i(30);
            p.menu_ramp = g.ramp(n);
            p.tail = 1 + g.i(20);
            p.overlay_level = g.i(129);
            p.ramps = g.ramps();
            if g.u(2) != 0 {
                p.ramps.config.state = 2;
            }
            p.width = 640;
            p.height = height;
            p.pending = i32::from(g.u(5) == 0);
            p.version = m0.version.clone();
            for (i, row) in m0.version_list.iter().enumerate() {
                p.version.rows[i] = row.clone();
            }
            p.version.count = g.i(4);
            p.version.shown = 1 + g.i(6);
            p.version.selected = g.i(4);
            p.version.first = g.i(3);
            if g.u(3) == 0 {
                let row = g.u(3) as usize;
                p.version.rows[row].value.clear();
            }
            if g.u(3) == 0 {
                let row = g.u(3) as usize;
                p.version.rows[row].label = 0;
            }
            let pf = text.pages(&p);
            htext(&mut h, &pf.text);
            h.boolean(pf.title_width.is_some());
            h.i32(pf.title_width.unwrap_or(0));
            h.u32(pf.unmodelled.len() as u32);
            for &x in &pf.unmodelled {
                h.u32(x);
            }
            let negative_year = g.u(8) == 0;
            let negative_hour = g.u(8) == 0;
            let year = if negative_year { -1 } else { 2000 + g.i(100) };
            let month = 1 + g.i(12);
            let day = 1 + g.i(31);
            let hour = if negative_hour { -1 } else { g.i(24) };
            let minute = g.i(60);
            let second = g.i(60);
            let mut t = TextFrameInputs { items: ClockItems { year, month, day, hour, minute, second }, ..Default::default() };
            t.item0 = g.i(3);
            t.overlay_level = g.i(129);
            t.tail = 1 + g.i(20);
            let n = 1 + g.i(30);
            t.menu = g.ramp(n);
            t.width = 640;
            t.height = height;
            if g.u(4) != 0 {
                t.ramps = Some(g.ramps());
            }
            htext(&mut h, &text.frame(&t));
            let fc = text.cache();
            h.u32(fc.list.len() as u32);
            for e in &fc.list {
                h.i32(e.code);
                h.i32(e.loaded);
                h.i32(e.cell);
                h.u32(e.block);
            }
            hstate(&mut h, text.font());
        }
    }
    h
}

fn run<A: Arithmetic>(tag: &str) {
    let Some(folder) = resources() else { return };
    let assets = cold_assets(&folder);
    for (name, h) in [("strings", strings::<A>(&assets)), ("text frames", frames::<A>(&assets))] {
        let name = format!("{tag} {name}");
        let want = EXPECTED.iter().find(|e| e.0 == name).unwrap();
        assert_eq!((h.count, h.h), (want.1, want.2), "{name}");
    }
}

#[test]
fn native_text() {
    run::<NativeArithmetic>("native");
}

#[test]
fn ee_text() {
    run::<EeArithmetic>("ee");
}
