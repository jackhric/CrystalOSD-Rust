// The app's Browser screen (scene::browser::screen) run from the module's start with nothing but the pad words of a capture, its
// frames against the console's passes (HDD OSD 1.10U, NTSC, two blank cards). Where the screen follows an APP POLICY (a choice
// where the facts stop) and the console differs, the frame is counted, with the reason, and not failed: the test fails only
// on a difference in a frame the policies do not touch. Skips without the fixture.
mod console;

use console::browser::{Tally, compare};
use console::inputs::{at, int};
use console::probes::{Image, load};
use console::{fixture, load_json, need, program};
use scene::arithmetic::EeArithmetic;
use scene::browser::screen::{BrowserScreen, BrowserSetup, CardSlot};
use scene::browser::shell::{Fade, Shell};
use scene::menu_types::PadWords;
use scene::text::{FontCache, FontCacheEntry, FontState, Text, TextInputs};

const CONTEXT: u32 = 0x0039_69b0;

// The library's context and the program's font state as the Browser's thread start finds them (the probe at 0x0024EAD8 of
// verify_browser_text.mjs): verify_text2.mjs readContext and tools/scene/instrument.mjs fontState, block addresses as offsets in the
// font data.
fn console_text(c: &console::probes::Capture) -> Option<Text<EeArithmetic>> {
    let r = c.records.iter().find(|r| r.pc == 0x0024_ead8)?;
    let i = Image::of([r]);
    let u16_at = |o: u32| u16::from_le_bytes(i.bytes(CONTEXT + o, 2).try_into().unwrap()) as i32;
    let data = i.u32(CONTEXT + 0x380);
    let offset = |a: u32| if a == 0 { 0 } else { a - data };
    let mut list = Vec::new();
    let mut address = i.u32(CONTEXT + 0x37c);
    while address != 0 {
        list.push(FontCacheEntry { code: i.s32(address), loaded: i.s32(address + 4), cell: i.s32(address + 8), block: offset(i.u32(address + 0x10)) });
        address = i.u32(address + 12);
    }
    let cache = FontCache {
        list,
        format: u16_at(0x35c),
        cell_w: u16_at(0x35e),
        cell_h: u16_at(0x360),
        cells: u16_at(0x362),
        width: u16_at(0x364),
        height: u16_at(0x366),
        log_w: u16_at(0x368),
        log_h: u16_at(0x36a),
        set_up: i.s32(CONTEXT + 0x36c),
        block: offset(i.u32(CONTEXT + 0x370)),
        memory: i.u32(CONTEXT + 0x350),
        texture: i.u32(CONTEXT + 0x354),
        table: i.u32(CONTEXT + 0x358),
    };
    let own = CONTEXT + 0x1000;
    let f = |o: u32| i.f32(own + o);
    let font = FontState {
        line_height: i.s32(own),
        fixed: i.s32(own + 4),
        percent: i.s32(own + 8),
        pitch: i.s32(own + 0xc),
        decoration: i.s32(own + 0x1c),
        clip: i.s32(own + 0x20),
        blank: i.s32(own + 0xa8),
        ascent: i.s32(own + 0xb0),
        dirty: i.s32(own + 0xb4),
        tv: f(0x24),
        ratio: f(0x28),
        locate: [f(0x30), f(0x34)],
        colour: [f(0x40), f(0x44), f(0x48), f(0x4c)],
        matrix: std::array::from_fn(|row| std::array::from_fn(|col| f(0x110 + 16 * row as u32 + 4 * col as u32))),
    };
    let inputs = TextInputs { cache, font, ..TextInputs::default() };
    Some(Text::new(console::font(), program(), &inputs).expect("the console's text state"))
}

