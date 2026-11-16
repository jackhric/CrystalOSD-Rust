use std::fs;
use std::path::Path;

use crate::{Result, fail};

fn get32(p: &[u8]) -> u32 {
    u32::from_le_bytes([p[0], p[1], p[2], p[3]])
}

fn get16(p: &[u8]) -> u16 {
    u16::from_le_bytes([p[0], p[1]])
}

pub fn write_wav(path: &Path, interleaved_stereo: &[i16], sample_rate: u32) -> Result<()> {
    let data_bytes = (interleaved_stereo.len() * 2) as u32;
    let mut out = vec![0u8; 44 + data_bytes as usize];
    out[0..4].copy_from_slice(b"RIFF");
    out[4..8].copy_from_slice(&(36 + data_bytes).to_le_bytes());
    out[8..16].copy_from_slice(b"WAVEfmt ");
    out[16..20].copy_from_slice(&16u32.to_le_bytes());
    out[20..22].copy_from_slice(&1u16.to_le_bytes());
    out[22..24].copy_from_slice(&2u16.to_le_bytes());
    out[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    out[28..32].copy_from_slice(&sample_rate.wrapping_mul(4).to_le_bytes());
    out[32..34].copy_from_slice(&4u16.to_le_bytes());
    out[34..36].copy_from_slice(&16u16.to_le_bytes());
    out[36..40].copy_from_slice(b"data");
    out[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    for (i, s) in interleaved_stereo.iter().enumerate() {
        out[44 + 2 * i..46 + 2 * i].copy_from_slice(&s.to_le_bytes());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).or_else(|_| fail(format!("{}: cannot write", path.display())))?;
    }
    fs::write(path, &out).or_else(|_| fail(format!("{}: cannot write", path.display())))
}

pub fn read_wav(path: &Path) -> Result<Vec<i16>> {
    let Ok(b) = fs::read(path) else {
        return fail(format!("{}: cannot read", path.display()));
    };
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..16] != b"WAVEfmt " || &b[36..40] != b"data" {
        return fail(format!("{}: not a canonical WAV", path.display()));
    }
    if get32(&b[16..]) != 16 || get16(&b[20..]) != 1 || get16(&b[22..]) != 2 || get16(&b[34..]) != 16 {
        return fail(format!("{}: not 16-bit stereo PCM", path.display()));
    }
    let data_bytes = get32(&b[40..]);
    if !data_bytes.is_multiple_of(2) || 44 + data_bytes as usize > b.len() {
        return fail(format!("{}: data size past the end", path.display()));
    }
    Ok((0..data_bytes as usize / 2).map(|i| get16(&b[44 + 2 * i..]) as i16).collect())
}
