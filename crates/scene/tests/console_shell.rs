// The Rust Browser shell (scene::browser::shell) carried from the module's start through every pass of a capture, against the
// words the console's master function reads in the next pass (the probes of verify_browser_enter.mjs / verify_browser_logic.mjs,
// exported by tools/browser/export_browser_fixture.mjs). Unlike the verifiers, which recompute each pass from the console's words,
// the shell here keeps its own state; it is re-seeded from the console only where the capture wrote a word (a stimulus) or a pass
// is missing. Skips without the fixtures.
mod console;

use console::fixture;
use console::need;
use console::probes::{Capture, Image, Record, load};
use scene::browser::shell::{ExitWords, Fade, Gate, PassInput, Shell};

struct Pcs {
    master: u32,
    start: u32,
    request: u32,
    alpha: u32,
    check: u32,
    exit_in: u32,
    exit_out: u32,
    open: Option<u32>,
}

// model/browser_shell.mjs A.
const HDD: Pcs = Pcs {
    master: 0x0025_3860,
    start: 0x0024_ce60,
    request: 0x0024_ebf0,
    alpha: 0x0024_cf54,
    check: 0x0024_f0a0,
    exit_in: 0x0024_ed20,
    exit_out: 0x0024_ed6c,
    open: Some(0x0025_bc08),
};
const ROM: Pcs = Pcs {
    master: 0x0024_d500,
    start: 0x0024_6810,
    request: 0x0024_8038,
    alpha: 0x0024_690c,
    check: 0x0024_84c8,
    exit_in: 0x0024_8160,
    exit_out: 0x0024_819c,
    open: None,
};

#[derive(Debug, Clone, Copy)]
struct Words {
    shell: Shell,
    gate: Gate,
    edge: u32,
    io: i32,
    dvd: i32,
    disc: i32,
}

fn fade_hdd(i: &Image) -> Fade {
    Fade { state: i.s32(0x0037_0660), count: i.s32(0x0037_066c), colour: i.s32(0x0037_0670), length: i.s32(0x0037_0674) }
}

fn fade_rom(i: &Image) -> Fade {
    Fade { state: i32::from(i.u8(0x002c_8bb4)), count: i.s32(0x002c_8bbc), colour: i.s32(0x002c_8bc0), length: i.s32(0x002c_8bc4) }
}

fn exit_hdd(i: &Image) -> ExitWords {
    ExitWords {
        destination: i.s32(0x001f_0648),
        previous: i.s32(0x001f_064c),
        execute: i.s32(0x001f_0010),
        flag: i.s32(0x001f_0644),
        word014: i.s32(0x001f_0014),
    }
}

fn exit_rom(i: &Image) -> ExitWords {
    ExitWords {
        destination: i.s32(0x001f_05e8),
        previous: i.s32(0x001f_05ec),
        execute: i.s32(0x001f_0014),
        flag: i.s32(0x001f_05e4),
        word014: i.s32(0x001f_0018),
    }
}

// model/browser_shell.mjs decode: the shell's words of a master-function record group.
fn decode(i: &Image, rom: bool) -> Words {
    if !rom {
        let depth = i.s32(0x0037_0728);
        return Words {
            shell: Shell {
                leave: i.s32(0x0037_06a8),
                pal: i.s32(0x0037_06b0),
                destination: i.s32(0x0037_06b4),
                execute: i.s32(0x0037_06b8),
                e8: i.s32(0x0037_08e8),
                ec: i.s32(0x0037_08ec),
                f0: i.s32(0x0037_08f0),
                be4: i.s32(0x0037_0be4),
                be8: i.s32(0x0037_0be8),
                fade: fade_hdd(i),
                exit: exit_hdd(i),
                mode: i.s32(0x0037_06d0),
                word68c: i.s32(0x0037_068c),
                byte625: i32::from(i.u8(0x0037_0625)),
            },
            gate: Gate {
                view: i.s32(0x0037_06f8),
                page_in: i.s32(0x0037_072c),
                hold: i32::from(i.u8(0x0037_0678)),
                dialog_word: i.s32(0x0037_0820),
                slide: i.s32(0x0037_0858),
                slide_b: i.s32(0x0037_085c),
                back_enabled: i32::from(i.u8(0x0037_0771)),
                page_kind: (depth == 0).then(|| i.s32(0x002b_d544)),
            },
            edge: i.u32(0x0037_07a4),
            io: i.s32(0x0037_05fc),
            dvd: i.s32(0x0037_0778),
            disc: i.s32(0x0037_0780),
        };
    }
    let depth = i.s32(0x002c_8cc8);
    Words {
        shell: Shell {
            leave: i.s32(0x002c_8c00),
            pal: i.s32(0x002c_8c08),
            destination: i.s32(0x002c_8c0c),
            execute: i.s32(0x002c_8c10),
            fade: fade_rom(i),
            exit: exit_rom(i),
            ..Shell::default()
        },
        gate: Gate {
            view: if i.s32(0x002c_8ce0) == 0 { 1 } else { 0 },
            page_in: i.s32(0x002c_8ccc),
            hold: i32::from(i.u8(0x002c_8bc8)),
            back_enabled: i32::from(i.u8(0x002c_8d09)),
            page_kind: (depth == 0).then(|| i.s32(0x002a_db44)),
            ..Gate::default()
        },
        edge: i.u32(0x002c_8d34),
        io: i.s32(0x002c_8b34),
        dvd: i.s32(0x002c_8d10),
        disc: i.s32(0x002c_8d18),
    }
}

