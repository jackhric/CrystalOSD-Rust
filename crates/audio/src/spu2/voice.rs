use super::envelope::{Envelope, VolumeSweep};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voice {
    pub left: VolumeSweep,
    pub right: VolumeSweep,
    pub envelope: Envelope,
    pub pitch: u16,
    pub start_address: u32,
    pub loop_address: u32,
    pub next_address: u32,
    pub prev1: i32,
    pub prev2: i32,
    pub modulated: bool,
    pub noise: bool,
    pub loop_mode: i8,
    pub loop_flags: i8,
    pub phase: i32,
    pub out_x: i32,
    pub fifo: [i32; 32],
    pub fifo_write: u32,
    pub fifo_read: u32,
    pub block: [i16; 28],
    pub block_pending: bool,
    pub dry_l: i32,
    pub dry_r: i32,
    pub wet_l: i32,
    pub wet_r: i32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            left: VolumeSweep::default(),
            right: VolumeSweep::default(),
            envelope: Envelope::default(),
            pitch: 0x3FFF,
            start_address: 0x2800,
            loop_address: 0x2800,
            next_address: 0x2801,
            prev1: 0,
            prev2: 0,
            modulated: false,
            noise: false,
            loop_mode: 0,
            loop_flags: 0,
            phase: 0,
            out_x: 0,
            fifo: [0; 32],
            fifo_write: 0,
            fifo_read: 0,
            block: [0; 28],
            block_pending: false,
            dry_l: -1,
            dry_r: -1,
            wet_l: -1,
            wet_r: -1,
        }
    }
}
