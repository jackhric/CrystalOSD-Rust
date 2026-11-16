use crate::arithmetic::Arithmetic;
use crate::cold_state::advance_rand;
use crate::frame::{Frame, TextureSet};
use crate::matrix::{Mat4, Vec4};
use crate::opening::cubes::{CubeWork, Cubes};
use crate::opening::flat::{Flat, FlatInputs, PROCESS_SCISSOR};
use crate::opening::fog::Fog;
use crate::opening::handoff::{HandOffInputs, decide};
use crate::opening::lights::Lights;
use crate::opening::timeline::{Matrices, Timeline, TimelineStep};
use crate::opening::towers::{TowerMatrices, Towers};
use crate::opening::types::{BootOptions, HandOff, History, SoundEvent};
use crate::program_image::ProgramImage;
use crate::sound::SoundCommand;

const NEAR_ENOUGH: f32 = 73.0;
const ELF_RAND_SEED: u32 = 1;
const GHOST_RAND_CALLS: u32 = 385;
const SET_UP_RAND_CALLS: u32 = GHOST_RAND_CALLS + 1;
const PHASE_MODULUS: u32 = 0x929;
const PHASE_BASE: u32 = 0xD80;
const RAND_CALLS_PER_QUAD: u32 = 4;

fn drawn_phase() -> u32 {
    let state = advance_rand(advance_rand(ELF_RAND_SEED, GHOST_RAND_CALLS), 1);
    (state & 0x7FFFFFFF) % PHASE_MODULUS + PHASE_BASE // InitLightsCubes 0x220CB0
}

fn tower_matrices(m: &Matrices) -> TowerMatrices {
    let mut identity: Mat4 = [[0.0; 4]; 4];
    for (i, row) in identity.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    TowerMatrices { base: identity, world_to_screen: m.world_to_screen, light: m.normal_light }
}

// facts/opening.md section 2: the intro as the OSD sends it. The timeline's call of a frame runs before the frame is drawn, so the
// call of the next frame has run when frame() returns: ended() and handOff() are known after the last drawn frame.
#[derive(Debug, Clone)]
pub struct Opening<A> {
    options: BootOptions,
    timeline: Timeline<A>,
    towers: Towers<A>,
    fog: Fog<A>,
    lights: Lights<A>,
    cubes: Cubes<A>,
    step: TimelineStep,
    camera: Vec4,
    counter: i32,
    ended: bool,
    rand_calls: u32,
    hand_off: HandOff,
    sounds: Vec<SoundEvent>,
    commands: Vec<SoundCommand>,
    work: Vec<CubeWork>,
}

