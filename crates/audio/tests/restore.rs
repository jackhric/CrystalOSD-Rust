mod common;

use std::path::PathBuf;

use audio::data::snd_image::SndImage;
use audio::driver::{Snapshot as DriverSnapshot, load_snapshot};
use audio::spu2::Spu2;
use audio::spu2::snapshot::{Json, Snapshot};
use common::{Hash, Rng, hash_spu};

// The C++ (CrystalOSD 039cc58) Snapshot::load and Spu2::restore on the generated spustate.json below, HDD OSD 1.10U's SPU
// RAM and random register and history bytes: seed, FNV-1a 64 of the JSON text, digest of the restored, history-loaded and
// 3000-sample-mixed SPU2.
const RESTORED: [(u64, u64, u64); 4] = [
    (31, 0x2c5ecf1534ef3220, 0x26cc1831c0accbb4),
    (32, 0x01bd34d969c25e7f, 0x37e591b0a78326a1),
    (33, 0xf3651dd8bb5df707, 0x76c343152dbff8dd),
    (34, 0x80d1c28b07079d5a, 0xcc872c6c9c325373),
];

fn num(r: &mut Rng, v: u32, negative: bool) -> String {
    match r.below(if negative { 5 } else { 4 }) {
        0 => format!("{v}"),
        1 => format!("\"{v:x}\""),
        2 => format!("\"0x{v:X}\""),
        3 => format!("\" {v:x}\""),
        _ => format!("{}", v as i32),
    }
}

fn n(r: &mut Rng, v: u32) -> String {
    num(r, v, false)
}

fn s(r: &mut Rng, v: u32) -> String {
    num(r, v, true)
}

fn flag(r: &mut Rng, b: bool) -> String {
    match r.below(3) {
        0 => if b { "true" } else { "false" }.to_owned(),
        1 => if b { "1" } else { "0" }.to_owned(),
        _ => if b { "\"1\"" } else { "\"0\"" }.to_owned(),
    }
}

fn sgn(r: &mut Rng, span: u32) -> u32 {
    (r.below(2 * span + 1) as i32 - span as i32) as u32
}

fn gate(r: &mut Rng) -> String {
    let g = if r.below(5) == 0 {
        r.next()
    } else if r.below(2) != 0 {
        0xFFFF_FFFF
    } else {
        0
    };
    s(r, g)
}

fn sweep(r: &mut Rng) -> String {
    let v = r.below(0x10000);
    let mut out = format!("{{\"reg\":{}", n(r, v));
    let v = sgn(r, 0x7FFF);
    out += &format!(",\"value\":{}", s(r, v));
    let v = r.below(0x8000);
    out += &format!(",\"counter\":{}}}", n(r, v));
    out
}

macro_rules! field {
    ($out:ident, $text:expr, $value:expr) => {
        $out += &format!($text, $value)
    };
}

fn voice(r: &mut Rng) -> String {
    let mut o = format!("{{\"volume\":{{\"left\":{}", sweep(r));
    field!(o, ",\"right\":{}}},\"adsr\":{{\"reg1\":", sweep(r));
    let v = r.below(0x10000);
    o += &n(r, v);
    let v = r.below(0x10000);
    field!(o, ",\"reg2\":{}", n(r, v));
    let v = r.below(0x8000);
    field!(o, ",\"value\":{}", n(r, v));
    let v = r.below(5);
    field!(o, ",\"phase\":{}", n(r, v));
    let v = r.below(0x8000);
    field!(o, ",\"counter\":{}}}", n(r, v));
    let v = r.below(0x10000);
    field!(o, ",\"pitch\":{}", n(r, v));
    for key in ["startA", "loopStartA", "nextA"] {
        let v = r.next();
        o += &format!(",\"{key}\":{}", n(r, v));
    }
    for key in ["prev1", "prev2"] {
        let v = sgn(r, 32768);
        o += &format!(",\"{key}\":{}", s(r, v));
    }
    let b = r.below(2) != 0;
    field!(o, ",\"modulated\":{}", flag(r, b));
    let b = r.below(4) == 0;
    field!(o, ",\"noise\":{}", flag(r, b));
    for key in ["loopMode", "loopFlags"] {
        let v = sgn(r, 128);
        o += &format!(",\"{key}\":{}", s(r, v));
    }
    let v = r.below(0x4000);
    field!(o, ",\"sp\":{}", n(r, v));
    let v = sgn(r, 32768);
    field!(o, ",\"outX\":{}", s(r, v));
    for key in ["decPosRead", "decPosWrite"] {
        let v = r.next();
        o += &format!(",\"{key}\":{}", n(r, v));
    }
    field!(o, ",\"gates\":{{\"dryL\":{}", gate(r));
    field!(o, ",\"dryR\":{}", gate(r));
    field!(o, ",\"wetL\":{}", gate(r));
    field!(o, ",\"wetR\":{}}}}}", gate(r));
    o
}

