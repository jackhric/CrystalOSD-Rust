use std::path::PathBuf;
use std::sync::Arc;

use app::boot_chain::{BootChain, BootPhase};
use app::clock_assets::rod_mesh_of;
use app::clock_screen::ClockScreen;
use app::golden::hash_frame;
use app::opening_screen::OpeningScreen;
use app::screen::FrameSource;
use scene::arithmetic::NativeArithmetic;
use scene::cold_start::{ColdAssets, cold_start};
use scene::cold_types::ColdInputs;
use scene::font::Font;
use scene::frame::{Frame, TextureSet};
use scene::opening::{BootOptions, FRAMES_TO_CLOCK, HandOff};
use scene::program_image::ProgramImage;

struct Counting {
    total: i32,
    steps: i32,
    frame: Frame,
}

impl Counting {
    fn new(total: i32) -> Self {
        Counting { total, steps: 0, frame: Frame::default() }
    }
}

impl FrameSource for Counting {
    fn step(&mut self, _: &mut ()) {
        self.steps += 1;
        self.frame.field = self.steps;
    }
    fn frame(&mut self) -> &mut Frame {
        &mut self.frame
    }
    fn done(&self) -> bool {
        self.steps >= self.total
    }
}

#[test]
fn mock_phases() {
    let mut chain = BootChain::new(Counting::new(246), Counting::new(1 << 30), FRAMES_TO_CLOCK);
    for i in 0..246 {
        assert_eq!(chain.phase(), BootPhase::Opening);
        chain.step(&mut ());
        assert_eq!(chain.frame().field, i + 1);
    }
    for _ in 0..FRAMES_TO_CLOCK {
        assert_eq!(chain.phase(), BootPhase::Gap);
        chain.step(&mut ());
        assert!(chain.frame().texture_set == TextureSet::Opening && !chain.frame().passes.is_empty());
    }
    assert!(chain.phase() == BootPhase::Clock && chain.clock().steps == 0);
    chain.step(&mut ());
    assert_eq!(chain.clock().steps, 1);
}

fn resources() -> Option<PathBuf> {
    let folder = std::env::var_os("CRYSTALOSD_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../CrystalOSD/Resources"));
    if folder.join("hddosd.elf").is_file() {
        Some(folder)
    } else {
        eprintln!("skipped: no resources (CRYSTALOSD_RESOURCES)");
        None
    }
}

#[test]
fn opening_then_clock() {
    let Some(folder) = resources() else { return };
    let set = assets::resources::decode_folder(&folder).unwrap();
    let program = Arc::new(ProgramImage::new(set.find("PROGRAM").unwrap().data.clone()).unwrap());
    let assets = ColdAssets {
        program: program.clone(),
        font: Some(Arc::new(Font::new(&set.find("FNTOSD").unwrap().data).unwrap())),
        mesh: rod_mesh_of(set.find("RODMESH").unwrap()).unwrap(),
        cube_mesh: rod_mesh_of(set.find("CUBEMESH").unwrap()).unwrap(),
    };
    let cold = ColdInputs { language: 1, screen_height: 0xE0, screen_code: 0x64, time_zone: 540, ..Default::default() };
    let start = cold_start::<NativeArithmetic>(&assets, &cold).unwrap();
    let run = |phase: u32, clock_forced: bool| -> Vec<u64> {
        let options = BootOptions { lights_phase: Some(phase), clock_forced, ..Default::default() };
        let mut chain = BootChain::new(OpeningScreen::new(&options, &program), ClockScreen::new(&start.clock, &start.frame).unwrap(), FRAMES_TO_CLOCK);
        let mut hashes = Vec::new();
        let (mut opening, mut gap, mut clock) = (0, 0, 0);
        let mut hand_off = HandOff::default();
        for _ in 0..246 + FRAMES_TO_CLOCK + 10 {
            let before = chain.phase();
            chain.step(&mut ());
            hashes.push(hash_frame(FrameSource::<()>::frame(&mut chain)));
            opening += i32::from(before == BootPhase::Opening);
            gap += i32::from(before == BootPhase::Gap);
            clock += i32::from(before == BootPhase::Clock);
            if before == BootPhase::Opening && chain.phase() != before {
                hand_off = *chain.opening().hand_off().unwrap();
            }
        }
        assert!(opening == 246 && gap == FRAMES_TO_CLOCK && clock == 10);
        assert_eq!(chain.phase(), BootPhase::Clock);
        println!("hand-off module {}, execute type {}", hand_off.module, hand_off.execute_app_type);
        hashes
    };
    let a = run(0xD80, false);
    let b = run(0xD80, false);
    assert_eq!(a, b);
    let forced = run(0xD80, true);
    assert_eq!(forced.len(), a.len());
    let other = run(0x123, false);
    assert_ne!(other, a);
}
