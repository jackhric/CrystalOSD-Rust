use std::path::Path;

use crate::bytes::{Error, Result, error, le16, le32, read_file};
use crate::elf_image::ElfImage;
use crate::expand::expand;
use crate::image_cipher::{CipherTables, decrypt_image};
use crate::program::is_hdd_osd_110u;
use crate::resources::PROGRAM_NAME;
use crate::romdir::{DEFAULT_LIMIT, romdir_entries, romdir_member, romdir_start};

// The ICOIMAGE members, in the order of browser_icon_indices (HDD OSD 1.10U 0x002BD180: resources 62..77); ROM 2.30 has the first 13.
pub const ICON_NAMES: [&str; 16] = [
    "ICOBDISC", "ICOBCDDA", "ICOBPS1M", "ICOBPS2M", "ICOBPKST", "ICOBPS1D", "ICOBPS2D", "ICOBDVDD", "ICOBYSYS", "ICOBFSCE", "ICOBFNOR", "ICOBFBRK", "ICOBQUES",
    "ICOBFHDD", "ICOBFDIR", "ICOBFFIL",
];
pub const ICO_LIMIT: usize = 0x51000;
pub const TEXTURE_BYTES: usize = 0x8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcoVertex {
    pub positions: Vec<[i16; 3]>,
    pub normal: [i16; 3],
    pub uv: [i16; 2],
    pub colour: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct IcoShapeTrack {
    pub shape: u32,
    pub keys: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IcoAnimation {
    pub length: u32,
    pub speed: f32,
    pub offset: u32,
    pub tracks: Vec<IcoShapeTrack>,
}

// A 128 x 128 PSMCT16 texture (A1B5G5R5), present when flag bit 2 is set.
#[derive(Debug, Clone, PartialEq)]
pub struct Ico {
    pub shapes: u32,
    pub flags: u32,
    pub header_float: f32,
    pub vertices: Vec<IcoVertex>,
    pub animation: IcoAnimation,
    pub texture: Option<Vec<u8>>,
}

fn le_f32(d: &[u8], at: usize) -> Result<f32> {
    le32(d, at).map(f32::from_bits)
}

fn le_i16(d: &[u8], at: usize) -> Result<i16> {
    le16(d, at).map(|v| v as i16)
}

// func_0023BDC8: signed halfwords n: n > 0 repeats the next halfword n times, n < 0 copies -n halfwords, 0 skips one.
pub fn rle16(src: &[u8], bytes: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(TEXTURE_BYTES);
    let halfwords = bytes / 2;
    let mut i = 0;
    while i < halfwords {
        let n = le_i16(src, 2 * i)?;
        i += 1;
        if n > 0 {
            let value = le16(src, 2 * i)?;
            i += 1;
            for _ in 0..n {
                out.extend_from_slice(&value.to_le_bytes());
            }
        } else if n < 0 {
            for _ in 0..n.unsigned_abs() {
                out.extend_from_slice(&le16(src, 2 * i)?.to_le_bytes());
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    Ok(out)
}

fn vertex_limit(shapes: u32) -> Option<usize> {
    match shapes {
        1 => Some(0x708),
        2 => Some(0x672),
        4 => Some(0x5dc),
        6 => Some(0x546),
        8 => Some(0x4b0),
        _ => None,
    }
}

// func_0023AA08: one animation; its frame length, speed, play offset and up to 8 shape tracks of up to 10 (time, value) keys.
fn parse_animation(d: &[u8], at: usize, shapes: u32) -> Result<(IcoAnimation, usize)> {
    if le32(d, at)? != 1 {
        return error("ico: the animation count is not 1");
    }
    let length = le32(d, at + 4)?;
    let speed = le_f32(d, at + 8)?;
    let offset = le32(d, at + 12)?;
    let count = le32(d, at + 16)?;
    if count > 8 {
        return error("ico: more than 8 shape tracks");
    }
    let mut p = at + 20;
    let mut tracks = Vec::new();
    for _ in 0..count {
        let shape = le32(d, p)?;
        let keys = le32(d, p + 4)?;
        if shape > shapes || keys > 10 {
            return error("ico: a shape track out of range");
        }
        p += 8;
        let mut list = Vec::new();
        for _ in 0..keys {
            list.push((le_f32(d, p)?, le_f32(d, p + 4)?));
            p += 8;
        }
        tracks.push(IcoShapeTrack { shape, keys: list });
    }
    Ok((IcoAnimation { length, speed, offset, tracks }, p))
}

// module_browser_23B130 (HDD OSD 1.10U 0x0023B130): the header, the vertex records, the animation, the texture, with the checks
// the program makes.
pub fn parse_ico(d: &[u8]) -> Result<Ico> {
    if le32(d, 0)? != 0x0001_0000 {
        return error("ico: no magic 0x00010000");
    }
    let shapes = le32(d, 4)?;
    let Some(limit) = vertex_limit(shapes) else { return error("ico: a shape count other than 1, 2, 4, 6, 8") };
    let flags = le32(d, 8)?;
    let header_float = le_f32(d, 12)?;
    if header_float < 0.0 {
        return error("ico: a negative header float");
    }
    let count = le32(d, 16)? as usize;
    let stride = 8 * shapes as usize + 16;
    if !count.is_multiple_of(3) || count > limit || count * stride > ICO_LIMIT - 0x14 {
        return error("ico: a vertex count the program refuses");
    }
    let mut vertices = Vec::with_capacity(count);
    for v in 0..count {
        let r = 0x14 + v * stride;
        let mut positions = Vec::with_capacity(shapes as usize);
        for k in 0..shapes as usize {
            let p = r + 8 * k;
            positions.push([le_i16(d, p)?, le_i16(d, p + 2)?, le_i16(d, p + 4)?]);
        }
        let n = r + 8 * shapes as usize;
        vertices.push(IcoVertex {
            positions,
            normal: [le_i16(d, n)?, le_i16(d, n + 2)?, le_i16(d, n + 4)?],
            uv: [le_i16(d, n + 8)?, le_i16(d, n + 10)?],
            colour: [d[n + 12], d[n + 13], d[n + 14], d[n + 15]],
        });
    }
    let (animation, after) = parse_animation(d, 0x14 + count * stride, shapes)?;
    let texture = if (flags >> 2) & 1 == 0 {
        None
    } else if (flags >> 3) & 1 == 0 {
        Some(d.get(after..after + TEXTURE_BYTES).ok_or_else(|| Error("ico: the texture runs past the end".into()))?.to_vec())
    } else {
        let size = le32(d, after)? as usize;
        let src = d.get(after + 4..after + 4 + size).ok_or_else(|| Error("ico: the compressed texture runs past the end".into()))?;
        let texture = rle16(src, size)?;
        if texture.len() != TEXTURE_BYTES {
            return error("ico: the compressed texture is not 0x8000 bytes");
        }
        Some(texture)
    };
    Ok(Ico { shapes, flags, header_float, vertices, animation, texture })
}

// The ICOB members of ICOIMAGE (plain, or decrypted with the tables of the hddosd.elf beside it), expanded and parsed, by name.
pub fn decode_icons(folder: &Path) -> Result<Vec<(String, Ico)>> {
    let mut image = read_file(&folder.join("ICOIMAGE"))?;
    if romdir_start(&image, DEFAULT_LIMIT).is_none() {
        let program = read_file(&folder.join(PROGRAM_NAME))?;
        let Some(elf) = ElfImage::new(&program).ok().filter(is_hdd_osd_110u) else {
            return error("ICOIMAGE is encrypted: HDD OSD 1.10U's hddosd.elf is needed beside it");
        };
        image = decrypt_image(&image, &CipherTables::from_program(&elf)?);
    }
    let Some(start) = romdir_start(&image, DEFAULT_LIMIT) else {
        return error("ICOIMAGE holds no directory");
    };
    let entries = romdir_entries(&image, start)?;
    let mut icons = Vec::new();
    for name in ICON_NAMES {
        let Ok(member) = romdir_member(&image, &entries, name) else { continue };
        let expanded = expand(member, 0, ICO_LIMIT)?;
        icons.push((name.to_owned(), parse_ico(&expanded.out[..expanded.size as usize])?));
    }
    Ok(icons)
}
