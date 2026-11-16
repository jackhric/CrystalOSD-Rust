// The Rust device row, Format dialog and Browser sound senders (scene::browser::{devices, format, sounds}) against the console's
// probed calls (the probes of verify_browser_devices.mjs, verify_browser_format.mjs and verify_browser_sounds.mjs, exported by
// tools/browser/export_browser_fixture.mjs). Skips without the fixtures.
mod console;

use std::collections::BTreeMap;

use console::fixture;
use console::need;
use console::probes::{Capture, Image, Record, load};
use scene::arithmetic::EeArithmetic;
use scene::browser::devices::{CursorState, Layout, SlotInfo, cursor_step, free_text, is_first_slot, scan_store, status_line, title_ids};
use scene::browser::format::{format_draw, format_step};
use scene::browser::sounds::{self, MoveFlags, RowError, RowWords};

type L = Layout<EeArithmetic>;

#[derive(Default)]
struct Tally {
    counts: BTreeMap<&'static str, usize>,
    differ: Vec<String>,
}

impl Tally {
    fn check(&mut self, section: &'static str, ok: bool, what: impl FnOnce() -> String) {
        *self.counts.entry(section).or_default() += 1;
        if !ok {
            self.differ.push(format!("{section}: {}", what()));
        }
    }
}

fn mem_s32(r: &Record, slot: usize, at: usize) -> i32 {
    i32::from_le_bytes(r.mem[slot].1[at..at + 4].try_into().unwrap())
}

fn mem_f32(r: &Record, slot: usize, at: usize) -> f32 {
    f32::from_le_bytes(r.mem[slot].1[at..at + 4].try_into().unwrap())
}

struct Pcs {
    is_first: u32,
    is_first_end: u32,
    scan_store: u32,
    scan_arrays: u32,
    title: u32,
    title_end: u32,
    title_range: (u32, u32),
    string: u32,
    layout: u32,
    item: u32,
    item_index: usize,
    phase: u32,
    input: u32,
    enqueue: u32,
    sound: (u32, u32),
    record_stride: u32,
    count: u32,
    scroll: u32,
}

// verify_browser_devices.mjs SPEC and CONFIG.
const HDD: Pcs = Pcs {
    is_first: 0x0024_93a8,
    is_first_end: 0x0024_93dc,
    scan_store: 0x0024_bae0,
    scan_arrays: 0x0024_bb00,
    title: 0x0025_3d38,
    title_end: 0x0025_3f84,
    title_range: (0x0025_3d38, 0x0025_3f88),
    string: 0x0020_81b8,
    layout: 0x0025_52e0,
    item: 0x0025_79a8,
    item_index: 16,
    phase: 0x0025_9ab8,
    input: 0x0025_4840,
    enqueue: 0x0020_0c00,
    sound: (0x6300, 0x0025_4f54),
    record_stride: 0x1c0,
    count: 0x1a0,
    scroll: 0x198,
};
const ROM: Pcs = Pcs {
    is_first: 0x0024_3368,
    is_first_end: 0x0024_339c,
    scan_store: 0x0024_53b0,
    scan_arrays: 0x0024_53c8,
    title: 0x0024_d8b0,
    title_end: 0x0024_da3c,
    title_range: (0x0024_d8b0, 0x0024_da44),
    string: 0x0020_4fd0,
    layout: 0x0024_f400,
    item: 0x0025_2670,
    item_index: 17,
    phase: 0x0025_2f38,
    input: 0x0024_e440,
    enqueue: 0x0020_0be8,
    sound: (0x5200, 0x0024_eb68),
    record_stride: 0x160,
    count: 0x140,
    scroll: 0x138,
};

fn of(c: &Capture, pc: u32) -> Vec<(usize, &Record)> {
    c.records.iter().enumerate().filter(|(_, r)| r.pc == pc).collect()
}

