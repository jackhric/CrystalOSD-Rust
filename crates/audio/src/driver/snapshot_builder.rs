use std::path::Path;

use assets::bytes::{le16, le32, read_file};
use assets::elf_image::ElfImage;
use assets::program::is_hdd_osd_110u;
use assets::resources::PROGRAM_NAME;

use super::memory::{IopMemory, LIBSD_BASE, OSDSND_BASE, OSDSND_SIZE, SOUND_IOP_BASE, Snapshot};
use crate::data::snd_image::{SOUND_MEMBERS, SndImage};
use crate::{Result, fail};

pub const LIBSD_MODULE_BASE: i64 = 0x8b830;
pub const LIBSD_IMAGE_SIZE: usize = 0x10130;
pub const SOUND_IOP_SIZE: usize = 0xf000;
pub const STAGING_BASE: i64 = 0xebc00;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundImages {
    pub snapshot: Snapshot,
    pub libsd: Vec<u8>,
    pub spu_ram: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagedMember {
    pub name: &'static str,
    pub address: i64,
}

pub const STAGED_MEMBERS: [StagedMember; 10] = [
    StagedMember { name: "SNDBOOTH", address: 0xebc00 },
    StagedMember { name: "SNDBOOTS", address: 0xecc00 },
    StagedMember { name: "SNDTNNLS", address: 0xedc00 },
    StagedMember { name: "SNDCLOKS", address: 0xeec00 },
    StagedMember { name: "SNDLOGOS", address: 0xefc00 },
    StagedMember { name: "SNDTM60S", address: 0xf0c00 },
    StagedMember { name: "SNDOSDDH", address: 0xf1c00 },
    StagedMember { name: "SNDTM30S", address: 0xf2c00 },
    StagedMember { name: "SNDWARNS", address: 0xf3c00 },
    StagedMember { name: "SNDRCLKS", address: 0xf4c00 },
];

const IOP_RELOCATABLE: u16 = 0xff80;
const ELF_MAGIC: u32 = 0x464c457f;
const PROGRAM_LOAD: u32 = 1;
const PROGRAM_IOP_MOD: u32 = 0x70000080;
const SECTION_REL: u32 = 9;
const IMPORT_MAGIC: u32 = 0x41e00000;
const EXPORT_MAGIC: u32 = 0x41c00000;
const JUMP_RETURN: u32 = 0x03e00008;
const RELOC_WORD: u32 = 2;
const RELOC_JUMP: u32 = 4;
const RELOC_HIGH: u32 = 5;
const RELOC_LOW: u32 = 6;

const OSDSND_NAME: &str = "sdr_driver";
const LIBSD_NAME: &str = "Sound_Device_Library";
const OSDSND_FILE: u32 = 37360;
const OSDSND_MEMORY: u32 = 42304;
const LIBSD_FILE: u32 = 18528;
const LIBSD_MEMORY: u32 = 18584;

#[derive(Clone, Copy)]
struct Relocation {
    offset: u32,
    kind: u32,
}

#[derive(Default)]
struct Irx {
    name: String,
    image: Vec<u8>,
    memory: u32,
    relocations: Vec<Relocation>,
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn parse_irx(elf: &[u8], at: usize) -> Result<Irx> {
    let f = &elf[at..];
    let phoff = le32(f, 28)?;
    let shoff = le32(f, 32)?;
    let phentsize = u32::from(le16(f, 42)?);
    let phnum = u32::from(le16(f, 44)?);
    let shentsize = u32::from(le16(f, 46)?);
    let shnum = u32::from(le16(f, 48)?);
    let mut irx = Irx::default();
    let mut loaded = false;
    let mut named = false;
    for i in 0..phnum {
        let p = phoff as usize + i as usize * phentsize as usize;
        let kind = le32(f, p)?;
        let offset = le32(f, p + 4)?;
        if kind == PROGRAM_LOAD && !loaded {
            let filesz = le32(f, p + 16)?;
            if offset as usize + filesz as usize > f.len() {
                return fail("IRX segment past the end");
            }
            irx.image = f[offset as usize..offset as usize + filesz as usize].to_vec();
            irx.memory = le32(f, p + 20)?;
            loaded = true;
        } else if kind == PROGRAM_IOP_MOD && !named {
            if offset as usize + 50 > f.len() {
                return fail("IRX module header past the end");
            }
            let mut k = 0;
            while k < 24 && f[offset as usize + 26 + k] != 0 {
                irx.name.push(char::from(f[offset as usize + 26 + k]));
                k += 1;
            }
            named = true;
        }
    }
    if !loaded || !named {
        return fail("not an IRX");
    }
    for i in 0..shnum {
        let s = shoff as usize + i as usize * shentsize as usize;
        if le32(f, s + 4)? != SECTION_REL {
            continue;
        }
        let offset = le32(f, s + 16)?;
        let size = le32(f, s + 20)?;
        let mut e = 0u32;
        while e.wrapping_add(8) <= size {
            irx.relocations
                .push(Relocation { offset: le32(f, offset.wrapping_add(e) as usize)?, kind: le32(f, offset.wrapping_add(e).wrapping_add(4) as usize)? & 0xff });
            e = e.wrapping_add(8);
        }
    }
    Ok(irx)
}

fn find_modules(elf: &[u8]) -> Vec<Irx> {
    let mut found = Vec::new();
    let mut at = 0;
    while at + 64 <= elf.len() {
        if le32(elf, at) == Ok(ELF_MAGIC)
            && le16(elf, at + 16) == Ok(IOP_RELOCATABLE)
            && let Ok(irx) = parse_irx(elf, at)
        {
            found.push(irx);
        }
        at += 1;
    }
    found
}

fn relocate(irx: &Irx, base: u32) -> Result<Vec<u8>> {
    let mut img = irx.image.clone();
    let mut high: Vec<usize> = Vec::new();
    for r in &irx.relocations {
        let a = r.offset as usize;
        if a + 4 > img.len() {
            continue;
        }
        let w = le32(&img, a)?;
        if r.kind == RELOC_WORD {
            put32(&mut img, a, w.wrapping_add(base));
        } else if r.kind == RELOC_JUMP {
            put32(&mut img, a, (w & 0xfc000000) | ((((w & 0x3ffffff) << 2).wrapping_add(base) >> 2) & 0x3ffffff));
        } else if r.kind == RELOC_HIGH {
            high.push(a);
        } else if r.kind == RELOC_LOW {
            let low = i32::from((w & 0xffff) as u16 as i16);
            let lo = (low.wrapping_add(base as i32) as u32) & 0xffff;
            for &h in &high {
                let hw = le32(&img, h)?;
                let n = ((hw & 0xffff) << 16).wrapping_add(low as u32).wrapping_add(base);
                put32(&mut img, h, (hw & 0xffff0000) | ((n.wrapping_add(0x8000) >> 16) & 0xffff));
            }
            high.clear();
            put32(&mut img, a, (w & 0xffff0000) | lo);
        }
    }
    Ok(img)
}

fn table_name(img: &[u8], at: usize) -> String {
    let mut name = String::new();
    let mut k = 0;
    while k < 8 && at + 12 + k < img.len() && img[at + 12 + k] != 0 {
        name.push(char::from(img[at + 12 + k]));
        k += 1;
    }
    name
}

fn resolve_imports(client: &mut [u8], provider: &[u8], library: &str) -> Result<()> {
    let mut exports = Vec::new();
    let mut at = 0;
    while at + 20 <= provider.len() {
        if le32(provider, at)? != EXPORT_MAGIC || table_name(provider, at) != library {
            at += 4;
            continue;
        }
        let mut p = at + 20;
        while p + 4 <= provider.len() && le32(provider, p)? != 0 {
            exports.push(le32(provider, p)?);
            p += 4;
        }
        break;
    }
    if exports.is_empty() {
        return fail("the sound library exports nothing");
    }
    let mut at = 0;
    while at + 20 <= client.len() {
        if le32(client, at)? != IMPORT_MAGIC || table_name(client, at) != library {
            at += 4;
            continue;
        }
        let mut p = at + 20;
        while p + 8 <= client.len() && le32(client, p)? == JUMP_RETURN {
            let index = (le32(client, p + 4)? & 0xffff) as usize;
            if index >= exports.len() {
                return fail("an import is past the export table");
            }
            put32(client, p, 0x08000000 | ((exports[index] >> 2) & 0x3ffffff));
            p += 8;
        }
        at += 4;
    }
    Ok(())
}

fn pick<'a>(modules: &'a [Irx], name: &str, file: u32, memory: u32) -> Result<&'a Irx> {
    let mut found: Option<&Irx> = None;
    for m in modules {
        if m.name != name {
            continue;
        }
        if found.is_some() {
            return fail(format!("two sound modules named {name}"));
        }
        found = Some(m);
    }
    let Some(found) = found else {
        return fail(format!("the program holds no {name} module"));
    };
    if found.image.len() != file as usize || found.memory != memory {
        return fail(format!("{name} is not HDD OSD 1.10U's module"));
    }
    Ok(found)
}

