use assets::bytes::{le16, le32};

use crate::{Result, fail};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SqChannel {
    pub channel: u8,
    pub program: u8,
    pub volume: u8,
    pub pan: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SqEvent {
    pub tick: u32,
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
    pub has_data2: bool,
    pub tempo: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SqSequence {
    pub volume: u16,
    pub resolution: u16,
    pub tempo: u32,
    pub channels: Vec<SqChannel>,
    pub events: Vec<SqEvent>,
}

const MAGIC_AT: usize = 0xC;
const CHANNELS_AT: usize = 0x10;
const CHANNEL_SIZE: usize = 16;
const CHANNEL_COUNT: usize = 16;
const EVENTS_AT: usize = 0x110;
const NO_PROGRAM: u8 = 0xFF;

fn byte_at(d: &[u8], at: usize) -> Result<u8> {
    match d.get(at) {
        Some(&b) => Ok(b),
        None => fail("SQ read past the end"),
    }
}

impl SqSequence {
    pub fn parse(data: &[u8]) -> Result<SqSequence> {
        if data.len() < EVENTS_AT || &data[MAGIC_AT..MAGIC_AT + 4] != b"SSsq" {
            return fail("not an SSsq sequence");
        }
        let mut seq = SqSequence { volume: le16(data, 0)?, resolution: le16(data, 2)?, tempo: le32(data, 4)?, ..Default::default() };
        for c in 0..CHANNEL_COUNT {
            let o = CHANNELS_AT + CHANNEL_SIZE * c;
            if data[o + 2] == NO_PROGRAM {
                continue;
            }
            seq.channels.push(SqChannel { channel: data[o + 1], program: data[o + 2], volume: data[o + 3], pan: data[o + 4] });
        }
        let mut at = EVENTS_AT;
        let mut tick = 0u32;
        let mut status = 0u8;
        loop {
            let mut s = byte_at(data, at)?;
            if s & 0x80 != 0 {
                status = s;
                at += 1;
            } else {
                s = status;
            }
            if s == 0 {
                return fail("SQ event without a status");
            }
            let mut event = SqEvent { tick, status: s, ..Default::default() };
            if s == 0xFF {
                let kind = byte_at(data, at)?;
                at += 1;
                event.data1 = kind;
                if kind == 0x2F {
                    seq.events.push(event);
                    break;
                }
                if kind != 0x51 {
                    return fail("unknown SQ meta event");
                }
                event.tempo = u32::from(byte_at(data, at)?) | u32::from(byte_at(data, at + 1)?) << 8 | u32::from(byte_at(data, at + 2)?) << 16;
                at += 3;
                seq.events.push(event);
                continue;
            }
            event.data1 = byte_at(data, at)?;
            at += 1;
            let kind = s & 0xF0;
            if kind != 0xC0 && kind != 0xD0 {
                event.data2 = byte_at(data, at)?;
                at += 1;
                event.has_data2 = true;
            }
            seq.events.push(event);
            let mut delta = 0u32;
            let mut i = 0;
            loop {
                if i == 4 {
                    return fail("SQ VLQ longer than four bytes");
                }
                let c = byte_at(data, at)?;
                at += 1;
                delta = (delta << 7) | u32::from(c & 0x7F);
                if c & 0x80 == 0 {
                    break;
                }
                i += 1;
            }
            tick = tick.wrapping_add(delta);
        }
        Ok(seq)
    }
}
