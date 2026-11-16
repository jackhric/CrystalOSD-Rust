mod common;

use common::Hash;
use common::scenarios::{SCENARIOS, run_scenario};
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic};

// FNV-1a 64 over the per-frame digests (scenarios.rs digest) of each session, from the C++ at commit 039cc58 driven by the
// same sessions (the scaffold of shell.rs, the scripts and settings of scenarios.rs): NativeArithmetic as the app binary
// builds it (ClockSceneNative, -O3 -ffp-contract=off, SCENE_NATIVE_ONLY), which equals the strict build on all of them;
// EeArithmetic from the tests' strict build (ClockScene, -O3 -ffp-model=strict -ffp-contract=off).
const EXPECTED: [(&str, u64, u64); 20] = [
    ("native ntsc-en-walk", 1700, 0x83c46174f0cc7597),
    ("native pal-fr-walk", 1700, 0xa4d0a31cf348251a),
    ("native ntsc-de-tour", 1450, 0x29af86c0a322a433),
    ("native pal-es-tour", 1450, 0xbe8367e19a9a3e27),
    ("native ntsc-it-random", 1600, 0x029f1a06ae34769f),
    ("native pal-nl-random", 1600, 0x69dd6d2463fa9cca),
    ("native ntsc-pt-random", 1600, 0x88ebddcf9a2e6d07),
    ("native pal-en-random", 1600, 0x2d17f2d1dddc75de),
    ("native ntsc-ja-idle", 400, 0x39ea897786c5dac8),
    ("native pal-de-idle", 400, 0xdc5a58373fb58dcf),
    ("ee ntsc-en-walk", 1700, 0x4b3e976e84e5d405),
    ("ee pal-fr-walk", 1700, 0x9c2764e6ba717feb),
    ("ee ntsc-de-tour", 1450, 0x6c0840b2c8259bff),
    ("ee pal-es-tour", 1450, 0x23519c6e0e568634),
    ("ee ntsc-it-random", 1600, 0x1f55964ce01dc1a1),
    ("ee pal-nl-random", 1600, 0xbd69cb6976e32bb5),
    ("ee ntsc-pt-random", 1600, 0xeb6d89e81d3620ae),
    ("ee pal-en-random", 1600, 0x9a096845136d7476),
    ("ee ntsc-ja-idle", 400, 0xf10eb5c50d9ac3ac),
    ("ee pal-de-idle", 400, 0xdd962823eff1f987),
];

fn sessions<A: Arithmetic>(tag: &str) {
    for s in &SCENARIOS {
        let Some(h) = run_scenario::<A>(s) else { return };
        let name = format!("{tag} {}", s.name);
        let want = EXPECTED.iter().find(|e| e.0 == name).unwrap();
        assert_eq!((h.count, h.h), (want.1, want.2), "{name}");
    }
}

#[test]
fn native_sessions() {
    sessions::<NativeArithmetic>("native");
}

#[test]
fn ee_sessions() {
    sessions::<EeArithmetic>("ee");
}

#[test]
#[ignore]
fn coverage() {
    for s in &SCENARIOS {
        let mut cover = common::scenarios::Coverage::default();
        common::scenarios::run_covered::<NativeArithmetic>(s, &mut cover);
        println!("{} {:?}", s.name, cover);
    }
}

#[allow(dead_code)]
fn unused(_: Hash) {}
