use crate::arithmetic::Arithmetic;
use crate::clock_date::{date_of, seconds_of};
use crate::clock_state::ClockTime;
use crate::cold_types::RtcTime;
use crate::program_image::ProgramImage;
use crate::{Error, Result};

const ZONE_TABLE: u32 = 0x2AD988;
const ZONE_COUNT: u32 = 0x2AE5A0;
const FRAME_RATE_NTSC: u32 = 0x36FC34;
const BASE_CITY: i32 = 0x33;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ColdTimeOut {
    pub time: ClockTime,
    pub day: i32,
    pub month: i32,
    pub year: i32,
    pub time_filled: bool,
    pub zone: [i32; 2],
    pub frame_period: f32,
}

fn byte_at(program: &ProgramImage, address: u32) -> i32 {
    ((program.word(address & !3) >> (8 * (address & 3))) & 0xFF) as i32
}

fn base_zone(program: &ProgramImage) -> Result<i32> {
    let count = program.integer(ZONE_COUNT);
    for i in 0..count {
        let entry = ZONE_TABLE.wrapping_add((i as u32).wrapping_mul(0x18));
        if byte_at(program, entry + 2) == BASE_CITY {
            return Ok(i32::from((program.word(entry) & 0xFFFF) as u16 as i16));
        }
    }
    Err(Error("no time zone entry for the base city".into()))
}

// HDD OSD 1.10U 0x235848, 0x2358F8, 0x235A68 verify_cold_time.mjs
pub fn cold_time<A: Arithmetic>(program: &ProgramImage, rtc: &RtcTime, time_zone: i32, summer: bool, pal: bool) -> Result<ColdTimeOut> {
    let minutes = (if summer { 60 } else { 0 }) + i64::from(time_zone) - i64::from(base_zone(program)?);
    let seconds = seconds_of(rtc.year, rtc.month, rtc.day, rtc.hour, rtc.minute, rtc.second) + minutes * 60;
    let date = date_of(seconds);
    let mut out = ColdTimeOut { year: date[0], month: date[1], day: date[2], ..Default::default() };
    out.time.hours = date[3];
    out.time.minutes = date[4];
    out.time.seconds = date[5];
    out.time.milliseconds = 0.0;
    out.time_filled = false;
    out.zone[0] = time_zone;
    out.zone[1] = i32::from(summer);
    let rate = if pal { 50.0 } else { program.single(FRAME_RATE_NTSC) };
    out.frame_period = A::div(1000.0, rate);
    Ok(out)
}
