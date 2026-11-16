use super::memory::LibsdImage;
use super::registers::reg::{self, *};
use super::sequencer::voice_volume_of;
use super::state::{BANK_FLAGS, BANKS, CTX, LIBSD_STATE, SLOTS, State, VOICES, div60, m, s16, u16};
use crate::spu_write::DriverCommand;
use crate::{Result, fail};

const EFFECT_TABLE_POINTER: i64 = m(0x921c);
const EFFECT_RECORD_POINTER: i64 = m(0x9210);
const PARAM_BASES: i64 = 0x8f690;

struct Table {
    at: i64,
    length: i64,
}

const TABLES: [Table; 4] =
    [Table { at: SLOTS, length: 0x660 }, Table { at: CTX, length: 0x10 }, Table { at: VOICES, length: 0x5a0 }, Table { at: BANK_FLAGS, length: 0x600 }];

fn param_address(lib: &LibsdImage, entry: u32) -> Result<u32> {
    let core = entry & 1;
    let stride = if entry & 0x80 != 0 { 0x28 } else { 0x400 };
    let base = lib.u32(PARAM_BASES + i64::from((entry >> 6) & 0x3fc))?;
    Ok((base.wrapping_add(core * stride).wrapping_add((entry & 0x3e) << 3)) & 0x1fffffff)
}

impl State {
    fn sd_init(&mut self) -> Result<()> {
        let mut w = |address: u32, value: i64| self.regs.write(address, value);
        w(SPDIF_MODE, 0x900);
        w(SPDIF_MEDIA, 0x200);
        w(SPDIF_7CA, 0x8);
        w(SPDIF_OUT, 0);
        w(SPDIF_OUT, 0x8000);
        for core in [0, 1] {
            w(volume(core, MVOLL)?, 0);
            w(volume(core, MVOLR)?, 0);
        }
        for core in [0, 1] {
            w(reg::core(core, ADMAS)?, 0);
        }
        for core in [0, 1] {
            w(reg::core(core, ATTR)?, 0);
        }
        for core in [0, 1] {
            w(reg::core(core, ATTR)?, 0x8000);
        }
        for core in [0, 1] {
            w(volume(core, MVOLL)?, 0);
            w(volume(core, MVOLR)?, 0);
        }
        for core in [0, 1] {
            w(reg::core(core, KOFF0)?, 0xffff);
            w(reg::core(core, KOFF1)?, 0xff);
        }
        for base in [PMON0, NON0] {
            for core in [0, 1] {
                w(reg::core(core, base)?, 0);
                w(reg::core(core, base + 2)?, 0);
            }
        }
        w(reg::core(0, ESAH)?, 0xe);
        w(reg::core(0, ESAL)?, 0xfff8);
        w(reg::core(1, ESAH)?, 0xf);
        w(reg::core(1, ESAL)?, 0xfff8);
        w(reg::core(0, TSAH)?, 0);
        w(reg::core(0, TSAL)?, 0x2800);
        for value in [0x707, 0] {
            for _ in 0..8 {
                w(reg::core(0, DATA)?, value);
            }
        }
        w(reg::core(0, ATTR)?, 0x8010);
        w(reg::core(0, ATTR)?, 0x8000);
        for v in 0..24 {
            let params = [(VOLL, 0), (VOLR, 0), (PITCH, 0x3fff), (ADSR1, 0), (ADSR2, 0)];
            for (param, value) in params {
                for core in [1, 0] {
                    w(voice(core, v, param)?, value);
                }
            }
            for low in [false, true] {
                for core in [1, 0] {
                    w(voice_start(core, v, low)?, if low { 0x2800 } else { 0 });
                }
            }
        }
        for core in [1, 0] {
            w(reg::core(core, KON0)?, 0xffff);
        }
        for core in [1, 0] {
            w(reg::core(core, KON1)?, 0xff);
        }
        for core in [1, 0] {
            w(reg::core(core, KOFF0)?, 0xffff);
        }
        for core in [1, 0] {
            w(reg::core(core, KOFF1)?, 0xff);
        }
        w(reg::core(0, ENDX1)?, 0);
        w(reg::core(0, ENDX0)?, 0);
        w(SPDIF_OUT, 0xc032);
        w(reg::core(0, ATTR)?, 0xc000);
        w(reg::core(1, ATTR)?, 0xc001);
        for core in [0, 1] {
            for base in [VMIXL0, VMIXR0, VMIXEL0, VMIXER0] {
                w(reg::core(core, base)?, 0xffff);
                w(reg::core(core, base + 2)?, 0xff);
            }
        }
        w(reg::core(0, MMIX)?, 0xff0);
        w(reg::core(1, MMIX)?, 0xffc);
        for core in [0, 1] {
            w(volume(core, MVOLL)?, 0);
            w(volume(core, MVOLR)?, 0);
        }
        for core in [0, 1] {
            w(volume(core, EVOLL)?, 0);
            w(volume(core, EVOLR)?, 0);
        }
        w(reg::core(0, EEA)?, 0xe);
        w(reg::core(1, EEA)?, 0xf);
        w(volume(0, AVOLL)?, 0);
        w(volume(0, AVOLR)?, 0);
        w(volume(1, AVOLL)?, 0x7fff);
        w(volume(1, AVOLR)?, 0x7fff);
        w(volume(0, BVOLL)?, 0);
        w(volume(0, BVOLR)?, 0);
        w(volume(1, BVOLL)?, 0);
        w(volume(1, BVOLR)?, 0);
        Ok(())
    }

