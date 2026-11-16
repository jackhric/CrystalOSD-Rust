use std::path::Path;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    offset: u32,
    address: u32,
    size: u32,
}

fn read32(d: &[u8], at: usize) -> Result<u32> {
    at.checked_add(4)
        .and_then(|end| d.get(at..end))
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error("program image: read past the end".into()))
}

fn read16(d: &[u8], at: usize) -> Result<u32> {
    at.checked_add(2)
        .and_then(|end| d.get(at..end))
        .map(|b| u32::from(u16::from_le_bytes(b.try_into().unwrap())))
        .ok_or_else(|| Error("program image: read past the end".into()))
}

fn fail<T>(result: Result<T>) -> T {
    result.unwrap_or_else(|e| panic!("{e}"))
}

// The program's own data, read from its ELF by EE address (References/model/clock_text.mjs elfImage): the
// loadable segments as the EE holds them when the program starts.
#[derive(Debug, Clone)]
pub struct ProgramImage {
    elf: Vec<u8>,
    segments: Vec<Segment>,
}

impl ProgramImage {
    pub fn new(elf: Vec<u8>) -> Result<Self> {
        if elf.len() < 52 || &elf[..4] != b"\x7fELF" {
            return Err(Error("program image: not an ELF".into()));
        }
        let phoff = read32(&elf, 28)?;
        let size = read16(&elf, 42)?;
        let count = read16(&elf, 44)?;
        let mut segments = Vec::new();
        for i in 0..count {
            let at = phoff as usize + i as usize * size as usize;
            if read32(&elf, at)? == 1 {
                segments.push(Segment { offset: read32(&elf, at + 4)?, address: read32(&elf, at + 8)?, size: read32(&elf, at + 16)? });
            }
        }
        Ok(Self { elf, segments })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read(path).map_err(|_| Error(format!("cannot open the program {}", path.display())))?;
        Self::new(data)
    }

    fn offset_of(&self, address: u32, length: u32) -> Result<usize> {
        for s in &self.segments {
            if address >= s.address && u64::from(address) + u64::from(length) <= u64::from(s.address) + u64::from(s.size) {
                return Ok(s.offset as usize + (address - s.address) as usize);
            }
        }
        Err(Error(format!("program image: no segment holds the address {address}")))
    }

    pub fn word(&self, address: u32) -> u32 {
        fail(self.offset_of(address, 4).and_then(|at| read32(&self.elf, at)))
    }

    pub fn integer(&self, address: u32) -> i32 {
        self.word(address) as i32
    }

    pub fn single(&self, address: u32) -> f32 {
        f32::from_bits(self.word(address))
    }

    pub fn double_at(&self, address: u32) -> f64 {
        f64::from_bits(u64::from(self.word(address)) | (u64::from(self.word(address + 4)) << 32))
    }

    // The bytes from `address` to the first zero.
    pub fn string(&self, address: u32) -> Vec<u8> {
        let start = fail(self.offset_of(address, 1));
        let mut end = start;
        while end < self.elf.len() && self.elf[end] != 0 {
            end += 1;
        }
        self.elf.get(start..end).map(<[u8]>::to_vec).unwrap_or_default()
    }
}
