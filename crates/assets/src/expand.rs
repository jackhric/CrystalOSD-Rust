use crate::bytes::{Result, be32, error, le32};

// The OSD's Expand (HDD OSD 1.10U 0x00200EE8, ROM 2.30 0x00200ED0), ported from References/model/sound_data.mjs
// expand: {u32 size, then blocks of a big-endian flag word and 30 literal or match items}.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expanded {
    pub size: u32,
    pub out: Vec<u8>,
    pub produced: usize,
    pub consumed: usize,
}

pub const DEFAULT_MAX_SIZE: usize = 1 << 24;

// References/model/sound_data.mjs expand: a match copies (w >> (14 - m)) + 3 bytes from (w & (0x3FFF >> m)) + 1 back;
// the loop stops once the output holds `size` bytes or more.
pub fn expand(src: &[u8], at: usize, max_size: usize) -> Result<Expanded> {
    let size = le32(src, at)?;
    // A match of two bytes gives at most 34 (mode 3: 31 + 3), so a stream cannot give more than 17 bytes per byte.
    if size as usize > max_size {
        return error(format!("Expand: the stream announces {size} bytes, more than {max_size}"));
    }
    if size as usize > (src.len() - at - 4) * 17 + 64 {
        return error("Expand: the stream announces more bytes than it can hold");
    }
    let mut out: Vec<u8> = Vec::with_capacity(size as usize + 64);
    let mut pos = at + 4;
    let (mut counter, mut flags, mut shift, mut mask) = (0u32, 0u32, 0u32, 0u32);
    let byte = |pos: &mut usize| -> Result<u8> {
        let Some(&value) = src.get(*pos) else {
            return error("Expand: the stream ends early");
        };
        *pos += 1;
        Ok(value)
    };
    loop {
        if counter == 0 {
            flags = be32(src, pos)?;
            pos += 4;
            let mode = flags & 3;
            mask = 0x3fff >> mode;
            shift = 14 - mode;
            counter = 30;
        }
        let first = byte(&mut pos)?;
        if flags & 0x8000_0000 != 0 {
            let word = u32::from(first) << 8 | u32::from(byte(&mut pos)?);
            let back = (word & mask) as usize + 1;
            if back > out.len() {
                return error(format!("Expand: a match reaches before the output (at {})", out.len()));
            }
            let start = out.len() - back;
            for from in start..start + (word >> shift) as usize + 3 {
                out.push(out[from]);
            }
        } else {
            out.push(first);
        }
        if out.len() >= size as usize {
            break;
        }
        counter -= 1;
        flags <<= 1;
    }
    let produced = out.len();
    out.truncate(size as usize);
    Ok(Expanded { size, out, produced, consumed: pos - at })
}
