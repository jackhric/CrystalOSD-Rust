use std::env;
use std::path::{Path, PathBuf};

use crate::bytes::{Error, Result, error, hash_of, read_file, write_file};
use crate::clock_textures::{BROWSER_TEXTURE_HEADER, BROWSER_TEXTURES, CLOCK_TEXTURES, OPENING_TEXTURES, PixelForm, convert_texture, raw_texture_size};
use crate::elf_image::ElfImage;
use crate::expand::expand;
use crate::image_cipher::{CipherTables, decrypt_image};
use crate::program::is_hdd_osd_110u;
use crate::romdir::{DEFAULT_LIMIT, romdir_entries, romdir_find, romdir_member, romdir_start};

/// The resource files do_load_resources reads (facts/hddosd-boot.md), by the names the OSD gives them, and the program.
pub const RESOURCE_NAMES: [&str; 6] = ["FNTOSD", "JISUCS", "SNDIMAGE", "TEXIMAGE", "ICOIMAGE", "SKBIMAGE"];
pub const PROGRAM_NAME: &str = "hddosd.elf";

const DECODED: [&str; 4] = ["TEXIMAGE", "FNTOSD", PROGRAM_NAME, "SNDIMAGE"];

// The rod mesh in HDD OSD 1.10U's data (facts/data/rod-mesh.json): 64 positions, 16 normals, 64 texture coordinates.
const FACES: u32 = 16;
const POSITIONS: u32 = 0x002b_4b90;
const NORMALS: u32 = 0x002b_5390;
const COORDINATES: u32 = 0x002b_4f90;

// The menus' cube mesh in the same data: 6 faces.
const CUBE_FACES: u32 = 6;
const CUBE_POSITIONS: u32 = 0x002b_5790;
const CUBE_NORMALS: u32 = 0x002b_5910;
const CUBE_COORDINATES: u32 = 0x002b_5970;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AssetKind {
    TextureRgba32 = 1,
    TextureIndexed = 2,
    Font = 3,
    Program = 4,
    Mesh = 5,
    SoundContainer = 6,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub hash: u64,
}

/// Mesh data: the rod's 64 positions, 16 normals and 64 texture coordinates, four floats each; width = faces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub kind: AssetKind,
    pub width: u32,
    pub height: u32,
    pub source: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetSet {
    pub sources: Vec<SourceFile>,
    pub assets: Vec<Asset>,
}

impl AssetSet {
    pub fn find(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }

    pub fn source_of(&self, asset: &Asset) -> &SourceFile {
        &self.sources[asset.source as usize]
    }
}

struct ReadFile {
    source: SourceFile,
    data: Vec<u8>,
}

fn read_sources(folder: &Path) -> Result<Vec<ReadFile>> {
    let mut files = Vec::new();
    for name in DECODED {
        let path = folder.join(name);
        if !path.is_file() {
            continue;
        }
        let data = read_file(&path)?;
        let absolute = std::path::absolute(&path).unwrap_or(path);
        let source = SourceFile { name: name.to_owned(), path: absolute.to_string_lossy().replace('\\', "/"), size: data.len() as u64, hash: hash_of(&data) };
        files.push(ReadFile { source, data });
    }
    Ok(files)
}

/// The files of `folder` the decode reads (TEXIMAGE, FNTOSD, hddosd.elf, those present), hashed.
pub fn folder_sources(folder: &Path) -> Result<Vec<SourceFile>> {
    Ok(read_sources(folder)?.into_iter().map(|f| f.source).collect())
}

fn texture(container: &[u8], entries: &[crate::romdir::RomdirEntry], name: &str, width: u32, height: u32, form: PixelForm, source: u32) -> Result<Asset> {
    let need = raw_texture_size(form, width, height);
    let raw = expand(romdir_member(container, entries, name)?, 0, need)?;
    if raw.size as usize != need {
        return error(format!("{name}: {} bytes, its form needs {need}", raw.size));
    }
    Ok(Asset { name: name.to_owned(), kind: AssetKind::TextureRgba32, width, height, source, data: convert_texture(&raw.out, width, height, form)? })
}

