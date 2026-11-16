use crate::arithmetic::Arithmetic;
use crate::clock_state::ClockTime;
use crate::clock_state::{
    ClockColours, ClockLogic, ClockState, Colour, CycleCounter, CycleCounters, CycleTables, EasedAngles, LogicConstants, RodState, RodsState,
};
use crate::cold_ramps::ColdLengths;
use crate::orbs::{ORB_COUNT, OrbRing};
use crate::program_image::ProgramImage;
use crate::ramp::Ramp;

const STATE: u32 = 0x404F70;
const EASED: u32 = 0x370A98;
const APPEARANCE: u32 = 0x2B5640;
const COLOURS: u32 = 0x2B5600;
const COUNTERS: u32 = 0x370268;
const TABLES: u32 = 0x2B5570;
const CONSTANTS: u32 = 0x36FBCC;
const COLOUR_OTHERS: u32 = 0x2B5650;
const COLOUR_CURRENT: u32 = 0x2B5660;
const ROD_STRIDE: u32 = 0x30;
const ACCENT: u32 = 0x250;
const FOURTH: u32 = 0x260;
const PROGRESS_TARGET: u32 = 0x270;
const ACCENT_FRAME: u32 = 0x280;
const CURRENT_TARGET: u32 = 0x290;
const RODS: i32 = 12;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ColdStateOut {
    pub rods: RodsState,
    pub eased: EasedAngles,
    pub colours: ClockColours,
    pub cycle: CycleCounters,
    pub cycle_tables: CycleTables,
    pub logic_constants: LogicConstants,
    pub appearance: Ramp,
    pub orb_random: [i32; ORB_COUNT],
    pub rand_state: u32,
    pub rings: [OrbRing; ORB_COUNT],
    pub sprite_fade: Ramp,
}

fn colour_at(program: &ProgramImage, address: u32) -> Colour {
    [program.integer(address), program.integer(address + 4), program.integer(address + 8), program.integer(address + 12)]
}

pub fn advance_rand(mut state: u32, calls: u32) -> u32 {
    for _ in 0..calls {
        state = state.wrapping_mul(0x41C64E6D).wrapping_add(12345); // HDD OSD 1.10U rand 0x29C6E8 facts/clock-rand-chain.md
    }
    state
}

pub fn rand_angles(state: &mut u32) -> [i32; ORB_COUNT] {
    let mut out = [0i32; ORB_COUNT];
    for angle in out.iter_mut() {
        *state = state.wrapping_mul(0x41C64E6D).wrapping_add(12345); // HDD OSD 1.10U rand 0x29C6E8 verify_cold_state.mjs
        *angle = ((*state & 0x7FFFFFFF) % 65536) as i32;
    }
    out
}

// HDD OSD 1.10U 0x22EF40, 0x22E9C8, 0x22EF20 verify_cold_state.mjs
pub fn cold_state<A: Arithmetic>(program: &ProgramImage, lengths: &ColdLengths, time: &ClockTime, rand_state: u32) -> ColdStateOut {
    let mut clock = ClockState { time: *time, ..Default::default() };
    clock.logic_constants =
        LogicConstants { rod_easing: program.single(CONSTANTS), seconds_easing: program.single(CONSTANTS + 4), progress_step: program.single(CONSTANTS + 8) };
    for i in 0..6u32 {
        clock.cycle_tables.base[i as usize] = colour_at(program, TABLES + 16 * i);
    }
    for i in 0..3u32 {
        clock.cycle_tables.accent[i as usize] = colour_at(program, TABLES + 0x60 + 16 * i);
    }
    clock.colours = ClockColours {
        base: colour_at(program, COLOURS),
        reflection: colour_at(program, COLOURS + 0x10),
        fourth: colour_at(program, COLOURS + 0x20),
        current_reflection: colour_at(program, COLOURS + 0x30),
    };
    clock.cycle_counters = CycleCounters {
        base: CycleCounter { index: program.integer(COUNTERS), wait: program.integer(COUNTERS + 4) },
        accent: CycleCounter { index: program.integer(COUNTERS + 8), wait: program.integer(COUNTERS + 12) },
        last: program.integer(COUNTERS + 16),
    };
    clock.eased.progress = program.single(EASED + 4);
    clock.eased.fraction = program.single(EASED + 8);
    for i in 0..RODS {
        let at = STATE + 0x10 + ROD_STRIDE * i as u32;
        clock.state.rods[i as usize] = RodState {
            appearance: program.single(at),
            progress: program.single(at + 4),
            base: colour_at(program, at + 0x10),
            reflection: colour_at(program, at + 0x20),
        };
    }
    clock.state.accent = colour_at(program, STATE + ACCENT);
    clock.state.fourth = colour_at(program, STATE + FOURTH);
    clock.state.progress_target = program.single(STATE + PROGRESS_TARGET);
    clock.state.accent_frame = colour_at(program, STATE + ACCENT_FRAME);
    clock.state.current_target = colour_at(program, STATE + CURRENT_TARGET);
    clock.appearance = Ramp {
        length: lengths.one,
        counter: program.integer(APPEARANCE + 4),
        changed: program.integer(APPEARANCE + 8),
        state: program.integer(APPEARANCE + 12),
    };

    clock.state.accent = colour_at(program, COLOUR_CURRENT);
    clock.state.fourth = colour_at(program, COLOUR_CURRENT);
    let hours = ClockLogic::<A>::hours(time);
    let seconds = ClockLogic::<A>::seconds(time);
    let current = A::to_int(hours) % RODS;
    let turn = ((current as u32) << 16) / RODS as u32;
    let seconds_turn = A::to_int(A::div(A::mul(seconds, 65536.0), 60.0));
    clock.state.current_rod = current;
    clock.state.rod_angle = turn as i16;
    clock.state.seconds_angle = seconds_turn as i16;
    clock.eased.second_hand = seconds_turn as i16;
    clock.eased.hour_hand = A::to_int(A::div(A::mul(hours, 65536.0), 12.0)) as i16;

    ClockLogic::<A>::angles(&mut clock);
    ClockLogic::<A>::colours(&mut clock);
    for i in 0..RODS {
        let rod = &mut clock.state.rods[i as usize];
        rod.appearance = 0.0;
        rod.progress = 0.0;
        rod.base = colour_at(program, if i == current { COLOUR_CURRENT } else { COLOUR_OTHERS });
    }

    let mut out = ColdStateOut {
        rods: clock.state,
        eased: clock.eased,
        colours: clock.colours,
        cycle: clock.cycle_counters,
        cycle_tables: clock.cycle_tables,
        logic_constants: clock.logic_constants,
        appearance: clock.appearance,
        rand_state,
        sprite_fade: lengths.sprite_fade,
        ..Default::default()
    };
    out.orb_random = rand_angles(&mut out.rand_state);
    out
}
