use std::path::PathBuf;

use app::golden::{Compared, GoldenChecker, format_line, hash_frame, parse_line};
use scene::frame::Frame;

fn baseline() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden/cpp-native.golden")
}

#[test]
fn lines_round_trip_the_cpp_baseline() {
    let text = std::fs::read_to_string(baseline()).unwrap();
    assert_eq!(text.lines().count(), 1719);
    for line in text.lines() {
        assert_eq!(format_line(&parse_line(line).unwrap()), line);
    }
}

#[test]
fn checker_accepts_the_baseline_and_names_a_difference() {
    let text = std::fs::read_to_string(baseline()).unwrap();
    let lines: Vec<_> = text.lines().filter_map(parse_line).collect();
    let mut checker = GoldenChecker::new(&baseline(), Compared::ALL);
    assert!(lines.iter().all(|line| checker.check(line)));
    assert_eq!(checker.compared(), checker.size());

    let mut checker = GoldenChecker::new(&baseline(), Compared::ALL);
    let mut wrong = lines[0].clone();
    wrong.scene ^= 1;
    assert!(!checker.check(&wrong));
    assert!(checker.report().contains("differs in scene"));
    let mut scene_blind = GoldenChecker::new(&baseline(), Compared { scene: false, ..Compared::ALL });
    assert!(scene_blind.check(&wrong));
}

// FNV-1a 64 of the bytes Golden.cpp hashes for a default scene::Frame: 640, 224, 0, 0 as i32; textAt 0 as u64;
// textureSet 0, depthBits 32; six zero glyph words; two zero sizes.
#[test]
fn empty_frame_hashes_the_bytes_the_cpp_hashes() {
    let mut bytes = Vec::new();
    for v in [640i32, 224, 0, 0] {
        bytes.extend(v.to_le_bytes());
    }
    bytes.extend(0u64.to_le_bytes());
    for v in [0i32, 32, 0, 0, 0, 0, 0, 0] {
        bytes.extend(v.to_le_bytes());
    }
    bytes.extend([0u8; 16]);
    assert_eq!(hash_frame(&Frame::default()), assets::bytes::hash_of(&bytes));
}