fn pair(r: &mut Rng) -> String {
    let v = sgn(r, 0x7FFF);
    let mut o = format!("[{}", s(r, v));
    let v = sgn(r, 0x7FFF);
    field!(o, ",{}]", s(r, v));
    o
}

fn gates(r: &mut Rng) -> String {
    let mut o = String::from("{");
    for (i, name) in ["inpL", "inpR", "sndL", "sndR", "extL", "extR"].iter().enumerate() {
        o += &format!("{}\"{name}\":{}", if i > 0 { "," } else { "" }, gate(r));
    }
    o + "}"
}

fn core(r: &mut Rng) -> String {
    let v = r.next();
    let mut o = format!("{{\"tsa\":{}", n(r, v));
    let v = r.next();
    field!(o, ",\"activeTsa\":{}", n(r, v));
    let v = r.below(0x100000);
    field!(o, ",\"irqa\":{}", n(r, v));
    let b = r.below(2) != 0;
    field!(o, ",\"irqEnable\":{}", flag(r, b));
    let b = r.below(4) != 0;
    field!(o, ",\"fxEnable\":{}", flag(r, b));
    let b = r.below(4) == 0;
    field!(o, ",\"mute\":{}", flag(r, b));
    let v = r.below(4);
    field!(o, ",\"autoDmaCtrl\":{}", n(r, v));
    let b = r.below(2) != 0;
    field!(o, ",\"admaInProgress\":{}", flag(r, b));
    let v = r.below(8);
    field!(o, ",\"dmaBits\":{}", s(r, v));
    let v = r.below(0x40);
    field!(o, ",\"noiseClk\":{}", n(r, v));
    for key in ["noiseCnt", "noiseOut"] {
        let v = r.next();
        o += &format!(",\"{key}\":{}", n(r, v));
    }
    for key in ["keyOn", "keyOff"] {
        let v = r.below(0x1000000);
        o += &format!(",\"{key}\":{}", n(r, v));
    }
    o += ",\"regs\":{";
    for (i, name) in ["pmon", "non", "vmixl", "vmixr", "vmixel", "vmixer", "endx"].iter().enumerate() {
        let v = r.below(0x1000000);
        o += &format!("{}\"{name}\":{}", if i > 0 { "," } else { "" }, n(r, v));
    }
    let v = r.below(0x10000);
    field!(o, ",\"mmix\":{}", n(r, v));
    let v = r.below(0x10000);
    field!(o, ",\"statx\":{}", n(r, v));
    let v = r.below(0x10000);
    field!(o, ",\"attr\":{}}}", n(r, v));
    field!(o, ",\"masterVol\":{{\"left\":{}", sweep(r));
    field!(o, ",\"right\":{}}}", sweep(r));
    field!(o, ",\"extVol\":{}", pair(r));
    field!(o, ",\"inpVol\":{}", pair(r));
    field!(o, ",\"fxVol\":{}", pair(r));
    field!(o, ",\"dryGate\":{}", gates(r));
    field!(o, ",\"wetGate\":{}", gates(r));
    let start = r.below(0xF0000);
    field!(o, ",\"effectsStartA\":{}", n(r, start));
    let end = if r.below(4) == 0 { start } else { start + 0x1000 + r.below(0xFFFFF - start - 0x1000) };
    field!(o, ",\"effectsEndA\":{}", n(r, end));
    let v = r.next();
    field!(o, ",\"revbSampleBufPos\":{}", n(r, v));
    o += ",\"revb\":{";
    for (i, name) in ["inCoefL", "inCoefR", "apf1Vol", "apf2Vol", "iirVol", "wallVol", "comb1Vol", "comb2Vol", "comb3Vol", "comb4Vol"].iter().enumerate() {
        let v = sgn(r, 0x8000);
        o += &format!("{}\"{name}\":{}", if i > 0 { "," } else { "" }, s(r, v));
    }
    for name in [
        "apf1Size",
        "apf2Size",
        "sameLSrc",
        "sameRSrc",
        "diffLSrc",
        "diffRSrc",
        "sameLDst",
        "sameRDst",
        "diffLDst",
        "diffRDst",
        "comb1LSrc",
        "comb1RSrc",
        "comb2LSrc",
        "comb2RSrc",
        "comb3LSrc",
        "comb3RSrc",
        "comb4LSrc",
        "comb4RSrc",
        "apf1LDst",
        "apf1RDst",
        "apf2LDst",
        "apf2RDst",
    ] {
        let v = if r.below(3) == 0 { r.next() } else { r.below(0x8000) };
        o += &format!(",\"{name}\":{}", n(r, v));
    }
    o += "},\"voices\":[\n";
    for v in 0..24 {
        if v > 0 {
            o += ",\n";
        }
        o += &voice(r);
    }
    o + "\n]}"
}

