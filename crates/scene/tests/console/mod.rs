#![allow(dead_code)]

pub mod browser;
pub mod gs;
pub mod inputs;
pub mod probes;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use scene::arithmetic::{EeArithmetic, float_bits};
use scene::clock::Clock;
use scene::font::Font;
use scene::orbs::ORB_COUNT;
use scene::program_image::ProgramImage;
use scene::ramp::Ramp;
use scene::rods::{RodMesh, load_rod_mesh};

use inputs::{at, bits, int};

pub type EeClock = Clock<EeArithmetic>;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// The data home (Sony-derived captures, fixtures and dumps, never in the repository): CRYSTALOSD_DATA, else the
// sibling CrystalOSD-Data of the checkout.
pub fn data_home() -> PathBuf {
    match std::env::var_os("CRYSTALOSD_DATA") {
        Some(dir) => PathBuf::from(dir),
        None => workspace().join("../CrystalOSD-Data"),
    }
}

// The C++ checkout, for the rod and cube meshes it carries (facts/data/rod-mesh.json, References/model/cube-mesh.json).
pub fn crystalosd_root() -> PathBuf {
    match std::env::var_os("CRYSTALOSD_ROOT") {
        Some(dir) => PathBuf::from(dir),
        None => workspace().join("../CrystalOSD"),
    }
}

// The files a console comparison needs, or None (with the reason printed) so that a test skips when the data home or
// the C++ checkout is absent.
pub fn need(paths: &[&Path]) -> bool {
    for path in paths {
        if !path.exists() {
            eprintln!("skipped: {} is absent", path.display());
            return false;
        }
    }
    true
}

pub fn fixture(name: &str, file: &str) -> PathBuf {
    data_home().join("fixtures").join(name).join(file)
}

pub fn dumps(file: &str) -> PathBuf {
    data_home().join("dumps/hddosd-host").join(file)
}

pub fn load_json(path: &Path) -> Value {
    let text = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_slice(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn rod_mesh() -> RodMesh {
    load_rod_mesh(&crystalosd_root().join("facts/data/rod-mesh.json")).unwrap()
}

pub fn cube_mesh() -> RodMesh {
    load_rod_mesh(&crystalosd_root().join("References/model/cube-mesh.json")).unwrap()
}

pub fn font() -> Arc<Font> {
    Arc::new(Font::load(&dumps("FNTOSD")).unwrap())
}

pub fn program() -> Arc<ProgramImage> {
    Arc::new(ProgramImage::load(&dumps("hddosd.elf")).unwrap())
}

fn same_ramp(r: &Ramp, j: &Value, what: &str, at_frame: &str) -> Result<(), String> {
    if r.length == int(at(j, "length")) && r.counter == int(at(j, "counter")) && r.changed == int(at(j, "changed")) && r.state == int(at(j, "state")) {
        Ok(())
    } else {
        Err(format!("{at_frame}: {what} {r:?} vs {j}"))
    }
}

fn same_int(v: i32, j: &Value, what: &str, at_frame: &str) -> Result<(), String> {
    if v == int(j) { Ok(()) } else { Err(format!("{at_frame}: {what} {v} vs {}", int(j))) }
}

fn same_bits(v: f32, j: &Value, what: &str, at_frame: &str) -> Result<(), String> {
    if float_bits(v) == bits(j) { Ok(()) } else { Err(format!("{at_frame}: {what} {:#010x} vs {j}", float_bits(v))) }
}

// tests/scene/SceneGate.cpp compareAfter: the state the frame leaves against the console's, piece by piece.
pub fn compare_after(clock: &EeClock, after: &Value, at_frame: &str) -> Result<(), String> {
    let s = clock.state();
    same_int(s.counter, at(after, "counter"), "counter", at_frame)?;
    same_int(s.level, at(after, "level"), "level", at_frame)?;
    same_int(s.mode, at(after, "mode"), "mode", at_frame)?;
    same_int(s.overlay_level, at(after, "overlayLevel"), "overlayLevel", at_frame)?;
    same_ramp(&s.vignette_ramp, at(after, "vignetteRamp"), "vignetteRamp", at_frame)?;
    same_ramp(&s.menu_ramp, at(after, "menuRamp"), "menuRamp", at_frame)?;
    same_ramp(&s.appearance, at(after, "appearance"), "appearance", at_frame)?;
    same_ramp(&clock.head().grey_ramp, at(after, "greyRamp"), "greyRamp", at_frame)?;
    same_ramp(&clock.orbs().sprite_fade, at(after, "spriteFade"), "spriteFade", at_frame)?;
    let eased = at(after, "eased");
    same_int(i32::from(s.eased.second_hand), at(eased, "secondHand"), "eased.secondHand", at_frame)?;
    same_int(i32::from(s.eased.hour_hand), at(eased, "hourHand"), "eased.hourHand", at_frame)?;
    same_bits(s.eased.progress, at(eased, "progress"), "eased.progress", at_frame)?;
    same_bits(s.eased.fraction, at(eased, "fraction"), "eased.fraction", at_frame)?;
    same_bits(s.camera_offset, at(after, "cameraOffset"), "cameraOffset", at_frame)?;
    same_bits(s.scene.scale, at(at(after, "scene"), "scale"), "scene.scale", at_frame)?;
    same_int(s.state.current_rod, at(at(after, "state"), "currentRod"), "currentRod", at_frame)?;
    for i in 0..3 {
        same_int(clock.head().greys[i], &at(after, "greys")[i], "greys", at_frame)?;
    }
    for k in 0..ORB_COUNT {
        let r = &clock.orbs().rings[k];
        let j = &at(after, "rings")[k];
        same_int(r.head, at(j, "head"), "ring head", at_frame)?;
        same_int(r.count, at(j, "count"), "ring count", at_frame)?;
        same_int(r.full, at(j, "full"), "ring full", at_frame)?;
    }
    let local = &clock.rod_template().local;
    let want = at(at(after, "template"), "local");
    for r in 0..4 {
        for c in 0..4 {
            same_bits(local[r][c], &want[r][c], "template.local", at_frame)?;
        }
    }
    Ok(())
}
