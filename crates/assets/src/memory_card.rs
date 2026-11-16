// facts/memory-card-image.md: the raw PS2 memory card image (HDD OSD 1.10U's Format writes it), read only. A port of the lab's
// reader tools/mc/read_card.mjs and tools/mc/ecc.mjs.

use std::path::Path;

use crate::bytes::{Result, error};

pub const RAW_PAGE: usize = 528;
pub const PAGE: usize = 512;
pub const RAW_SIZE: usize = 8_650_752;
pub const MAGIC: &[u8; 28] = b"Sony PS2 Memory Card Format ";

// The console's library counts free FAT entries below this cluster (measured: hddosd-110U-browser-devices-card, -free-fat10,
// -free-hi10, -free-w170, -free-w170b; not a verifier).
pub const LIBRARY_COUNTED_CLUSTERS: usize = 8001;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Superblock {
    pub version: String,
    pub page_len: u16,
    pub pages_per_cluster: u16,
    pub pages_per_block: u16,
    pub clusters_per_card: u32,
    pub alloc_offset: u32,
    pub alloc_end: u32,
    pub rootdir_cluster: u32,
    pub backup_block1: u32,
    pub backup_block2: u32,
    pub ifc_list: [u32; 32],
    pub card_type: u8,
    pub card_flags: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Time {
    pub sec: u8,
    pub min: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub mode: u16,
    pub length: u32,
    pub created: Time,
    pub cluster: u32,
    pub dir_entry: u32,
    pub modified: Time,
    pub attr: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardInfo {
    None,
    Unformatted,
    Formatted { free_clusters: u32, cluster_bytes: u32, free_kb: u32, root_entries: u32, root_names: Vec<String> },
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}

fn latin1_until_nul(d: &[u8]) -> String {
    d.iter().take_while(|&&b| b != 0).map(|&b| char::from(b)).collect()
}

pub fn page_data(image: &[u8], page: usize) -> &[u8] {
    &image[page * RAW_PAGE..page * RAW_PAGE + PAGE]
}

pub fn page_spare(image: &[u8], page: usize) -> &[u8] {
    &image[page * RAW_PAGE + PAGE..(page + 1) * RAW_PAGE]
}

// Section 4: per 128-byte chunk 3 bytes (column parity, line parity 0, line parity 1), initial 0x77, 0x7F, 0x7F; the column
// parities on bits 0, 1, 2, 4, 5, 6 as c1, c3, c5, c0, c2, c4.
fn column_mask(b: u8) -> u8 {
    let bit = |n: u32| (b >> n) & 1;
    let c0 = bit(1) ^ bit(3) ^ bit(5) ^ bit(7);
    let c1 = bit(0) ^ bit(2) ^ bit(4) ^ bit(6);
    let c2 = bit(2) ^ bit(3) ^ bit(6) ^ bit(7);
    let c3 = bit(0) ^ bit(1) ^ bit(4) ^ bit(5);
    let c4 = bit(4) ^ bit(5) ^ bit(6) ^ bit(7);
    let c5 = bit(0) ^ bit(1) ^ bit(2) ^ bit(3);
    c1 | (c3 << 1) | (c5 << 2) | (c0 << 4) | (c2 << 5) | (c4 << 6)
}

pub fn ecc_chunk(chunk: &[u8]) -> [u8; 3] {
    let (mut column, mut line0, mut line1) = (0x77u8, 0x7fu8, 0x7fu8);
    for (i, &b) in chunk.iter().enumerate() {
        column ^= column_mask(b);
        if b.count_ones() & 1 == 1 {
            line0 ^= !(i as u8) & 0x7f;
            line1 ^= i as u8;
        }
    }
    [column, line0 & 0x7f, line1]
}

pub fn ecc_page(data: &[u8]) -> Vec<u8> {
    data.chunks(128).flat_map(ecc_chunk).collect()
}

// Section 2: the superblock in the data of page 0, or None without the magic (an unformatted card).
pub fn superblock(image: &[u8]) -> Option<Superblock> {
    let d = page_data(image, 0);
    if &d[..28] != MAGIC {
        return None;
    }
    Some(Superblock {
        version: latin1_until_nul(&d[0x1c..0x28]),
        page_len: u16_at(d, 0x28),
        pages_per_cluster: u16_at(d, 0x2a),
        pages_per_block: u16_at(d, 0x2c),
        clusters_per_card: u32_at(d, 0x30),
        alloc_offset: u32_at(d, 0x34),
        alloc_end: u32_at(d, 0x38),
        rootdir_cluster: u32_at(d, 0x3c),
        backup_block1: u32_at(d, 0x40),
        backup_block2: u32_at(d, 0x44),
        ifc_list: std::array::from_fn(|i| u32_at(d, 0x50 + 4 * i)),
        card_type: d[0x150],
        card_flags: d[0x151],
    })
}

pub fn cluster_bytes(image: &[u8], sb: &Superblock, cluster: u32) -> Vec<u8> {
    let per = sb.pages_per_cluster as usize;
    (0..per).flat_map(|p| page_data(image, cluster as usize * per + p).to_vec()).collect()
}

// Section 3: the FAT through the indirect FAT clusters; an entry with the top bit clear is free, set is the next cluster,
// 0xFFFFFFFF the last.
pub fn fat(image: &[u8], sb: &Superblock) -> Vec<u32> {
    let per_cluster = sb.pages_per_cluster as usize * PAGE / 4;
    let mut entries = Vec::new();
    for &ifc in &sb.ifc_list {
        if ifc == 0xffff_ffff || ifc == 0 {
            continue;
        }
        let table = cluster_bytes(image, sb, ifc);
        for i in 0..per_cluster {
            let fat_cluster = u32_at(&table, i * 4);
            if fat_cluster == 0xffff_ffff {
                continue;
            }
            let bytes = cluster_bytes(image, sb, fat_cluster);
            entries.extend((0..per_cluster).map(|j| u32_at(&bytes, j * 4)));
        }
    }
    entries
}

pub fn chain(table: &[u32], first: u32) -> Vec<u32> {
    let mut seen: Vec<u32> = Vec::new();
    let mut cluster = first;
    while !seen.contains(&cluster) {
        seen.push(cluster);
        let Some(&next) = table.get(cluster as usize) else { break };
        if next & 0x8000_0000 == 0 || next == 0xffff_ffff {
            break;
        }
        cluster = next & 0x7fff_ffff;
    }
    seen
}

fn time_at(e: &[u8], at: usize) -> Time {
    Time { sec: e[at + 1], min: e[at + 2], hour: e[at + 3], day: e[at + 4], month: e[at + 5], year: u16_at(e, at + 6) }
}

pub fn entry_at(buffer: &[u8], offset: usize) -> Entry {
    let e = &buffer[offset..offset + PAGE];
    Entry {
        mode: u16_at(e, 0),
        length: u32_at(e, 4),
        created: time_at(e, 8),
        cluster: u32_at(e, 0x10),
        dir_entry: u32_at(e, 0x14),
        modified: time_at(e, 0x18),
        attr: u32_at(e, 0x20),
        name: latin1_until_nul(&e[0x40..0x60]),
    }
}

// The entries of a directory: its chain's clusters, counted from the alloc offset, 512 bytes an entry, `count` of them.
pub fn directory(image: &[u8], sb: &Superblock, table: &[u32], first: u32, count: usize) -> Vec<Entry> {
    let bytes: Vec<u8> = chain(table, first).into_iter().flat_map(|c| cluster_bytes(image, sb, sb.alloc_offset + c)).collect();
    (0..count).take_while(|i| (i + 1) * PAGE <= bytes.len()).map(|i| entry_at(&bytes, i * PAGE)).collect()
}

// Section 5: none, unformatted (no magic), or formatted with the free clusters counted from the FAT below the alloc end, the
// cluster size, the free kilobytes, the root's entry count (the length of `.`) and its names. The rule of the image itself.
pub fn info(image: Option<&[u8]>) -> CardInfo {
    let Some(image) = image else { return CardInfo::None };
    let Some(sb) = superblock(image) else { return CardInfo::Unformatted };
    let table = fat(image, &sb);
    let free = table.iter().take(sb.alloc_end as usize).filter(|&&e| e & 0x8000_0000 == 0).count() as u32;
    let entries = u32_at(&cluster_bytes(image, &sb, sb.alloc_offset + sb.rootdir_cluster), 4);
    let names = directory(image, &sb, &table, sb.rootdir_cluster, entries as usize).into_iter().map(|e| e.name).collect();
    let cluster = u32::from(sb.pages_per_cluster) * u32::from(sb.page_len);
    CardInfo::Formatted { free_clusters: free, cluster_bytes: cluster, free_kb: free * cluster / 1024, root_entries: entries, root_names: names }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EccReport {
    pub pages: usize,
    pub bad_pages: Vec<usize>,
}

// The programmed pages (not 0xFF throughout, data and spare) and those whose spare's twelve ECC bytes differ from the data's.
pub fn ecc_report(image: &[u8]) -> EccReport {
    let mut pages = 0;
    let mut bad_pages = Vec::new();
    for p in 0..image.len() / RAW_PAGE {
        let (data, spare) = (page_data(image, p), page_spare(image, p));
        if data.iter().all(|&x| x == 0xff) && spare.iter().all(|&x| x == 0xff) {
            continue;
        }
        pages += 1;
        if ecc_page(data) != spare[..12] {
            bad_pages.push(p);
        }
    }
    EccReport { pages, bad_pages }
}

// The free count the console's library gives sceMcGetInfo for a formatted image (measured, see LIBRARY_COUNTED_CLUSTERS): the
// free FAT entries among clusters 0..8000. None for an unformatted image (the library gives type 2, free 0).
pub fn library_free(image: &[u8]) -> Option<i32> {
    let sb = superblock(image)?;
    let table = fat(image, &sb);
    Some(table.iter().take(LIBRARY_COUNTED_CLUSTERS).filter(|&&e| e & 0x8000_0000 == 0).count() as i32)
}

pub fn read_image(path: &Path) -> Result<Vec<u8>> {
    let image = std::fs::read(path).map_err(|e| crate::bytes::Error(format!("{}: {e}", path.display())))?;
    if image.len() != RAW_SIZE {
        return error(format!("{}: {} bytes, a card image has {RAW_SIZE} (16384 pages of 528)", path.display(), image.len()));
    }
    Ok(image)
}