fn screen_run(name: &str, cards: [Option<CardSlot>; 2]) {
    let probes = fixture(name, "probes.json");
    let passes = fixture(name, "passes.json");
    if !need(&[&probes, &passes, &console::dumps("hddosd.elf")]) {
        return;
    }
    let c = load(&probes);
    let dump = load_json(&passes);
    let frames: Vec<&serde_json::Value> = at(&dump, "frames").as_array().unwrap().iter().filter(|f| f["browser"].as_bool().unwrap_or(false)).collect();
    let start = c.records.iter().find(|r| r.pc == 0x0024_ce60).expect("the thread start");
    let i = Image::of([start]);
    let before = Shell {
        fade: Fade { state: i.s32(0x0037_0660), count: i.s32(0x0037_066c), colour: i.s32(0x0037_0670), length: i.s32(0x0037_0674) },
        ..Shell::default()
    };
    let setup = BrowserSetup { pal: false, width: 640, height: 224, mechacon_param: 0x0700_0010, cards, program: Some(program()) };
    let mut screen = BrowserScreen::start(setup, &before, console_text(&c));
    let mut tally = Tally::default();
    let mut equal_run = 0;
    let mut first_difference = None;
    let mut differ = Vec::new();
    let mut outside = 0;
    let mut mine: Vec<(i32, u16, bool)> = Vec::new();
    for f in &frames {
        let frame = int(at(f, "frame"));
        let Some(master) = c.first(0x0025_3860, frame) else { continue };
        let m = Image::of(c.at(master.pc, frame));
        let pad = PadWords { held: m.u32(0x0037_07a0), pressed: m.u32(0x0037_07a4), released: 0, repeating: m.u32(0x0037_07a4) };
        screen.field = int(at(f, "field"));
        if std::env::var_os("SCREEN_TRACE").is_some() {
            eprintln!("frame {frame}");
        }
        let out = screen.frame(&pad);
        if std::env::var("SCREEN_TEXT").is_ok_and(|v| v.split(',').any(|n| n.parse() == Ok(frame))) {
            for run in out.frame.texts.iter().filter(|r| r.place.is_some()) {
                eprintln!("frame {frame}: {} {:?} {:?}", run.label, String::from_utf8_lossy(&run.text), run.place);
            }
        }
        for sound in &out.sounds {
            mine.push((frame, sound.a2, sound.carried_a3));
        }
        // facts/browser-chrome.md section 7: background variants 1 to 4 (D_003705D4) are not specified; the card page draws one.
        let variant = c.first(0x0024_8a90, frame).map_or(0, |r| Image::of([r]).s32(0x0037_05d4));
        if variant != 0 {
            outside += 1;
            if out.frame.frame.passes.iter().any(|p| p.material.texture == scene::frame::GLYPH_TEXTURE)
                && let Err(e) = console::browser::compare_text(&out.frame, f, 640, 224, &mut tally)
            {
                differ.push(format!("frame {frame}: {e}"));
            }
            continue;
        }
        match compare(&out.frame, f, 640, 224, false, &mut tally) {
            Ok(()) => {
                if first_difference.is_none() {
                    equal_run += 1;
                }
            }
            Err(e) => {
                first_difference.get_or_insert(frame);
                differ.push(format!("frame {frame}: {e}"));
            }
        }
        if screen.exited() {
            break;
        }
    }
    println!(
        "{name}: the screen from the pad: {} Browser frames, {} equal ({} from the start before the first difference), {} differ, {} with a background variant the facts do not specify",
        frames.len(),
        tally.frames,
        equal_run,
        differ.len(),
        outside
    );
    println!(
        "{name}: {} glyphs drawn by the scene equal fan for fan ({} frames compared on their text alone); {} by colour only",
        tally.fans, tally.text_frames, tally.glyphs
    );
    for d in differ.iter().take(60) {
        println!("  {d}");
    }
    // The Browser's commands (0x6300, 1, a2) through the gate 0x0024E090, in order; a pass's command may land in the next frame.
    let unique = gate_calls(&c);
    println!("{name}: sounds: the screen queued {} commands, the console's gate saw {}", mine.len(), unique.len());
    if !unique.is_empty() {
        assert_eq!(mine.len(), unique.len(), "{name}: the screen's commands {mine:?}, the console's {unique:?}");
        for ((f, a2, _), (g, b2)) in mine.iter().zip(&unique) {
            assert!(a2 == b2 && (g - f).abs() <= 1, "{name}: the screen sent {a2} at frame {f}, the console {b2} at frame {g}");
        }
    }
    assert!(screen.exited(), "the screen did not leave");
    assert!(differ.is_empty(), "{name}: {} frames differ", differ.len());
}

const BLANK: Option<CardSlot> = Some(CardSlot { formatted: false, free: 0 });

#[test]
fn screen_enter_dialog_leave() {
    screen_run("scratch-hdd-chrome-leave", [BLANK, BLANK]);
}

#[test]
fn screen_device_row_card_page_format_dialog() {
    screen_run("hddosd-110U-browser-session", [BLANK, BLANK]);
}

// The gate calls (0x0024E090) of the Browser's command 0x6300: a capture with the probes of several verifiers holds one record per
// probe at a shared pc, so only the records of the first probe there are counted.
fn gate_calls(c: &console::probes::Capture) -> Vec<(i32, u16)> {
    let first = c.records.iter().filter(|r| r.pc == 0x0024_e090).map(|r| r.index).min().flatten();
    c.records.iter().filter(|r| r.pc == 0x0024_e090 && r.gpr[4] == 0x6300 && r.index == first).map(|r| (r.frame, r.gpr[6] as u16)).collect()
}

// The same session with the text probes (verify_text2.mjs, verify_browser_text.mjs): the text drawn and compared glyph for glyph.
#[test]
fn screen_with_text() {
    screen_run("hddosd-110U-browser-text-session2", [BLANK, BLANK]);
}