fn devices(c: &Capture, t: &mut Tally) {
    let p = if c.rom { &ROM } else { &HDD };
    let ends = of(c, p.is_first_end);
    for (k, (_, entry)) in of(c, p.is_first).into_iter().enumerate() {
        let (kind, free) = (entry.gpr[4] as i32, entry.gpr[5] as i32);
        let want = is_first_slot(kind, free).expect("a slot state the facts cover");
        let got = ends.get(k).map(|(_, e)| e.gpr[5] as i32);
        t.check("scan.kb", got == Some(want), || format!("isfirstslot({kind}, {free}) = {got:?}, the port {want}"));
    }
    let arrays = of(c, p.scan_arrays);
    for (at, store) in of(c, p.scan_store) {
        let slot = store.gpr[20] as usize;
        let o = slot * 16;
        let info =
            SlotInfo { kind: mem_s32(store, 0, o), free: mem_s32(store, 0, o + 4), format: mem_s32(store, 0, o + 8), multitap: mem_s32(store, 0, o + 12) };
        let Some((_, done)) = arrays.iter().find(|(a, _)| *a > at) else { continue };
        let want = scan_store(&info).expect("a slot state the facts cover");
        let got = (mem_s32(done, 0, slot * 4), mem_s32(done, 1, slot * 4), mem_s32(done, 2, slot * 4));
        t.check("scan.arrays", got == (want.kind, want.kilobytes, want.formatted), || format!("slot {slot}: {got:?}, the port {want:?}"));
    }
    let strings = of(c, p.string);
    let title_ends = of(c, p.title_end);
    for (at, call) in of(c, p.title) {
        let end = title_ends.iter().find(|(a, _)| *a > at).map_or(usize::MAX, |(a, _)| *a);
        let ids: Vec<i32> = strings
            .iter()
            .filter(|(a, s)| *a > at && *a < end && s.gpr[31] >= p.title_range.0 && s.gpr[31] < p.title_range.1)
            .map(|(_, s)| s.gpr[4] as i32)
            .collect();
        let want = title_ids(call.gpr[4] as i32, call.gpr[5] as i32, c.rom).expect("a device the facts cover");
        t.check("title", ids == want, || format!("device {} type {}: {ids:x?}, the port {want:x?}", call.gpr[4], call.gpr[5]));
    }
    for (_, r) in of(c, p.layout) {
        let want = L::page(mem_s32(r, 0, 8), mem_s32(r, 3, 0), mem_s32(r, 0, 0)).expect("a page the facts cover");
        let got = (mem_s32(r, 0, 12), mem_s32(r, 0, 16), mem_f32(r, 1, 0).to_bits(), mem_f32(r, 2, 0).to_bits(), mem_f32(r, 2, 4).to_bits());
        t.check("layout", got == (want.cols, want.rows, want.d8.to_bits(), want.f0.to_bits(), want.f4.to_bits()), || format!("{got:?}, the port {want:?}"));
    }
    for (_, r) in of(c, p.item) {
        let want = L::item(r.gpr[p.item_index] as i32, mem_s32(r, 2, 0), mem_f32(r, 1, 0), mem_f32(r, 1, 4));
        let got = (mem_f32(r, 0, 0), mem_f32(r, 0, 4));
        t.check("item", got.0.to_bits() == want.x.to_bits() && got.1.to_bits() == want.y.to_bits(), || format!("{got:?}, the port {want:?}"));
    }
    let phases = of(c, p.phase);
    for pair in phases.windows(2) {
        let (a, b) = (pair[0].1, pair[1].1);
        let want = L::next_phase(mem_f32(a, 0, 0), mem_f32(a, 2, 0)).expect("a phase below 2^20");
        t.check("phase", mem_f32(b, 0, 0).to_bits() == want.to_bits() && mem_f32(b, 1, 0).to_bits() == mem_f32(a, 0, 0).to_bits(), || {
            format!("{} -> {}", mem_f32(a, 0, 0), mem_f32(b, 0, 0))
        });
    }
    if !c.rom {
        let flags = of(c, 0x0025_1880);
        for (at, r) in of(c, 0x0025_1890) {
            let device = mem_s32(r, 5, 0);
            let flag = if device != 0 { flags.iter().rfind(|(a, _)| *a < at).map_or(0, |(_, f)| f.gpr[2] as i32) } else { 0 };
            let want =
                status_line(flag, device, mem_s32(r, 0, 0), mem_s32(r, 1, 0), mem_s32(r, 2, 0), mem_s32(r, 3, 0), mem_s32(r, 4, 0)).expect("an NTSC line");
            t.check("status", r.gpr[16] as i32 == want.id && r.gpr[6] as i32 == want.alpha, || {
                format!("id {:#x} alpha {}, the port {want:?}", r.gpr[16], r.gpr[6] as i32)
            });
        }
        let exits: Vec<(usize, &Record)> = [0x0025_401c, 0x0025_4044, 0x0025_4090, 0x0025_40b8].iter().flat_map(|&pc| of(c, pc)).collect();
        for (at, entry) in of(c, 0x0025_3fa0) {
            let kb = entry.gpr[5] as i32;
            let want = free_text(kb).expect("under 10000 KB");
            let exit = exits.iter().filter(|(a, _)| *a > at).min_by_key(|(a, _)| *a).map(|(_, e)| *e);
            let next_exit = exit.map_or(usize::MAX, |e| c.records.iter().position(|r| std::ptr::eq(r, e)).unwrap());
            let ids: Vec<i32> = strings
                .iter()
                .filter(|(a, s)| *a > at && *a < next_exit && s.gpr[31] >= 0x0025_3fa0 && s.gpr[31] < 0x0025_4100)
                .map(|(_, s)| s.gpr[4] as i32)
                .collect();
            let ok = exit.is_some_and(|e| e.pc == want.exit && e.gpr[6] as i32 == want.args[0].unwrap() && want.args[1].is_none_or(|b| e.gpr[7] as i32 == b))
                && ids == vec![want.id];
            t.check("free", ok, || format!("{kb} KB: the port {want:?}"));
        }
    }
    // Left and Right (verify_browser_devices.mjs cursor and sound): a frame's input against the next frame's cursor.
    let inputs: BTreeMap<i32, &Record> = of(c, p.input).into_iter().filter(|(_, r)| r.mem.len() == 14).map(|(_, r)| (r.frame, r)).collect();
    let mut moves: BTreeMap<i32, usize> = BTreeMap::new();
    for (_, e) in of(c, p.enqueue).into_iter().filter(|(_, e)| e.mem.is_empty()) {
        if e.gpr[4] == p.sound.0 && e.gpr[6] == 6 && e.gpr[31] == p.sound.1 {
            *moves.entry(e.frame).or_default() += 1;
        }
    }
    for (&frame, r) in &inputs {
        let Some(next) = inputs.get(&(frame + 1)) else { continue };
        if mem_s32(next, 9, 0) != mem_s32(r, 9, 0) || mem_s32(next, 10, 0) != mem_s32(r, 10, 0) {
            continue;
        }
        let rec = (p.record_stride * mem_s32(r, 9, 0) as u32) as usize;
        let s = CursorState {
            cursor: mem_s32(r, 1, 0),
            first: mem_s32(r, 2, 0),
            dialog: mem_s32(r, 3, 0),
            page_in: mem_s32(r, 4, 0),
            hold: i32::from(r.mem[5].1[0]),
            slide: mem_s32(r, 6, 0),
            slide_b: mem_s32(r, 6, 4),
            pal: mem_s32(r, 7, 0),
            mode: if c.rom { if mem_s32(r, 8, 0) != 0 { 2 } else { 0 } } else { mem_s32(r, 8, 0) },
            device: if c.rom { 0 } else { mem_s32(r, 10, 0) },
            list_count: mem_s32(r, 11, 0),
            busy: mem_s32(r, 11, 4),
            edge: mem_s32(r, 0, 4) as u32,
            repeat: mem_s32(r, 0, 12) as u32,
            record_count: mem_s32(r, 13, rec + p.count as usize),
            scroll_top: mem_s32(r, 13, rec + p.scroll as usize),
        };
        let want = cursor_step(&s);
        if !want.modelled {
            *t.counts.entry("unmodelled frames").or_default() += 1;
            continue;
        }
        t.check("cursor", mem_s32(next, 1, 0) == want.cursor, || {
            format!("frame {frame}: cursor {} -> {}, the port {}", s.cursor, mem_s32(next, 1, 0), want.cursor)
        });
        let sent = moves.get(&frame).copied().unwrap_or(0);
        t.check("sound", sent == usize::from(want.sound), || format!("frame {frame}: {sent} move sounds, the port {}", want.sound));
    }
}

