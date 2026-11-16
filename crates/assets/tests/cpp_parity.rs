use std::path::PathBuf;

use assets::bytes::hash_of;
use assets::elf_image::ElfImage;
use assets::image_cipher::{CipherTables, decrypt_block, key_schedule};
use assets::program::program_digest;
use assets::resources::decode_folder;

fn resources() -> Option<PathBuf> {
    let folder = std::env::var_os("CRYSTALOSD_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../CrystalOSD/Resources"));
    if folder.join("TEXIMAGE").is_file() {
        Some(folder)
    } else {
        eprintln!("skipped: no resource files in {}", folder.display());
        None
    }
}

// What the C++ decodeFolder (CrystalOSD 039cc58) gives for HDD OSD 1.10U's files: name, kind, width, height, source,
// size, FNV-1a 64 of the data.
const SOURCES: [(&str, u64, u64); 4] = [
    ("TEXIMAGE", 236964, 0x0f46f200ed709410),
    ("FNTOSD", 1489358, 0x4389c266d54be963),
    ("hddosd.elf", 3864601, 0x2af3b0c7d2e58cc5),
    ("SNDIMAGE", 407188, 0x728d3347c7657e92),
];

const ASSETS: [(&str, u32, u32, u32, u32, usize, u64); 24] = [
    ("TEXCFLOW", 1, 64, 64, 0, 16384, 0x588c17298050c409),
    ("TEXCKABE", 1, 128, 128, 0, 65536, 0x37ac9b4d6f8bb95d),
    ("TEXCBUMP", 1, 64, 64, 0, 16384, 0x649ce8911891facd),
    ("TEXCBINV", 1, 64, 64, 0, 16384, 0xe8f6c5cfac430715),
    ("TEXCSMOK", 1, 64, 64, 0, 16384, 0x350ee05edfc6597b),
    ("TEXCREFA", 1, 64, 64, 0, 16384, 0x6a68329f572c1812),
    ("TEXCNAVI", 1, 64, 64, 0, 16384, 0x83623d80c5759215),
    ("TEXCBLUR", 1, 64, 64, 0, 16384, 0xc8107ebef2c8fc55),
    ("TEXCSTSL", 1, 64, 64, 0, 16384, 0x2f7665f54efb594d),
    ("TEXCMARU", 1, 64, 64, 0, 16384, 0x1362c0aca244478e),
    ("TEXOSCE", 1, 256, 64, 0, 65536, 0xee7a9ba42f8aaa1d),
    ("TEXOFOG1", 1, 64, 64, 0, 16384, 0xb0eddf176be6b7d8),
    ("TEXOFOG2", 1, 64, 64, 0, 16384, 0x721f4b06589f6915),
    ("TEXOFOG4", 1, 64, 64, 0, 16384, 0x8f65095bf3d6ef95),
    ("TEXOWAL0", 1, 256, 256, 0, 262144, 0x9ef9002526598b6d),
    ("TEXOCRBL", 1, 64, 64, 0, 16384, 0x77d99f664003a085),
    ("TEXOREF", 1, 128, 128, 0, 65536, 0xbb28e0c91a81ccf5),
    ("TEXOBLP", 1, 64, 64, 0, 16384, 0xba39de92d4e9fb49),
    ("TEXOBLPR", 1, 64, 64, 0, 16384, 0xb7c1a557433b509e),
    ("FNTOSD", 3, 0, 0, 1, 1489358, 0x4389c266d54be963),
    ("PROGRAM", 4, 0, 0, 2, 3864601, 0x2af3b0c7d2e58cc5),
    ("RODMESH", 5, 16, 0, 2, 2304, 0x089224cdec960db1),
    ("CUBEMESH", 5, 6, 0, 2, 864, 0x60fdc05d1f828055),
    ("SNDIMAGE", 6, 0, 0, 3, 407188, 0xb4513bdf80767c8f),
];

#[test]
fn decode_folder_matches_cpp() {
    let Some(folder) = resources() else { return };
    let set = decode_folder(&folder).unwrap();
    let sources: Vec<_> = set.sources.iter().map(|s| (s.name.as_str(), s.size, s.hash)).collect();
    assert_eq!(sources, SOURCES);
    let decoded: Vec<_> = set.assets.iter().map(|a| (a.name.as_str(), a.kind as u32, a.width, a.height, a.source, a.data.len(), hash_of(&a.data))).collect();
    assert_eq!(decoded, ASSETS);
}

#[test]
fn program_and_cipher_match_cpp() {
    let Some(folder) = resources() else { return };
    let data = std::fs::read(folder.join("hddosd.elf")).unwrap();
    let elf = ElfImage::new(&data).unwrap();
    assert_eq!(program_digest(&elf), Ok(0xe0eae235376170d2));
    assert_eq!(elf.entry(), 0x00200008);
    assert_eq!(elf.segments().len(), 1);
    let tables = CipherTables::from_program(&elf).unwrap();
    let keys = key_schedule(&tables);
    assert_eq!(tables.key, 0x73d542f56f2b8cc6);
    assert_eq!(keys[0], 0xa9cf467f63340000);
    assert_eq!(keys[15], 0x9ca637bd11e70000);
    assert_eq!(decrypt_block(&tables, &keys, 0x0123456789abcdef), 0xd28a46c271b05479);
}
