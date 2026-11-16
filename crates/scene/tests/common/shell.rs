// Test scaffolding, not the app shell: just enough of Source/App (Input.cpp PadReader, HostInputs.cpp, Main.cpp startClock,
// stepClock, consoleNow, clockTime, clockItems, the golden branch) to drive the scene as CrystalOSD does. The app-shell port
// does these properly.
#![allow(dead_code)]

use scene::arithmetic::Arithmetic;
use scene::clock::{Clock, FrameInputs};
use scene::clock_date::BASE_ZONE;
use scene::clock_state::ClockTime;
use scene::cold_start::{ColdAssets, cold_start, fill_time};
use scene::cold_types::{ColdInputs, RtcTime};
use scene::console_clock::{DateWords, local_of, rtc_at, unix_of, zone_of};
use scene::frame::Frame;
use scene::menu_types::{ConfigItems, PadWords};
use scene::text::ClockItems;

// Input.cpp PadReader with one source.
#[derive(Debug, Clone, Copy, Default)]
pub struct PadReader {
    held: u32,
    previous: u32,
    pressed: u32,
    released: u32,
}

impl PadReader {
    pub fn down(&mut self, bit: u32) {
        if self.held & bit == 0 {
            self.pressed |= bit;
        }
        self.held |= bit;
    }
    pub fn up(&mut self, bit: u32) {
        self.held &= !bit;
        if self.held & bit == 0 {
            self.released |= bit;
        }
    }
    pub fn frame(&mut self) -> PadWords {
        let now = self.held;
        let mut w = PadWords { held: now, pressed: (now & !self.previous) | self.pressed, released: (self.previous & !now) | self.released, repeating: 0 };
        w.repeating = w.pressed;
        self.previous = now;
        self.pressed = 0;
        self.released = 0;
        w
    }
}

const NANOS: i64 = 1_000_000_000;

pub struct ConsoleNow {
    pub rtc: DateWords,
    pub local: DateWords,
    pub milliseconds: f32,
}

// Main.cpp consoleNow on a system_clock time point in nanoseconds (libstdc++'s system_clock::duration).
pub fn console_now(now_ns: i64, zone_word: u32) -> ConsoleNow {
    let second = now_ns.div_euclid(NANOS);
    let rtc = rtc_at(second);
    ConsoleNow { rtc, local: local_of(&rtc, zone_of(zone_word)), milliseconds: (now_ns - second * NANOS) as f32 / 1_000_000.0 }
}

pub fn clock_time(now_ns: i64, zone_word: u32) -> ClockTime {
    let c = console_now(now_ns, zone_word);
    ClockTime { milliseconds: c.milliseconds, seconds: c.local[5], minutes: c.local[4], hours: c.local[3] }
}

pub fn clock_items(now_ns: i64, zone_word: u32) -> ClockItems {
    let l = console_now(now_ns, zone_word).local;
    ClockItems { year: l[0], month: l[1], day: l[2], hour: l[3], minute: l[4], second: l[5] }
}

// Main.cpp's golden clock: goldenStart + duration_cast<nanoseconds>(duration<double>(ticks * 1001 / 60000)).
pub fn golden_now(start_seconds: i64, ticks: u64) -> i64 {
    start_seconds * NANOS + (ticks as f64 * 1001.0 / 60000.0 * 1e9) as i64
}

// HostInputs.cpp hostInputs with no stored settings and no flags: the host zone's offset (minutes, without summer time)
// and its summer flag, the console's RTC (JST) at `now`.
pub fn host_inputs(pal: bool, now_ns: i64, zone_minutes: i32, summer: bool) -> ColdInputs {
    let r = rtc_at(now_ns.div_euclid(NANOS));
    ColdInputs {
        language: 1,
        aspect: 0,
        date_format: 0,
        time_format: 0,
        summer_time: summer,
        pal,
        screen_width: 640,
        screen_height: if pal { 0x100 } else { 0xE0 },
        rand_state: 1,
        wide: false,
        screen_code: 0x64,
        time_zone: zone_minutes,
        rtc: RtcTime { year: r[0], month: r[1], day: r[2], hour: r[3], minute: r[4], second: r[5] },
        ..Default::default()
    }
}

// Main.cpp startClock and stepClock: the clock screen as the app drives it.
pub struct Session<A> {
    pub clock: Clock<A>,
    pub inputs: FrameInputs,
    pub items: ConfigItems,
    pub reader: PadReader,
    cold_word: u32,
    offset_ns: i64,
    started: bool,
}

impl<A: Arithmetic> Session<A> {
    pub fn start(assets: &ColdAssets, cold: &ColdInputs) -> Self {
        let mut start = cold_start::<A>(assets, cold).unwrap();
        if let Some(menus) = start.clock.menus.as_mut() {
            menus.options.browser_enters = false;
        }
        start.clock.pass_names = false;
        let clock = Clock::<A>::new(&start.clock).unwrap();
        let mut inputs = start.frame;
        let cold_word = inputs.menu.mechacon_param.map_or(0, |p| p[0]);
        inputs.field = 0;
        inputs.thread_step = false;
        let items = start.clock.menus.as_ref().unwrap().items;
        Self { clock, inputs, items, reader: PadReader::default(), cold_word, offset_ns: cold.clock_offset.unwrap_or(0) * NANOS, started: true }
    }

    pub fn zone_word(&self) -> u32 {
        self.clock.externals().mechacon_param.map_or(self.cold_word, |p| p[0])
    }

    // produce() then stepClock() for one frame at the fixed `wall_ns`.
    pub fn step(&mut self, wall_ns: i64) -> Frame {
        let now = wall_ns + self.offset_ns;
        if !self.started {
            self.inputs.time = clock_time(now, self.zone_word());
            self.inputs.items = clock_items(now, self.zone_word());
        }
        self.started = false;
        if self.clock.menus().unwrap().page.level != 1 {
            let local = clock_items(now, self.zone_word());
            self.items[6] = local.year;
            self.items[7] = local.month;
            self.items[8] = local.day;
            self.items[9] = local.hour;
            self.items[10] = local.minute;
            self.items[11] = local.second;
        }
        self.inputs.menu.pad = self.reader.frame();
        self.inputs.menu.rtc_mirror = Some(console_now(now, self.zone_word()).rtc);
        self.inputs.config_items = Some(self.items);
        let produced = self.clock.frame(&self.inputs);
        if let Some(written) = self.clock.rtc_write() {
            self.offset_ns = (unix_of(written) - wall_ns.div_euclid(NANOS) - i64::from(BASE_ZONE) * 60) * NANOS;
        }
        self.items = *self.clock.items();
        fill_time(&mut self.inputs);
        self.inputs.thread_step = true;
        self.inputs.field = 0;
        self.inputs.display_index ^= 1;
        produced
    }
}
