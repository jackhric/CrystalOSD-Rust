use super::driver::{EnvxFeed, FillEvent};
use super::iop_clock::{EnvxRead, IopClock};
use super::memory::{IopMemory, LibsdImage, OSDSND_BASE, SOUND_IOP_BASE, Snapshot};
use super::registers::{Registers, reg};
use crate::Result;

pub const fn m(offset: i64) -> i64 {
    OSDSND_BASE + offset
}

pub const SLOTS: i64 = m(0x9ed0);
pub const CTX: i64 = m(0xa530);
pub const VOICES: i64 = m(0x9330);
pub const BANKS: i64 = m(0x98d4);
pub const BANK_FLAGS: i64 = m(0x98d0);
pub const LIBSD_STATE: i64 = 0x90000;

pub fn s16(v: i64) -> i32 {
    i32::from(v as u16 as i16)
}

pub fn u16(v: i64) -> u32 {
    u32::from(v as u16)
}

pub fn i32(v: i64) -> i32 {
    v as u32 as i32
}

pub fn mul_hi(a: i64, b: i64) -> i32 {
    ((i64::from(i32(a)) * i64::from(i32(b))) >> 32) as i32
}

pub fn div60(x: i64) -> i32 {
    let v = i32(x);
    let hi = mul_hi(i64::from(v), 0x88888889);
    (hi.wrapping_add(v) >> 5).wrapping_sub(v >> 31)
}

pub use super::sequencer::voice_volume_of;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectAttr {
    pub core: u32,
    pub mode: i32,
    pub depth_l: u16,
    pub depth_r: u16,
    pub delay: u32,
    pub feedback: u32,
}

pub struct State {
    pub mem: IopMemory,
    pub lib: LibsdImage,
    pub regs: Registers,
    pub feed: EnvxFeed,
    pub fills: Vec<FillEvent>,
    pub snapshot: Snapshot,
    pub libsd_start: Vec<u8>,
    pub clock: Option<Box<IopClock>>,
    pub envx_log: Vec<EnvxRead>,
    pub started: bool,
}

impl State {
    pub fn new(snapshot: &Snapshot, libsd_image: Vec<u8>, envx: EnvxFeed, cold_start: bool) -> Result<State> {
        let mut lib = LibsdImage::new(libsd_image);
        if cold_start {
            for core in [0i64, 1] {
                for at in [0x24i64, 0x2c, 0x30] {
                    lib.write32(LIBSD_STATE + at + 20 * core, 0)?;
                }
            }
        }
        let mut mem = IopMemory::new();
        mem.load(OSDSND_BASE, &snapshot.osdsnd)?;
        mem.load(SOUND_IOP_BASE, &snapshot.iop)?;
        let libsd_start = lib.bytes().to_vec();
        Ok(State {
            mem,
            lib,
            regs: Registers::default(),
            feed: envx,
            fills: Vec::new(),
            snapshot: snapshot.clone(),
            libsd_start,
            clock: None,
            envx_log: Vec::new(),
            started: false,
        })
    }

    pub fn feed_tick(&mut self, core: u32, voice: u32) -> u16 {
        let v = (self.feed.tick)(core, voice);
        if self.clock.is_some() {
            self.envx_log.push(EnvxRead { core, voice, value: v });
        }
        v
    }

    pub fn feed_allocation(&mut self, core: u32, voice: u32) -> u16 {
        let v = (self.feed.allocation)(core, voice);
        if self.clock.is_some() {
            self.envx_log.push(EnvxRead { core, voice, value: v });
        }
        v
    }

    pub fn get_switch(&self, core: i64, offset0: u32) -> Result<u32> {
        Ok(u32::from(self.regs.read(reg::core(core, offset0)?)) | u32::from(self.regs.read(reg::core(core, offset0 + 2)?)) << 16)
    }

    pub fn set_switch(&mut self, core: i64, offset0: u32, bits: u32) -> Result<()> {
        self.regs.write(reg::core(core, offset0)?, i64::from(bits & 0xffff));
        self.regs.write(reg::core(core, offset0 + 2)?, i64::from((bits >> 16) & 0xffff));
        Ok(())
    }

    pub fn set_param(&mut self, core: i64, voice: i64, param: u32, value: i64) -> Result<()> {
        self.regs.write(reg::voice(core, voice, param)?, value);
        Ok(())
    }

    pub fn set_addr(&mut self, core: i64, voice: i64, byte_address: u32) -> Result<()> {
        let half = byte_address >> 1;
        self.regs.write(reg::voice_start(core, voice, false)?, i64::from(half >> 16));
        self.regs.write(reg::voice_start(core, voice, true)?, i64::from(half & 0xffff));
        Ok(())
    }
}
