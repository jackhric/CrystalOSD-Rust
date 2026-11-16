// The device row's icons (HDD OSD 1.10U): scene::browser::icons run on the matrices and lights the console sent VU1 (the probes of
// tools/browser/verify_browser_icons.mjs at 0x0023A878, the slot loads at 0x0023AFC8) and the members decoded from ICOIMAGE, every
// triangle against the console's card-row passes (texture t3a98, triangles), vertex for vertex, y on the field's half line. Skips
// without the fixture.
mod console;

use assets::ico::{ICON_NAMES, decode_icons};
use console::inputs::{at, int};
use console::probes::{Image, load};
use console::{fixture, load_json, need};
use scene::arithmetic::EeArithmetic;
use scene::browser::icons::{IconConstants, IconInputs, IconItem, IconPipeline, IconRecord, IconVertexIn, IconVu1, vertex_qwords};
use scene::matrix::Mat4;

const KICK: u32 = 0x0023_a878;
const LOAD: u32 = 0x0023_afc8;
const ROWS: u32 = 0x002a_d620;
const DESCRIPTORS: u32 = 0x002a_5a80;

fn mat(i: &Image, address: u32) -> Mat4 {
    std::array::from_fn(|r| std::array::from_fn(|c| i.f32(address + 16 * r as u32 + 4 * c as u32)))
}

fn icons_run(name: &str) {
    let probes = fixture(name, "probes.json");
    let passes = fixture(name, "passes.json");
    let dumps = console::dumps("");
    if !need(&[&probes, &passes, &dumps.join("ICOIMAGE"), &dumps.join("hddosd.elf")]) {
        return;
    }
    let members: Vec<Vec<IconVertexIn>> = {
        let decoded = decode_icons(&dumps).expect("ICOIMAGE");
        ICON_NAMES
            .iter()
            .map(|n| {
                let (_, ico) = decoded.iter().find(|(m, _)| m == n).expect("a member");
                let records: Vec<IconRecord> = ico.vertices.iter().map(|v| (v.positions[0], v.normal, v.uv, v.colour)).collect();
                vertex_qwords::<EeArithmetic>(&records)
            })
            .collect()
    };
    let c = load(&probes);
    let dump = load_json(&passes);
    let mut slots = std::collections::HashMap::new();
    let mut pipe = IconPipeline::default();
    let mut mine: std::collections::BTreeMap<i32, Vec<Vec<scene::frame::Vertex>>> = std::collections::BTreeMap::new();
    for r in &c.records {
        let i = Image::of([r]);
        if r.pc == LOAD && i.has(ROWS, 0x100) {
            let member = (0..16u32).find(|k| i.u32(ROWS + 16 * k + 4) == r.gpr[5]).expect("the loaded member's row");
            slots.insert(r.gpr[4], member as usize);
        }
        if r.pc != KICK || !i.has(0x002a_7f30, 0xe0) {
            continue;
        }
        let slot = (r.gpr[5] - DESCRIPTORS) / 0x50;
        let block = 0x002a_80b0 + 0xe0 * slot;
        let inputs = IconInputs {
            b: mat(&i, 0x002a_7f38),
            a: mat(&i, block + 8),
            colours: mat(&i, 0x002a_7f88),
            vectors: mat(&i, 0x002a_7fd0),
            bias: i.f32(block + 0x90),
        };
        let member = slots.get(&slot).copied().expect("a member loaded in the slot");
        let sent = IconVu1::<EeArithmetic>::draw(&mut pipe, &inputs, &members[member]);
        mine.entry(r.frame).or_default().push(IconVu1::<EeArithmetic>::triangles(&sent));
    }
    let (mut draws, mut vertices, mut frames) = (0usize, 0usize, 0usize);
    for f in at(&dump, "frames").as_array().unwrap() {
        let frame = int(at(f, "frame"));
        let shift = if int(at(f, "field")) != 0 { 0.5f32 } else { 0.0 };
        let theirs: Vec<console::gs::Pass> = at(f, "passes")
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["card"].as_bool().unwrap_or(false) && p["texture"]["source"]["image"].as_str() == Some("t3a98-2-2-7x7"))
            .map(console::gs::read_pass)
            .collect();
        let Some(ours) = mine.get(&frame) else {
            assert!(theirs.is_empty(), "{name} frame {frame}: the console draws {} icons, no kick was probed", theirs.len());
            continue;
        };
        assert_eq!(ours.len(), theirs.len(), "{name} frame {frame}: icon draws");
        for (k, (a, b)) in ours.iter().zip(&theirs).enumerate() {
            assert_eq!(a.len(), b.vertices.len(), "{name} frame {frame} icon {k}: vertices");
            for (n, (v, w)) in a.iter().zip(&b.vertices).enumerate() {
                let same = v.x == w.x
                    && v.y - shift == w.y
                    && v.z == w.depth
                    && [f32::from(v.r), f32::from(v.g), f32::from(v.b), f32::from(v.a)] == [w.r, w.g, w.b, w.a]
                    && [v.u, v.v, v.q] == [w.s, w.t, w.q];
                assert!(same, "{name} frame {frame} icon {k} vertex {n}: {v:?} against {w:?}");
            }
            vertices += a.len();
            draws += 1;
        }
        frames += 1;
    }
    println!("{name}: {frames} frames, {draws} icon draws, {vertices} vertices equal");
    assert!(draws > 0);
}

