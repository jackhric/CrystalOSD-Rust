use crate::bytes::{Result, error, le16, le32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub offset: u32,
    pub address: u32,
    pub size: u32,
}

/// The loadable segments of an EE ELF, read by EE address.
#[derive(Debug, Clone)]
pub struct ElfImage<'a> {
    entry: u32,
    elf: &'a [u8],
    segments: Vec<Segment>,
}

impl<'a> ElfImage<'a> {
    pub fn new(elf: &'a [u8]) -> Result<Self> {
        if elf.len() < 52 || &elf[..4] != b"\x7fELF" {
            return error("not an ELF");
        }
        let entry = le32(elf, 24)?;
        let phoff = le32(elf, 28)? as usize;
        let size = usize::from(le16(elf, 42)?);
        let count = usize::from(le16(elf, 44)?);
        let mut segments = Vec::new();
        for i in 0..count {
            let at = phoff + i * size;
            if le32(elf, at)? == 1 {
                segments.push(Segment { offset: le32(elf, at + 4)?, address: le32(elf, at + 8)?, size: le32(elf, at + 16)? });
            }
        }
        Ok(Self { entry, elf, segments })
    }

    pub fn bytes(&self, address: u32, length: u32) -> Result<&'a [u8]> {
        for s in &self.segments {
            if address < s.address || u64::from(address) + u64::from(length) > u64::from(s.address) + u64::from(s.size) {
                continue;
            }
            let start = s.offset as usize + (address - s.address) as usize;
            if let Some(view) = self.elf.get(start..start + length as usize) {
                return Ok(view);
            }
        }
        error(format!("ELF: no segment holds the address {address}"))
    }

    pub fn entry(&self) -> u32 {
        self.entry
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}
