use super::registers::reg;
use super::state::{State, m, s16};
use crate::Result;
use crate::spu_write::DriverCommand;

const BANK_TABLE: i64 = m(0x98d4);
const BUSY_MASK: i64 = m(0x9184);
const PRIORITIES: i64 = m(0x9188);
const EFFECT_VOICES: i32 = 24;

fn effect_pitch(u: u32) -> i32 {
    let x = 44100u32.wrapping_mul(u) as i32;
    let hi = ((i64::from(x) * 0x057619f1) >> 32) as i32;
    (hi >> 10) + if x < 0 { 1 } else { 0 }
}

fn priority(i: i32) -> i64 {
    PRIORITIES + 4 * i64::from(i)
}

impl State {
    pub fn effect(&mut self, command: &DriverCommand) -> Result<()> {
        let bank_index = i64::from(s16(i64::from(command.words[0])));
        let effect = s16(i64::from(command.words[1]));
        let at = BANK_TABLE + 12 * bank_index;
        let hd = i64::from(self.mem.u32(at)?);
        let bd_base = i64::from(self.mem.u32(at + 4)?);
        let table = hd + i64::from(self.mem.u32(hd + 0x2c)?);
        if self.mem.u32(table + 4)? < effect as u32 {
            return Ok(());
        }
        let record = table + 0x20 + (i64::from(effect) << 6);

        let voice;
        {
            let mut steal = -1;
            let mut free = -1;
            let mut released = 0u32;
            let mut lowest = 0x7fffu32;
            for i in 0..EFFECT_VOICES {
                let e = u32::from(self.feed_allocation(0, i as u32)) & 0xffff;
                if ((!self.mem.u32(BUSY_MASK)?) >> i) & 1 != 0 {
                    free = i;
                }
                if e < lowest && self.mem.u32(priority(i))? == 0 {
                    lowest = e;
                    steal = i;
                }
                if e < 2 {
                    released |= 1u32 << i;
                    self.mem.write32(priority(i), 0)?;
                }
            }
            let chosen = if free == -1 {
                if steal == -1 {
                    return Ok(());
                }
                self.mem.write32(priority(steal), 0)?;
                steal
            } else {
                self.mem.write32(priority(free), 0)?;
                free
            };
            self.mem.write32(BUSY_MASK, i64::from((self.mem.u32(BUSY_MASK)? & !released) | (1u32 << chosen)))?;
            voice = i64::from(chosen);
        }

        let master = self.mem.u32(table)?;
        let left = u32::from(self.mem.u16(record)?).wrapping_mul(master) >> 7;
        let right = u32::from(self.mem.u16(record + 2)?).wrapping_mul(master) >> 7;
        let pitch = effect_pitch(u32::from(self.mem.u16(record + 4)?));
        let address = ((bd_base as u32) << 3).wrapping_add(self.mem.u32(record + 0x10)?);
        let wet = self.mem.u16(record + 0xe)? & 0x80 != 0;
        let bit = 1u32 << voice;
        self.set_param(0, voice, reg::VOLL, i64::from(left))?;
        self.set_param(0, voice, reg::VOLR, i64::from(right))?;
        self.set_param(0, voice, reg::PITCH, i64::from(pitch))?;
        self.set_addr(0, voice, address)?;
        self.set_param(0, voice, reg::ADSR1, i64::from(self.mem.u16(record + 8)?))?;
        self.set_param(0, voice, reg::ADSR2, i64::from(self.mem.u16(record + 10)?))?;
        self.set_switch(0, reg::VMIXEL0, self.get_switch(0, reg::VMIXEL0)? | if wet { bit } else { 0 })?;
        self.set_switch(0, reg::VMIXER0, self.get_switch(0, reg::VMIXER0)? | if wet { bit } else { 0 })?;
        self.set_switch(0, reg::VMIXEL0, self.get_switch(0, reg::VMIXEL0)? ^ if wet { 0 } else { bit })?;
        self.set_switch(0, reg::VMIXER0, self.get_switch(0, reg::VMIXER0)? ^ if wet { 0 } else { bit })?;
        self.set_switch(0, reg::KON0, bit)?;
        Ok(())
    }
}