fn state(r: &mut Rng) -> String {
    let v = r.next();
    let mut o = format!("{{\"ticks\":{}", n(r, v));
    let v = r.below(0x10000);
    field!(o, ", \"outPos\":{}", n(r, v));
    let v = r.below(8);
    field!(o, ", \"playMode\":{}", s(r, v));
    o += ", \"extra\": [1.5, null, \"x\\n\", {\"a\": -2e3}],";
    let v = r.below(0x10000);
    field!(o, " \"spdif\":{{\"out\":{}", n(r, v));
    for key in ["info", "mode", "media"] {
        let v = r.below(0x10000);
        o += &format!(",\"{key}\":{}", n(r, v));
    }
    let v = r.below(0x10000);
    field!(o, ",\"protection\":{}}},\n \"cores\":[", n(r, v));
    o += &core(r);
    field!(o, ",{}]}}\n", core(r));
    o
}

#[test]
fn restore_matches_cpp() {
    let Some(folder) = common::resources() else { return };
    let spu_ram = SndImage::from_folder(&folder).unwrap().spu_ram().unwrap();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("audio-restore");
    std::fs::create_dir_all(&dir).unwrap();
    for &(seed, text_hash, state_hash) in &RESTORED {
        let mut r = Rng(seed);
        let text = state(&mut r);
        assert_eq!(common::fnv(text.as_bytes()), text_hash, "seed {seed}: the generated JSON differs from the C++ probe's");
        let regs: Vec<u8> = (0..0x10000).map(|_| r.next() as u8).collect();
        let history: Vec<u8> = (0..8192).map(|_| r.next() as u8).collect();
        let name = format!("probe{seed}");
        std::fs::write(dir.join(format!("{name}.start.spuram.bin")), &spu_ram).unwrap();
        std::fs::write(dir.join(format!("{name}.start.spuregs.bin")), &regs).unwrap();
        std::fs::write(dir.join(format!("{name}.start.spustate.json")), &text).unwrap();
        let mut spu = Spu2::new();
        spu.restore(&Snapshot::load(&dir, &name, "start").unwrap()).unwrap();
        let mut h = Hash::default();
        hash_spu(&mut h, &spu);
        spu.restore_history(&history).unwrap();
        hash_spu(&mut h, &spu);
        let mut pcm = vec![0i16; 2 * 3000];
        spu.render(&mut pcm, 3000);
        pcm.iter().for_each(|&x| h.i16(x));
        hash_spu(&mut h, &spu);
        assert_eq!(h.0, state_hash, "seed {seed}");
    }
    assert!(Snapshot::load(&dir, "absent", "start").is_err());
    let mut spu = Spu2::new();
    let short = Snapshot { ram: vec![0; 10], regs: vec![0; 0x10000], state: Json::Null };
    assert!(spu.restore(&short).is_err());
    assert!(spu.restore_history(&[0; 100]).is_err());
}

#[test]
fn json_reads_what_the_captures_hold() {
    let j = Json::parse(r#" {"a": [1, -2, 18446744073709551615, 1.5e3, "x\"A"], "b": {"c": true, "c": false}, "d": null} "#).unwrap();
    let a = j.at("a").unwrap();
    assert_eq!(a.size(), 5);
    assert_eq!(a.index(0).unwrap(), &Json::Integer(1));
    assert_eq!(a.index(1).unwrap(), &Json::Integer(-2));
    assert_eq!(a.index(2).unwrap(), &Json::Unsigned(u64::MAX));
    assert_eq!(a.index(3).unwrap(), &Json::Float(1500.0));
    assert_eq!(a.index(4).unwrap(), &Json::String("x\"A".into()));
    assert_eq!(j.at("b").unwrap().at("c").unwrap(), &Json::Bool(false));
    assert!(j.at("e").is_err() && a.index(5).is_err() && Json::parse("[1,").is_err() && Json::parse("{} x").is_err());
}

#[test]
fn driver_snapshot_loads_its_regions() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("audio-driver-snapshot");
    std::fs::create_dir_all(&dir).unwrap();
    let stem = dir.join("probe");
    std::fs::write(dir.join("probe.start.osdsnd.bin"), [1u8, 2, 3]).unwrap();
    let _ = std::fs::remove_file(dir.join("probe.start.iopsound.bin"));
    assert_eq!(load_snapshot(&stem).unwrap(), DriverSnapshot { osdsnd: vec![1, 2, 3], iop: vec![] });
    std::fs::write(dir.join("probe.start.iopsound.bin"), [9u8]).unwrap();
    assert_eq!(load_snapshot(&stem).unwrap().iop, vec![9]);
    std::fs::write(dir.join("probe.start.osdsnd.bin"), vec![0u8; 0xb001]).unwrap();
    assert!(load_snapshot(&stem).is_err());
    assert!(load_snapshot(&dir.join("absent")).is_err());
}