fn format_dialog(c: &Capture, t: &mut Tally) {
    let logic: BTreeMap<i32, &Record> = of(c, 0x0025_5c18).into_iter().filter(|(_, r)| r.mem.len() == 8).map(|(_, r)| (r.frame, r)).collect();
    let mut sounds_at: BTreeMap<i32, Vec<u16>> = BTreeMap::new();
    for (_, e) in of(c, 0x0020_0c00) {
        if e.gpr[31] >= 0x0025_5c18 && e.gpr[31] < 0x0025_5e4c && e.gpr[4] == 0x6300 && e.gpr[5] == 1 {
            sounds_at.entry(e.frame).or_default().push(e.gpr[6] as u16);
        }
    }
    for (&frame, r) in &logic {
        let state = mem_s32(r, 0, 4);
        let cursor = i32::from(r.mem[0].1[0]);
        let want = format_step(state, cursor, mem_s32(r, 4, 0) as u32, mem_s32(r, 2, 0), i32::from(r.mem[3].1[0]));
        let got = sounds_at.get(&frame).cloned().unwrap_or_default();
        t.check("format sound", got == want.sounds, || format!("frame {frame}: {got:?}, the port {:?}", want.sounds));
        if let Some(next) = logic.get(&(frame + 1)) {
            let got = (mem_s32(next, 0, 4), i32::from(next.mem[0].1[0]), mem_s32(next, 1, 0));
            t.check("format step", got == (want.state, want.cursor, want.dialog), || format!("frame {frame}: {got:?}, the port {want:?}"));
        }
    }
    let renders = of(c, 0x0025_5868);
    let strings = of(c, 0x0020_81b8);
    let heights: Vec<(usize, &Record)> = [0x0024_d550, 0x0024_d900].iter().flat_map(|&pc| of(c, pc)).collect();
    for (k, (at, r)) in renders.iter().enumerate() {
        if r.mem.len() < 6 {
            continue;
        }
        let last = renders.get(k + 1).map_or(usize::MAX, |(a, _)| *a);
        let inside = |rec: &Record| rec.gpr[31] >= 0x0025_5868 && rec.gpr[31] < 0x0025_59fc;
        let ids: Vec<i32> = strings.iter().filter(|(a, s)| a > at && *a < last && inside(s)).map(|(_, s)| s.gpr[4] as i32).collect();
        let mut hs: Vec<(usize, i32)> = heights.iter().filter(|(a, s)| a > at && *a < last && inside(s)).map(|(a, s)| (*a, s.gpr[5] as i32)).collect();
        hs.sort();
        let want = format_draw(mem_s32(r, 0, 4), i32::from(r.mem[0].1[0])).expect("a drawn state");
        let hs: Vec<i32> = hs.into_iter().map(|(_, h)| h).collect();
        t.check("format draw", ids == want.ids && hs == want.heights, || format!("ids {ids:x?} heights {hs:x?}, the port {want:?}"));
        let line = status_line(0, 0, mem_s32(r, 1, 0), mem_s32(r, 2, 0), mem_s32(r, 3, 0), mem_s32(r, 4, 0), mem_s32(r, 5, 0)).expect("an NTSC line");
        t.check("format alpha", r.gpr[4] as i32 == line.alpha, || format!("alpha {}, the port {}", r.gpr[4] as i32, line.alpha));
    }
}