    pub fn init(&mut self, command: &DriverCommand) -> Result<()> {
        let a = command.words[0];
        let b = command.words[1];
        let c = command.words[2];
        let mem = &mut self.mem;
        match command.id {
            0x6010 => {
                for t in &TABLES {
                    mem.fill(t.at, t.length)?;
                }
                for pointer in [EFFECT_TABLE_POINTER, EFFECT_RECORD_POINTER] {
                    let target = i64::from(mem.u32(pointer)?);
                    if target != 0 {
                        mem.fill(target, 4)?;
                    }
                }
                for j in 0..24 {
                    mem.write16(VOICES + 0x3c * j + 6, 0xffff)?;
                }
                self.sd_init()?;
                for core in [0i64, 1] {
                    self.regs.write(reg::core(core, EEA)?, i64::from(((0x1fffffu32 - ((core as u32) << 17)) >> 17) & 0xffff));
                    let mut attr = self.get_effect_attr(core)?;
                    attr.depth_l = 0;
                    attr.depth_r = 0;
                    attr.mode |= 0x100;
                    self.set_effect_attr(&attr)?;
                }
            }
            0x6060 => {
                if mem.u32(i64::from(a) + 0xc)? != 0x64685353 {
                    return Ok(());
                }
                for k in 0..0x7f {
                    if mem.u32(BANK_FLAGS + 12 * k)? == 0 {
                        mem.write32(BANK_FLAGS + 12 * k, 1)?;
                        mem.write32(BANK_FLAGS + 12 * k + 4, i64::from(a))?;
                        mem.write32(BANK_FLAGS + 12 * k + 8, i64::from(b >> 3))?;
                        return Ok(());
                    }
                }
                return Ok(());
            }
            0x6090 => {
                let bank = i64::from(s16(i64::from(a)));
                if bank >= 0x80 || mem.u32(i64::from(b) + 0xc)? != 0x71735353 {
                    return Ok(());
                }
                if mem.u32(BANK_FLAGS + 12 * bank)? != 1 {
                    return Ok(());
                }
                for i in 0..24 {
                    let slot = SLOTS + 0x44 * i;
                    if mem.u16(slot + 0x26)? != 0 || mem.u16(slot + 0x28)? != 0 {
                        continue;
                    }
                    let b = i64::from(b);
                    mem.write16(slot + 0x26, 1)?;
                    mem.write16(slot + 0x28, 0)?;
                    mem.write32(slot + 4, 0)?;
                    mem.write32(slot + 0x18, b)?;
                    mem.write32(slot + 0x14, 0x110)?;
                    mem.write8(slot, i64::from(mem.u8(b + 0x110)?))?;
                    mem.write8(slot + 2, i64::from(mem.u8(b + 0x111)?))?;
                    mem.write8(slot + 3, i64::from(mem.u8(b + 0x112)?))?;
                    mem.write8(slot + 1, i64::from(mem.u8(slot)?))?;
                    mem.write16(slot + 0x24, bank)?;
                    mem.write16(slot + 0x40, i64::from(mem.u16(b + 2)?))?;
                    mem.write16(slot + 0x3e, i64::from(mem.u16(b + 4)?))?;
                    let product = (u32::from(mem.u16(slot + 0x40)?).wrapping_mul(u32::from(mem.u16(slot + 0x3e)?)) << 12) as i32;
                    let hz = u32::from(mem.u16(CTX + 0xe)?);
                    let divided = if hz == 0 { 0 } else { product / hz as i32 };
                    mem.write32(slot + 8, i64::from(div60(i64::from(divided))))?;
                    return Ok(());
                }
                return Ok(());
            }
            0x60a0 => mem.write16(CTX + 0xe, i64::from(u16(i64::from(a))))?,
            0x60c0 => {
                let mut attr = self.get_effect_attr(i64::from(s16(i64::from(a))))?;
                attr.mode = (u16(i64::from(b)) | 0x100) as i32;
                self.set_effect_attr(&attr)?;
            }
            0x6130 => {
                let index = s16(i64::from(a));
                let slot = SLOTS + 0x44 * i64::from(index);
                if index < 0x18 && s16(i64::from(b)) < 0x80 && mem.u16(slot + 0x26)? == 1 {
                    let sq = i64::from(mem.u32(slot + 0x18)?);
                    mem.write32(m(0x9218), sq)?;
                    mem.write8(sq, i64::from(b))?;
                    mem.write32(m(0x9214), sq + 0x10)?;
                    for j in 0..24i64 {
                        let v = VOICES + 0x3c * j;
                        let mem = &mut self.mem;
                        if mem.u16(v)? == 1 && i64::from(mem.u16(v + 6)?) == i64::from(index) && mem.u16(v + 0x1c)? == mem.u16(slot + 0x24)? {
                            mem.write16(v + 0x2a, i64::from(mem.u8(sq)?))?;
                            let left = voice_volume_of(&self.mem, j as i32, 0)?;
                            self.set_param(1, j, VOLL, i64::from(left))?;
                            let right = voice_volume_of(&self.mem, j as i32, 1)?;
                            self.set_param(1, j, VOLR, i64::from(right))?;
                        }
                    }
                }
            }
            0x6140 => {
                let slot = SLOTS + 0x44 * i64::from(s16(i64::from(a)));
                let bank_index = u32::from(mem.u16(slot + 0x24)?);
                let hd = i64::from(mem.u32(BANKS + 12 * i64::from(bank_index))?);
                if mem.u16(slot + 0x26)? == 1 && mem.u32(BANK_FLAGS + 12 * i64::from(bank_index))? == 1 && mem.u32(hd + 0x10)? != 0xffffffff {
                    mem.write16(slot + 0x28, 1)?;
                    mem.write16(slot + 0x3c, 0)?;
                }
                mem.write32(slot + 0x20, 0)?;
            }
            0x6150 => self.stop(i64::from(s16(i64::from(a))), s16(i64::from(b)), c)?,
            0x8010 => {
                let address = param_address(&self.lib, u16(i64::from(a)))?;
                self.regs.write(address, i64::from(u16(i64::from(b))));
            }
            0x8070 => {
                let entry = u16(i64::from(a));
                let value = u16(i64::from(b));
                let core = i64::from(entry & 1);
                let attr_address = reg::core(core, ATTR)?;
                let attr = u32::from(self.regs.read(attr_address));
                let kind = entry & !1u32;
                if kind == 0xa {
                    if value != 0 {
                        return fail(format!("SPDIF mode {value} is not modelled"));
                    }
                    self.regs.write(SPDIF_MEDIA, 0x200);
                    self.regs.write(SPDIF_OUT, 0xc032);
                    self.regs.write(SPDIF_MODE, 0x900);
                } else if kind == 8 {
                    self.regs.write(attr_address, i64::from((attr & 0xc0ff) | ((value & 0x3f) << 8)));
                } else {
                    let bit = u32::from(self.lib.u16(LIBSD_STATE + 0x12 + i64::from(entry & 0xe))?) & 31;
                    self.regs.write(attr_address, i64::from(((attr & !(1u32 << bit)) | ((value & 1) << bit)) & 0xffff));
                }
            }
            0x61a0 | 0x6070 | 0x6200 | 0x6310 => {
                if command.id == 0x6310 {
                    let bank = i64::from(s16(i64::from(a)));
                    let hd = i64::from(mem.u32(BANKS + 12 * bank)?);
                    let at = hd + i64::from(mem.u32(hd + 0x2c)?);
                    mem.write32(at, i64::from(s16(i64::from(b))))?;
                }
            }
            id => return fail(format!("command 0x{id:x} is not modelled")),
        }
        Ok(())
    }

