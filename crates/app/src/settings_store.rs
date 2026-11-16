use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::json::{self, Json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub low: i32,
    pub high: i32,
}

// HDD OSD 1.10U config_get_osd_language 0x203DD8 (language 1 to 7), config_get_aspect_ratio 0x203D30 (0 to 2), config_set_timezone_offset
// 0x203F00 (11 bits), config_get_date_format 0x204048 (0 to 2), config_get_time_format 0x204008 (1 bit).
pub const LANGUAGE: Range = Range { low: 1, high: 7 };
pub const ASPECT: Range = Range { low: 0, high: 2 };
pub const TIME_ZONE: Range = Range { low: -1024, high: 1023 };
pub const DATE_FORMAT: Range = Range { low: 0, high: 2 };
pub const TIME_FORMAT: Range = Range { low: 0, high: 1 };

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoredSettings {
    pub language: Option<i32>,
    pub aspect: Option<i32>,
    pub time_zone: Option<i32>,
    pub date_format: Option<i32>,
    pub time_format: Option<i32>,
    pub summer_time: Option<bool>,
    pub clock_offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

// An overflow guard for the date arithmetic only; the clock's year range decides what is accepted.
const OFFSET_GUARD: i64 = 100 * 366 * 86400;

const KEYS: [&str; 7] = ["language", "aspect", "timeZone", "summerTime", "dateFormat", "timeFormat", "clockOffset"];

fn fail<T>(text: String) -> Result<T, Error> {
    Err(Error(text))
}

fn whole_number(v: &Json, key: &str) -> Result<i64, Error> {
    match *v {
        Json::Integer(i) => Ok(i),
        Json::Unsigned(u) if u > i64::MAX as u64 => fail(format!("settings.json: {key} is out of range")),
        Json::Unsigned(u) => Ok(u as i64),
        _ => fail(format!("settings.json: {key} is not an integer")),
    }
}

fn integer(j: &BTreeMap<Vec<u8>, Json>, key: &str, range: Range) -> Result<i32, Error> {
    let value = whole_number(&j[key.as_bytes()], key)?;
    if value < i64::from(range.low) || value > i64::from(range.high) {
        return fail(format!("settings.json: {key} {value} is outside {}..{}", range.low, range.high));
    }
    Ok(value as i32)
}

pub fn read_stored(path: &Path) -> Result<StoredSettings, Error> {
    let mut out = StoredSettings::default();
    let Ok(mut file) = fs::File::open(path) else { return Ok(out) };
    let mut text = Vec::new();
    if let Err(error) = io::Read::read_to_end(&mut file, &mut text) {
        return fail(format!("basic_filebuf::underflow error reading the file: {}", os_message(&error)));
    }
    let j = json::parse(&text).map_err(|e| Error(format!("settings.json: {e}")))?;
    let Json::Object(j) = j else { return fail("settings.json: not an object".to_owned()) };
    if let Some(key) = j.keys().find(|k| !KEYS.iter().any(|known| known.as_bytes() == k.as_slice())) {
        return fail(format!("settings.json: unknown key {}", String::from_utf8_lossy(key)));
    }
    let has = |key: &str| j.contains_key(key.as_bytes());
    if has("language") {
        out.language = Some(integer(&j, "language", LANGUAGE)?);
    }
    if has("aspect") {
        out.aspect = Some(integer(&j, "aspect", ASPECT)?);
    }
    if has("timeZone") {
        out.time_zone = Some(integer(&j, "timeZone", TIME_ZONE)?);
    }
    if let Some(v) = j.get(b"summerTime".as_slice()) {
        out.summer_time = match *v {
            Json::Boolean(b) => Some(b),
            Json::Integer(i) if matches!(i as i32, 0 | 1) => Some(i as i32 == 1),
            Json::Unsigned(u) if matches!(u as i32, 0 | 1) => Some(u as i32 == 1),
            _ => return fail("settings.json: summerTime is not a boolean".to_owned()),
        };
    }
    if has("dateFormat") {
        out.date_format = Some(integer(&j, "dateFormat", DATE_FORMAT)?);
    }
    if has("timeFormat") {
        out.time_format = Some(integer(&j, "timeFormat", TIME_FORMAT)?);
    }
    if let Some(v) = j.get(b"clockOffset".as_slice()) {
        let value = whole_number(v, "clockOffset")?;
        if !(-OFFSET_GUARD..=OFFSET_GUARD).contains(&value) {
            return fail(format!("settings.json: clockOffset {value} is out of range"));
        }
        out.clock_offset = Some(value);
    }
    Ok(out)
}

fn os_message(error: &io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_owned(),
        None => text,
    }
}

pub fn write_stored(path: &Path, settings: &StoredSettings) -> Result<(), Error> {
    let mut j = Map::new();
    if let Some(v) = settings.language {
        j.insert("language".into(), v.into());
    }
    if let Some(v) = settings.aspect {
        j.insert("aspect".into(), v.into());
    }
    if let Some(v) = settings.time_zone {
        j.insert("timeZone".into(), v.into());
    }
    if let Some(v) = settings.summer_time {
        j.insert("summerTime".into(), v.into());
    }
    if let Some(v) = settings.date_format {
        j.insert("dateFormat".into(), v.into());
    }
    if let Some(v) = settings.time_format {
        j.insert("timeFormat".into(), v.into());
    }
    if let Some(v) = settings.clock_offset {
        j.insert("clockOffset".into(), v.into());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| Error(format!("filesystem error: cannot create directories: {} [{}]", os_message(&e), parent.display())))?;
    }
    let mut temporary = PathBuf::from(path).into_os_string();
    temporary.push(format!(".{}.tmp", std::process::id()));
    let temporary = PathBuf::from(temporary);
    let text = serde_json::to_string_pretty(&Value::Object(j)).expect("a JSON object") + "\n";
    let written = fs::File::create(&temporary).and_then(|mut out| {
        out.write_all(text.as_bytes())?;
        out.flush()
    });
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
        return fail(format!("settings.json: cannot write {}", temporary.display()));
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return fail(format!("settings.json: cannot replace {}: {}", path.display(), os_message(&error)));
    }
    Ok(())
}
