use super::registers::reg;
use super::state::{State, s16, u16};
use crate::Result;
use crate::spu_write::DriverCommand;

impl State {
    pub fn ramp(&mut self, command: &DriverCommand) -> Result<()> {
        let core = i64::from(s16(i64::from(command.words[0])));
        let a = u16(i64::from(command.words[1]));
        let b = u16(i64::from(command.words[2]));
        if command.id == 0x6120 {
            self.regs.write(reg::volume(core, reg::MVOLL)?, i64::from(a));
            self.regs.write(reg::volume(core, reg::MVOLR)?, i64::from(b));
        } else if command.id == 0x60d0 {
            let mut attr = self.get_effect_attr(core)?;
            attr.depth_l = a as u16;
            attr.depth_r = b as u16;
            attr.mode |= 0x100;
            self.set_effect_attr(&attr)?;
        }
        Ok(())
    }
}
