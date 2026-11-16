use std::path::Path;
use std::str::FromStr;

use chrono::{NaiveDate, Offset, TimeZone};
use chrono_tz::{OffsetComponents, Tz};
use scene::clock_date::BASE_ZONE;
use scene::cold_types::{ColdInputs, RtcTime};

use crate::settings_store::{ASPECT, Error, LANGUAGE, Range, StoredSettings, TIME_ZONE, read_stored};

pub const NANOS: i64 = 1_000_000_000;

// HDD OSD 1.10U 0x203A20 config_set_default_main: language field 0 reads as 1, aspect 0, summer 0, 24 hour, date format 0.
const DEFAULT_LANGUAGE: i32 = 1;
const DEFAULT_ASPECT: i32 = 0;
const DEFAULT_DATE_FORMAT: i32 = 0;
const DEFAULT_TIME_FORMAT: i32 = 0;

const NO_DISC: u32 = 0x64;
const SCREEN_WIDTH: u32 = 640;
const SCREEN_HEIGHT_NTSC: u32 = 0xE0;
const SCREEN_HEIGHT_PAL: u32 = 0x100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostOptions {
    pub pal: bool,
    pub language: Option<i32>,
    pub aspect: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneInfo {
    pub offset_minutes: i64,
    pub save_minutes: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostZone(pub Tz);

impl HostZone {
    pub fn locate(name: &str) -> Option<HostZone> {
        Tz::from_str(name).ok().map(HostZone)
    }

    pub fn current() -> Option<HostZone> {
        if let Ok(link) = std::fs::read_link("/etc/localtime") {
            let parts: Vec<String> = link.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            if parts.len() > 2 {
                let last = &parts[parts.len() - 1];
                if let Some(zone) = Self::locate(last) {
                    return Some(zone);
                }
                if let Some(zone) = Self::locate(&format!("{}/{last}", parts[parts.len() - 2])) {
                    return Some(zone);
                }
            }
        }
        for file in ["/etc/timezone", "/var/db/zoneinfo"] {
            if let Ok(text) = std::fs::read_to_string(file) {
                let name = text.split_whitespace().next().unwrap_or("");
                if let Some(zone) = Self::locate(name) {
                    return Some(zone);
                }
            }
        }
        Self::locate("UTC")
    }

    pub fn info(&self, now_ns: i64) -> ZoneInfo {
        let utc = chrono::DateTime::from_timestamp(now_ns.div_euclid(NANOS), 0).expect("a representable time").naive_utc();
        let offset = self.0.offset_from_utc_datetime(&utc);
        ZoneInfo { offset_minutes: i64::from(offset.fix().local_minus_utc()) / 60, save_minutes: offset.dst_offset().num_seconds() / 60 }
    }

    pub fn to_sys_earliest(&self, year: i32, month: u32, day: u32, hour: u32) -> i64 {
        let local = NaiveDate::from_ymd_opt(year, month, day).and_then(|d| d.and_hms_opt(hour, 0, 0)).expect("a valid local time");
        match self.0.from_local_datetime(&local).earliest() {
            Some(t) => t.timestamp(),
            None => {
                let before = self.0.offset_from_utc_datetime(&local);
                local.and_utc().timestamp() - i64::from(before.fix().local_minus_utc())
            }
        }
    }
}

pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn checked(key: &str, value: i64, range: Range, origin: &str) -> Result<i32, Error> {
    if value < i64::from(range.low) || value > i64::from(range.high) {
        return Err(Error(format!("{origin}: {key} {value} is outside {}..{}", range.low, range.high)));
    }
    Ok(value as i32)
}

pub fn host_inputs_from(options: &HostOptions, now_ns: i64, settings: &Path, zone: &HostZone) -> Result<ColdInputs, Error> {
    host_inputs(options, now_ns, &read_stored(settings)?, zone)
}

pub fn host_inputs(options: &HostOptions, now_ns: i64, stored: &StoredSettings, zone: &HostZone) -> Result<ColdInputs, Error> {
    let mut out = ColdInputs {
        language: DEFAULT_LANGUAGE,
        aspect: DEFAULT_ASPECT,
        date_format: DEFAULT_DATE_FORMAT,
        time_format: DEFAULT_TIME_FORMAT,
        summer_time: false,
        pal: options.pal,
        screen_width: SCREEN_WIDTH,
        screen_height: if options.pal { SCREEN_HEIGHT_PAL } else { SCREEN_HEIGHT_NTSC },
        rand_state: 1,
        wide: false,
        screen_code: NO_DISC,
        ..Default::default()
    };
    if let Some(v) = stored.language {
        out.language = v;
    }
    if let Some(v) = stored.aspect {
        out.aspect = v;
    }
    if let Some(v) = stored.time_zone {
        out.time_zone = v;
    }
    if let Some(v) = stored.summer_time {
        out.summer_time = v;
    }
    if let Some(v) = stored.date_format {
        out.date_format = v;
    }
    if let Some(v) = stored.time_format {
        out.time_format = v;
    }
    out.clock_offset = stored.clock_offset;
    if let Some(v) = options.language {
        out.language = checked("language", i64::from(v), LANGUAGE, "--language")?;
    }
    if let Some(v) = options.aspect {
        out.aspect = checked("aspect", i64::from(v), ASPECT, "--aspect")?;
    }
    let info = zone.info(now_ns);
    if stored.time_zone.is_none() {
        out.time_zone = checked("timeZone", info.offset_minutes - info.save_minutes, TIME_ZONE, "host time zone")?;
    }
    if stored.summer_time.is_none() {
        out.summer_time = info.save_minutes != 0;
    }

    let rtc_at = |offset: i64| {
        let console = now_ns.div_euclid(NANOS) + offset + i64::from(BASE_ZONE) * 60;
        let day = console.div_euclid(86400);
        let (year, month, date) = civil_from_days(day);
        let time = console - day * 86400;
        RtcTime {
            year: year as i32,
            month: month as i32,
            day: date as i32,
            hour: (time / 3600) as i32,
            minute: (time / 60 % 60) as i32,
            second: (time % 60) as i32,
        }
    };
    out.rtc = rtc_at(out.clock_offset.unwrap_or(0));
    if out.clock_offset.is_some() && (out.rtc.year < 2000 || out.rtc.year > 2099) {
        out.clock_offset = None;
        out.rtc = rtc_at(0);
    }
    Ok(out)
}
