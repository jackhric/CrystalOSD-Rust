use crate::clock_date::{BASE_ZONE, date_of, seconds_of};

pub type DateWords = [i32; 6];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleZone {
    pub minutes: i32,
    pub summer: bool,
}

fn seconds_of_words(d: &DateWords) -> i64 {
    seconds_of(d[0], d[1], d[2], d[3], d[4], d[5])
}

fn unix_epoch() -> i64 {
    seconds_of(1970, 1, 1, 0, 0, 0)
}

// config_get_timezone_offset and config_get_daylight_saving: bits 9..19 signed, bit 29
pub fn zone_of(settings_word: u32) -> ConsoleZone {
    ConsoleZone { minutes: ((settings_word << 12) as i32) >> 21, summer: ((settings_word >> 29) & 1) != 0 }
}

// func_00214728: from the base zone to the configured one, an hour more with summer time
pub fn local_of(rtc: &DateWords, zone: ConsoleZone) -> DateWords {
    let t = seconds_of_words(rtc) - i64::from(BASE_ZONE) * 60 + i64::from(zone.minutes) * 60;
    date_of(if zone.summer { t + 3600 } else { t })
}

// func_00214788 via func_002148B8: the inverse of func_00214728; a negative year or hour leaves the items
pub fn rtc_of(local: &DateWords, zone: ConsoleZone) -> DateWords {
    if local[3] < 0 || local[0] < 0 {
        return *local;
    }
    let t = seconds_of_words(local);
    date_of((if zone.summer { t - 3600 } else { t }) - i64::from(zone.minutes) * 60 + i64::from(BASE_ZONE) * 60)
}

// the console RTC runs in the base zone: UTC plus the zone of city 0x33
pub fn rtc_at(host_utc_seconds: i64) -> DateWords {
    date_of(unix_epoch() + host_utc_seconds + i64::from(BASE_ZONE) * 60)
}

pub fn unix_of(rtc: &DateWords) -> i64 {
    seconds_of_words(rtc) - unix_epoch()
}
