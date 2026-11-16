use assets::bytes::{le16, le32};

use super::driver::FillEvent;
use super::registers::reg;
use super::state::{EffectAttr, LIBSD_STATE, State};
use crate::{Result, fail};

const SIZE_TABLE: i64 = 0x8f7e8;
const PRESET_TABLE: i64 = 0x8f810;
const PRESET_SIZE: i64 = 0x44;

#[derive(Clone, Copy)]
struct PresetField {
    at: u32,
    address: u32,
    volume: i32,
}

const NO_VOLUME: i32 = -1;

const fn make_fields() -> [PresetField; 32] {
    let mut f = [PresetField { at: 0, address: 0, volume: 0 }; 32];
    f[0] = PresetField { at: 0x04, address: 0x172, volume: NO_VOLUME };
    f[1] = PresetField { at: 0x06, address: 0x174, volume: NO_VOLUME };
    let volumes = [reg::IIR_VOL, reg::COMB1_VOL, reg::COMB2_VOL, reg::COMB3_VOL, reg::COMB4_VOL, reg::WALL_VOL, reg::APF1_VOL, reg::APF2_VOL];
    let mut k = 0;
    while k < 8 {
        f[2 + k] = PresetField { at: 0x08 + 2 * k as u32, address: 0, volume: volumes[k] as i32 };
        k += 1;
    }
    let mut k = 0;
    while k < 20 {
        f[10 + k] = PresetField { at: 0x18 + 2 * k as u32, address: 0x176 + 2 * k as u32, volume: NO_VOLUME };
        k += 1;
    }
    f[30] = PresetField { at: 0x40, address: 0, volume: reg::IN_COEF_L as i32 };
    f[31] = PresetField { at: 0x42, address: 0, volume: reg::IN_COEF_R as i32 };
    f
}

const FIELDS: [PresetField; 32] = make_fields();

impl State {
    pub fn get_effect_attr(&self, core: i64) -> Result<EffectAttr> {
        reg::check_core(core)?;
        Ok(EffectAttr {
            core: core as u32,
            mode: self.lib.u32(LIBSD_STATE + 0x24 + 20 * core)? as i32,
            depth_l: self.regs.read(reg::volume(core, reg::EVOLL)?),
            depth_r: self.regs.read(reg::volume(core, reg::EVOLR)?),
            delay: self.lib.u32(LIBSD_STATE + 0x2c + 20 * core)?,
            feedback: self.lib.u32(LIBSD_STATE + 0x30 + 20 * core)?,
        })
    }

    pub fn set_effect_attr(&mut self, attr: &EffectAttr) -> Result<i32> {
        let core = i64::from(attr.core);
        let mut mode = attr.mode;
        let mut clear = false;
        let mut saved = 0u32;
        if mode & 0x100 != 0 {
            mode &= !0x100;
            clear = true;
            saved = self.lib.u32(LIBSD_STATE + 0x24 + 20 * core)?;
        }
        if mode >= 10 {
            return Ok(-1);
        }
        if mode >= 7 {
            return fail(format!("effect mode {mode} (echo, delay, pipe) is not modelled"));
        }
        if mode < 0 {
            return fail(format!("effect mode {mode} is not modelled"));
        }
        self.lib.write32(LIBSD_STATE + 0x24 + 20 * core, mode as u32)?;
        let end = (u32::from(self.regs.read(reg::core(core, reg::EEA)?)) << 17) | 0x1ffff;
        let size = |lib: &super::memory::LibsdImage, m: u32| -> Result<u64> { Ok(u64::from(lib.u32(SIZE_TABLE + 4 * i64::from(m))?) * 8) };
        let start_of = |lib: &super::memory::LibsdImage, m: u32| -> Result<u32> { Ok(u64::from(end).wrapping_sub(size(lib, m)?.wrapping_sub(1)) as u32) };
        let start = start_of(&self.lib, mode as u32)?;
        self.lib.write32(LIBSD_STATE + 0xc0 + 4 * core, start)?;
        let preset = self.lib.slice(PRESET_TABLE + PRESET_SIZE * i64::from(mode), PRESET_SIZE)?.to_vec();
        let attr_address = reg::core(core, reg::ATTR)?;
        let enabled = self.regs.read(attr_address) & 0x80 != 0;
        let fill = |s: &mut State, m: u32| -> Result<()> {
            if m == 0 {
                return Ok(());
            }
            let bytes = size(&s.lib, m)?;
            let start = start_of(&s.lib, m)?;
            s.fills.push(FillEvent { core: core as u32, start, bytes, transferred: bytes.div_ceil(64) * 64 });
            Ok(())
        };
        if enabled {
            self.regs.write(attr_address, i64::from(self.regs.read(attr_address) & 0xff7f));
            if clear {
                fill(self, saved)?;
            }
        }
        self.regs.write(reg::volume(core, reg::EVOLL)?, i64::from(attr.depth_l));
        self.regs.write(reg::volume(core, reg::EVOLR)?, i64::from(attr.depth_r));
        let flags = le32(&preset, 0)?;
        for (bit, field) in FIELDS.iter().enumerate() {
            if flags != 0 && (flags >> bit) & 1 == 0 {
                continue;
            }
            let value = le16(&preset, field.at as usize)?;
            if field.volume != NO_VOLUME {
                self.regs.write(reg::volume(core, field.volume as u32)?, i64::from(value));
            } else {
                let wide = u32::from(value) << 2;
                self.regs.write(reg::core(core, 2 * field.address)?, i64::from(wide >> 16));
                self.regs.write(reg::core(core, 2 * field.address + 2)?, i64::from(wide & 0xffff));
            }
        }
        self.regs.write(reg::core(core, reg::ESAH)?, i64::from((start >> 17) & 0xffff));
        self.regs.write(reg::core(core, reg::ESAL)?, i64::from((start >> 1) & 0xffff));
        if clear {
            fill(self, mode as u32)?;
        }
        if enabled {
            self.regs.write(attr_address, i64::from(self.regs.read(attr_address) | 0x80));
        }
        Ok(0)
    }
}