// The device row's senders (verify_browser_sounds.mjs, the Browser half): each func_00254840 call's words, the port's commands
// against the gate calls that follow it (a2 and return address).
fn row_sounds(c: &Capture, t: &mut Tally) {
    let image = |r: &Record| Image::of([r]);
    let mut calls: Vec<(usize, &Record)> = Vec::new();
    for (k, r) in c.records.iter().enumerate() {
        let root = r.pc == 0x0025_4840 && r.mem.len() == 8 && r.mem[7].0 == 0x0037_0770 && r.mem[7].1.len() == 8;
        let format = r.pc == 0x0025_5c18 && r.mem.len() == 2 && r.mem[0].0 == 0x0037_0bbc;
        if root || format {
            calls.push((k, r));
        }
    }
    for (n, &(at, root)) in calls.iter().enumerate() {
        let end = calls.get(n + 1).map_or(c.records.len(), |(a, _)| *a);
        let between: Vec<&Record> = c.records[at + 1..end].iter().collect();
        if root.pc == 0x0025_5c18 {
            let edge = mem_s32(root, 1, 0) as u32;
            let step = format_step(mem_s32(root, 0, 4), i32::from(root.mem[0].1[0]), edge, 0, 0);
            let mine: Vec<(u16, u32)> = step.sounds.iter().map(|&a2| (a2, sounds::format_ra(a2, edge))).collect();
            let got: Vec<(u16, u32)> = between.iter().filter(|r| r.pc == 0x0024_e090).map(|g| (g.gpr[6] as u16, g.gpr[31])).collect();
            t.check("format dialog sounds", got == mine, || format!("frame {}: {got:x?}, the port {mine:x?}", root.frame));
            continue;
        }
        let e = image(root);
        let find = |pc: u32| between.iter().copied().find(|r| r.pc == pc);
        let mut w = RowWords {
            held: e.u32(0x0037_07a0),
            edge: e.u32(0x0037_07a4),
            repeat: e.u32(0x0037_07ac),
            dialog_word: e.s32(0x0037_0820),
            page_in: e.s32(0x0037_072c),
            pal: e.s32(0x0037_06b0),
            hold: i32::from(e.u8(0x0037_0678)),
            slide: e.s32(0x0037_0858),
            slide_b: e.s32(0x0037_085c),
            mode: e.s32(0x0037_06d0),
            back: i32::from(e.u8(0x0037_0771)),
            enter: i32::from(e.u8(0x0037_0772)),
            options: i32::from(e.u8(0x0037_0773)),
            ..RowWords::default()
        };
        if let Some(pg) = find(0x0025_48f4) {
            let i = image(pg);
            w.page_kind = Some(i.s32(pg.gpr[17].wrapping_add(0x124)));
            w.page_flags = Some(i.u32(pg.gpr[17].wrapping_add(0xf8)));
        }
        if let Some(pop) = find(0x0025_4938) {
            w.popped_kind = Some(pop.gpr[3] as i32);
        }
        if let Some(item) = find(0x0025_4a04) {
            let i = image(item);
            let base = item.gpr[18];
            w.count = Some(item.gpr[3] as i32);
            w.word600 = Some(i.s32(0x0037_0600));
            w.word604 = Some(i.s32(0x0037_0604));
            w.item90 = Some(i.u32(base.wrapping_add(0x90)));
            w.item_b0 = Some(i.s32(base.wrapping_add(0xb0)));
            w.item690 = Some(i.s32(base.wrapping_add(0x690)));
        }
        if let Some(f) = find(0x0025_4c90) {
            let i = image(f);
            let sp = f.gpr[29];
            w.flags = Some(MoveFlags { up: i.s32(sp), down: i.s32(sp + 4), left: i.s32(sp + 8), right: i.s32(sp + 0xc) });
        }
        if let Some(pr) = find(0x0025_4fc4) {
            w.predicate = Some(pr.gpr[2] as i32);
        }
        let want = match sounds::row(&w, 0x1a, 0x15) {
            Ok(list) => list,
            Err(RowError::NotProbed(what)) => panic!("frame {}: {what} was not probed on this path", root.frame),
            Err(RowError::NotReached(what)) => panic!("frame {}: {what} (read, not reached by a capture)", root.frame),
        };
        let gates: Vec<&Record> = between.iter().copied().filter(|r| r.pc == 0x0024_e090).collect();
        let got: Vec<(u16, u32)> = gates.iter().map(|g| (g.gpr[6] as u16, g.gpr[31])).collect();
        let mine: Vec<(u16, u32)> = want.iter().map(|s| (s.a2, s.ra)).collect();
        t.check("row sounds", got == mine, || format!("frame {}: {got:x?}, the port {mine:x?}", root.frame));
        for g in gates {
            let gi = image(g);
            t.check("gate", sounds::gate(gi.s32(0x0037_0740), gi.s32(0x0037_06f8), gi.s32(0x0037_0460)), || {
                format!("frame {}: a gated command the gate would drop", g.frame)
            });
        }
    }
}