    fn stop(&mut self, index: i64, mode: i32, c: u32) -> Result<()> {
        let mem = &mut self.mem;
        let slot = SLOTS + 0x44 * index;
        let bank_index = u32::from(mem.u16(slot + 0x24)?);
        let hd = i64::from(mem.u32(BANKS + 12 * i64::from(bank_index))?);
        if !(mem.u16(slot + 0x26)? == 1 && mem.u32(BANK_FLAGS + 12 * i64::from(bank_index))? == 1 && mem.u32(hd + 0x10)? != 0xffffffff) {
            return Ok(());
        }
        let mut silence = 0;
        let mut proceed = true;
        if mode == 0 || mode == 1 {
            mem.write32(slot + 0x14, 0x110)?;
            mem.write16(slot + 0x28, 0)?;
            mem.write32(slot + 0x20, 1)?;
            mem.write16(slot + 0x3c, 0)?;
            mem.write16(slot + 0x2a, 0)?;
            mem.write32(slot + 4, 0)?;
            if mode == 1 {
                silence = 1;
            }
        } else if mode == 2 || mode == 3 {
            if mem.u16(slot + 0x28)? == 1 {
                mem.write16(slot + 0x28, 0)?;
                mem.write16(slot + 0x3c, 1)?;
                if mode == 3 {
                    proceed = false;
                }
            } else if mem.u32(slot + 0x20)? == 0 {
                mem.write16(slot + 0x28, 1)?;
                mem.write16(slot + 0x3c, 0)?;
                proceed = false;
            } else if mode == 3 {
                proceed = false;
            }
        } else {
            proceed = false;
        }
        if !proceed {
            return Ok(());
        }
        for j in 0..24i64 {
            let mem = &mut self.mem;
            let v = VOICES + 0x3c * j;
            if i64::from(mem.u16(v + 6)?) != index {
                continue;
            }
            mem.write16(v, 0)?;
            mem.write16(v + 8, 1)?;
            mem.write16(v + 0x2e, 0x40)?;
            mem.write16(v + 0x14, 0)?;
            mem.write16(v + 0x16, 0)?;
            mem.write32(slot + 0x10, i64::from(mem.u32(slot + 0x10)? | (1u32 << j)))?;
            if silence == 1 {
                self.set_param(1, j, ADSR1, 0)?;
                self.set_param(1, j, ADSR2, 0)?;
            } else if c != 0 {
                let adsr2 = self.regs.read(reg::voice(1, j, ADSR2)?);
                self.set_param(1, j, ADSR2, i64::from((u32::from(adsr2) & 0xffc0) | u16(i64::from(c))))?;
            }
        }
        let channels = i64::from(self.mem.u32(slot + 0x18)?) + 0x10;
        for k in 0..16 {
            self.mem.write8(channels + 16 * k + 9, 0)?;
        }
        let keys = self.mem.u32(slot + 0x10)?;
        self.set_switch(1, KOFF0, keys)?;
        self.mem.write32(slot + 0x10, 0)?;
        Ok(())
    }
}
