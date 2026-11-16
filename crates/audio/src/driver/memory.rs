use std::path::Path;

use assets::bytes::{le16, le32, read_file};

use crate::{Result, fail};

pub const OSDSND_BASE: i64 = 0x90130;
pub const OSDSND_SIZE: usize = 0xb000;
pub const SOUND_IOP_BASE: i64 = 0xeb000;
pub const LIBSD_BASE: i64 = 0x80000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IopMemory {
    ram: Vec<u8>,
}

impl Default for IopMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl IopMemory {
    pub const SIZE: usize = 0x200000;

    pub fn new() -> Self {
        Self { ram: vec![0; Self::SIZE] }
    }

    fn check(&self, at: i64, length: i64) -> Result<usize> {
        if at < 0 || at + length > self.ram.len() as i64 {
            return fail(format!("IOP RAM access out of range: {at}"));
        }
        Ok(at as usize)
    }

    pub fn load(&mut self, at: i64, data: &[u8]) -> Result<()> {
        let start = self.check(at, data.len() as i64)?;
        self.ram[start..start + data.len()].copy_from_slice(data);
        Ok(())
    }

    pub fn u8(&self, at: i64) -> Result<u8> {
        Ok(self.ram[self.check(at, 1)?])
    }

    pub fn u16(&self, at: i64) -> Result<u16> {
        le16(&self.ram, self.check(at, 2)?)
    }

    pub fn u32(&self, at: i64) -> Result<u32> {
        le32(&self.ram, self.check(at, 4)?)
    }

    pub fn s32(&self, at: i64) -> Result<i32> {
        Ok(self.u32(at)? as i32)
    }

    pub fn write8(&mut self, at: i64, value: i64) -> Result<()> {
        let i = self.check(at, 1)?;
        self.ram[i] = value as u8;
        Ok(())
    }

    pub fn write16(&mut self, at: i64, value: i64) -> Result<()> {
        let i = self.check(at, 2)?;
        self.ram[i] = value as u8;
        self.ram[i + 1] = (value >> 8) as u8;
        Ok(())
    }

    pub fn write32(&mut self, at: i64, value: i64) -> Result<()> {
        let i = self.check(at, 4)?;
        for k in 0..4 {
            self.ram[i + k] = (value >> (8 * k)) as u8;
        }
        Ok(())
    }

    pub fn fill(&mut self, at: i64, length: i64) -> Result<()> {
        let start = self.check(at, length)?;
        self.ram[start..start + length as usize].fill(0);
        Ok(())
    }

    pub fn bytes(&self) -> &[u8] {
        &self.ram
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibsdImage {
    image: Vec<u8>,
}

impl LibsdImage {
    pub fn new(image: Vec<u8>) -> Self {
        Self { image }
    }

    fn check(&self, address: i64, length: i64) -> Result<usize> {
        let at = address - LIBSD_BASE;
        if at < 0 || at + length > self.image.len() as i64 {
            return fail(format!("libsd image access out of range: {address}"));
        }
        Ok(at as usize)
    }

    pub fn u32(&self, address: i64) -> Result<u32> {
        le32(&self.image, self.check(address, 4)?)
    }

    pub fn u16(&self, address: i64) -> Result<u16> {
        le16(&self.image, self.check(address, 2)?)
    }

    pub fn write32(&mut self, address: i64, value: u32) -> Result<()> {
        let i = self.check(address, 4)?;
        self.image[i..i + 4].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn slice(&self, address: i64, length: i64) -> Result<&[u8]> {
        let i = self.check(address, length)?;
        Ok(&self.image[i..i + length as usize])
    }

    pub fn bytes(&self) -> &[u8] {
        &self.image
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub osdsnd: Vec<u8>,
    pub iop: Vec<u8>,
}

pub fn load_snapshot(stem: &Path) -> Result<Snapshot> {
    let stem = stem.to_string_lossy();
    let mut s = Snapshot { osdsnd: read_file(Path::new(&format!("{stem}.start.osdsnd.bin")))?, iop: Vec::new() };
    let iop = format!("{stem}.start.iopsound.bin");
    if Path::new(&iop).exists() {
        s.iop = read_file(Path::new(&iop))?;
    }
    if s.osdsnd.len() > OSDSND_SIZE {
        return fail(format!("{stem}: OSDSND snapshot larger than its region"));
    }
    Ok(s)
}
