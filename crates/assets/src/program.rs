use crate::bytes::{FNV_OFFSET, Result, hash_bytes};
use crate::elf_image::ElfImage;

// programDigest of HDD OSD 1.10U's hddosd.elf (SHA-1 e932f350...) and of its host copy alike.
const HDD_OSD_110U: u64 = 0xe0ea_e235_3761_70d2;
const HOST_CHANGES: [(u32, u32); 2] = [(0x002a_d240, 0x002a_d970), (0x0034_86b8, 0x0034_86be)];

fn mix(hash: u64, value: u32) -> u64 {
    hash_bytes(&value.to_le_bytes(), hash)
}

/// FNV-1a 64 over the entry point, the loadable segments' addresses and sizes, and their bytes but for the two ranges the
/// host copy changes (the resource table 0x002AD240..0x002AD970 and the device prefix 0x003486B8..0x003486BE).
pub fn program_digest(program: &ElfImage) -> Result<u64> {
    let mut hash = mix(FNV_OFFSET, program.entry());
    hash = mix(hash, program.segments().len() as u32);
    for s in program.segments() {
        hash = mix(mix(hash, s.address), s.size);
        let mut at = s.address;
        let end = s.address.wrapping_add(s.size);
        while at < end {
            let mut next = end;
            for &(from, to) in &HOST_CHANGES {
                if at >= from && at < to {
                    next = end.min(to);
                } else if from > at && from < next {
                    next = from;
                }
            }
            if !HOST_CHANGES.iter().any(|&(from, to)| at >= from && at < to) {
                hash = hash_bytes(program.bytes(at, next - at)?, hash);
            }
            at = next;
        }
    }
    Ok(hash)
}

/// Whether the ELF is HDD OSD 1.10U's program (the original or the host copy of make_hddosd_host.mjs), whose
/// addresses the decode and the text read.
pub fn is_hdd_osd_110u(program: &ElfImage) -> bool {
    program_digest(program) == Ok(HDD_OSD_110U)
}
