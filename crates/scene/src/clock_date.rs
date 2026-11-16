use crate::console_clock::zone_of;

const MONTHS: [[i32; 12]; 2] = [[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31], [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]];

fn leap(y: i32) -> i32 {
    if y % 400 == 0 {
        1
    } else if y % 100 == 0 {
        0
    } else if (y & 3) == 0 {
        1
    } else {
        0
    }
}

fn month_length(row: i32, month: i32) -> i32 {
    if !(0..=11).contains(&month) {
        panic!("month outside 1..12");
    }
    MONTHS[row as usize][month as usize]
}

fn zoned(seconds: i64, settings: u32) -> i64 {
    let zone = zone_of(settings);
    let t = seconds - i64::from(BASE_ZONE) * 60 + i64::from(zone.minutes) * 60;
    if zone.summer { t + 3600 } else { t }
}

fn days_in(y: i32, month: i32) -> i32 {
    if month == 2 {
        return if y % 400 == 0 {
            29
        } else if y % 100 == 0 {
            28
        } else if (y & 3) == 0 {
            29
        } else {
            28
        };
    }
    month_length(0, month - 1)
}

pub const BASE_ZONE: i32 = 540;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DateCheck {
    pub items: [i32; 6],
    pub ranges: [[i32; 2]; 6],
}

pub fn seconds_of(y: i32, mo: i32, d: i32, h: i32, mi: i32, s: i32) -> i64 {
    let years = y - 1600;
    let mut days = d - 1;
    let row = leap(years);
    for i in 0..mo - 1 {
        days += month_length(row, i);
    }
    let fours = (if years + 3 > -1 { years + 3 } else { years + 6 }) >> 2;
    days += 365 * years + fours - (years + 99) / 100 + (years + 399) / 400;
    ((i64::from(days) * 24 + i64::from(h)) * 60 + i64::from(mi)) * 60 + i64::from(s)
}

pub fn date_of(seconds: i64) -> [i32; 6] {
    let mut t = seconds;
    let s = (t % 60) as i32;
    t /= 60;
    let mi = (t % 60) as i32;
    t /= 60;
    let h = (t % 24) as i32;
    t /= 24;
    let mut y = 1600;
    if t > 0x23ab0 {
        t -= 0x23ab1;
        y += (t / 0x23ab1 + 1) as i32 * 400;
        t %= 0x23ab1;
    }
    let mut short_year = 0;
    if t >= 0x8ead {
        t -= 0x8ead;
        y += (t / 0x8eac + 1) as i32 * 100;
        t %= 0x8eac;
        short_year = 1;
    }
    let four = 0x5b5 - i64::from(short_year);
    if t < four {
        short_year ^= 1;
    } else {
        t -= four;
        y += (t / 0x5b5 + 1) as i32 * 4;
        t %= 0x5b5;
        short_year = 1;
    }
    let first = i64::from(short_year) + 0x16d;
    if t >= first {
        t -= first;
        y += 1 + (t / 0x16d) as i32;
        t %= 0x16d;
    }
    let row = leap(y);
    let mut m = 0;
    while t >= i64::from(month_length(row, m)) {
        t -= i64::from(month_length(row, m));
        m += 1;
    }
    [y, m + 1, t as i32 + 1, h, mi, s]
}

pub fn date_check(items: &[i32; 6], settings: u32) -> DateCheck {
    let lo = date_of(zoned(0x2f0605980, settings));
    let hi = date_of(zoned(0x3ac796cff, settings));
    let (y, mo, d, h) = (items[0], items[1], items[2], items[3]);
    let later = lo[0] < y;
    let earlier = y < hi[0];
    let min_month = if later { 1 } else { lo[1] };
    let max_month = if earlier { 12 } else { hi[1] };
    let min_day = if later || min_month < mo { 1 } else { lo[2] };
    let mut max_day = if earlier || mo < max_month { 31 } else { hi[2] };
    let min_hour = if later || min_month < mo || min_day < d { 0 } else { lo[3] };
    let max_hour = if earlier || mo < max_month || d < max_day { 23 } else { hi[3] };
    let min_minute = if later || min_month < mo || min_day < d || min_hour < h { 0 } else { lo[4] };
    let max_minute = if earlier || mo < max_month || d < max_day || h < max_hour { 59 } else { hi[4] };
    let mut out = *items;
    let t = seconds_of(items[0], items[1], items[2], items[3], items[4], items[5]);
    let low = zoned(0x2f0605980, settings);
    let high = zoned(0x3ac796cff, settings);
    if t < low {
        out = date_of(low);
    }
    if high < (if t < low { low } else { t }) {
        out = date_of(high);
    }
    let dim = days_in(out[0], out[1]);
    if dim < max_day {
        max_day = dim;
    }
    if max_day < out[2] {
        out[2] = max_day;
    }
    DateCheck { items: out, ranges: [[lo[0], hi[0]], [min_month, max_month], [min_day, max_day], [min_hour, max_hour], [min_minute, max_minute], [0, 59]] }
}
