use crate::bytes::{Result, error, le16, le32};

// romdir_get_offset and romdir_search_entry (HDD OSD 1.10U 0x0020E418 / 0x0020E4B0), ported from
// References/model/sound_data.mjs romdirStart and romdirEntries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RomdirEntry {
    pub name: String,
    pub size: u32,
    pub offset: u32,
}

pub const DEFAULT_LIMIT: usize = 0x2000;

/// The RESET entry whose size rounded up to 16 equals its own offset, searched below `limit`.
pub fn romdir_start(image: &[u8], limit: usize) -> Option<usize> {
    let mut at = 0;
    while at < limit && at + 16 <= image.len() {
        let reset = le32(image, at) == Ok(0x4553_4552) && le32(image, at + 4) == Ok(0x54) && le16(image, at + 8) == Ok(0);
        if reset && le32(image, at + 12).is_ok_and(|size| (size.wrapping_add(15) & !15) as usize == at) {
            return Some(at);
        }
        at += 16;
    }
    None
}

pub fn romdir_entries(image: &[u8], start: usize) -> Result<Vec<RomdirEntry>> {
    let mut entries = Vec::new();
    let mut offset = 0u32;
    let mut at = start;
    loop {
        if at + 16 > image.len() {
            return error("ROMDIR: truncated");
        }
        if le32(image, at)? == 0 {
            break;
        }
        let name = &image[at..at + 10];
        let length = name.iter().position(|&b| b == 0).unwrap_or(10);
        let size = le32(image, at + 12)?;
        entries.push(RomdirEntry { name: String::from_utf8_lossy(&name[..length]).into_owned(), size, offset });
        offset = offset.wrapping_add(size.wrapping_add(15) & !15);
        at += 16;
    }
    Ok(entries)
}

pub fn romdir_find<'e>(entries: &'e [RomdirEntry], name: &str) -> Option<&'e RomdirEntry> {
    entries.iter().find(|e| e.name == name)
}

pub fn romdir_member<'a>(image: &'a [u8], entries: &[RomdirEntry], name: &str) -> Result<&'a [u8]> {
    let Some(entry) = romdir_find(entries, name) else {
        return error(format!("ROMDIR: no member {name}"));
    };
    let start = entry.offset as usize;
    match image.get(start..start + entry.size as usize) {
        Some(member) => Ok(member),
        None => error(format!("ROMDIR: member {name} runs past the image")),
    }
}
