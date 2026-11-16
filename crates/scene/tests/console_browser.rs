// The Rust Browser against the console's own frames and words, from fixtures exported by tools/browser/export_browser_fixture.mjs
// (the trace's probe records and the GS dump's passes, the card row marked) in the data home; each test skips without them.
mod console;

use console::browser::{Tally, compare};
use console::inputs::{at, int};
use console::probes::{Capture, Image, load};
use console::{fixture, load_json, need, program};
use scene::browser::chrome::{ChromeState, DialogDraw, GUIDE_X_ENGLISH, Guide, GuideSlot, Titles, chrome_frame};
use scene::browser::shell::{Fade, fade_step};
use scene::program_image::ProgramImage;

struct Strings {
    program: Option<std::sync::Arc<ProgramImage>>,
}

impl Strings {
    fn get(&self, language: u32, id: i32) -> Vec<u8> {
        let Some(p) = &self.program else { return Vec::new() };
        let table = p.word(0x002a_d200 + 4 * language);
        p.string(p.word(table + 4 * id as u32))
    }
}

// HDD OSD probes of verify_browser_chrome.mjs.
const BG: u32 = 0x0024_8a90;
const BANDS: u32 = 0x0024_d0c8;
const GUIDE: u32 = 0x0025_24d0;
const VIDMODE: u32 = 0x0025_2574;
const WIDTH: u32 = 0x0025_26e4;
const TITLE: u32 = 0x0025_0da0;
const DIALOG: u32 = 0x0025_bd18;
const ROM_FADE_ALPHA: u32 = 0x0024_690c;
const ROM_FADE_ENTRY: u32 = 0x0024_6860;
const HDD_FADE_RECORD: u32 = 0x0037_0660;

// The fade record (model/browser_shell.mjs fadeOf): HDD OSD 0x00370660 state, +0xC count, +0x10 colour, +0x14 length; ROM 2.30
// 0x002C8BB4 state (a byte), +8 count, +0xC colour, +0x10 length.
fn fade_of(image: &Image, base: u32, rom: bool) -> Fade {
    let (count, colour, length) = if rom { (8, 0xc, 0x10) } else { (0xc, 0x10, 0x14) };
    Fade {
        state: if rom { i32::from(image.u8(base)) } else { image.s32(base) },
        count: image.s32(base + count),
        colour: image.s32(base + colour),
        length: image.s32(base + length),
    }
}

// The page state of an HDD OSD frame from its probes (verify_browser_chrome.mjs probedState), the strings from the program.
fn hdd_state(c: &Capture, frame: i32, width: i32, height: i32, field: i32, strings: &Strings) -> Option<(ChromeState, Option<i32>, u32)> {
    let one = |pc| c.first(pc, frame);
    let (bg, bands, guide, title) = (one(BG)?, one(BANDS)?, one(GUIDE)?, one(TITLE)?);
    let bg_image = Image::of([bg]);
    let fade = fade_of(&bg_image, HDD_FADE_RECORD, false);
    let screen = Image::of([bands]);
    assert_eq!((screen.s32(0x001f_0cb4), screen.s32(0x001f_0cb8)), (width, height), "frame {frame}: the screen record and the environment differ");
    let g = Image::of([guide]);
    let language = g.u32(0x0037_06ac);
    let vidmode = one(VIDMODE).is_some_and(|r| r.gpr[2] != 0);
    let label_width = one(WIDTH).map(|r| r.gpr[2] as i32);
    let slot3 = one(WIDTH).and_then(|r| {
        let ptr = Image::of([r]).u32(0x0037_0b9c);
        strings.program.as_ref().map(|p| p.string(ptr))
    });
    let slots: Vec<GuideSlot> = (0..4)
        .map(|s4| (s4, g.s32(0x004d_a340 + 4 * s4 as u32)))
        .filter(|&(_, counter)| counter != 0)
        .map(|(s4, counter)| {
            let s2 = if vidmode && s4 == 1 {
                2
            } else if vidmode && s4 == 2 {
                1
            } else {
                s4
            };
            let id = 0x54 + s2;
            let id = if vidmode && (id == 0x55 || id == 0x56) { 0x55 + 0x56 - id } else { id };
            GuideSlot { s4, counter, label: if s4 == 3 { slot3.clone().unwrap_or_default() } else { strings.get(language, id) } }
        })
        .collect();
    let xs: [i32; 3] = match &strings.program {
        Some(p) => std::array::from_fn(|k| p.integer(0x002b_d2e0 + 16 * language + 4 * k as u32)),
        None => GUIDE_X_ENGLISH,
    };
    let dialog = one(DIALOG).map(|d| {
        let i = Image::of([d]);
        DialogDraw {
            n: i.s32(0x0037_08f0),
            step: i.s32(0x0037_0be4),
            kind: i.s32(0x0037_0be8),
            abort: i.s32(0x0037_06a8) != 0 || i.s32(0x0037_0778) != 0 || i.s32(0x0037_0780) != 0,
            lines: [0x15c, 0x15d, 0x15e, 0x15f].iter().map(|&id| strings.get(language, id)).collect(),
        }
    });
    let alpha = if fade.state != 0 { fade_step(fade).0 } else { None };
    let state = ChromeState {
        width,
        height,
        field,
        display_index: field,
        fade,
        tile_counter: bg_image.s32(HDD_FADE_RECORD + 0xc),
        page_in: Image::of([title]).s32(0x0037_072c),
        titles: Titles { left: strings.get(language, 6), right: strings.get(language, 0x1a1), right_alpha: None, right_second: Vec::new() },
        dialog,
        guide: Guide { slots, swap: vidmode, xs, slot3_x: label_width.map_or(0, |w| width - w - 0x34), dy: 2 },
        rom: false,
    };
    Some((state, alpha, language))
}