fn load(ram: &mut [u8], at: i64, data: &[u8]) -> Result<()> {
    if at < 0 || at as usize + data.len() > ram.len() {
        return fail("sound image outside IOP RAM");
    }
    ram[at as usize..at as usize + data.len()].copy_from_slice(data);
    Ok(())
}

fn round16(v: usize) -> usize {
    (v + 15) & !15
}

pub fn build_sound_images(program: &[u8], sound: &SndImage) -> Result<SoundImages> {
    if !is_hdd_osd_110u(&ElfImage::new(program)?) {
        return fail("the sound driver is read from HDD OSD 1.10U's program");
    }
    let modules = find_modules(program);
    let osdsnd = pick(&modules, OSDSND_NAME, OSDSND_FILE, OSDSND_MEMORY)?;
    let libsd = pick(&modules, LIBSD_NAME, LIBSD_FILE, LIBSD_MEMORY)?;

    let libsd_image = relocate(libsd, LIBSD_MODULE_BASE as u32)?;
    let mut osdsnd_image = relocate(osdsnd, OSDSND_BASE as u32)?;
    resolve_imports(&mut osdsnd_image, &libsd_image, "libsd")?;

    let mut ram = vec![0u8; IopMemory::SIZE];
    load(&mut ram, LIBSD_MODULE_BASE, &libsd_image)?;
    load(&mut ram, OSDSND_BASE, &osdsnd_image)?;

    let mut area: Vec<u8> = Vec::new();
    let mut body_start = 0;
    for name in SOUND_MEMBERS {
        let member = sound.member(name)?;
        let start = round16(area.len());
        area.resize(start + member.len(), 0);
        area[start..].copy_from_slice(member);
        if name == "SNDOSDDB" {
            body_start = start;
        }
    }
    let window = (SOUND_IOP_BASE + SOUND_IOP_SIZE as i64 - STAGING_BASE) as usize;
    let length = (area.len() - body_start).min(window);
    load(&mut ram, STAGING_BASE, &area[body_start..body_start + length])?;
    for m in &STAGED_MEMBERS {
        let mut padded = sound.member(m.name)?.clone();
        padded.resize(round16(padded.len()), 0);
        load(&mut ram, m.address, &padded)?;
    }

    Ok(SoundImages {
        snapshot: Snapshot {
            osdsnd: ram[OSDSND_BASE as usize..OSDSND_BASE as usize + OSDSND_SIZE].to_vec(),
            iop: ram[SOUND_IOP_BASE as usize..SOUND_IOP_BASE as usize + SOUND_IOP_SIZE].to_vec(),
        },
        libsd: ram[LIBSD_BASE as usize..LIBSD_BASE as usize + LIBSD_IMAGE_SIZE].to_vec(),
        spu_ram: sound.spu_ram()?,
    })
}

pub fn build_sound_images_from_folder(resources: &Path) -> Result<SoundImages> {
    let program = read_file(&resources.join(PROGRAM_NAME))?;
    build_sound_images(&program, &SndImage::from_folder(resources)?)
}
