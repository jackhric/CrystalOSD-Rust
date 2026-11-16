use scene::arithmetic::NativeArithmetic;
use scene::frame::Frame;
use scene::opening::{BootOptions, HandOff, Opening, SoundEvent};
use scene::program_image::ProgramImage;
use scene::sound::SoundCommand;

use crate::screen::FrameSource;

pub struct OpeningScreen {
    opening: Opening<NativeArithmetic>,
    frame: Frame,
    display: i32,
}

impl OpeningScreen {
    pub fn new(options: &BootOptions, program: &ProgramImage) -> Self {
        OpeningScreen { opening: Opening::new(options, program), frame: Frame::default(), display: 0 }
    }

    pub fn counter(&self) -> i32 {
        self.opening.counter()
    }

    pub fn stage(&self) -> i32 {
        self.opening.timeline().state().stage
    }

    pub fn camera_z(&self) -> f32 {
        self.opening.timeline().state().camera[2]
    }

    pub fn hand_off(&self) -> Option<&HandOff> {
        self.opening.hand_off()
    }

    pub fn rand_state(&self) -> u32 {
        self.opening.rand_state()
    }

    pub fn sounds(&self) -> &[SoundEvent] {
        self.opening.sounds()
    }

    pub fn commands(&self) -> &[SoundCommand] {
        self.opening.commands()
    }

    pub fn start_commands(&self) -> Vec<SoundCommand> {
        self.opening.start_commands()
    }
}

impl<C> FrameSource<C> for OpeningScreen {
    fn step(&mut self, _: &mut C) {
        self.frame = self.opening.frame(self.display, 0);
        self.display ^= 1;
    }

    fn frame(&mut self) -> &mut Frame {
        &mut self.frame
    }

    fn done(&self) -> bool {
        self.opening.ended()
    }
}
