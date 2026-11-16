mod common;

use std::path::PathBuf;

use common::shell::{PadReader, Session, golden_now, host_inputs};
use common::{cold_assets, hash_frame, resources};
use scene::arithmetic::NativeArithmetic;
use scene::menu_types::pad;

// Source/App/GoldenScenario.cpp: after how many clock frames each button goes down (it comes up six frames later).
const PRESSES: [(u64, u32); 16] = [
    (120, pad::DOWN),
    (200, pad::UP),
    (280, pad::TRIANGLE),
    (360, pad::DOWN),
    (420, pad::UP),
    (480, pad::CIRCLE),
    (560, pad::DOWN),
    (640, pad::CROSS),
    (760, pad::DOWN),
    (820, pad::DOWN),
    (880, pad::UP),
    (940, pad::CROSS),
    (1040, pad::CIRCLE),
    (1120, pad::SQUARE),
    (1220, pad::SQUARE),
    (1320, pad::CIRCLE),
];

// The golden run of golden/cpp-native.golden (`./bin/CrystalOSD --golden record`, the C++ at commit 039cc58 on this
// machine): its clock starts at 2026-01-01 12:00 in the host's zone, America/New_York (17:00 UTC, offset -300 minutes, no
// summer time), on the 281st logic frame, after the opening (245 frames) and the black gap (35). The opening leaves the
// rand state at 1615070519 (opening::Opening<NativeArithmetic>::randState() of that run, printed by a C++ probe);
// gsAllocator is assets::clockTexturesEnd(), 770048.
const GOLDEN_START: i64 = 1767286800;
const FIRST_CLOCK_FRAME: u64 = 280;
const OPENING_RAND_STATE: u32 = 1615070519;
const CLOCK_TEXTURES_END: u32 = 770048;

fn golden_lines() -> Option<Vec<(u64, u64)>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden/cpp-native.golden");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("skipped: no {}", path.display());
        return None;
    };
    Some(
        text.lines()
            .map(|l| {
                let mut w = l.split_whitespace();
                let frame = w.next().unwrap().parse().unwrap();
                let scene = w.nth(2).unwrap().strip_prefix("scene=").unwrap();
                (frame, u64::from_str_radix(scene, 16).unwrap())
            })
            .collect(),
    )
}

// Every clock frame of the golden file's scene= column, frames 281 to 1719 (the first 280 are the opening and the gap).
#[test]
fn golden_scene_column() {
    let Some(folder) = resources() else { return };
    let Some(lines) = golden_lines() else { return };
    let assets = cold_assets(&folder);
    let mut session: Option<Session<NativeArithmetic>> = None;
    let mut reader = PadReader::default();
    let mut compared = 0;
    for logic in FIRST_CLOCK_FRAME..lines.len() as u64 {
        let clock_frames = logic - FIRST_CLOCK_FRAME + 1;
        for (after, bit) in PRESSES {
            if after == clock_frames {
                reader.down(bit);
            }
            if after + 6 == clock_frames {
                reader.up(bit);
            }
        }
        let now = golden_now(GOLDEN_START, logic);
        let s = session.get_or_insert_with(|| {
            let mut cold = host_inputs(false, now, -300, false);
            cold.wide = true;
            cold.rand_state = OPENING_RAND_STATE;
            cold.gs_allocator = CLOCK_TEXTURES_END;
            Session::start(&assets, &cold)
        });
        s.reader = reader;
        let frame = s.step(now);
        reader = s.reader;
        let (number, want) = lines[logic as usize];
        assert_eq!(number, logic + 1);
        assert_eq!(hash_frame(&frame), want, "golden frame {number}");
        compared += 1;
    }
    assert_eq!(compared, 1439);
}
