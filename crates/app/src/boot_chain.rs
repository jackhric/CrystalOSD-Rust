use scene::arithmetic::NativeArithmetic;
use scene::frame::{Frame, TextureSet};
use scene::opening::Flat;

use crate::screen::FrameSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    Opening,
    Gap,
    Clock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Active {
    Own,
    Opening,
    Clock,
}

pub struct BootChain<O, K> {
    opening: O,
    clock: K,
    gap: i32,
    gap_left: i32,
    display: i32,
    phase: BootPhase,
    frame: Frame,
    active: Active,
}

impl<O, K> BootChain<O, K> {
    pub fn new(opening: O, clock: K, gap_frames: i32) -> Self {
        BootChain { opening, clock, gap: gap_frames, gap_left: gap_frames, display: 0, phase: BootPhase::Opening, frame: Frame::default(), active: Active::Own }
    }

    pub fn phase(&self) -> BootPhase {
        self.phase
    }

    pub fn name(&self) -> &'static str {
        match self.phase {
            BootPhase::Opening => "opening",
            BootPhase::Gap => "black gap",
            BootPhase::Clock => "clock",
        }
    }

    pub fn opening(&self) -> &O {
        &self.opening
    }

    pub fn opening_mut(&mut self) -> &mut O {
        &mut self.opening
    }

    pub fn clock(&mut self) -> &mut K {
        &mut self.clock
    }
}

impl<C, O: FrameSource<C>, K: FrameSource<C>> FrameSource<C> for BootChain<O, K> {
    fn step(&mut self, context: &mut C) {
        if self.phase == BootPhase::Opening {
            self.opening.step(context);
            self.active = Active::Opening;
            if self.opening.done() {
                self.phase = if self.gap > 0 { BootPhase::Gap } else { BootPhase::Clock };
            }
            return;
        }
        if self.phase == BootPhase::Gap {
            self.active = Active::Own;
            self.frame = Frame { display_index: self.display, texture_set: TextureSet::Opening, depth_bits: 24, ..Default::default() };
            Flat::<NativeArithmetic>::scissor_default(&mut self.frame.passes);
            self.display ^= 1;
            self.gap_left -= 1;
            if self.gap_left == 0 {
                self.phase = BootPhase::Clock;
            }
            return;
        }
        self.clock.step(context);
        self.active = Active::Clock;
    }

    fn frame(&mut self) -> &mut Frame {
        match self.active {
            Active::Own => &mut self.frame,
            Active::Opening => self.opening.frame(),
            Active::Clock => self.clock.frame(),
        }
    }

    fn done(&self) -> bool {
        false
    }
}
