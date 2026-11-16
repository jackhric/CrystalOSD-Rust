#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    #[default]
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl Phase {
    pub fn from_u32(value: u32) -> Option<Phase> {
        match value {
            0 => Some(Phase::Off),
            1 => Some(Phase::Attack),
            2 => Some(Phase::Decay),
            3 => Some(Phase::Sustain),
            4 => Some(Phase::Release),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Envelope {
    pub phase: Phase,
    pub level: i32,
    pub counter: u32,
    pub reg1: u16,
    pub reg2: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VolumeSweep {
    pub reg: u16,
    pub level: i32,
    pub counter: u32,
}

const TOP: i32 = 0x7FFF;
const COUNTER_FULL: u32 = 0x8000;

#[derive(Clone, Copy)]
struct Ramp {
    shift: i32,
    step: i32,
    exponential: bool,
    decrease: bool,
}

fn field(reg: u32, low: u32, width: u32) -> i32 {
    ((reg >> low) & ((1u32 << width) - 1)) as i32
}

fn signed_step(code: i32, decrease: bool) -> i32 {
    if decrease { code - 8 } else { 7 - code }
}

// psx-spx "Envelope Operation": a wait of 1 << max(0, shift - 11) cycles is kept as a counter filling to 0x8000.
fn ramp_delta(counter: &mut u32, magnitude: i32, ramp: Ramp) -> i32 {
    let mut slowdown = 0.max(ramp.shift - 11);
    if ramp.exponential && !ramp.decrease && magnitude > 0x6000 {
        slowdown += 2;
    }
    *counter = counter.wrapping_add(1u32.max(COUNTER_FULL >> slowdown));
    if *counter < COUNTER_FULL {
        return 0;
    }
    *counter = 0;
    let mut delta = ramp.step * (1 << 0.max(11 - ramp.shift));
    if ramp.exponential && ramp.decrease {
        delta = delta.wrapping_mul(magnitude) >> 15;
    }
    delta
}

fn attack_ramp(reg1: u16) -> Ramp {
    Ramp { shift: field(reg1.into(), 10, 5), step: signed_step(field(reg1.into(), 8, 2), false), exponential: reg1 >> 15 != 0, decrease: false }
}

fn decay_ramp(reg1: u16) -> Ramp {
    Ramp { shift: field(reg1.into(), 4, 4), step: -8, exponential: true, decrease: true }
}

fn sustain_ramp(reg2: u16) -> Ramp {
    let decrease = (reg2 >> 14) & 1 != 0;
    Ramp { shift: field(reg2.into(), 8, 5), step: signed_step(field(reg2.into(), 6, 2), decrease), exponential: reg2 >> 15 != 0, decrease }
}

fn release_ramp(reg2: u16) -> Ramp {
    Ramp { shift: field(reg2.into(), 0, 5), step: -8, exponential: (reg2 >> 5) & 1 != 0, decrease: true }
}

impl Envelope {
    pub fn key_on(&mut self) {
        self.phase = Phase::Attack;
        self.level = 0;
        self.counter = 0;
    }

    pub fn key_off(&mut self) {
        if self.phase == Phase::Off {
            return;
        }
        self.phase = Phase::Release;
        self.counter = 0;
    }

    pub fn step(&mut self) -> bool {
        match self.phase {
            Phase::Off => return false,
            Phase::Attack => {
                self.level = TOP.min(self.level.wrapping_add(ramp_delta(&mut self.counter, self.level, attack_ramp(self.reg1))));
                if self.level == TOP {
                    self.phase = Phase::Decay;
                }
            }
            Phase::Decay => {
                self.level = 0.max(self.level.wrapping_add(ramp_delta(&mut self.counter, self.level, decay_ramp(self.reg1))));
                if self.level <= ((field(self.reg1.into(), 0, 4) + 1) << 11) {
                    self.phase = Phase::Sustain;
                }
            }
            Phase::Sustain => {
                self.level = self.level.wrapping_add(ramp_delta(&mut self.counter, self.level, sustain_ramp(self.reg2))).clamp(0, TOP);
            }
            Phase::Release => {
                self.level = 0.max(self.level.wrapping_add(ramp_delta(&mut self.counter, self.level, release_ramp(self.reg2))));
            }
        }
        if (self.phase == Phase::Sustain || self.phase == Phase::Release) && self.level == 0 {
            self.phase = Phase::Off;
        }
        self.phase != Phase::Off
    }

    pub fn step_with(&mut self, adsr1: u16, adsr2: u16) -> bool {
        self.reg1 = adsr1;
        self.reg2 = adsr2;
        self.step()
    }
}

impl VolumeSweep {
    pub fn write(&mut self, value: u16) {
        self.reg = value;
        if value & 0x8000 == 0 {
            self.level = i32::from((value << 1) as i16);
        }
    }

    pub fn step(&mut self) {
        if self.reg & 0x8000 == 0 {
            return;
        }
        let reg = u32::from(self.reg);
        let decrease = (reg >> 13) & 1 != 0;
        let sign: i32 = if (reg >> 12) & 1 != 0 { -1 } else { 1 };
        let ramp = Ramp { shift: field(reg, 2, 5), step: signed_step(field(reg, 0, 2), decrease), exponential: (reg >> 14) & 1 != 0, decrease };
        let magnitude = sign.wrapping_mul(self.level);
        self.level = sign * magnitude.wrapping_add(ramp_delta(&mut self.counter, magnitude, ramp)).clamp(0, TOP);
    }

    pub fn step_with(&mut self, value: u16) {
        self.write(value);
        self.step();
    }
}
