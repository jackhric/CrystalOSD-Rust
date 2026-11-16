use scene::clock_date::{BASE_ZONE, date_of, seconds_of};
use scene::cold_types::ColdInputs;
use scene::console_clock::{ConsoleZone, DateWords, local_of, rtc_at, rtc_of, unix_of, zone_of};
use scene::rods::load_rod_mesh;
use scene::sound::{CLOCK_INIT_FRAMES, PLACEHOLDER_A3, SoundCommand, THREAD_START_LAG_FRAMES, clock_set_up, clock_thread_start};

const MESH: &str = r#"{"faces": 1, "comment": "xé\n",
 "positions": {"bits": ["3f800000", "0x40000000", "  C0400000", "-1", "1ffffffff", "+7f800000", "0X7fc00001", "00000001", "3F800000xyz", "0", "80000000", "ffffffff", "1", "2", "3", "4"]},
 "normals": {"bits": ["3f800000", "0", "0", "3f800000"], "bits": ["40000000", "0", "0", "3f800000"]},
 "coordinates": {"bits": ["1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f", "10"]}}"#;

fn mesh_file(name: &str, text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("scene-test-{}-{name}", std::process::id()));
    std::fs::write(&path, text).unwrap();
    path
}

// What loadRodMesh (CrystalOSD 039cc58, nlohmann json and std::stoul) gives for the same three files.
#[test]
fn rod_mesh_json_matches_cpp() {
    let path = mesh_file("mesh.json", MESH);
    let mesh = load_rod_mesh(&path).unwrap();
    let bits = |part: &[[f32; 4]]| part.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(
        bits(&mesh.positions),
        [0x3f800000, 0x40000000, 0xc0400000, 0xffffffff, 0xffffffff, 0x7f800000, 0x7fc00001, 0x00000001, 0x3f800000, 0, 0x80000000, 0xffffffff, 1, 2, 3, 4]
    );
    assert_eq!(bits(&mesh.normals), [0x40000000, 0, 0, 0x3f800000]);
    assert_eq!(bits(&mesh.coordinates), (1..=16).collect::<Vec<u32>>());
    let two = mesh_file("mesh2.json", &MESH.replace("\"faces\": 1", "\"faces\": 2"));
    assert!(load_rod_mesh(&two).is_err());
    let bad = mesh_file("mesh3.json", r#"{"faces": 1, "positions": {"bits": ["zz", "0", "0", "0"]}, "normals": {"bits": []}, "coordinates": {"bits": []}}"#);
    assert!(load_rod_mesh(&bad).is_err());
    for p in [path, two, bad] {
        std::fs::remove_file(p).unwrap();
    }
}

// Port of tests/scene/ConsoleClockTest.cpp (CrystalOSD 039cc58) without its captured pairs.
#[test]
fn console_clock() {
    assert!(zone_of(0x07000010).minutes == 0 && !zone_of(0x07000010).summer);
    assert!(zone_of(0x27000010).summer);
    assert_eq!(zone_of(0x400 << 9).minutes, -1024);
    assert_eq!(zone_of(0x3FF << 9).minutes, 1023);

    assert_eq!(rtc_at(0), [1970, 1, 1, 9, 0, 0]);
    assert_eq!(unix_of(&rtc_at(1700000000)), 1700000000 + i64::from(BASE_ZONE) * 60);
    assert_eq!(rtc_of(&[-1, 1, 1, 0, 0, 0], ConsoleZone::default()), [-1, 1, 1, 0, 0, 0]);

    let zones = [(-720, false), (-720, true), (0, false), (0, true), (540, false), (540, true), (1023, false), (1023, true)];
    let first = seconds_of(2000, 1, 1, 0, 0, 0);
    let last = seconds_of(2099, 12, 31, 23, 0, 0);
    let mut count = 0;
    for (minutes, summer) in zones {
        let zone = ConsoleZone { minutes, summer };
        let mut t = first;
        while t <= last {
            let local: DateWords = date_of(t);
            assert_eq!(local_of(&rtc_of(&local, zone), zone), local);
            count += 1;
            t += 3600;
        }
    }
    assert_eq!(count, 8 * 876600);
}

#[test]
fn sound_commands() {
    let mut out = Vec::new();
    clock_thread_start(&mut out);
    clock_set_up(&mut out);
    let s = |id, a1, a2, a3, carried_a2, carried_a3| SoundCommand { id, a1, a2, a3, carried_a2, carried_a3 };
    assert_eq!(out, [s(0x6150, 1, 0, 0, false, false), s(0x6150, 6, 0, 0xF, false, false), s(0x6140, 2, 0, 0, true, true)]);
    assert_eq!((CLOCK_INIT_FRAMES, THREAD_START_LAG_FRAMES, PLACEHOLDER_A3), (31, 2, 0));
}

#[test]
fn cold_inputs_default() {
    let c = ColdInputs::default();
    assert_eq!((c.city, c.rand_state, c.screen_width, c.screen_height, c.clock_offset), (0x33, 1, 640, 224, None));
    assert!(!c.pal && !c.wide && c.language == 0 && c.gs_allocator == 0);
}