impl<A: Arithmetic> Opening<A> {
    pub fn new(options: &BootOptions, program: &ProgramImage) -> Self {
        let mut timeline = Timeline::<A>::new(options, &Timeline::<A>::initial_state(1));
        let towers = Towers::<A>::new(&options.history.unwrap_or([Default::default(); 21] as History), program);
        let lights = Lights::<A>::new(options.lights_phase.unwrap_or(drawn_phase()));
        let cubes = Cubes::<A>::new(program);
        let mut step = timeline.step(options.disc.first);
        timeline.scene_set_up();
        let camera = timeline.state().camera;
        if !options.cold_start {
            step.matrices = Matrices::default();
        }
        Self {
            options: *options,
            timeline,
            towers,
            fog: Fog::new(),
            lights,
            cubes,
            step,
            camera,
            counter: 1,
            ended: false,
            rand_calls: SET_UP_RAND_CALLS,
            hand_off: HandOff::default(),
            sounds: Vec::new(),
            commands: Vec::new(),
            work: Vec::new(),
        }
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    pub fn rand_calls(&self) -> u32 {
        self.rand_calls
    }

    pub fn rand_state(&self) -> u32 {
        advance_rand(ELF_RAND_SEED, self.rand_calls)
    }

    // The counter of the frame frame() draws next.
    pub fn counter(&self) -> i32 {
        self.counter
    }

    pub fn sounds(&self) -> &[SoundEvent] {
        &self.sounds
    }

    pub fn commands(&self) -> &[SoundCommand] {
        &self.commands
    }

    pub fn hand_off(&self) -> Option<&HandOff> {
        if self.ended { Some(&self.hand_off) } else { None }
    }

    pub fn timeline(&self) -> &Timeline<A> {
        &self.timeline
    }

    pub fn fog(&self) -> &Fog<A> {
        &self.fog
    }

    pub fn lights(&self) -> &Lights<A> {
        &self.lights
    }

    // What the cubes drew in the last frame.
    pub fn cube_work(&self) -> &[CubeWork] {
        &self.work
    }

    fn advance(&mut self) {
        self.counter = self.timeline.state().counter;
        self.step = self.timeline.step(self.options.disc.later);
        self.camera = self.timeline.state().camera;
        if self.step.result == 0 {
            return;
        }
        self.ended = true;
        let input = HandOffInputs {
            snapshot: self.timeline.state().snapshot as u32,
            clock_forced: self.options.clock_forced,
            hdd_ready: self.options.hdd_ready,
            hdd_exec: self.options.hdd_exec,
            ..Default::default()
        };
        self.hand_off = decide(&input);
    }

    // opening_thread_set_vars_2 0x0021AE78 (jal 0x0021ABF0, before OpeningInit): module 4 queues 0x6150 then 0x6140 on the ring temporaries;
    // module 1 queues 0x6140, 0 with the registers it was entered with (measured 0 and 0).
    pub fn start_commands(&self) -> Vec<SoundCommand> {
        if self.options.module == 4 {
            return vec![
                SoundCommand { id: 0x6150, a1: 2, a2: 0, a3: 0, ..Default::default() },
                SoundCommand { id: 0x6140, a1: 6, a2: 0, a3: 0, carried_a2: true, carried_a3: true },
            ];
        }
        if self.options.module == 1 {
            return vec![SoundCommand { id: 0x6140, a1: 0, a2: 0, a3: 0, ..Default::default() }];
        }
        Vec::new()
    }

    // One module frame in the OSD's order: scissor, towers, ghost, copy, fog, lights, cubes, blur, fade, logo, bars. The towers stand
    // where the options' history puts them; lights and cubes are drawn while the camera is nearer than 73.
    pub fn frame(&mut self, display_index: i32, field: i32) -> Frame {
        if self.ended {
            panic!("the opening has ended");
        }
        let mut out = Frame { field, display_index, texture_set: TextureSet::Opening, depth_bits: 24, ..Default::default() };

        let m = &self.step.matrices;
        let counter = self.counter;
        let flat = FlatInputs {
            counter,
            display_index,
            field,
            logo_alpha: self.step.logo_alpha,
            fade_alpha: self.step.fade_alpha,
            blur_level: self.step.blur_level,
            fill_sprites: if counter < 2 && self.options.cold_start { 1 } else { 2 },
        };

        let passes = &mut out.passes;
        Flat::<A>::scissor(&flat, passes);
        let towers = passes.len();
        self.towers.draw(counter, &tower_matrices(m), &self.camera, passes, None);
        for pass in &mut passes[towers..] {
            pass.scissor = Some(PROCESS_SCISSOR);
        }
        Flat::<A>::ghost(&flat, passes);
        Flat::<A>::copy_to_store(&flat, passes);
        self.fog.draw(&m.world_to_screen, passes);
        self.work.clear();
        if self.camera[2] < NEAR_ENOUGH {
            self.lights.draw(counter, &m.world_to_screen, passes);
            self.rand_calls = self.rand_calls.wrapping_add(RAND_CALLS_PER_QUAD.wrapping_mul(self.lights.stats().quads_drawn as u32));
            self.cubes.draw(m, &self.camera, passes, Some(&mut self.work), counter & 1);
        }
        if self.step.blur_level > 0 {
            Flat::<A>::blur(&flat, passes);
        }
        if self.step.fade_alpha >= 0 {
            Flat::<A>::fade(&flat, passes);
        }
        if self.step.logo_alpha >= 0 {
            Flat::<A>::logo(&flat, passes);
        }
        Flat::<A>::bars(&flat, passes);

        self.sounds = self.step.sounds.clone();
        self.commands.clear();
        // Stage 2 handler 0x0021F0F8: a 0x6140 carries a2 = low 16 bits of D_003DB800 (0x0021EF44) and a3 as entered (measured 0x8000).
        for e in &self.sounds {
            if e.id == 0x6140 {
                self.commands.push(SoundCommand { id: 0x6140, a1: e.argument as u16, a2: 0xB800, a3: 0x8000, ..Default::default() });
            } else if e.id == 0x6150 {
                self.commands.push(SoundCommand { id: 0x6150, a1: 0, a2: 0, a3: e.argument as u16, ..Default::default() });
            }
        }
        self.advance();
        out
    }
}
