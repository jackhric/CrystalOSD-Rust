use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, as_float};
use crate::matrix::{Mat4, Vec4};
use crate::opening::types::{BootOptions, SoundEvent};
use crate::opening::vu0::Vu0;

// facts/opening.md section 3: block B at 0x003DB800, in the order of its offsets.
pub type Block = [f32; 15];

pub mod block_at {
    pub const AX: usize = 0;
    pub const AY: usize = 1;
    pub const AZ: usize = 2;
    pub const VX: usize = 3;
    pub const VY: usize = 4;
    pub const VZ: usize = 5;
    pub const PX: usize = 6;
    pub const PY: usize = 7;
    pub const PZ: usize = 8;
    pub const WX: usize = 9;
    pub const WY: usize = 10;
    pub const ROLL_ACC: usize = 11;
    pub const QX: usize = 12;
    pub const QY: usize = 13;
    pub const ROLL_VEL: usize = 14;
}

pub mod timeline_constant {
    pub const HDD_ROLL_VELOCITY: f32 = 0.0004;
    pub const HDD_WAIT_VZ: f32 = -0.00014;
    pub const HDD_LATE_VZ: f32 = 2.5e-5;
    pub const HDD_EXEC_VZ: f32 = 0.003;
    pub const STAGE_ONE_AZ: f32 = 4e-7;
    pub const HDD_AZ: f32 = 4e-4;
    pub const HDD_ROLL_ACC: f32 = 8e-5;
    pub const PLAIN_VZ: f32 = 0.0099;
    pub const PLAIN_ROLL_ACC: f32 = 0.000195;
    pub const PI: f32 = std::f32::consts::PI;
    pub const TWO_PI: f32 = std::f32::consts::TAU;
}

// verify_opening_camera_v2.mjs read(probe), verify_opening3_stages.mjs read(probe), the logo of func_0021DB50.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineState {
    pub counter: i32,
    pub stage: i32,
    pub block: Block,
    pub camera: Vec4,
    pub direction: Vec4,
    pub up: Vec4,
    pub roll: f32,
    pub go: bool,
    pub pending: i32,
    pub scene: i32,
    pub snapshot: i32,
    pub lights: [Vec4; 3],
    pub logo_state: i32,
    pub logo_value: i32,
    pub logo_step: i32,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            counter: 1,
            stage: 0,
            block: [0.0; 15],
            camera: [0.0; 4],
            direction: [0.0; 4],
            up: [0.0; 4],
            roll: 0.0,
            go: false,
            pending: 1,
            scene: 0,
            snapshot: 0,
            lights: [[0.0; 4]; 3],
            logo_state: 0,
            logo_value: 0,
            logo_step: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Matrices {
    pub camera: Mat4,
    pub view_screen: Mat4,
    pub world_to_screen: Mat4,
    pub normal_light: Mat4,
}

// result: 0 while the intro runs, 1 (scene + 1) on the call that ends it. fadeAlpha, blurLevel, logoAlpha: the arguments of the
// draw functions of the frame, -1 (blurLevel 0) when the frame does not call them.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineStep {
    pub result: i32,
    pub matrices: Matrices,
    pub fade_alpha: i32,
    pub blur_level: i32,
    pub logo_alpha: i32,
    pub logo_value: i32,
    pub sounds: Vec<SoundEvent>,
}

impl Default for TimelineStep {
    fn default() -> Self {
        Self { result: 0, matrices: Matrices::default(), fade_alpha: -1, blur_level: 0, logo_alpha: -1, logo_value: 0, sounds: Vec::new() }
    }
}

const FPS: i32 = 60;

