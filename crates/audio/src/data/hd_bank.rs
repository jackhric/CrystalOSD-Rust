use assets::bytes::{le16, le32};

use crate::{Result, fail};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tone {
    pub key_low: u8,
    pub key_high: u8,
    pub root_key: u8,
    pub fine_tune: i8,
    pub sample_start: u16,
    pub adsr1: u16,
    pub adsr2: u16,
    pub pan: u8,
    pub bend_range: u8,
    pub flags: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    pub id: u8,
    pub head: u8,
    pub volume: u8,
    pub pan: u8,
    pub offset: u32,
    pub tones: Vec<Tone>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Effect {
    pub left: u16,
    pub right: u16,
    pub pitch: u16,
    pub flags_a: u16,
    pub adsr1: u16,
    pub adsr2: u16,
    pub word12: u16,
    pub word14: u16,
    pub bd_offset: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HdBank {
    pub programs: Vec<Program>,
    pub velocity: Vec<u8>,
    pub effects: Vec<Effect>,
    pub effect_table: Option<u32>,
    pub effect_master: u8,
}

const MAGIC_AT: usize = 0xC;
const PROGRAM_TABLE_FIELD: usize = 0x10;
const VELOCITY_TABLE_FIELD: usize = 0x14;
const EFFECT_TABLE_FIELD: usize = 0x2C;
const NO_TABLE: u32 = 0xFFFF_FFFF;
const EFFECTS_AT: usize = 0x20;
const EFFECT_SIZE: usize = 64;
const EFFECT_READ: usize = 20;
const MAX_EFFECTS: usize = 14;
const TONE_SIZE: usize = 16;
const PROGRAM_HEADER: usize = 8;
const VELOCITY_COUNT: usize = 128;

fn byte_at(d: &[u8], at: usize) -> Result<u8> {
    match d.get(at) {
        Some(&b) => Ok(b),
        None => fail("read past the end"),
    }
}

fn table(d: &[u8], field: usize) -> Result<Option<u32>> {
    let at = le32(d, field)?;
    Ok(if at == NO_TABLE { None } else { Some(at) })
}

impl HdBank {
    pub fn parse(data: &[u8]) -> Result<HdBank> {
        if data.len() < 0x18 || &data[MAGIC_AT..MAGIC_AT + 4] != b"SShd" {
            return fail("not an SShd bank");
        }
        let mut bank = HdBank::default();
        let program_table = table(data, PROGRAM_TABLE_FIELD)?;
        let velocity_table = table(data, VELOCITY_TABLE_FIELD)?;
        bank.effect_table = table(data, EFFECT_TABLE_FIELD)?;
        if let Some(base) = program_table {
            let base = base as usize;
            if base + 2 > data.len() {
                return fail("program table past the end");
            }
            let top = le16(data, base)?;
            for p in 0..=u32::from(top) {
                let rel = le16(data, base + 2 + 2 * p as usize)?;
                if rel == 0xFFFF {
                    continue;
                }
                let at = base + rel as usize;
                if at + PROGRAM_HEADER > data.len() {
                    return fail("program past the end");
                }
                let mut program = Program { id: p as u8, head: data[at], volume: data[at + 1], pan: data[at + 2], offset: at as u32, tones: Vec::new() };
                for t in 0..u32::from((program.head & 0x7F) + 1) {
                    let o = at + PROGRAM_HEADER + TONE_SIZE * t as usize;
                    if o + TONE_SIZE > data.len() {
                        return fail("tone past the end");
                    }
                    program.tones.push(Tone {
                        key_low: data[o],
                        key_high: data[o + 1],
                        root_key: data[o + 2],
                        fine_tune: data[o + 3] as i8,
                        sample_start: le16(data, o + 4)?,
                        adsr1: le16(data, o + 6)?,
                        adsr2: le16(data, o + 8)?,
                        pan: data[o + 11],
                        bend_range: data[o + 13],
                        flags: data[o + 15],
                    });
                }
                bank.programs.push(program);
            }
        }
        if let Some(velocity) = velocity_table {
            let at = velocity as usize + 2;
            if at + VELOCITY_COUNT > data.len() {
                return fail("velocity table past the end");
            }
            bank.velocity = data[at..at + VELOCITY_COUNT].to_vec();
        }
        if let Some(base) = bank.effect_table {
            let base = base as usize;
            bank.effect_master = byte_at(data, base)?;
            let mut n = 0;
            while n < MAX_EFFECTS && base + EFFECTS_AT + EFFECT_SIZE * n + EFFECT_READ <= data.len() {
                let o = base + EFFECTS_AT + EFFECT_SIZE * n;
                bank.effects.push(Effect {
                    left: le16(data, o)?,
                    right: le16(data, o + 2)?,
                    pitch: le16(data, o + 4)?,
                    flags_a: le16(data, o + 6)?,
                    adsr1: le16(data, o + 8)?,
                    adsr2: le16(data, o + 10)?,
                    word12: le16(data, o + 12)?,
                    word14: le16(data, o + 14)?,
                    bd_offset: le32(data, o + 16)?,
                });
                n += 1;
            }
        }
        Ok(bank)
    }

    pub fn program(&self, id: u8) -> Option<&Program> {
        self.programs.iter().find(|p| p.id == id)
    }
}
