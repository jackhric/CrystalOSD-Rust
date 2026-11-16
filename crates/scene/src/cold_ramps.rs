use crate::ramp::Ramp;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ColdLengths {
    pub menu_short: i32,
    pub menu_long: i32,
    pub one: i32,
    pub vignette: Ramp,
    pub grey: Ramp,
    pub cube: Ramp,
    pub menu: Ramp,
    pub config: Ramp,
    pub version: Ramp,
    pub sprite_fade: Ramp,
    pub main_menu: Ramp,
    pub body: i32,
    pub tail: i32,
    pub vignette_length: i32,
}

pub fn ramp_reset(mut ramp: Ramp) -> Ramp {
    ramp.counter = 0;
    ramp.changed = 0;
    ramp.state = 0;
    ramp
}

pub fn ramp_start(mut ramp: Ramp) -> Ramp {
    if ramp.state == 0 {
        ramp.counter = 0;
        ramp.changed = 1;
        ramp.state = 1;
    }
    ramp
}

pub fn ramp_reverse(mut ramp: Ramp) -> Ramp {
    if ramp.state == 2 {
        ramp.counter = ramp.length;
        ramp.changed = 1;
        ramp.state = 3;
    }
    ramp
}

pub fn ramp_set(mut ramp: Ramp, length: i32) -> Ramp {
    ramp.length = length;
    ramp
}

pub fn ramp_is(ramp: &Ramp, state: i32) -> bool {
    ramp.state == state
}

pub fn ramp_scaled(ramp: &Ramp, scale: i32) -> i32 {
    ramp.counter.wrapping_mul(scale) / ramp.length
}

pub fn cold_lengths(pal: bool) -> ColdLengths {
    let fps = if pal { 50 } else { 60 };
    let mut out = ColdLengths { menu_short: fps * 40 / 60, ..Default::default() }; // HDD OSD 1.10U 0x234B88 verify_cold_ramps.mjs
    out.menu_long = fps * 80 / 60;
    out.one = 1;
    out.vignette_length = out.menu_long;
    out.vignette = ramp_reset(ramp_set(Ramp::default(), out.menu_long));
    out.sprite_fade = ramp_start(ramp_reset(ramp_set(Ramp::default(), fps * 256 / 60))); // 0x238F20
    out.body = fps * 40 / 60; // 0x2324C8
    out.tail = fps / 6;
    out.cube = ramp_set(Ramp::default(), out.body);
    out.menu = ramp_set(Ramp::default(), out.body + out.tail);
    out.config = ramp_set(Ramp::default(), out.menu_short + out.body + out.tail);
    out.main_menu = ramp_reset(ramp_set(Ramp::default(), out.tail));
    out.version = ramp_set(Ramp::default(), out.tail); // 0x22A9C0
    out.grey = ramp_set(Ramp::default(), out.menu_short); // 0x232858
    out
}
