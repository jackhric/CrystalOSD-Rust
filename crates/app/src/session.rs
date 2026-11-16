use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use audio::ee_sound_queue::SoundCommand as AudioCommand;
use audio::live_audio::LiveAudio;
use core_log::Log;
use scene::arithmetic::NativeArithmetic;
use scene::browser::screen::{BrowserScreen, BrowserSetup, CardSlot};
use scene::browser::shell::Shell as BrowserWords;
use scene::clock::{Clock, ClockInputs, FrameInputs};
use scene::clock_date::BASE_ZONE;
use scene::clock_state::ClockTime;
use scene::cold_start::{ColdAssets, cold_start, fill_time};
use scene::console_clock::{DateWords, local_of, rtc_at, unix_of, zone_of};
use scene::frame::Frame;
use scene::menu_types::ConfigItems;
use scene::opening::{BootOptions, FRAMES_TO_CLOCK, History, HistoryEntry};
use scene::program_image::ProgramImage;
use scene::sound::{CLOCK_INIT_FRAMES, SoundCommand, THREAD_START_LAG_FRAMES, clock_set_up, clock_thread_start};
use scene::text::ClockItems;

use crate::boot_chain::{BootChain, BootPhase};
use crate::debug_panel::PanelState;
use crate::golden::hash_bytes;
use crate::host_inputs::{HostOptions, HostZone, NANOS, host_inputs};
use crate::input::PadReader;
use crate::native_frames::{first_frame, next_frame};
use crate::opening_screen::OpeningScreen;
use crate::profile::{Profile, Stage};
use crate::screen::FrameSource;
use crate::screens::{screen_name, screen_of};
use crate::settings_store::{StoredSettings, read_stored, write_stored};

pub struct ConsoleNow {
    pub rtc: DateWords,
    pub local: DateWords,
    pub milliseconds: f32,
}

pub fn utc_seconds(now_ns: i64) -> i64 {
    now_ns.div_euclid(NANOS)
}

pub fn console_now(now_ns: i64, zone_word: u32) -> ConsoleNow {
    let second = now_ns.div_euclid(NANOS);
    let rtc = rtc_at(second);
    ConsoleNow { rtc, local: local_of(&rtc, zone_of(zone_word)), milliseconds: (now_ns - second * NANOS) as f32 / 1_000_000.0 }
}

// The OSD's time keeper gives hours, minutes, seconds and the milliseconds into the second; here the console's local time.
pub fn clock_time(now_ns: i64, zone_word: u32) -> ClockTime {
    let c = console_now(now_ns, zone_word);
    ClockTime { milliseconds: c.milliseconds, seconds: c.local[5], minutes: c.local[4], hours: c.local[3] }
}

// Configuration items 6 to 0xB, which the clock screen's text formats: the local date and time.
pub fn clock_items(now_ns: i64, zone_word: u32) -> ClockItems {
    let l = console_now(now_ns, zone_word).local;
    ClockItems { year: l[0], month: l[1], day: l[2], hour: l[3], minute: l[4], second: l[5] }
}

