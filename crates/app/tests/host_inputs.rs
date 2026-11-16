use std::path::{Path, PathBuf};

use app::host_inputs::{HostOptions, HostZone, NANOS, host_inputs_from};
use app::settings_store::{StoredSettings, read_stored, write_stored};
use scene::cold_types::ColdInputs;

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn at(y: i64, m: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
    (days_from_civil(y, m, d) * 86400 + h * 3600 + mi * 60 + s) * NANOS
}

fn temp() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
}

fn write(name: &str, text: &str) -> PathBuf {
    let path = temp().join(format!("HostInputsTest-{name}.json"));
    std::fs::write(&path, text).unwrap();
    path
}

fn rejects(options: HostOptions, path: &Path, zone: &HostZone) -> bool {
    host_inputs_from(&options, at(2024, 1, 1, 0, 0, 0), path, zone).is_err()
}

fn inputs(options: HostOptions, now: i64, path: &Path, zone: &HostZone) -> ColdInputs {
    host_inputs_from(&options, now, path, zone).unwrap()
}

fn flags(language: Option<i32>, aspect: Option<i32>) -> HostOptions {
    HostOptions { pal: false, language, aspect }
}

#[test]
fn host_inputs_test() {
    let tokyo = HostZone::locate("Asia/Tokyo").unwrap();
    let new_york = HostZone::locate("America/New_York").unwrap();
    let none = temp().join("HostInputsTest-absent.json");
    let _ = std::fs::remove_file(&none);
    let d = HostOptions::default();

    let now = at(2024, 3, 1, 12, 30, 45);
    let defaults = inputs(d, now, &none, &tokyo);
    assert!(
        !defaults.pal
            && defaults.language == 1
            && defaults.aspect == 0
            && defaults.time_zone == 540
            && !defaults.summer_time
            && defaults.date_format == 0
            && defaults.time_format == 0
    );
    assert!(defaults.rand_state == 1 && !defaults.wide && defaults.screen_code == 0x64);
    assert!(defaults.screen_width == 640 && defaults.screen_height == 224);
    let r = defaults.rtc;
    assert!(r.year == 2024 && r.month == 3 && r.day == 1 && r.hour == 21 && r.minute == 30 && r.second == 45);

    let pal = inputs(HostOptions { pal: true, ..d }, now, &none, &tokyo);
    assert!(pal.pal && pal.screen_height == 256 && pal.screen_width == 640);

    let set_flags = inputs(flags(Some(4), Some(2)), now, &none, &tokyo);
    assert!(set_flags.language == 4 && set_flags.aspect == 2);

    let file = write("full", r#"{"language":3,"aspect":1,"timeZone":-180,"summerTime":true,"dateFormat":2,"timeFormat":1}"#);
    let set = inputs(d, now, &file, &new_york);
    assert!(set.language == 3 && set.aspect == 1 && set.time_zone == -180 && set.summer_time && set.date_format == 2 && set.time_format == 1);
    let overridden = inputs(flags(Some(5), None), now, &file, &new_york);
    assert!(overridden.language == 5 && overridden.aspect == 1);

    let partial = write("partial", r#"{"summerTime":0}"#);
    let half = inputs(d, now, &partial, &new_york);
    assert!(half.language == 1 && !half.summer_time && half.time_zone == -300);

    for (name, text) in [
        ("lang0", r#"{"language":0}"#),
        ("lang8", r#"{"language":8}"#),
        ("aspect3", r#"{"aspect":3}"#),
        ("zone", r#"{"timeZone":1024}"#),
        ("date", r#"{"dateFormat":3}"#),
        ("time", r#"{"timeFormat":2}"#),
        ("summer", r#"{"summerTime":2}"#),
        ("type", r#"{"language":"en"}"#),
        ("unknown", r#"{"languages":1}"#),
        ("broken", "{"),
        ("array", "[]"),
    ] {
        assert!(rejects(d, &write(name, text), &tokyo), "{name}");
    }
    assert!(rejects(flags(Some(9), None), &none, &tokyo));
    assert!(rejects(flags(None, Some(3)), &none, &tokyo));

    let before = at(2024, 3, 10, 6, 59, 59);
    let winter = inputs(d, before, &none, &new_york);
    let summer = inputs(d, before + NANOS, &none, &new_york);
    assert!(winter.time_zone == -300 && summer.time_zone == -300 && !winter.summer_time && summer.summer_time);
    assert!(winter.rtc.day == 10 && winter.rtc.hour == 15 && winter.rtc.minute == 59 && winter.rtc.second == 59);
    assert!(summer.rtc.day == 10 && summer.rtc.hour == 16 && summer.rtc.minute == 0 && summer.rtc.second == 0);

    assert!(defaults.time_zone == 540 && !defaults.summer_time);
    let forced_summer = inputs(d, before + NANOS, &write("zoneOnly", r#"{"timeZone":60}"#), &new_york);
    assert!(forced_summer.time_zone == 60 && forced_summer.summer_time);
    let forced_zone = inputs(d, before + NANOS, &write("summerOnly", r#"{"summerTime":false}"#), &new_york);
    assert!(forced_zone.time_zone == -300 && !forced_zone.summer_time);

    let edge = at(2023, 12, 31, 15, 0, 0);
    let year = inputs(d, edge, &none, &tokyo);
    assert!(year.rtc.year == 2024 && year.rtc.month == 1 && year.rtc.day == 1 && year.rtc.hour == 0);

    let stored = temp().join("HostInputsTest-store").join("settings.json");
    let _ = std::fs::remove_dir_all(stored.parent().unwrap());
    let all = StoredSettings {
        language: Some(3),
        aspect: Some(2),
        time_zone: Some(-180),
        date_format: Some(1),
        time_format: Some(1),
        summer_time: Some(true),
        clock_offset: Some(-31536000),
    };
    write_stored(&stored, &all).unwrap();
    assert_eq!(read_stored(&stored).unwrap(), all);
    assert!(!PathBuf::from(format!("{}.tmp", stored.display())).exists());
    let only_offset = StoredSettings { clock_offset: Some(3600), ..Default::default() };
    write_stored(&stored, &only_offset).unwrap();
    let sparse = read_stored(&stored).unwrap();
    assert!(sparse.language.is_none() && sparse.summer_time.is_none() && sparse.clock_offset == Some(3600));
    let shifted = inputs(d, now, &stored, &tokyo);
    assert!(shifted.clock_offset == Some(3600) && shifted.rtc.hour == 22 && shifted.rtc.minute == 30);
    assert!(read_stored(&none).unwrap().clock_offset.is_none());
    for (name, text) in [
        ("offsetText", r#"{"clockOffset":"1"}"#),
        ("offsetHuge", r#"{"clockOffset":900000000000}"#),
        ("offsetUnsigned", r#"{"clockOffset":18446744073709551615}"#),
        ("languageWrap", r#"{"language":4294967297}"#),
        ("languageUnsigned", r#"{"language":18446744073709551615}"#),
    ] {
        assert!(rejects(d, &write(name, text), &tokyo), "{name}");
    }
    let dropped = inputs(d, now, &write("offsetYear", r#"{"clockOffset":3000000000}"#), &tokyo);
    assert!(dropped.clock_offset.is_none() && dropped.rtc.year == 2024 && dropped.rtc.hour == 21);
    let kept = inputs(d, now, &write("offsetKept", r#"{"clockOffset":86400}"#), &tokyo);
    assert!(kept.clock_offset == Some(86400) && kept.rtc.day == 2);
    let blocked = stored.parent().unwrap().join("blocked.json");
    std::fs::create_dir_all(blocked.join("inside")).unwrap();
    assert!(write_stored(&blocked, &only_offset).is_err());
    for entry in std::fs::read_dir(stored.parent().unwrap()).unwrap() {
        assert_ne!(entry.unwrap().path().extension().and_then(|e| e.to_str()), Some("tmp"));
    }
    assert!(rejects(d, &write("offsetKey", r#"{"clockoffset":1}"#), &tokyo));
}

#[test]
fn messages_are_the_cpp_ones() {
    let message = |text: &str| read_stored(&write("message", text)).unwrap_err().0;
    assert_eq!(message(r#"{"language":0}"#), "settings.json: language 0 is outside 1..7");
    assert_eq!(message(r#"{"zeta":1,"alpha":2}"#), "settings.json: unknown key alpha");
    assert_eq!(message(r#"{"language":"en"}"#), "settings.json: language is not an integer");
    assert_eq!(message(r#"{"language":1.0}"#), "settings.json: language is not an integer");
    assert_eq!(message(r#"{"language":18446744073709551615}"#), "settings.json: language is out of range");
    assert_eq!(message(r#"{"summerTime":2}"#), "settings.json: summerTime is not a boolean");
    assert_eq!(message(r#"{"clockOffset":900000000000}"#), "settings.json: clockOffset 900000000000 is out of range");
    assert_eq!(message("[]"), "settings.json: not an object");
    let tokyo = HostZone::locate("Asia/Tokyo").unwrap();
    let none = temp().join("HostInputsTest-absent2.json");
    let e = host_inputs_from(&flags(Some(9), None), 0, &none, &tokyo).unwrap_err().0;
    assert_eq!(e, "--language: language 9 is outside 1..7");
    let truncated = read_stored(&write("summerWrap", r#"{"summerTime":4294967297}"#)).unwrap();
    assert_eq!(truncated.summer_time, Some(true));
}

#[test]
fn written_file_is_the_cpp_layout() {
    let path = temp().join("HostInputsTest-layout").join("settings.json");
    write_stored(&path, &StoredSettings { language: Some(3), clock_offset: Some(-60), summer_time: Some(false), ..Default::default() }).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\n  \"clockOffset\": -60,\n  \"language\": 3,\n  \"summerTime\": false\n}\n");
    write_stored(&path, &StoredSettings::default()).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}\n");
}
