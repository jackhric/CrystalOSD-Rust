use crate::bytes::{Result, le32, le64};
use crate::elf_image::ElfImage;

// The program's addresses (HDD OSD 1.10U, References/model/sound_data.mjs CIPHER_TABLES): the expansion, P permutation,
// S-box, PC-1 and PC-2 tables, and the two key words do_load_resources reads (D_002AD970, D_002AD978).
const EXPANSION: u32 = 0x0036_d940;
const PERMUTATION: u32 = 0x0036_da40;
const SBOXES: u32 = 0x0036_dac0;
const PC1: u32 = 0x0036_dcc0;
const PC2: u32 = 0x0036_dd00;
const KEY_WORDS: u32 = 0x002a_d970;
// do_load_resources builds the key from its two words with this constant (sound_data.mjs imageKey).
const KEY_MIX: u64 = 0x2cba_db31_cccb_12f6;
const HIGH32: u64 = 0xffff_ffff_0000_0000;
const LOW32: u64 = 0xffff_ffff;

// The HDD OSD container decrypt of do_load_resources (HDD OSD 1.10U 0x0020AB98, the loop at 0x0020B030), ported from
// References/model/sound_data.mjs. The tables and the key words are the program's: read from the user's hddosd.elf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CipherTables {
    pub expansion: [u64; 32],
    pub permutation: [u32; 32],
    pub sboxes: [u8; 0x200],
    pub pc1: [u8; 0x38],
    pub pc2: [u8; 0x30],
    pub key: u64,
}

impl CipherTables {
    pub fn from_program(program: &ElfImage) -> Result<Self> {
        let expansion_bytes = program.bytes(EXPANSION, 0x100)?;
        let permutation_bytes = program.bytes(PERMUTATION, 0x80)?;
        let mut expansion = [0u64; 32];
        let mut permutation = [0u32; 32];
        for i in 0..32 {
            expansion[i] = le64(expansion_bytes, 8 * i)?;
            permutation[i] = le32(permutation_bytes, 4 * i)?;
        }
        let words = program.bytes(KEY_WORDS, 16)?;
        Ok(Self {
            expansion,
            permutation,
            sboxes: program.bytes(SBOXES, 0x200)?.try_into().unwrap(),
            pc1: program.bytes(PC1, 0x38)?.try_into().unwrap(),
            pc2: program.bytes(PC2, 0x30)?.try_into().unwrap(),
            key: (le64(words, 8)? ^ KEY_MIX).wrapping_sub(5).wrapping_sub(le64(words, 0)?),
        })
    }
}

fn initial_permutation(mut block: u64) -> u64 {
    let (mut high, mut low) = (0u64, 0u64);
    for byte in (1..=8).rev() {
        let unit = 1u64 << (byte - 1);
        if block & 0x01 != 0 {
            high |= unit << 32;
        }
        if block & 0x02 != 0 {
            low |= unit;
        }
        if block & 0x04 != 0 {
            high |= unit << 40;
        }
        if block & 0x08 != 0 {
            low |= unit << 8;
        }
        if block & 0x10 != 0 {
            high |= unit << 48;
        }
        if block & 0x20 != 0 {
            low |= unit << 16;
        }
        if block & 0x40 != 0 {
            high |= unit << 56;
        }
        if block & 0x80 != 0 {
            low |= unit << 24;
        }
        block >>= 8;
    }
    high | low
}

fn final_permutation(mut block: u64) -> u64 {
    let (mut high, mut low) = (0u64, 0u64);
    for byte in 0..8u32 {
        let unit = 1u64 << (((byte >> 2) ^ 1) | ((byte & 3) << 1));
        if block & 0x01 != 0 {
            high |= unit << 56;
        }
        if block & 0x02 != 0 {
            low |= unit << 48;
        }
        if block & 0x04 != 0 {
            high |= unit << 40;
        }
        if block & 0x08 != 0 {
            low |= unit << 32;
        }
        if block & 0x10 != 0 {
            high |= unit << 24;
        }
        if block & 0x20 != 0 {
            low |= unit << 16;
        }
        if block & 0x40 != 0 {
            high |= unit << 8;
        }
        if block & 0x80 != 0 {
            low |= unit;
        }
        block >>= 8;
    }
    high | low
}

fn expansion_of(t: &CipherTables, half: u64) -> u64 {
    (0..32).filter(|i| (half >> (32 + i)) & 1 != 0).fold(0, |out, i| out | t.expansion[i])
}

fn substitution(t: &CipherTables, mut value: u64) -> u64 {
    let mut out = 0u64;
    for r#box in 0..8 {
        let index = ((value & 0x20) | ((value >> 1) & 0xf) | ((value << 4) & 0x10)) as usize;
        out |= u64::from(t.sboxes[64 * r#box + index]) << (32 + 4 * r#box);
        value >>= 6;
    }
    out
}

fn permutation_of(t: &CipherTables, value: u64) -> u64 {
    (0..32).filter(|i| (value >> (32 + i)) & 1 != 0).fold(0u64, |out, i| out | u64::from(t.permutation[i])) << 32
}

fn rotate28(half: u64, by: u32) -> u64 {
    ((half << by) | (half >> (28 - by))) & 0x0fff_ffff
}

pub fn key_schedule(t: &CipherTables) -> [u64; 16] {
    let mut permuted = 0u64;
    for (i, &bit) in t.pc1.iter().enumerate() {
        if (t.key >> (bit & 63)) & 1 != 0 {
            permuted |= (1u64 << 63) >> i;
        }
    }
    let mut left = permuted >> 36;
    let mut right = (permuted >> 8) & 0x0fff_ffff;
    let mut keys = [0u64; 16];
    for (n, round) in (1..=16).rev().enumerate() {
        let by = if round >= 15 || round == 8 || round == 1 { 1 } else { 2 };
        left = rotate28(left, by);
        right = rotate28(right, by);
        let joined = (left << 36) | (right << 8);
        for (i, &bit) in t.pc2.iter().enumerate() {
            if (joined >> (bit & 63)) & 1 != 0 {
                keys[n] |= (1u64 << 63) >> i;
            }
        }
    }
    keys
}

pub fn decrypt_block(t: &CipherTables, keys: &[u64; 16], block: u64) -> u64 {
    let start = initial_permutation(block);
    let mut left = (start & LOW32) << 32;
    let mut right = start & HIGH32;
    let mut next = 0u64;
    for round in (0..16).rev() {
        let mixed = expansion_of(t, left) ^ keys[round];
        next = permutation_of(t, substitution(t, mixed >> 16)) ^ right;
        right = left;
        left = next;
    }
    final_permutation(next | (right >> 32))
}

// The loop at 0x0020B030: the first 64 of every 512 blocks of 8 bytes, when the file has more than 63.
pub fn decrypt_image(file: &[u8], t: &CipherTables) -> Vec<u8> {
    let keys = key_schedule(t);
    let mut out = file.to_vec();
    let blocks = file.len() >> 3;
    let mut base = 0;
    while blocks > base + 0x3f {
        for k in 0..0x40 {
            let at = 8 * (base + k);
            let cipher = u64::from_le_bytes(file[at..at + 8].try_into().unwrap());
            out[at..at + 8].copy_from_slice(&decrypt_block(t, &keys, cipher).to_le_bytes());
        }
        base += 0x200;
    }
    out
}
