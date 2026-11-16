use scene::arithmetic::NativeArithmetic;
use scene::clock::{Clock, ClockInputs, FrameInputs};
use scene::clock_state::{ClockState, ClockTime};
use scene::cold_start::fill_time;
use scene::frame::Frame;
use scene::text::ClockItems;

use crate::native_frames::{first_frame, next_frame};
use crate::screen::FrameSource;

pub struct ClockScreen {
    clock: Clock<NativeArithmetic>,
    inputs: FrameInputs,
    frame: Frame,
}

impl ClockScreen {
    pub fn new(inputs: &ClockInputs, frame: &FrameInputs) -> scene::Result<Self> {
        let mut screen = ClockScreen { clock: Clock::new(inputs)?, inputs: *frame, frame: Frame::default() };
        first_frame(&mut screen.inputs);
        screen.inputs.thread_step = false;
        Ok(screen)
    }

    pub fn set_time(&mut self, time: ClockTime, items: ClockItems) {
        self.inputs.time = time;
        self.inputs.items = items;
    }

    pub fn state(&self) -> &ClockState {
        self.clock.state()
    }
}

impl<C> FrameSource<C> for ClockScreen {
    fn step(&mut self, _: &mut C) {
        self.frame = self.clock.frame(&self.inputs);
        fill_time(&mut self.inputs);
        self.inputs.thread_step = true;
        next_frame(&mut self.inputs);
    }

    fn frame(&mut self) -> &mut Frame {
        &mut self.frame
    }

    fn done(&self) -> bool {
        false
    }
}
