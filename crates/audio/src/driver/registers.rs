use std::collections::HashMap;

use crate::spu_write::{SpuWrite, WriteStream};

pub mod reg {
    use crate::{Result, fail};

    pub const BASE: u32 = 0x1f90_0000;

    pub const VOLL: u32 = 0;
    pub const VOLR: u32 = 1;
    pub const PITCH: u32 = 2;
    pub const ADSR1: u32 = 3;
    pub const ADSR2: u32 = 4;

    pub const PMON0: u32 = 0x180;
    pub const PMON1: u32 = 0x182;
    pub const NON0: u32 = 0x184;
    pub const NON1: u32 = 0x186;
    pub const VMIXL0: u32 = 0x188;
    pub const VMIXL1: u32 = 0x18a;
    pub const VMIXEL0: u32 = 0x18c;
    pub const VMIXEL1: u32 = 0x18e;
    pub const VMIXR0: u32 = 0x190;
    pub const VMIXR1: u32 = 0x192;
    pub const VMIXER0: u32 = 0x194;
    pub const VMIXER1: u32 = 0x196;
    pub const MMIX: u32 = 0x198;
    pub const ATTR: u32 = 0x19a;
    pub const KON0: u32 = 0x1a0;
    pub const KON1: u32 = 0x1a2;
    pub const KOFF0: u32 = 0x1a4;
    pub const KOFF1: u32 = 0x1a6;
    pub const TSAH: u32 = 0x1a8;
    pub const TSAL: u32 = 0x1aa;
    pub const DATA: u32 = 0x1ac;
    pub const ADMAS: u32 = 0x1b0;
    pub const ESAH: u32 = 0x2e0;
    pub const ESAL: u32 = 0x2e2;
    pub const EEA: u32 = 0x33c;
    pub const ENDX0: u32 = 0x340;
    pub const ENDX1: u32 = 0x342;

    pub const MVOLL: u32 = 0;
    pub const MVOLR: u32 = 1;
    pub const EVOLL: u32 = 2;
    pub const EVOLR: u32 = 3;
    pub const AVOLL: u32 = 4;
    pub const AVOLR: u32 = 5;
    pub const BVOLL: u32 = 6;
    pub const BVOLR: u32 = 7;
    pub const MVOLXL: u32 = 8;
    pub const MVOLXR: u32 = 9;
    pub const IIR_VOL: u32 = 10;
    pub const COMB1_VOL: u32 = 11;
    pub const COMB2_VOL: u32 = 12;
    pub const COMB3_VOL: u32 = 13;
    pub const COMB4_VOL: u32 = 14;
    pub const WALL_VOL: u32 = 15;
    pub const APF1_VOL: u32 = 16;
    pub const APF2_VOL: u32 = 17;
    pub const IN_COEF_L: u32 = 18;
    pub const IN_COEF_R: u32 = 19;

    pub const SPDIF_OUT: u32 = BASE + 0x7c0;
    pub const SPDIF_MODE: u32 = BASE + 0x7c6;
    pub const SPDIF_MEDIA: u32 = BASE + 0x7c8;
    pub const SPDIF_7CA: u32 = BASE + 0x7ca;

    pub fn check_core(core: i64) -> Result<()> {
        if core != 0 && core != 1 {
            return fail(format!("no SPU2 core {core}"));
        }
        Ok(())
    }

    pub fn check_voice(voice: i64) -> Result<()> {
        if !(0..24).contains(&voice) {
            return fail(format!("no SPU2 voice {voice}"));
        }
        Ok(())
    }

    pub fn voice(core: i64, v: i64, param: u32) -> Result<u32> {
        check_core(core)?;
        check_voice(v)?;
        Ok(BASE + core as u32 * 0x400 + v as u32 * 16 + param * 2)
    }

    pub fn voice_start(core: i64, v: i64, low: bool) -> Result<u32> {
        check_core(core)?;
        check_voice(v)?;
        Ok(BASE + core as u32 * 0x400 + 0x1c0 + v as u32 * 12 + if low { 2 } else { 0 })
    }

    pub fn core(c: i64, offset: u32) -> Result<u32> {
        check_core(c)?;
        Ok((BASE + c as u32 * 0x400).wrapping_add(offset))
    }

    pub fn volume(c: i64, index: u32) -> Result<u32> {
        check_core(c)?;
        Ok(BASE + 0x760 + c as u32 * 0x28 + index * 2)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registers {
    pub values: HashMap<u32, u16>,
    pub log: WriteStream,
    pub frame: u32,
}

impl Registers {
    pub fn write(&mut self, address: u32, value: i64) {
        let v = value as u16;
        self.values.insert(address, v);
        self.log.push(SpuWrite { frame: self.frame, sample: 0, address, value: v, cycle: 0, resume: 0 });
    }

    pub fn read(&self, address: u32) -> u16 {
        self.values.get(&address).copied().unwrap_or(0)
    }

    pub fn seed(&mut self, address: u32, value: u16) {
        self.values.insert(address, value);
    }
}