#[derive(Debug, Clone)]
pub struct Timeline<A> {
    options: BootOptions,
    state: TimelineState,
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> Timeline<A> {
    pub fn new(options: &BootOptions, initial: &TimelineState) -> Self {
        Self { options: *options, state: *initial, arithmetic: PhantomData }
    }

    // The state of the module's first call: OpeningInitAnimation 0x0021EE48 (step z 0.04, roll velocity 0.001), nothing placed yet.
    pub fn initial_state(counter: i32) -> TimelineState {
        let mut s = TimelineState { counter, ..Default::default() };
        s.block[block_at::PZ] = 0.04;
        s.block[block_at::ROLL_VEL] = 0.001;
        s
    }

    // OpeningInitOpeningScene 0x00221BB8, run between the first and the second call.
    pub fn scene_set_up(&mut self) {
        self.state.camera = [0.0, 0.0, 16.0, 0.0];
        self.state.direction = [0.0, 0.0, 1.0, 0.0];
        self.state.up = [0.0, 1.0, 0.0, 1.0];
        self.state.roll = -0.12;
        self.state.lights = [[0.0, 0.0, -1.0, 0.0], [0.5, 0.5, 0.0, 0.0], [-0.5, -0.5, 0.0, 0.0]];
    }

    pub fn state(&self) -> &TimelineState {
        &self.state
    }

    // OpeningProcessInner 0x0021EF00 once, then the matrices; the frame counter moves on afterwards.
    pub fn step(&mut self, disc_state: u32) -> TimelineStep {
        use block_at::*;
        let options = &mut self.options;
        let s = &mut self.state;
        let mut out = TimelineStep { result: s.scene, ..Default::default() };

        // facts/opening.md 3 stage selection (0x0021EF4C): threshold[stage] < camera.z
        const THRESHOLDS: [i32; 8] = [16, 56, 104, 320, 672, 800, 1160, 1160];
        if s.stage < 8 && (THRESHOLDS[s.stage as usize] as f32) < s.camera[2] {
            s.stage += 1;
        }

        // facts/opening.md 3 stages 1 to 3 (verify_opening3_stages.mjs stages)
        if s.stage == 1 {
            if options.hdd_ready {
                s.block[ROLL_VEL] = timeline_constant::HDD_ROLL_VELOCITY;
                s.block[VZ] = if s.counter < FPS * 20 / 6 { timeline_constant::HDD_WAIT_VZ } else { timeline_constant::HDD_LATE_VZ };
                if options.hdd_exec != 0 {
                    s.block[VZ] = timeline_constant::HDD_EXEC_VZ;
                    s.go = true;
                    s.stage += 1;
                } else if FPS * 20 < s.counter {
                    options.hdd_ready = false;
                }
            } else {
                s.block[AZ] = timeline_constant::STAGE_ONE_AZ;
                let index = disc_state.wrapping_sub(0x64);
                let releases = index < 17 && ((0x1DFC1u32 >> index) & 1) != 0;
                if releases && 2 * FPS < s.counter {
                    s.go = true;
                    s.stage += 1;
                }
            }
        } else if s.stage == 2 {
            if FPS * 10 < s.counter {
                s.go = true;
            }
            if s.go {
                if s.pending == 1 {
                    let counter = s.counter;
                    let mut queue = |id: u32, argument: i32| out.sounds.push(SoundEvent { id, argument, counter });
                    if options.clock_forced {
                        queue(0x6140, 1);
                    } else if options.hdd_ready && options.hdd_exec == 1 {
                        queue(0x6150, 0xF);
                    } else {
                        s.snapshot = disc_state as i32;
                        let index = disc_state.wrapping_sub(0x6A);
                        if index == 0 || index == 1 || index == 8 || index == 9 {
                            queue(0x6150, 0xF);
                        } else if (2..=4).contains(&index) {
                            queue(0x6140, 7);
                            queue(0x6150, 0x11);
                        } else {
                            queue(0x6140, 1);
                        }
                    }
                    s.pending = -1;
                }
                if options.hdd_ready || options.hdd_exec != 0 {
                    s.block[AZ] = timeline_constant::HDD_AZ;
                    s.block[ROLL_ACC] = timeline_constant::HDD_ROLL_ACC;
                } else {
                    s.block[VZ] = timeline_constant::PLAIN_VZ;
                    s.block[ROLL_ACC] = timeline_constant::PLAIN_ROLL_ACC;
                }
                s.block[VX] = 0.0;
                s.block[VY] = 0.0;
                s.block[WX] = 0.0;
                s.block[WY] = 0.0;
            }
        } else if s.stage == 3 {
            out.result = s.scene + 1;
        }

        // facts/opening.md 3 integrator (0x0021F37C..0x0021F508, verify_opening_camera_v2.mjs integrate), NTSC k = 1
        let k = 1.0f32;
        let advance = |twice: f32, add: f32| A::mul(A::mul(A::add_exact(A::add_exact(twice, twice), add), 0.5), k);
        let input = s.block;
        let b = &mut s.block;
        b[ROLL_VEL] = A::add_exact(input[ROLL_VEL], advance(input[ROLL_ACC], 0.0));
        b[PX] = A::add_exact(input[PX], advance(input[VX], input[AX]));
        b[PY] = A::add_exact(input[PY], advance(input[VY], input[AY]));
        b[PZ] = A::add_exact(input[PZ], advance(input[VZ], input[AZ]));
        b[VZ] = A::add_exact(input[VZ], A::mul(input[AZ], k));
        let mut roll = A::add_exact(s.roll, advance(b[ROLL_VEL], input[ROLL_ACC]));
        b[QX] = A::add_exact(input[QX], advance(input[WX], 0.0));
        b[QY] = A::add_exact(input[QY], advance(input[WY], 0.0));
        s.camera[0] = A::add_exact(s.camera[0], advance(b[PX], input[VX]));
        s.camera[1] = A::add_exact(s.camera[1], advance(b[PY], input[VY]));
        s.camera[2] = A::add_exact(s.camera[2], advance(b[PZ], b[VZ]));
        const PI: f32 = timeline_constant::PI;
        const TWO_PI: f32 = timeline_constant::TWO_PI;
        if PI < roll {
            roll = A::sub_exact(roll, TWO_PI);
        }
        if roll < -PI {
            roll = A::add_exact(roll, TWO_PI);
        }
        s.roll = roll;
        s.up[0] = A::sinf(roll);
        s.up[1] = A::cosf(roll);

        // facts/opening.md 3 matrices (0x0021F528..0x0021F5C0, verify_opening_inputs.mjs): ay is D_0036F984, z range 1 .. D_0036FA08
        let m = &mut out.matrices;
        m.normal_light = Vu0::<A>::normal_light_matrix(&s.lights[0], &s.lights[1], &s.lights[2]);
        m.camera = Vu0::<A>::camera_matrix(&s.camera, &s.direction, &s.up);
        m.view_screen = Vu0::<A>::view_screen_matrix(1024.0, 1.0, as_float(0x3eea4e16), 2048.0, 2048.0, 1.0, as_float(0x4b7fffff), 1.0, 65536.0);
        m.world_to_screen = Vu0::<A>::mul_matrix(&m.view_screen, &m.camera);

        if out.result == s.scene {
            // func_00221B00 fade, func_00221A50 blur level, func_0021DB50 logo (verify_opening_camera_v2.mjs): the camera after the integrator
            let z = s.camera[2];
            if s.counter < 2 {
                out.fade_alpha = 0x80;
            }
            if 72.0 < z {
                out.fade_alpha = A::to_int(A::mul(A::mul(A::sub_exact(z, 72.0), 128.0), 0.03125));
            }
            if 320.0 <= z {
                out.fade_alpha = 0x80;
            }
            if 56.0 < z {
                let mut level = A::to_int(A::quotient_exact(A::sub_exact(z, 56.0), 12.0));
                if level >= 4 {
                    level = 3;
                }
                if level < 0 {
                    level = 0;
                }
                out.blur_level = level;
            }
            if s.logo_state == 1 || (s.logo_state == 0 && 18.0 < z) {
                s.logo_state = 1;
                s.logo_value = s.logo_value.wrapping_add(s.logo_step);
                out.logo_alpha = if s.logo_value < 0x70 { s.logo_value } else { 0x70 };
                if s.logo_value >= 0xF0 {
                    s.logo_step = -4;
                }
                if s.logo_value <= 0 {
                    s.logo_state = -1;
                    s.logo_step = 4;
                }
            }
            out.logo_value = s.logo_value;
        }
        s.counter = s.counter.wrapping_add(1);
        out
    }
}