// func_00259240: the item's local matrix (probed on the stack at 0x002593A8) and matrix A (func_002567D0's a1) from the item record
// and the row (the entry probe at 0x00259240). The scale is item + 0x60: no item of kind 1 or 0xC (which multiply it by
// D_0036FF00 first) is drawn in these captures, and func_00264498 gives 1.0 in all of them (a local matrix would differ otherwise).
fn matrices_run(name: &str) {
    let probes = fixture(name, "probes.json");
    if !need(&[&probes, &console::dumps("hddosd.elf")]) {
        return;
    }
    let k = IconConstants::of(&console::program());
    let c = load(&probes);
    let mut item: Option<(IconItem, Mat4, f32)> = None;
    let mut local: Option<Mat4> = None;
    let (mut locals, mut matrices) = (0, 0);
    for r in &c.records {
        let i = Image::of([r]);
        match r.pc {
            0x0025_9240 => {
                let (a0, a1) = (r.gpr[4], r.gpr[5]);
                let fields = IconItem {
                    angles: [i.f32(a0), i.f32(a0 + 4), i.f32(a0 + 8)],
                    position: std::array::from_fn(|n| i.f32(a0 + 0x10 + 4 * n as u32)),
                    kind: i.s32(a0 + 0x130),
                };
                item = Some((fields, mat(&i, a1 + 0x30), i.f32(a0 + 0x60)));
            }
            0x0025_93a8 => {
                let Some((fields, _, scale)) = item else { continue };
                let mine = IconVu1::<EeArithmetic>::local_matrix(&fields, scale, &k);
                assert_eq!(
                    mine.map(|row| row.map(f32::to_bits)),
                    mat(&i, r.gpr[29]).map(|row| row.map(f32::to_bits)),
                    "{name} frame {}: the local matrix",
                    r.frame
                );
                local = Some(mine);
                locals += 1;
            }
            0x0025_67d0 => {
                let (Some((_, row, _)), Some(l)) = (item, local) else { continue };
                let a = IconVu1::<EeArithmetic>::item_matrix(&row, &l);
                assert_eq!(a.map(|row| row.map(f32::to_bits)), mat(&i, r.gpr[5]).map(|row| row.map(f32::to_bits)), "{name} frame {}: matrix A", r.frame);
                matrices += 1;
                item = None;
                local = None;
            }
            _ => {}
        }
    }
    println!("{name}: {locals} local matrices, {matrices} matrices A equal");
    assert!(matrices > 0);
}

#[test]
fn matrices_from_items() {
    matrices_run("hddosd-110U-browser-icons-none");
    matrices_run("hddosd-110U-browser-icons-one");
    matrices_run("hddosd-110U-browser-icons-two");
}

#[test]
fn icons_no_card() {
    icons_run("hddosd-110U-browser-icons-none");
}

#[test]
fn icons_one_card() {
    icons_run("hddosd-110U-browser-icons-one");
}

#[test]
fn icons_two_cards() {
    icons_run("hddosd-110U-browser-icons-two");
}
