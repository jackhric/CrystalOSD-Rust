use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn error<T>(message: impl Into<String>) -> Result<T> {
    Err(Error(message.into()))
}

fn window<const N: usize>(d: &[u8], at: usize) -> Result<[u8; N]> {
    at.checked_add(N).and_then(|end| d.get(at..end)).map(|s| s.try_into().unwrap()).ok_or_else(|| Error("read past the end".into()))
}

pub fn le16(d: &[u8], at: usize) -> Result<u16> {
    window(d, at).map(u16::from_le_bytes)
}

pub fn le32(d: &[u8], at: usize) -> Result<u32> {
    window(d, at).map(u32::from_le_bytes)
}

pub fn le64(d: &[u8], at: usize) -> Result<u64> {
    window(d, at).map(u64::from_le_bytes)
}

pub fn be32(d: &[u8], at: usize) -> Result<u32> {
    window(d, at).map(u32::from_be_bytes)
}

pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|_| Error(format!("cannot open {}", path.display())))
}

pub fn write_file(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|_| Error(format!("cannot write {}", path.display())))?;
    }
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = Path::new(&partial);
    fs::write(partial, data).map_err(|_| Error(format!("cannot write {}", partial.display())))?;
    fs::rename(partial, path).map_err(|_| Error(format!("cannot write {}", path.display())))
}

pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub fn hash_bytes(data: &[u8], seed: u64) -> u64 {
    data.iter().fold(seed, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME))
}

/// FNV-1a, 64 bits: the cache key of a source file (not a security hash).
pub fn hash_of(data: &[u8]) -> u64 {
    hash_bytes(data, FNV_OFFSET)
}
