mod opening_common;

use opening_common::{golden_scenes, hash_frame, program};
use scene::arithmetic::NativeArithmetic;
use scene::frame::{Frame, TextureSet};
use scene::opening::{BootOptions, FRAMES_TO_CLOCK, Flat, Opening};

// The boot as the C++ app runs it at commit 039cc58 (Source/App/Main.cpp buildBoot, OpeningScreen, BootChain): default BootOptions
// (no history, the drawn lights phase), OpeningScreen::step drawing frame(display, 0) with display 0, 1, 0, ..., then
// BootChain's black gap of kFramesToClock frames. golden/cpp-native.golden is the C++ app's `--golden record` run: its frames 1..280
// are these (the phase column is the chain's phase after the step, so the last opening frame reads black_gap and the last gap frame clock).
#[test]
fn boot_opening_equals_the_golden_scene_column() {
    let (Some(program), Some(golden)) = (program(), golden_scenes()) else { return };
    let mut opening = Opening::<NativeArithmetic>::new(&BootOptions::default(), &program);
    let mut hashes = Vec::new();
    let mut display = 0;
    while !opening.ended() {
        hashes.push(hash_frame(&opening.frame(display, 0)));
        display ^= 1;
    }
    assert_eq!(hashes.len(), 246);
    let mut display = 0;
    for _ in 0..FRAMES_TO_CLOCK {
        let mut frame = Frame { display_index: display, texture_set: TextureSet::Opening, depth_bits: 24, ..Default::default() };
        Flat::<NativeArithmetic>::scissor_default(&mut frame.passes);
        hashes.push(hash_frame(&frame));
        display ^= 1;
    }
    assert_eq!(hashes.len(), 280);
    for (n, hash) in hashes.iter().enumerate() {
        let (frame, phase, want) = &golden[n];
        assert_eq!(*frame, n as u64 + 1);
        let expected_phase = if n < 245 {
            "opening"
        } else if n < 279 {
            "black_gap"
        } else {
            "clock"
        };
        assert_eq!(phase, expected_phase, "frame {frame}");
        assert_eq!(*hash, *want, "frame {frame}: scene hash");
    }
}