#[derive(Default, Debug)]
struct Tally {
    compared: usize,
    differ: Vec<String>,
    passes: usize,
    alphas: usize,
    requests: usize,
    exits: usize,
    reseeds: usize,
}

impl Tally {
    fn check<T: PartialEq + std::fmt::Debug>(&mut self, what: &str, frame: i32, ours: T, theirs: T) {
        self.compared += 1;
        if ours != theirs {
            self.differ.push(format!("frame {frame}: {what}: shell {ours:?}, console {theirs:?}"));
        }
    }
}

fn same_words(t: &mut Tally, frame: i32, ours: &Shell, theirs: &Shell, rom: bool, c: &Capture) {
    t.check("leave counter", frame, ours.leave, theirs.leave);
    t.check("destination", frame, ours.destination, theirs.destination);
    t.check("execute type", frame, ours.execute, theirs.execute);
    t.check("exit words", frame, ours.exit, theirs.exit);
    t.check("fade state", frame, ours.fade.state, theirs.fade.state);
    t.check("fade count", frame, ours.fade.count, theirs.fade.count);
    t.check("fade length", frame, ours.fade.length, theirs.fade.length);
    if !rom {
        for (name, address, a, b) in [
            ("dialog counter", 0x0037_08f0, ours.f0, theirs.f0),
            ("dialog direction", 0x0037_0be4, ours.be4, theirs.be4),
            ("dialog kind", 0x0037_0be8, ours.be8, theirs.be8),
            ("HDD status 1", 0x0037_08e8, ours.e8, theirs.e8),
            ("HDD status 2", 0x0037_08ec, ours.ec, theirs.ec),
        ] {
            if !c.written(frame, address) {
                t.check(name, frame, a, b);
            }
        }
        t.check("mode word", frame, ours.mode, theirs.mode);
        t.check("D_0037068C", frame, ours.word68c, theirs.word68c);
        t.check("D_00370625", frame, ours.byte625, theirs.byte625);
    }
}

