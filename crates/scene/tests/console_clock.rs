// The Rust scene against the console's own frames: the fixtures exported from a capture (tools/scene/export_fixture.mjs
// for the probed inputs and the state each frame leaves, tools/parity/frame_passes.mjs for the passes the console sent
// to the GS) read from the data home; each frame computed with EeArithmetic and compared pass for pass, vertex for
// vertex. The tests skip when the data home has no such fixture.
mod console;

use console::gs::{TextCount, clock_layout, compare_frame, from_scene};
use console::inputs::{at, clock_inputs, frame_inputs, int};
use console::{EeClock, compare_after, cube_mesh, fixture, font, load_json, need, program, rod_mesh};

// MenusParityTest.cpp on hddosd-110U-whole3-menu (the C++ gate MenusMenu): the main menu at rest, with its text.
#[test]
fn whole3_menu_against_the_console() {
    let scene_file = fixture("hddosd-110U-whole3-menu", "scene.json");
    let passes_file = fixture("hddosd-110U-whole3-menu", "passes.json");
    let root = console::crystalosd_root();
    if !need(&[&scene_file, &passes_file, &console::dumps("FNTOSD"), &console::dumps("hddosd.elf"), &root.join("facts/data/rod-mesh.json")]) {
        return;
    }
    let scene = load_json(&scene_file);
    let dump = load_json(&passes_file);
    let frames = at(&scene, "frames").as_array().unwrap();
    let dump_frames = at(&dump, "frames").as_array().unwrap();
    let first = at(&frames[0], "input");
    let mut inputs = clock_inputs(first, &rod_mesh(), Some(&cube_mesh()));
    assert!(inputs.menus.is_some(), "the fixture holds no menus");
    inputs.font = Some(font());
    inputs.program = Some(program());
    let pages = at(at(at(&frames[0], "expect"), "text"), "pages").as_array().unwrap();
    if let Some(page) = pages.first() {
        inputs.text.font = console::inputs::font_state(at(page, "own"));
    }
    let mut clock = EeClock::new(&inputs).unwrap();
    let mut compared = 0;
    let mut passes = 0;
    let mut text = TextCount::default();
    for (k, frame) in frames.iter().enumerate() {
        let where_ = format!("frame {k}");
        let mut input = frame_inputs(at(frame, "input"));
        input.thread_step = k > 0;
        let out = clock.frame(&input);
        if let Err(e) = compare_after(&clock, at(at(frame, "expect"), "after"), &where_) {
            panic!("{e}");
        }
        if k < dump_frames.len() {
            let gs = from_scene(&out, &clock_layout(out.width, out.height, out.display_index));
            passes += compare_frame(&gs, &dump_frames[k], &mut text, &where_, false).unwrap_or_else(|e| panic!("{e}"));
            compared += 1;
        }
    }
    println!(
        "whole3-menu: {} frames carried, {compared} compared with the console, {passes} passes equal ({} glyph passes, {} hint pictures)",
        frames.len(),
        text.font,
        text.hint
    );
    assert!(compared > 0 && passes > 0);
}

// ClockParityTest.cpp on hddosd-110U-whole3-clock: the clock alone, every frame from the capture's own inputs. This
// fixture carries no font context, so the clock runs without text and the dump's text passes are absorbed (counted).
#[test]
fn whole3_clock_against_the_console() {
    let scene_file = fixture("hddosd-110U-whole3-clock", "scene.json");
    let passes_file = fixture("hddosd-110U-whole3-clock", "passes.json");
    if !need(&[
        &scene_file,
        &passes_file,
        &console::dumps("FNTOSD"),
        &console::dumps("hddosd.elf"),
        &console::crystalosd_root().join("facts/data/rod-mesh.json"),
    ]) {
        return;
    }
    let scene = load_json(&scene_file);
    let dump = load_json(&passes_file);
    let frames = at(&scene, "frames").as_array().unwrap();
    let dump_frames = at(&dump, "frames").as_array().unwrap();
    let inputs = clock_inputs(at(&frames[0], "input"), &rod_mesh(), None);
    let mut clock = EeClock::new(&inputs).unwrap();
    let mut compared = 0;
    let mut passes = 0;
    let mut text = TextCount::default();
    for frame in frames {
        let index = int(at(frame, "index")) as usize;
        let where_ = format!("frame {index}");
        let input = frame_inputs(at(frame, "input"));
        let out = clock.frame(&input);
        if let Err(e) = compare_after(&clock, at(at(frame, "expect"), "after"), &where_) {
            panic!("{e}");
        }
        assert_eq!((out.field, out.display_index), (input.field, input.display_index));
        let Some(dump_frame) = dump_frames.get(index) else { continue };
        assert_eq!(int(at(dump_frame, "frame")) as usize, index);
        let gs = from_scene(&out, &clock_layout(out.width, out.height, out.display_index));
        passes += compare_frame(&gs, dump_frame, &mut text, &where_, true).unwrap_or_else(|e| panic!("{e}"));
        compared += 1;
    }
    println!(
        "whole3-clock: {} frames carried, {compared} compared with the console, {passes} passes equal, {} text passes of the dump absorbed",
        frames.len(),
        text.absorbed
    );
    assert!(compared > 0 && passes > 0);
}

// The comparison must be able to fail: the first frame of whole3-menu with one vertex moved by a sixteenth of a pixel.
#[test]
fn comparison_control_fails() {
    let scene_file = fixture("hddosd-110U-whole3-menu", "scene.json");
    let passes_file = fixture("hddosd-110U-whole3-menu", "passes.json");
    if !need(&[
        &scene_file,
        &passes_file,
        &console::dumps("FNTOSD"),
        &console::dumps("hddosd.elf"),
        &console::crystalosd_root().join("facts/data/rod-mesh.json"),
    ]) {
        return;
    }
    let scene = load_json(&scene_file);
    let dump = load_json(&passes_file);
    let frames = at(&scene, "frames").as_array().unwrap();
    let mut inputs = clock_inputs(at(&frames[0], "input"), &rod_mesh(), Some(&cube_mesh()));
    inputs.font = Some(font());
    inputs.program = Some(program());
    let pages = at(at(at(&frames[0], "expect"), "text"), "pages").as_array().unwrap();
    if let Some(page) = pages.first() {
        inputs.text.font = console::inputs::font_state(at(page, "own"));
    }
    let mut clock = EeClock::new(&inputs).unwrap();
    let mut input = frame_inputs(at(&frames[0], "input"));
    input.thread_step = false;
    let out = clock.frame(&input);
    let mut gs = from_scene(&out, &clock_layout(out.width, out.height, out.display_index));
    let mut text = TextCount::default();
    assert!(compare_frame(&gs, &at(&dump, "frames")[0], &mut text, "frame 0", false).is_ok());
    let moved = gs.iter_mut().find(|p| !p.vertices.is_empty()).unwrap();
    moved.vertices[0].x += 0.0625;
    let result = compare_frame(&gs, &at(&dump, "frames")[0], &mut text, "frame 0", false);
    println!("control: {result:?}");
    assert!(result.is_err(), "a moved vertex is not seen");
}