fn report(name: &str, t: &Tally, needed: &[&str]) {
    for d in t.differ.iter().take(10) {
        eprintln!("{name}: {d}");
    }
    println!("{name}: {:?}, {} differ", t.counts, t.differ.len());
    assert!(t.differ.is_empty(), "{name}: {} items differ", t.differ.len());
    for section in needed {
        assert!(t.counts.get(section).copied().unwrap_or(0) > 0, "{name}: nothing compared for {section}");
    }
}

fn devices_of(name: &str, needed: &[&str]) {
    let path = fixture(name, "probes.json");
    if !need(&[&path]) {
        return;
    }
    let c = load(&path);
    let mut t = Tally::default();
    devices(&c, &mut t);
    if name.contains("format") || name.contains("formatted") {
        format_dialog(&c, &mut t);
    }
    report(name, &t, needed);
}

#[test]
fn device_row_against_the_console() {
    let base = ["scan.kb", "scan.arrays", "title", "layout", "item", "phase", "cursor", "sound"];
    devices_of("hddosd-110U-browser-devices-blank", &base);
    devices_of("hddosd-110U-browser-devices-nocard", &base);
    devices_of("rom-0230A-browser-devices-blank", &base);
    devices_of("hddosd-110U-browser-devices-card", &[&base[..], &["status", "free"]].concat());
}

#[test]
fn format_dialog_against_the_console() {
    let base = ["scan.kb", "title", "layout", "item", "cursor", "status", "format sound", "format step", "format draw", "format alpha"];
    devices_of("hddosd-110U-browser-format-no", &base);
    devices_of("hddosd-110U-browser-devices-formatted", &[&base[..], &["free"]].concat());
}

#[test]
fn row_sounds_against_the_console() {
    for name in ["hddosd-110U-browser-sounds-row", "hddosd-110U-browser-sounds-enter"] {
        let path = fixture(name, "probes.json");
        if !need(&[&path]) {
            return;
        }
        let c = load(&path);
        let mut t = Tally::default();
        row_sounds(&c, &mut t);
        report(name, &t, &["row sounds"]);
    }
}