pub fn system_now() -> i64 {
    let now = std::time::SystemTime::now();
    match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i64,
        Err(e) => -(e.duration().as_nanos() as i64),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fatal {
    DeviceLost(String),
    Error(String),
}

impl From<render::Error> for Fatal {
    fn from(e: render::Error) -> Self {
        match e {
            render::Error::DeviceLost(s) => Fatal::DeviceLost(s),
            other => Fatal::Error(other.to_string()),
        }
    }
}

pub struct FunctionScreen<C> {
    step: Box<dyn FnMut(&mut C) -> Frame>,
    frame: Frame,
}

impl<C> FunctionScreen<C> {
    pub fn new(step: impl FnMut(&mut C) -> Frame + 'static) -> Self {
        FunctionScreen { step: Box::new(step), frame: Frame::default() }
    }
}

impl<C> FrameSource<C> for FunctionScreen<C> {
    fn step(&mut self, context: &mut C) {
        self.frame = (self.step)(context);
    }

    fn frame(&mut self) -> &mut Frame {
        &mut self.frame
    }

    fn done(&self) -> bool {
        false
    }
}

pub type Chain = BootChain<OpeningScreen, FunctionScreen<Session>>;

// The clock's last frame is P + 131 after the menu's Cross (facts/clock-transitions.md section 2) and the Browser's first pass
// P + 135 (hddosd-110U-browser-sounds-enter: the thread starts in frame 134): three frames draw nothing, the third starts the thread.
pub const TO_BROWSER_FRAMES: u32 = 3;
// facts/browser-shell.md section 6: five frames between the Browser's last pass and the clock's first frame (HDD OSD, NTSC).
pub const TO_CLOCK_FRAMES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserPhase {
    Off,
    Leaving(u32),
    Running,
    HandOff(u32),
}

// newlib's rand: the state after `draws` calls (facts/clock-transitions.md section 3, the seven orb angles of a clock start).
fn rand_after(state: u32, draws: u32) -> u32 {
    (0..draws).fold(state, |s, _| s.wrapping_mul(0x41c6_4e6d).wrapping_add(12345))
}

pub struct SessionConfig {
    pub host: HostOptions,
    pub host_zone: HostZone,
    pub cold_assets: ColdAssets,
    pub program: Arc<ProgramImage>,
    pub pass_names: bool,
    pub save: bool,
    pub settings: PathBuf,
    pub lights_phase: Option<u32>,
    pub towers_demo: bool,
    pub browser: bool,
    pub memory_cards: [Option<CardSlot>; 2],
}

pub struct Session {
    pub config: SessionConfig,
    pub stored_at_start: StoredSettings,
    pub clock: Option<Clock<NativeArithmetic>>,
    pub clock_inputs: Option<ClockInputs>,
    pub inputs: FrameInputs,
    pub offset_ns: i64,
    pub fixed_now: Option<i64>,
    pub golden_start: i64,
    pub golden_ticks: u64,
    pub cold_word: u32,
    pub reader: PadReader,
    pub items: ConfigItems,
    pub visited: Vec<String>,
    pub soak_failed: bool,
    pub adjust_expected: bool,
    pub adjust_failed: bool,
    pub adjust_writes: i32,
    pub year_outside: i32,
    pub pending_year: Option<i32>,
    pub audio: Option<LiveAudio>,
    pub thread_start_in: u32,
    pub clock_thread_done: bool,
    pub set_up_in: u32,
    pub boot_clock_forced: bool,
    pub boot_rand: u32,
    pub error: Option<Fatal>,
    pub browser: Option<BrowserScreen<NativeArithmetic>>,
    pub browser_phase: BrowserPhase,
    pub browser_words: BrowserWords,
    pub clock_wide: bool,
    pub clock_rand: u32,
}

impl Session {
    pub fn new(config: SessionConfig, stored_at_start: StoredSettings) -> Self {
        Session {
            config,
            stored_at_start,
            clock: None,
            clock_inputs: None,
            inputs: FrameInputs::default(),
            offset_ns: 0,
            fixed_now: None,
            golden_start: 0,
            golden_ticks: 0,
            cold_word: 0,
            reader: PadReader::default(),
            items: [0; 20],
            visited: Vec::new(),
            soak_failed: false,
            adjust_expected: false,
            adjust_failed: false,
            adjust_writes: 0,
            year_outside: 0,
            pending_year: None,
            audio: None,
            thread_start_in: 0,
            clock_thread_done: false,
            set_up_in: 0,
            boot_clock_forced: false,
            boot_rand: 0,
            error: None,
            browser: None,
            browser_phase: BrowserPhase::Off,
            browser_words: BrowserWords::default(),
            clock_wide: false,
            clock_rand: 0,
        }
    }

    pub fn wall_now(&self) -> i64 {
        self.fixed_now.unwrap_or_else(system_now)
    }

    pub fn zone_word(&self) -> u32 {
        if let Some(clock) = &self.clock
            && let Some(param) = clock.externals().mechacon_param
        {
            return param[0];
        }
        self.cold_word
    }

    pub fn set_console_to(&mut self, rtc: &DateWords) {
        self.offset_ns = (unix_of(rtc) - utc_seconds(self.wall_now()) - i64::from(BASE_ZONE) * 60) * NANOS;
    }

    pub fn queue_sounds(&mut self, sounds: &[SoundCommand]) {
        let Some(audio) = &mut self.audio else { return };
        for c in sounds {
            audio.queue(AudioCommand { id: c.id, a1: c.a1, a2: c.a2, a3: c.a3 }, c.carried_a2, c.carried_a3);
        }
    }

    pub fn clock_thread_starts(&mut self) {
        if self.clock_thread_done {
            return;
        }
        self.clock_thread_done = true;
        self.set_up_in = CLOCK_INIT_FRAMES;
        let mut sounds = Vec::new();
        clock_thread_start(&mut sounds);
        self.queue_sounds(&sounds);
    }

    pub fn clock_sets_up(&mut self, forced: bool) {
        if forced {
            return;
        }
        let mut sounds = Vec::new();
        clock_set_up(&mut sounds);
        self.queue_sounds(&sounds);
    }

    pub fn clock(&self) -> &Clock<NativeArithmetic> {
        self.clock.as_ref().expect("a started clock")
    }

    pub fn step_clock(&mut self) -> Frame {
        let now = self.wall_now() + self.offset_ns;
        if let Some(year) = self.pending_year.take()
            && clock_items(now, self.zone_word()).year != year
        {
            self.adjust_failed = true;
        }
        if let Some(menus) = self.clock().menus() {
            let level = menus.page.level;
            if level != 1 {
                let local = clock_items(self.wall_now() + self.offset_ns, self.zone_word());
                self.year_outside = local.year;
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
        }
        let inputs = self.inputs;
        let produced = self.clock.as_mut().expect("a started clock").frame(&inputs);
        let sounds = self.clock().sounds().to_vec();
        self.queue_sounds(&sounds);
        if let Some(written) = *self.clock().rtc_write() {
            self.set_console_to(&written);
            self.adjust_writes += 1;
            let year = self.clock().items()[6];
            self.pending_year = Some(year);
            if year != self.year_outside + 1 {
                self.adjust_failed = true;
            }
            if self.config.save {
                let saved = read_stored(&self.config.settings).and_then(|mut stored| {
                    stored.clock_offset = Some(self.offset_ns / NANOS);
                    write_stored(&self.config.settings, &stored)?;
                    Ok(stored.clock_offset)
                });
                match saved {
                    Ok(offset) => self.stored_at_start.clock_offset = offset,
                    Err(error) => core_log::log!(Error, Clock, "settings not saved: {}", error),
                }
            }
        }
        if let Some(menus) = self.clock().menus() {
            let state = self.clock().state();
            let screen = screen_name(screen_of(menus, &state.menu_ramp));
            let failed = state.mode == 3 || state.scene.leaving != 0 || menus.screen_code == 9999;
            self.items = *self.clock().items();
            if self.visited.last().is_none_or(|last| last != screen) {
                self.visited.push(screen.to_owned());
            }
            if failed {
                self.soak_failed = true;
            }
        }
        fill_time(&mut self.inputs);
        self.inputs.thread_step = true;
        next_frame(&mut self.inputs);
        produced
    }

    pub fn start_clock(&mut self, wide: bool, rand_state: Option<u32>) -> Result<(), Fatal> {
        let mut cold = host_inputs(&self.config.host, self.wall_now(), &self.stored_at_start, &self.config.host_zone).map_err(|e| Fatal::Error(e.0))?;
        if let Some(stored) = self.stored_at_start.clock_offset
            && cold.clock_offset.is_none()
        {
            core_log::log!(Warn, Clock, "clockOffset {} s puts the clock outside 2000..2099: dropped", stored);
        }
        self.offset_ns = cold.clock_offset.unwrap_or(0) * NANOS;
        core_log::log!(
            Info,
            Clock,
            "start: console RTC {:04}-{:02}-{:02} {:02}:{:02}:{:02}, offset {} s",
            cold.rtc.year,
            cold.rtc.month,
            cold.rtc.day,
            cold.rtc.hour,
            cold.rtc.minute,
            cold.rtc.second,
            cold.clock_offset.unwrap_or(0)
        );
        cold.wide = wide;
        if let Some(rand) = rand_state {
            cold.rand_state = rand;
        }
        cold.gs_allocator = assets::clock_textures::clock_textures_end();
        self.clock_wide = wide;
        self.clock_rand = cold.rand_state;
        let start = cold_start::<NativeArithmetic>(&self.config.cold_assets, &cold).map_err(|e| Fatal::Error(e.0))?;
        let mut clock_inputs = start.clock;
        if let Some(menus) = clock_inputs.menus.as_mut() {
            menus.options.browser_enters = self.config.browser;
        }
        clock_inputs.pass_names = self.config.pass_names;
        self.clock = Some(Clock::new(&clock_inputs).map_err(|e| Fatal::Error(e.0))?);
        self.inputs = start.frame;
        self.cold_word = self.inputs.menu.mechacon_param.map_or(0, |p| p[0]);
        first_frame(&mut self.inputs);
        self.inputs.thread_step = false;
        self.items = clock_inputs.menus.as_ref().expect("the clock's menus").items;
        self.clock_inputs = Some(clock_inputs);
        Ok(())
    }

    pub fn settle(&mut self, mut done: impl FnMut(&Session) -> bool) {
        let mut n = 0;
        while !done(self) && n < 2000 {
            let now = self.wall_now() + self.offset_ns;
            self.inputs.time = clock_time(now, self.zone_word());
            self.inputs.items = clock_items(now, self.zone_word());
            self.step_clock();
            n += 1;
        }
    }

    pub fn boot_options(&self) -> BootOptions {
        let mut boot = BootOptions { lights_phase: self.config.lights_phase, ..Default::default() };
        if self.config.towers_demo {
            let mut history: History = [HistoryEntry::default(); 21];
            let names = ["DEMO A", "DEMO B", "DEMO C", "DEMO D", "DEMO E"];
            for (i, name) in names.iter().enumerate() {
                history[i].name[..name.len()].copy_from_slice(name.as_bytes());
                history[i].count = (3 + 4 * i) as u8;
                history[i].mask = (0x0Fu32 << i) as u8;
                history[i].main_cell = i as u8;
            }
            boot.history = Some(history);
        }
        boot
    }

    fn clock_screen_step(&mut self) -> Frame {
        if self.error.is_some() {
            return Frame::default();
        }
        if self.clock.is_none() {
            let rand = self.boot_rand;
            if let Err(error) = self.start_clock(true, Some(rand)) {
                self.error = Some(error);
                return Frame::default();
            }
        }
        self.step_screen()
    }

    // The clock, or the Browser when the main menu's Cross on its first item has sent the clock away (scene::menus, browser_enters).
    pub fn step_screen(&mut self) -> Frame {
        match self.browser_phase {
            BrowserPhase::Off => {
                let frame = self.step_clock();
                let state = self.clock().state();
                if self.config.browser && state.scene.leaving != 0 && state.mode == 3 && state.overlay_level == 0 {
                    self.browser_phase = BrowserPhase::Leaving(TO_BROWSER_FRAMES);
                }
                frame
            }
            BrowserPhase::Leaving(n) => {
                if n > 1 {
                    self.browser_phase = BrowserPhase::Leaving(n - 1);
                } else {
                    self.start_browser();
                    self.browser_phase = BrowserPhase::Running;
                }
                Frame::default()
            }
            BrowserPhase::Running => {
                let pad = self.reader.frame();
                let browser = self.browser.as_mut().expect("a running Browser");
                let out = browser.frame(&pad);
                self.browser_words = browser.shell;
                self.queue_sounds(&out.sounds);
                if out.exit.is_some() {
                    self.browser_phase = BrowserPhase::HandOff(TO_CLOCK_FRAMES);
                }
                if self.visited.last().is_none_or(|last| last != "Browser") {
                    self.visited.push("Browser".to_owned());
                }
                out.frame.frame
            }
            BrowserPhase::HandOff(n) => {
                if n > 0 {
                    self.browser_phase = BrowserPhase::HandOff(n - 1);
                    return Frame::default();
                }
                self.browser = None;
                self.browser_phase = BrowserPhase::Off;
                let rand = rand_after(self.clock_rand, 7);
                let wide = self.clock_wide;
                if let Err(error) = self.start_clock(wide, Some(rand)) {
                    self.error = Some(error);
                    return Frame::default();
                }
                self.clock_thread_done = false;
                self.clock_thread_starts();
                self.step_clock()
            }
        }
    }

    // The Browser's thread start (facts/browser-shell.md section 1) with the cards the host gave and the 0x8070 push (section 7).
    fn start_browser(&mut self) {
        let mechacon = self.clock.as_ref().and_then(|c| c.externals().mechacon_param).map_or(0, |p| p[0]);
        let setup = BrowserSetup {
            pal: self.config.host.pal,
            width: 640,
            height: if self.config.host.pal { 256 } else { 224 },
            mechacon_param: mechacon,
            cards: self.config.memory_cards,
            program: Some(self.config.program.clone()),
        };
        let text = self.clock.as_ref().and_then(|c| c.text().cloned());
        let browser = BrowserScreen::start(setup, &self.browser_words, text);
        if let (Some(audio), Some(a3)) = (&mut self.audio, browser.spdif) {
            audio.push_spdif(a3);
        }
        self.browser = Some(browser);
    }
}

fn phase_name(phase: BootPhase) -> &'static str {
    match phase {
        BootPhase::Opening => "opening",
        BootPhase::Gap => "black gap",
        BootPhase::Clock => "clock",
    }
}

pub struct Shell {
    pub session: Session,
    pub chain: Option<Chain>,
    pub frame: Frame,
    pub fresh: bool,
    pub logic_frames: u64,
    pub last_phase: BootPhase,
    pub empty_warned: bool,
    pub trace: Option<File>,
    pub profile: Profile,
    pub profiling: bool,
    pub panel: PanelState,
}

impl Shell {
    pub fn new(session: Session) -> Self {
        Shell {
            session,
            chain: None,
            frame: Frame::default(),
            fresh: false,
            logic_frames: 0,
            last_phase: BootPhase::Opening,
            empty_warned: false,
            trace: None,
            profile: Profile::default(),
            profiling: false,
            panel: PanelState::default(),
        }
    }

    pub fn boot_opening(&self) -> Option<&OpeningScreen> {
        self.chain.as_ref().map(|c| c.opening())
    }

    pub fn build_boot(&mut self) {
        let boot = self.session.boot_options();
        let opening = OpeningScreen::new(&boot, &self.session.config.program);
        let s = &mut self.session;
        s.boot_clock_forced = boot.clock_forced;
        s.clock_thread_done = false;
        s.set_up_in = 0;
        s.thread_start_in = 0;
        s.queue_sounds(&opening.start_commands());
        s.clock = None;
        let clock_screen = FunctionScreen::new(Session::clock_screen_step);
        self.chain = Some(BootChain::new(opening, clock_screen, FRAMES_TO_CLOCK));
    }

    pub fn screen_key(&self) -> String {
        if let Some(chain) = &self.chain
            && chain.phase() != BootPhase::Clock
        {
            return chain.name().to_owned();
        }
        self.session.visited.last().cloned().unwrap_or_else(|| "clock".to_owned())
    }

    pub fn produce(&mut self) -> Result<(), Fatal> {
        let produce_start = crate::sdl::ticks_ns();
        let s = &mut self.session;
        let now = s.wall_now() + s.offset_ns;
        s.inputs.time = clock_time(now, s.zone_word());
        s.inputs.items = clock_items(now, s.zone_word());
        Log::get().set_frame(self.logic_frames);
        let mut sent = String::new();
        if let Some(audio) = &mut s.audio {
            for c in audio.drain() {
                sent += &format!("{}{:x}({},{},{})", if sent.is_empty() { "" } else { "," }, c.id, c.a1, c.a2, c.a3);
            }
        }
        if s.set_up_in > 0 {
            s.set_up_in -= 1;
            if s.set_up_in == 0 {
                let forced = s.boot_clock_forced;
                s.clock_sets_up(forced);
            }
        }
        if s.thread_start_in > 0 {
            s.thread_start_in -= 1;
            if s.thread_start_in == 0 {
                s.clock_thread_starts();
            }
        }
        if let Some(chain) = &mut self.chain {
            let in_opening = chain.phase() == BootPhase::Opening;
            if chain.phase() == BootPhase::Clock {
                s.boot_rand = chain.opening().rand_state();
            }
            chain.step(s);
            if let Some(error) = s.error.take() {
                return Err(error);
            }
            self.frame = std::mem::take(chain.frame());
            if chain.phase() != self.last_phase {
                let opening = chain.opening();
                let hand_off = match opening.hand_off() {
                    Some(h) => format!("module {} execute type {}", h.module, h.execute_app_type),
                    None => "none".to_owned(),
                };
                core_log::log!(
                    Info,
                    Boot,
                    "phase {} -> {}, opening counter {}, hand-off {}",
                    phase_name(self.last_phase),
                    chain.name(),
                    opening.counter(),
                    hand_off
                );
                if chain.phase() != BootPhase::Opening {
                    s.thread_start_in = THREAD_START_LAG_FRAMES;
                }
                self.last_phase = chain.phase();
            }
            if in_opening {
                let commands = chain.opening().commands().to_vec();
                s.queue_sounds(&commands);
            }
        } else {
            self.frame = s.step_screen();
        }
        if let Some(audio) = &mut s.audio {
            audio.set_muted(self.panel.mute);
            audio.set_volume(self.panel.volume);
            audio.step();
        }
        self.logic_frames += 1;
        let in_clock = self.chain.as_ref().is_none_or(|c| c.phase() == BootPhase::Clock);
        if !self.empty_warned && self.frame.passes.is_empty() && in_clock {
            core_log::log!(Warn, Clock, "the clock produced a frame with no passes");
            self.empty_warned = true;
        }
        let produce_ms = (crate::sdl::ticks_ns() - produce_start) as f64 * 1e-6;
        if let Some(trace) = &mut self.trace {
            let s = &self.session;
            let vertices: usize = self.frame.passes.iter().map(|p| p.vertices.len()).sum();
            let counter = match (&s.clock, self.chain.as_ref().map(|c| c.opening())) {
                (Some(clock), _) if in_clock => clock.state().counter,
                (_, Some(opening)) => opening.counter(),
                _ => 0,
            };
            let clock = s.clock.as_ref().filter(|_| in_clock);
            let line = format!(
                r#"{{"frame":{},"phase":"{}","counter":{},"screen":"{}","mode":{},"ramp":{},"sent":"{}","queued":{},"passes":{},"vertices":{},"produceMs":{:.3},"error":"{}"}}"#,
                self.logic_frames,
                self.chain.as_ref().map_or("clock", |c| c.name()),
                counter,
                if in_clock { s.visited.last().map_or("", |v| v.as_str()) } else { "" },
                clock.map_or(-1, |c| c.state().mode),
                clock.map_or(-1, |c| c.state().menu_ramp.state),
                sent,
                s.audio.as_ref().map_or(-1, |a| a.queued_frames()),
                self.frame.passes.len(),
                vertices,
                produce_ms,
                if Log::get().last_error().is_empty() { "" } else { "yes" }
            );
            let _ = writeln!(trace, "{line}");
        }
        self.fresh = true;
        if self.profiling {
            let key = self.screen_key();
            self.profile.add(&key, Stage::Produce, produce_ms);
        }
        let s = &mut self.session;
        if s.fixed_now.is_some() {
            s.golden_ticks += 1;
            s.fixed_now = Some(s.golden_start + (s.golden_ticks as f64 * 1001.0 / 60000.0 * 1e9) as i64);
        }
        Ok(())
    }

    pub fn sound_hash(&self) -> u64 {
        match &self.session.audio {
            Some(audio) => hash_bytes(&audio.last_frame_bytes(), assets::bytes::FNV_OFFSET),
            None => 0,
        }
    }
}