/// The clock's assets from a folder of raw resource files: TEXIMAGE plain (a ROM's) or encrypted (an installed HDD
/// OSD's, decrypted with the tables of the hddosd.elf beside it); FNTOSD and hddosd.elf as they are; the rod mesh
/// from hddosd.elf.
pub fn decode_folder(folder: &Path) -> Result<AssetSet> {
    let files = read_sources(folder)?;
    let index = |name: &str| files.iter().position(|f| f.source.name == name).map(|i| i as u32);
    let mut set = AssetSet { sources: files.iter().map(|f| f.source.clone()).collect(), assets: Vec::new() };
    let Some(tex_image) = index("TEXIMAGE") else {
        return error(format!("no TEXIMAGE in {}", folder.display()));
    };
    // Only HDD OSD 1.10U's program holds the tables, the mesh and the text data at the addresses read here.
    let mut program = index(PROGRAM_NAME);
    let mut elf = None;
    let mut refused = String::new();
    if let Some(p) = program {
        elf = ElfImage::new(&files[p as usize].data).ok().filter(is_hdd_osd_110u);
        if elf.is_none() {
            refused = "hddosd.elf is not HDD OSD 1.10U".to_owned();
            eprintln!("assets: {refused}: its mesh and text are not read");
            program = None;
        }
    }

    // A ROM's TEXIMAGE is a plain ROMDIR archive; an installed HDD OSD's is encrypted (facts/hddosd-boot.md).
    let mut container = files[tex_image as usize].data.clone();
    if romdir_start(&container, DEFAULT_LIMIT).is_none() {
        let Some(elf) = &elf else {
            return error(if refused.is_empty() {
                "TEXIMAGE is encrypted: its hddosd.elf is needed beside it".to_owned()
            } else {
                format!("{refused}: the encrypted TEXIMAGE cannot be read")
            });
        };
        container = decrypt_image(&container, &CipherTables::from_program(elf)?);
    }
    let Some(start) = romdir_start(&container, DEFAULT_LIMIT) else {
        return error("TEXIMAGE holds no directory, plain or decrypted with the tables of hddosd.elf");
    };
    let entries = romdir_entries(&container, start)?;
    for info in &CLOCK_TEXTURES {
        set.assets.push(texture(&container, &entries, info.name, info.width, info.height, info.form, tex_image)?);
    }
    for info in &OPENING_TEXTURES {
        set.assets.push(texture(&container, &entries, info.name, info.width, info.height, info.form, tex_image)?);
    }
    if let Some(font) = index("FNTOSD") {
        set.assets.push(Asset { name: "FNTOSD".into(), kind: AssetKind::Font, width: 0, height: 0, source: font, data: files[font as usize].data.clone() });
    }
    if let (Some(program), Some(elf)) = (program, &elf) {
        set.assets.push(Asset {
            name: "PROGRAM".into(),
            kind: AssetKind::Program,
            width: 0,
            height: 0,
            source: program,
            data: files[program as usize].data.clone(),
        });
        let mesh_of = |faces: u32, positions: u32, normals: u32, coordinates: u32| -> Result<Vec<u8>> {
            let mut mesh = Vec::new();
            for (address, count) in [(positions, faces * 4), (normals, faces), (coordinates, faces * 4)] {
                mesh.extend_from_slice(elf.bytes(address, count * 16)?);
            }
            Ok(mesh)
        };
        set.assets.push(Asset {
            name: "RODMESH".into(),
            kind: AssetKind::Mesh,
            width: FACES,
            height: 0,
            source: program,
            data: mesh_of(FACES, POSITIONS, NORMALS, COORDINATES)?,
        });
        set.assets.push(Asset {
            name: "CUBEMESH".into(),
            kind: AssetKind::Mesh,
            width: CUBE_FACES,
            height: 0,
            source: program,
            data: mesh_of(CUBE_FACES, CUBE_POSITIONS, CUBE_NORMALS, CUBE_COORDINATES)?,
        });
    }
    if let Some(sound) = index("SNDIMAGE") {
        let mut sounds = files[sound as usize].data.clone();
        let mut readable = romdir_start(&sounds, DEFAULT_LIMIT).is_some();
        if !readable && let Some(elf) = &elf {
            sounds = decrypt_image(&sounds, &CipherTables::from_program(elf)?);
            readable = romdir_start(&sounds, DEFAULT_LIMIT).is_some();
        }
        if readable {
            set.assets.push(Asset { name: "SNDIMAGE".into(), kind: AssetKind::SoundContainer, width: 0, height: 0, source: sound, data: sounds });
        } else {
            eprintln!("assets: SNDIMAGE holds no directory, plain or decrypted: the sound is not read");
        }
    }
    Ok(set)
}

