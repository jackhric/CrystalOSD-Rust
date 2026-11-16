use std::collections::BTreeMap;
use std::path::Path;

use assets::bytes::read_file;
use assets::elf_image::ElfImage;
use assets::expand::{DEFAULT_MAX_SIZE, expand};
use assets::image_cipher::{CipherTables, decrypt_image};
use assets::resources::PROGRAM_NAME;
use assets::romdir::{DEFAULT_LIMIT, romdir_entries, romdir_member, romdir_start};

use crate::{Result, fail};

pub const SOUND_MEMBERS: [&str; 12] =
    ["SNDBOOTH", "SNDBOOTB", "SNDBOOTS", "SNDTNNLS", "SNDCLOKS", "SNDTM30S", "SNDTM60S", "SNDOSDDH", "SNDOSDDB", "SNDLOGOS", "SNDWARNS", "SNDRCLKS"];
pub const SPU_RAM_SIZE: u32 = 0x200000;
pub const BOOT_BODY_AT: u32 = 0x5010;
pub const OSDD_BODY_AT: u32 = 0x85010;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SndImage {
    members: BTreeMap<String, Vec<u8>>,
}

impl SndImage {
    pub fn from_container(container: &[u8]) -> Result<SndImage> {
        let Some(start) = romdir_start(container, DEFAULT_LIMIT) else {
            return fail("SNDIMAGE holds no ROMDIR");
        };
        let entries = romdir_entries(container, start)?;
        let mut image = SndImage::default();
        for name in SOUND_MEMBERS {
            let mut raw = expand(romdir_member(container, &entries, name)?, 0, DEFAULT_MAX_SIZE)?;
            raw.out.resize(raw.size as usize, 0);
            image.members.insert(name.to_owned(), raw.out);
        }
        Ok(image)
    }

    pub fn from_folder(folder: &Path) -> Result<SndImage> {
        let mut container = read_file(&folder.join("SNDIMAGE"))?;
        if romdir_start(&container, DEFAULT_LIMIT).is_none() {
            let program = read_file(&folder.join(PROGRAM_NAME))?;
            container = decrypt_image(&container, &CipherTables::from_program(&ElfImage::new(&program)?)?);
        }
        Self::from_container(&container)
    }

    pub fn member(&self, name: &str) -> Result<&Vec<u8>> {
        match self.members.get(name) {
            Some(m) => Ok(m),
            None => fail(format!("no sound member {name}")),
        }
    }

    pub fn spu_ram(&self) -> Result<Vec<u8>> {
        let mut ram = vec![0u8; SPU_RAM_SIZE as usize];
        let boot = self.member("SNDBOOTB")?;
        let osdd = self.member("SNDOSDDB")?;
        if BOOT_BODY_AT as usize + boot.len() > SPU_RAM_SIZE as usize || OSDD_BODY_AT as usize + osdd.len() > SPU_RAM_SIZE as usize {
            return fail("a sound body does not fit the SPU2 RAM");
        }
        ram[BOOT_BODY_AT as usize..BOOT_BODY_AT as usize + boot.len()].copy_from_slice(boot);
        ram[OSDD_BODY_AT as usize..OSDD_BODY_AT as usize + osdd.len()].copy_from_slice(osdd);
        Ok(ram)
    }
}
