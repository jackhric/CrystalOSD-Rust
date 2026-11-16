// The Rust card reader (assets::memory_card) against the lab's JS reader (tools/mc/read_card.mjs) on card images in the data home:
// cards/listing.json is tools/browser/card_listing.mjs's listing of the same files. The images: osd-formatted.ps2 and
// osd-formatted-exact.ps2 (formatted by the console's own Format dialog: captures hddosd-110U-browser-format-yes and
// hddosd-110U-browser-devices-formatted), fat10 / hi10 / w170 / w170b (variants of it, ECC rewritten), the two directory shapes of
// verify_mc_image.mjs's controls, and a blank card. Skips without them.
use std::path::{Path, PathBuf};

use assets::memory_card::{CardInfo, chain, directory, ecc_page, ecc_report, fat, info, library_free, read_image, superblock};
use serde_json::Value;

fn cards() -> PathBuf {
    let home = std::env::var_os("CRYSTALOSD_DATA").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../CrystalOSD-Data"));
    home.join("cards")
}

fn fnv(words: &[u32]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for w in words {
        for k in 0..4 {
            h ^= (w >> (8 * k)) & 0xff;
            h = h.wrapping_mul(0x0100_0193);
        }
    }
    h
}

fn n(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an integer: {v}"))
}

#[test]
fn reader_equals_the_lab_reader() {
    let listing_path = cards().join("listing.json");
    if !listing_path.exists() {
        eprintln!("skipped: {} is absent", listing_path.display());
        return;
    }
    let listing: Value = serde_json::from_slice(&std::fs::read(&listing_path).unwrap()).unwrap();
    let mut compared = 0;
    for (name, want) in listing.as_object().unwrap() {
        let image = read_image(&cards().join(name)).unwrap();
        let i = &want["info"];
        match info(Some(&image)) {
            CardInfo::Unformatted => assert_eq!(i["state"], "unformatted", "{name}"),
            CardInfo::None => panic!("{name}: an image is never none"),
            CardInfo::Formatted { free_clusters, cluster_bytes, free_kb, root_entries, root_names } => {
                assert_eq!(i["state"], "formatted", "{name}");
                assert_eq!(i64::from(free_clusters), n(&i["freeClusters"]), "{name}: free clusters");
                assert_eq!(i64::from(cluster_bytes), n(&i["clusterBytes"]), "{name}: cluster bytes");
                assert_eq!(i64::from(free_kb), n(&i["freeKB"]), "{name}: free KB");
                assert_eq!(i64::from(root_entries), n(&i["rootEntries"]), "{name}: root entries");
                let names: Vec<&str> = i["rootNames"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
                assert_eq!(root_names, names, "{name}: root names");
            }
        }
        let ecc = ecc_report(&image);
        assert_eq!(ecc.pages as i64, n(&want["ecc"]["pages"]), "{name}: programmed pages");
        let bad: Vec<i64> = want["ecc"]["badPages"].as_array().unwrap().iter().map(n).collect();
        assert_eq!(ecc.bad_pages.iter().map(|&p| p as i64).collect::<Vec<_>>(), bad, "{name}: pages with a wrong ECC");
        match superblock(&image) {
            None => assert!(want["superblock"].is_null(), "{name}: superblock"),
            Some(sb) => {
                let s = &want["superblock"];
                assert_eq!(sb.version, s["version"].as_str().unwrap(), "{name}");
                for (ours, key) in [
                    (i64::from(sb.page_len), "pageLen"),
                    (i64::from(sb.pages_per_cluster), "pagesPerCluster"),
                    (i64::from(sb.pages_per_block), "pagesPerBlock"),
                    (i64::from(sb.clusters_per_card), "clustersPerCard"),
                    (i64::from(sb.alloc_offset), "allocOffset"),
                    (i64::from(sb.alloc_end), "allocEnd"),
                    (i64::from(sb.rootdir_cluster), "rootdirCluster"),
                    (i64::from(sb.backup_block1), "backupBlock1"),
                    (i64::from(sb.backup_block2), "backupBlock2"),
                    (i64::from(sb.card_type), "cardType"),
                    (i64::from(sb.card_flags), "cardFlags"),
                ] {
                    assert_eq!(ours, n(&s[key]), "{name}: superblock {key}");
                }
                let ifc: Vec<i64> = s["ifcList"].as_array().unwrap().iter().map(n).collect();
                assert_eq!(sb.ifc_list.iter().map(|&x| i64::from(x)).collect::<Vec<_>>(), ifc, "{name}: indirect FAT list");
                let table = fat(&image, &sb);
                assert_eq!(table.len() as i64, n(&want["fat"]["length"]), "{name}: FAT length");
                assert_eq!(i64::from(fnv(&table)), n(&want["fat"]["hash"]), "{name}: FAT entries");
                let root = directory(&image, &sb, &table, sb.rootdir_cluster, want["root"].as_array().unwrap().len());
                for (e, w) in root.iter().zip(want["root"].as_array().unwrap()) {
                    assert_eq!(
                        (i64::from(e.mode), i64::from(e.length), i64::from(e.cluster), i64::from(e.dir_entry), i64::from(e.attr)),
                        (n(&w["mode"]), n(&w["length"]), n(&w["cluster"]), n(&w["dirEntry"]), n(&w["attr"])),
                        "{name}: entry {}",
                        e.name
                    );
                    assert_eq!(e.name, w["name"].as_str().unwrap(), "{name}: entry name");
                    for (t, key) in [(&e.created, "created"), (&e.modified, "modified")] {
                        let tw = &w[key];
                        assert_eq!(
                            (i64::from(t.sec), i64::from(t.min), i64::from(t.hour), i64::from(t.day), i64::from(t.month), i64::from(t.year)),
                            (n(&tw["sec"]), n(&tw["min"]), n(&tw["hour"]), n(&tw["day"]), n(&tw["month"]), n(&tw["year"])),
                            "{name}: entry {} {key}",
                            e.name
                        );
                    }
                }
            }
        }
        compared += 1;
    }
    println!("{compared} card images: the Rust reader's listing equals the lab reader's");
    assert!(compared > 0);
}

// Controls: the reader must notice a damaged image (verify_mc_image.mjs's controls).
#[test]
fn reader_controls() {
    let path = cards().join("osd-formatted.ps2");
    if !path.exists() {
        eprintln!("skipped: {} is absent", path.display());
        return;
    }
    let oracle = read_image(&path).unwrap();
    let mut hit = oracle.clone();
    hit[82 * 528 + 3] ^= 1;
    assert_eq!(ecc_report(&hit).bad_pages, vec![82], "a flipped data bit is that page");
    let mut no_magic = oracle.clone();
    no_magic[0] ^= 0x20;
    assert_eq!(info(Some(&no_magic)), CardInfo::Unformatted);
    assert_eq!(info(None), CardInfo::None);
    assert_eq!(ecc_page(&[0xff; 512])[..3], [0x77, 0x7f, 0x7f], "an erased chunk's ECC");
    assert_eq!(chain(&[0x8000_0000], 0), vec![0], "a self-looping chain ends");
    assert_eq!(chain(&[0x8000_0001, 0x7fff_ffff], 0), vec![0, 1], "a chain stops at a free entry");
    assert_eq!(chain(&[0x8000_0002, 0xffff_ffff, 0x8000_0001], 0), vec![0, 2, 1], "a chain follows the next-cluster bits");
}

// The free count the console's library gave sceMcGetInfo for each image (isfirstslot's argument, probed: captures
// hddosd-110U-browser-devices-card, -free-fat10, -free-hi10, -free-w170, -free-w170b; measured, not a verifier), and the kilobytes
// the Browser shows, isfirstslot's max(free - 2, 0) (verified, facts/browser-devices.md section 1).
#[test]
fn library_free_count_measured() {
    let measured = [("osd-formatted.ps2", 8000), ("fat10.ps2", 7990), ("hi10.ps2", 8000), ("w170.ps2", 8000), ("w170b.ps2", 8000)];
    for (name, free) in measured {
        let path = cards().join(name);
        if !path.exists() {
            eprintln!("skipped: {} is absent", path.display());
            return;
        }
        let image = read_image(&path).unwrap();
        assert_eq!(library_free(&image), Some(free), "{name}");
    }
    let blank = read_image(&cards().join("blank.ps2")).unwrap();
    assert_eq!(library_free(&blank), None);
}
