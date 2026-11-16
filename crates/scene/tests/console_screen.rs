// The app's Browser screen (scene::browser::screen) run from the module's start with nothing but the pad words of a capture, its
// frames against the console's passes (HDD OSD 1.10U, NTSC, two blank cards). Where the screen follows an APP POLICY (a choice
// where the facts stop) and the console differs, the frame is counted, with the reason, and not failed: the test fails only
// on a difference in a frame the policies do not touch. Skips without the fixture.
mod console;

use console::browser::{Tally, compare};
use console::inputs::{at, int};
use console::probes::{Image, load};
use console::{fixture, load_json, need, program};
use scene::browser::screen::{BrowserScreen, BrowserSetup, CardSlot};
use scene::browser::shell::{Fade, Shell};
use scene::menu_types::PadWords;

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
    let mut screen = BrowserScreen::start(setup, &before);
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
        let out = screen.frame(&pad);
        for sound in &out.sounds {
            mine.push((frame, sound.a2, sound.carried_a3));
        }
        // facts/browser-chrome.md section 7: background variants 1 to 4 (D_003705D4) are not specified; the card page draws one.
        let variant = c.first(0x0024_8a90, frame).map_or(0, |r| Image::of([r]).s32(0x0037_05d4));
        if variant != 0 {
            outside += 1;
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