fn carry(name: &str, flip_video: bool) -> Option<Tally> {
    let path = fixture(name, "probes.json");
    if !need(&[&path]) {
        return None;
    }
    let c = load(&path);
    let rom = c.rom;
    let pcs = if rom { &ROM } else { &HDD };
    // model/browser_shell.mjs walk: a pass is the master's records and every record after them up to the next master (the leave
    // counter's test and the exit run after the wait for the next frame, so their records fall in the next capture frame).
    let mut passes: Vec<(i32, Vec<&Record>, Vec<&Record>)> = Vec::new();
    let mut before_first: Vec<&Record> = Vec::new();
    for r in &c.records {
        if r.pc == pcs.master {
            match passes.last_mut() {
                Some((frame, masters, events)) if *frame == r.frame && events.is_empty() => masters.push(r),
                _ => passes.push((r.frame, vec![r], vec![])),
            }
        } else if let Some((_, _, events)) = passes.last_mut() {
            events.push(r);
        } else {
            before_first.push(r);
        }
    }
    let masters: Vec<i32> = passes.iter().map(|p| p.0).collect();
    let event = |n: usize, pc: u32| -> Option<&Record> { passes[n].2.iter().copied().find(|r| r.pc == pc) };
    let mut t = Tally::default();
    let first = masters[0];
    let start = before_first.iter().copied().rfind(|r| r.pc == pcs.start).expect("the capture holds the thread start");
    let words0 = decode(&Image::of(passes[0].1.iter().copied()), rom);
    let fade_before = if rom { fade_rom(&Image::of([start])) } else { fade_hdd(&Image::of([start])) };
    let pal = (words0.shell.pal != 0) ^ flip_video;
    let (e8, ec) = match pcs.open.and_then(|pc| before_first.iter().copied().rfind(|r| r.pc == pc)) {
        Some(open) => {
            let i = Image::of([open]);
            (i.s32(0x0037_08e8), i.s32(0x0037_08ec))
        }
        None => (words0.shell.e8, words0.shell.ec),
    };
    let before = Shell { fade: fade_before, ..words0.shell };
    let mut s = Shell::start(pal, (!rom).then_some((e8, ec)), &before);
    t.check("24CE60 argument (the fade length)", first, s.fade.length, start.gpr[4] as i32);
    t.check("first pass: PAL flag", first, s.pal, words0.shell.pal);
    same_words(&mut t, first, &s, &words0.shell, rom, &c);
    for (n, &frame) in masters.iter().enumerate() {
        let w = decode(&Image::of(passes[n].1.iter().copied()), rom);
        let check = event(n, pcs.check).map(|r| Image::of([r]));
        let io = check.as_ref().map_or(w.io, |i| i.s32(if rom { 0x002c_8b34 } else { 0x0037_05fc }));
        let requested = event(n, pcs.request);
        let trail = s.step(&w.gate, &PassInput { edge: w.edge, dvd_error: w.dvd, disc_error: w.disc, io }, rom.then_some(requested.is_some()));
        t.passes += 1;
        t.check("leave requested", frame, trail.requested, requested.is_some());
        if trail.requested {
            t.requests += 1;
            t.check("destination argument", frame, 2, requested.map_or(-1, |r| r.gpr[4] as i32));
        }
        let alpha = event(n, pcs.alpha).map(|r| r.gpr[19] as i32);
        t.check("fade alpha", frame, trail.alpha, alpha);
        if alpha.is_some() {
            t.alphas += 1;
        }
        if let Some(i) = &check {
            t.check("counter at the check", frame, trail.checked, i.s32(if rom { 0x002c_8c00 } else { 0x0037_06a8 }));
        }
        t.check("exit", frame, trail.exited, event(n, pcs.exit_in).is_some());
        if trail.exited {
            t.exits += 1;
            if let Some(out) = event(n, pcs.exit_out).map(|r| Image::of([r])) {
                t.check("exit stores", frame, s.exit, if rom { exit_rom(&out) } else { exit_hdd(&out) });
            }
        }
        if let Some(&next) = masters.get(n + 1) {
            let nw = decode(&Image::of(passes[n + 1].1.iter().copied()), rom);
            if next == frame + 1 {
                same_words(&mut t, next, &s, &nw.shell, rom, &c);
            }
            if next != frame + 1 || c.writes.iter().any(|x| x.frame == next) {
                s = Shell { pal: s.pal, ..nw.shell };
                t.reseeds += 1;
            }
        }
    }
    Some(t)
}

fn run(name: &str) {
    let Some(t) = carry(name, false) else { return };
    for d in t.differ.iter().take(10) {
        eprintln!("{name}: {d}");
    }
    println!(
        "{name}: {} passes carried, {} of {} items equal; {} fade alphas, {} leave requests, {} exits; re-seeded {} times",
        t.passes,
        t.compared - t.differ.len(),
        t.compared,
        t.alphas,
        t.requests,
        t.exits,
        t.reseeds
    );
    assert!(t.differ.is_empty(), "{name}: {} items differ", t.differ.len());
    assert!(t.passes > 0 && t.alphas > 0 && t.exits == 1);
    let control = carry(name, true).unwrap();
    println!("{name}: control (the other video mode's lengths): {} items differ", control.differ.len());
    assert!(!control.differ.is_empty(), "{name}: the control finds nothing");
}

#[test]
fn shell_enter_dialog_leave_hdd() {
    run("hddosd-110U-browser-sounds-enter");
    run("hddosd-110U-browser-enter-dialog-circle2");
}

#[test]
fn shell_cold_start_with_stimuli_hdd() {
    run("hddosd-110U-browser-enter-direct");
}

#[test]
fn shell_enter_leave_rom() {
    run("rom-0230A-browser-enter-circle1");
}
