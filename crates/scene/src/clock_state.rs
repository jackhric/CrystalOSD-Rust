use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, s16, u16};
use crate::matrix::Vec4;
use crate::ramp::{Ramp, tick_ramp};

pub type Colour = [i32; 4];

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClockTime {
    pub milliseconds: f32,
    pub seconds: i32,
    pub minutes: i32,
    pub hours: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EasedAngles {
    pub second_hand: i16,
    pub hour_hand: i16,
    pub progress: f32,
    pub fraction: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RodState {
    pub appearance: f32,
    pub progress: f32,
    pub base: Colour,
    pub reflection: Colour,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RodsState {
    pub current_rod: i32,
    pub seconds_angle: i16,
    pub rod_angle: i16,
    pub rods: [RodState; 12],
    pub accent: Colour,
    pub fourth: Colour,
    pub progress_target: f32,
    pub accent_frame: Colour,
    pub current_target: Colour,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClockColours {
    pub base: Colour,
    pub reflection: Colour,
    pub fourth: Colour,
    pub current_reflection: Colour,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CycleCounter {
    pub index: i32,
    pub wait: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CycleCounters {
    pub base: CycleCounter,
    pub accent: CycleCounter,
    pub last: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CycleTables {
    pub base: [Colour; 6],
    pub accent: [Colour; 3],
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LogicConstants {
    pub rod_easing: f32,
    pub seconds_easing: f32,
    pub progress_step: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SceneScale {
    pub scale: f32,
    pub leaving: i32,
    pub field: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Proportions {
    pub ax: f32,
    pub ay: f32,
}

// The named state the clock's logic and camera read and write (facts/clock-state.md, facts/clock-camera.md).
// time, scene.field and eased.fraction come from outside the logic: the time keeper, the field flag
// and the orbs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClockState {
    pub time: ClockTime,
    pub eased: EasedAngles,
    pub state: RodsState,
    pub appearance: Ramp,
    pub colours: ClockColours,
    pub cycle_counters: CycleCounters,
    pub cycle_tables: CycleTables,
    pub logic_constants: LogicConstants,

    pub scene: SceneScale,
    pub scale_target: f32,
    pub scale_factor: f32,
    pub time_filled: i32,
    pub spin: Option<u16>,

    pub mode: i32,
    pub level: i32,
    pub overlay_level: i32,
    pub vignette_ramp: Ramp,
    pub vignette_length: i32,
    pub menu_ramp: Ramp,
    pub tail: i32,
    pub counter: i32,

    pub proportions: Proportions,
    pub position: Vec4,
    pub direction: Vec4,
    pub up: Vec4,
    pub rotation: Vec4,
    pub camera_offset: f32,
    pub zmax: f32,
    pub camera_factor: f32,
}

fn seconds<A: Arithmetic>(time: &ClockTime) -> f32 {
    A::add(A::from_int(time.seconds), A::div(time.milliseconds, 1000.0))
}

fn minutes<A: Arithmetic>(time: &ClockTime) -> f32 {
    A::add(A::from_int(time.minutes), A::div(seconds::<A>(time), 60.0))
}

fn hours<A: Arithmetic>(time: &ClockTime) -> f32 {
    A::add(A::from_int(time.hours), A::div(minutes::<A>(time), 60.0))
}

// One step of an eased 16-bit angle: the difference, wrapped to 16 bits, times a factor.
fn ease<A: Arithmetic>(target: i64, old: i32, factor: f32) -> i32 {
    let wrapped = i32::from(target.wrapping_sub(i64::from(old)) as u16 as i16);
    u16(A::to_int(A::add(A::mul(A::from_int(wrapped), factor), A::from_int(s16(old)))))
}

// facts/clock-state.md: HDD func_0022EBD0, the time becomes the current rod, the eased angles and the progress target.
fn time_to_angles<A: Arithmetic>(clock: &mut ClockState) {
    let time = clock.time;
    let k1 = clock.logic_constants.rod_easing;
    let k2 = clock.logic_constants.seconds_easing;
    let current = A::to_int(hours::<A>(&time)) % 12;
    let turn = (f64::from((current as u32) << 16) / 12.0) as u32;
    let old_seconds = i32::from(clock.state.seconds_angle);
    let old_rod = i32::from(clock.state.rod_angle);
    let snap = s16(old_seconds).abs() < 201 || s16(old_seconds) == -32768;
    let seconds_turn = A::to_int(A::div(A::mul(seconds::<A>(&time), 65536.0), 60.0));
    let hour_old = i32::from(clock.eased.hour_hand);
    let hour_step = s16(A::to_int(A::sub(A::div(A::mul(hours::<A>(&time), 65536.0), 12.0), A::from_int(hour_old))));
    let progress = A::sub(1.0, A::div(minutes::<A>(&time), 60.0));
    clock.state.current_rod = current;
    clock.state.rod_angle = (if snap { u16(turn as i32) } else { ease::<A>(i64::from(turn), old_rod, k1) }) as i16;
    clock.state.seconds_angle = ease::<A>(i64::from(seconds_turn), old_seconds, k2) as i16;
    let second_hand = ease::<A>(i64::from(seconds_turn), i32::from(clock.eased.second_hand as u16), k2);
    clock.eased.hour_hand = u16(A::to_int(A::add(A::mul(A::from_int(hour_step), k2), A::from_int(hour_old)))) as i16;
    clock.eased.second_hand = second_hand as i16;
    clock.state.progress_target = progress;
    clock.eased.progress = progress;
}

// Move four ints one unit each toward a target; true when nothing had to move.
fn chase(colour: &mut Colour, target: &Colour) -> bool {
    let mut moved = false;
    for c in 0..4 {
        if colour[c] == target[c] {
            continue;
        }
        colour[c] += if colour[c] < target[c] { 1 } else { -1 };
        moved = true;
    }
    !moved
}

// A colour that walks through a table, one unit every ninth call (HDD func_0022EAD0, func_0022EB50).
fn cycle(colour: &mut Colour, counter: &mut CycleCounter, table: &[Colour]) {
    if (counter.wait as u32) < 8 {
        counter.wait += 1;
        return;
    }
    let mut index = counter.index;
    if chase(colour, &table[index as usize]) {
        index += 1;
        if table[index as usize][3] == -1 {
            index = 0;
        }
    }
    counter.index = index;
    counter.wait = 0;
}

// facts/clock-state.md: HDD func_0022EE20 and func_0022E8C0, the two cycling colours, their mix, and every chaser.
fn cycle_colours(clock: &mut ClockState) {
    let state = &mut clock.state;
    let base = &mut clock.colours;
    cycle(&mut base.base, &mut clock.cycle_counters.base, &clock.cycle_tables.base);
    state.current_target = base.base;
    state.accent_frame = [0xa7, 0xd9, 0xff, 0];
    cycle(&mut state.accent_frame, &mut clock.cycle_counters.accent, &clock.cycle_tables.accent);
    for c in 0..3 {
        state.current_target[c] = (state.accent_frame[c] + base.base[c]) >> 1;
    }
    clock.cycle_counters.last = 3;

    for i in 0..12 {
        let current = i as i32 == state.current_rod;
        let target = if current { state.current_target } else { base.base };
        chase(&mut state.rods[i].base, &target);
        chase(&mut state.rods[i].reflection, if current { &base.current_reflection } else { &base.reflection });
    }
    let accent_frame = state.accent_frame;
    chase(&mut state.accent, &accent_frame);
    chase(&mut state.fourth, &base.fourth);
}

// Per-frame state the frame touches outside step(), in the model's order (clock_frame.mjs frame):
//   1. menuStep (clock_rest.mjs, menus) ticks menuRamp; blurLevel reads menuRamp.counter, so it runs before step.
//   2. Camera::matrices, then the head: the background draw ticks greyRamp (clock_rest.mjs background()).
//   3. The overlay draw ticks vignetteRamp (tickRamp) before step; overlayStep reads its state.
//   4. step().
// between/endOfFrame belong to the menus and are not part of the clock screen.
// spin (the cubes' angle) is present only when the capture has it; step() advances it by 0x1e.
// What the frame function does to the state after everything is drawn: logic, scale, blurLevel,
// the frame counter, overlayStep (HDD build).
pub struct ClockLogic<A>(PhantomData<A>);

impl<A: Arithmetic> ClockLogic<A> {
    pub fn step(clock: &mut ClockState) {
        Self::logic(clock);
        Self::scale(clock);
        Self::blur_level(clock);
        clock.counter += 1;
        Self::overlay_step(clock);
    }

    pub fn logic(clock: &mut ClockState) {
        tick_ramp(&mut clock.appearance);
        Self::angles(clock);
        Self::colours(clock);
        let step = clock.logic_constants.progress_step;
        let current = clock.state.current_rod;
        let quotient = f64::from(clock.appearance.counter) * 128.0 / f64::from(clock.appearance.length);
        let appearance = A::cut(quotient.trunc() * 0.0078125);
        for i in 0..12 {
            let mut progress = 0.0;
            if i == current {
                let target = clock.state.progress_target;
                let old = clock.state.rods[i as usize].progress;
                if old < target {
                    progress = A::add(old, step);
                    if target < progress {
                        progress = target;
                    }
                } else {
                    progress = target;
                }
            }
            clock.state.rods[i as usize].progress = progress;
            clock.state.rods[i as usize].appearance = appearance;
        }
    }

    pub fn seconds(time: &ClockTime) -> f32 {
        seconds::<A>(time)
    }

    pub fn hours(time: &ClockTime) -> f32 {
        hours::<A>(time)
    }

    pub fn angles(clock: &mut ClockState) {
        time_to_angles::<A>(clock);
    }

    pub fn colours(clock: &mut ClockState) {
        cycle_colours(clock);
    }

    // facts/clock-state.md: first half of HDD func_00232640, the scene scale eases toward its target.
    pub fn scale(clock: &mut ClockState) {
        if let Some(spin) = clock.spin {
            clock.spin = Some(spin.wrapping_add(0x1e));
        }
        let old = clock.scene.scale;
        let filled = clock.time_filled != 0;
        clock.scene.scale = if filled { A::add(A::mul(A::sub(clock.scale_target, old), clock.scale_factor), old) } else { 0.0 };
    }

    // facts/clock-state.md: second half of HDD func_00232640, the blur level from the menu ramp.
    pub fn blur_level(clock: &mut ClockState) {
        let counter = i64::from(clock.menu_ramp.counter);
        let tail = i64::from(clock.tail);
        clock.level = if counter < tail { (10 - (counter * 10) / tail) as i32 } else { 0 };
    }

    // facts/clock-state.md: HDD func_00234EA8, the overlay level's state machine.
    pub fn overlay_step(clock: &mut ClockState) {
        let mode = clock.mode;
        if mode <= 0 {
            return;
        }
        let mut level = clock.overlay_level;
        if mode < 3 {
            level += 1;
            let mut now = mode;
            if level > 0x80 {
                level = 0x80;
                now = 0;
                clock.mode = 0;
            }
            clock.overlay_level = level;
            if now == 2 && level == 0x80 - clock.vignette_length {
                let ramp = &mut clock.vignette_ramp;
                if ramp.state == 0 {
                    ramp.counter = 0;
                    ramp.changed = 1;
                    ramp.state = 1;
                }
            }
        } else if mode == 3 {
            level -= 1;
            clock.overlay_level = if level < 0 { 0 } else { level };
        }
    }
}