// ROM 2.30's strings are not looked up (facts/browser-chrome.md section 5): its runs are checked for their colours only.
const UNKNOWN: &[u8] = b"?";

// ROM 2.30: no exact probes of the page; the state of a frame of the leave fade is the one verify_browser_chrome.mjs declares
// (state 3, length 90, counter 90 - j, the guide slots 1 and 2 at 0x18), with the fade record probed by the shell's probes.
fn rom_state(c: &Capture, frame: i32, j: i32, width: i32, height: i32, field: i32) -> Option<(ChromeState, Option<i32>)> {
    let entry = c.first(ROM_FADE_ENTRY, frame)?;
    let fade = fade_of(&Image::of([entry]), 0x002c_8bb4, true);
    if fade.state != 3 {
        return None;
    }
    assert_eq!(fade.count, 90 - j, "frame {frame}: the declared leave fade");
    let alpha = c.first(ROM_FADE_ALPHA, frame).map(|r| r.gpr[19] as i32);
    let state = ChromeState {
        width,
        height,
        field,
        display_index: field,
        fade,
        tile_counter: fade.count,
        page_in: 0x1a,
        titles: Titles { left: UNKNOWN.to_vec(), right: UNKNOWN.to_vec(), right_alpha: Some(0x80), right_second: Vec::new() },
        dialog: None,
        guide: Guide {
            slots: vec![GuideSlot { s4: 1, counter: 0x18, label: UNKNOWN.to_vec() }, GuideSlot { s4: 2, counter: 0x18, label: UNKNOWN.to_vec() }],
            swap: true,
            xs: GUIDE_X_ENGLISH,
            slot3_x: 0,
            dy: 1,
        },
        rom: true,
    };
    Some((state, alpha))
}

fn chrome_capture(name: &str, rom: bool) -> Option<Tally> {
    chrome_run(name, rom, false)
}

// `perturb`: the fade and tile counters one higher than the console's, a control that must fail.
fn chrome_run(name: &str, rom: bool, perturb: bool) -> Option<Tally> {
    let probes = fixture(name, "probes.json");
    let passes = fixture(name, "passes.json");
    if !need(&[&probes, &passes]) {
        return None;
    }
    let strings = Strings { program: (!rom && console::dumps("hddosd.elf").exists()).then(program) };
    let c = load(&probes);
    let dump = load_json(&passes);
    let mut tally = Tally::default();
    let mut j = 0;
    let mut failures = Vec::new();
    for f in at(&dump, "frames").as_array().unwrap() {
        if !f["browser"].as_bool().unwrap_or(false) {
            continue;
        }
        let frame = int(at(f, "frame"));
        let field = int(at(f, "field"));
        let (w, h) = (640, 224);
        let computed = if rom { rom_state(&c, frame, j, w, h, field).map(|(s, a)| (s, a, 1)) } else { hdd_state(&c, frame, w, h, field, &strings) };
        let Some((mut state, alpha, _)) = computed else { continue };
        if perturb {
            state.fade.count += 1;
            state.tile_counter += 1;
        }
        let alpha = if perturb && state.fade.state != 0 { fade_step(state.fade).0 } else { alpha };
        if rom {
            j += 1;
        }
        let ours = chrome_frame(&state, alpha);
        if let Err(e) = compare(&ours, f, w, h, rom, &mut tally) {
            failures.push(format!("{name} frame {frame}: {e}"));
        }
    }
    if perturb {
        println!("{name}: control (fade and tile counters + 1): {} frames differ", failures.len());
        assert!(!failures.is_empty(), "{name}: the control finds nothing");
        return Some(tally);
    }
    for f in failures.iter().take(8) {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{name}: {} frames differ", failures.len());
    println!(
        "{name}: {} Browser frames, {} passes and {} vertices equal, {} glyphs by shape, {} card-row passes excluded, {} head clears",
        tally.frames, tally.passes, tally.vertices, tally.glyphs, tally.card, tally.head_clears
    );
    Some(tally)
}

#[test]
fn chrome_entry_against_the_console() {
    if let Some(t) = chrome_capture("hddosd-110U-browser-chrome-root", false) {
        assert!(t.frames > 0);
    }
}

#[test]
fn chrome_leave_against_the_console() {
    if let Some(t) = chrome_capture("scratch-hdd-chrome-leave", false) {
        assert!(t.frames > 0);
    }
}

#[test]
fn chrome_leave_rom_against_the_console() {
    if let Some(t) = chrome_capture("scratch-rom-chrome-leave", true) {
        assert!(t.frames > 0);
    }
}

#[test]
fn chrome_controls_fail() {
    chrome_run("hddosd-110U-browser-chrome-root", false, true);
    chrome_run("scratch-hdd-chrome-leave", false, true);
    chrome_run("scratch-rom-chrome-leave", true, true);
}