// facts/browser-chrome.md section 3: the Browser's eight TEXB sheets from the same TEXIMAGE (plain, or decrypted with the tables of
// the hddosd.elf beside it), kept apart from decode_folder's set.
pub fn decode_browser_sheets(folder: &Path) -> Result<Vec<Asset>> {
    let files = read_sources(folder)?;
    let index = |name: &str| files.iter().position(|f| f.source.name == name);
    let Some(tex_image) = index("TEXIMAGE") else {
        return error(format!("no TEXIMAGE in {}", folder.display()));
    };
    let mut container = files[tex_image].data.clone();
    if romdir_start(&container, DEFAULT_LIMIT).is_none() {
        let Some(elf) = index(PROGRAM_NAME).and_then(|p| ElfImage::new(&files[p].data).ok()).filter(is_hdd_osd_110u) else {
            return error("TEXIMAGE is encrypted: HDD OSD 1.10U's hddosd.elf is needed beside it");
        };
        container = decrypt_image(&container, &CipherTables::from_program(&elf)?);
    }
    let Some(start) = romdir_start(&container, DEFAULT_LIMIT) else {
        return error("TEXIMAGE holds no directory");
    };
    let entries = romdir_entries(&container, start)?;
    let mut sheets = Vec::new();
    for info in &BROWSER_TEXTURES {
        let need = BROWSER_TEXTURE_HEADER + (info.width * info.height * 4) as usize;
        let raw = expand(romdir_member(&container, &entries, info.name)?, 0, need + 64)?;
        if (raw.size as usize) < need {
            return error(format!("{}: {} bytes, a {} x {} sheet needs {need}", info.name, raw.size, info.width, info.height));
        }
        let data = raw.out[BROWSER_TEXTURE_HEADER..need].to_vec();
        sheets.push(Asset {
            name: info.name.to_owned(),
            kind: AssetKind::TextureRgba32,
            width: info.width,
            height: info.height,
            source: tex_image as u32,
            data,
        });
    }
    Ok(sheets)
}

/// Writes the BIOS ROM's members that bear a resource name, bytes unchanged, into `folder`; a file already there
/// with other bytes is refused. Returns the names written or found equal.
pub fn extract_bios(rom: &Path, folder: &Path) -> Result<Vec<String>> {
    let image = read_file(rom)?;
    let Some(start) = romdir_start(&image, 0x10_0000) else {
        return error(format!("{} is not a BIOS ROM image: no ROMDIR", rom.display()));
    };
    let entries = romdir_entries(&image, start)?;
    // Every target is checked before any is written: a refusal leaves the folder as it was.
    let mut members = Vec::new();
    let mut names = Vec::new();
    for name in RESOURCE_NAMES {
        if romdir_find(&entries, name).is_none() {
            continue;
        }
        let member = romdir_member(&image, &entries, name)?;
        let path = folder.join(name);
        if path.exists() {
            if read_file(&path)? != member {
                return Err(Error(format!("{} exists with other bytes: nothing extracted", path.display())));
            }
        } else {
            members.push((path, member));
        }
        names.push(name.to_owned());
    }
    if names.is_empty() {
        return error(format!("{} holds none of the OSD's resource files", rom.display()));
    }
    for (path, member) in members {
        write_file(&path, member)?;
    }
    Ok(names)
}

/// %LOCALAPPDATA%/CrystalOSD (or $XDG_CACHE_HOME, ~/.cache), else ./cache: where settings.json lives.
pub fn user_data_directory() -> PathBuf {
    let variable = |name: &str| env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(base) = variable("LOCALAPPDATA").or_else(|| variable("XDG_CACHE_HOME")) {
        return base.join("CrystalOSD");
    }
    if let Some(base) = variable("HOME") {
        return base.join(".cache").join("CrystalOSD");
    }
    env::current_dir().unwrap_or_default().join("cache")
}
