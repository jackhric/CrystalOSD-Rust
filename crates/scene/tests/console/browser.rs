use std::collections::BTreeMap;

use serde_json::Value;

use scene::browser::chrome::{BrowserFrame, TextRun};

use super::gs::{Layout, Pass, Primitive, TextureImage, block_name, from_scene, pass_difference, read_pass, state_difference};
use super::inputs::at;

// facts/browser-chrome.md section 3: the TEXB sheets allocated from 0x2800, 64 blocks each (128 for TEXBCDPB, index 5).
pub fn sheet_id(index: i32) -> String {
    let blocks = [64, 64, 64, 64, 64, 128, 64, 64];
    let tbp: u32 = 0x2800 + blocks[..index as usize].iter().sum::<u32>();
    format!("t{tbp:04x}-1-0-6x6")
}

pub fn browser_layout(display: &str, width: i32, height: i32) -> Layout {
    let mut textures = BTreeMap::new();
    for index in 0..8 {
        textures.insert(index, TextureImage { id: sheet_id(index), width: 64, height: if index == 5 { 128 } else { 64 } });
    }
    Layout {
        targets: vec![display.to_owned(), block_name(0xffff), block_name(0xfffe)],
        width: width as u32,
        height: height as u32,
        target_texture: [1024, 256],
        textures,
    }
}

// Consecutive passes of one state are one draw on the GS (the parser's rule): merged on both sides before the comparison.
pub fn merge(passes: Vec<Pass>) -> Vec<Pass> {
    let mut out: Vec<Pass> = Vec::new();
    for p in passes {
        if let Some(last) = out.last_mut()
            && state_difference(last, &p).is_none()
        {
            last.vertices.extend(p.vertices);
            continue;
        }
        out.push(p);
    }
    out
}

// Glyphs sample a paletted 4-bit texture (PSM 0x14): the HDD OSD's font cache t2a40 (fans), ROM 2.30's glyph pages (sprites).
pub fn is_text(p: &Pass) -> bool {
    p.texture.as_ref().is_some_and(|t| !t.source_is_target && t.source.contains("-20-"))
}

// A text run's glyphs on the console: a glyph is a fan of 12 vertices, 10 triangles (facts/text.md section 3), 30 vertices as
// the parser lists them; ROM 2.30 draws its text as sprites (facts/browser-chrome.md section 5, colour only).
fn glyph_colours(passes: &[&Pass]) -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    for p in passes {
        let per = if p.primitive == Primitive::Triangles { 30 } else { 2 };
        for g in p.vertices.chunks(per) {
            out.push([g[0].r, g[0].g, g[0].b, g[0].a]);
            assert!(g.iter().all(|v| [v.r, v.g, v.b, v.a] == [g[0].r, g[0].g, g[0].b, g[0].a]), "a glyph whose vertices differ in colour");
        }
    }
    out
}

fn run_matches(runs: &[&TextRun], glyphs: &[[f32; 4]], rom: bool) -> Result<usize, String> {
    if rom {
        for g in glyphs {
            let ok = runs.iter().any(|r| {
                let c = r.colour;
                [g[0], g[1], g[2]] == [(c & 0xff) as f32, ((c >> 8) & 0xff) as f32, ((c >> 16) & 0xff) as f32] && r.alpha.is_none_or(|a| g[3] == a as f32)
            });
            if !ok {
                return Err(format!("a ROM glyph colour {g:?} is none of the runs'"));
            }
        }
        return Ok(glyphs.len());
    }
    let mut k = 0;
    for run in runs {
        let mut accepted = None;
        for _ in 0..scene::browser::chrome::glyph_count(&run.text) {
            let Some(g) = glyphs.get(k) else {
                return Err(format!("text run \"{}\" ({}): the console has {} glyphs here", run.label, String::from_utf8_lossy(&run.text), glyphs.len()));
            };
            let c = run.colour;
            let rgb = [(c & 0xff) as f32, ((c >> 8) & 0xff) as f32, ((c >> 16) & 0xff) as f32];
            let alpha_ok = match run.alpha {
                Some(a) => g[3] == a as f32,
                None => g[3] <= 128.0 && *accepted.get_or_insert(g[3]) == g[3],
            };
            if [g[0], g[1], g[2]] != rgb || !alpha_ok {
                return Err(format!("text run \"{}\": glyph colour {g:?}, the run says {:06x} alpha {:?}", run.label, run.colour, run.alpha));
            }
            k += 1;
        }
    }
    if k != glyphs.len() {
        return Err(format!("{} console glyphs, the runs have {k}", glyphs.len()));
    }
    Ok(k)
}

#[derive(Default, Debug)]
pub struct Tally {
    pub frames: usize,
    pub passes: usize,
    pub vertices: usize,
    pub glyphs: usize,
    pub card: usize,
    pub head_clears: usize,
    pub text_frames: usize,
    pub fans: usize,
}

// A frame whose other passes the facts do not specify (a background variant): its text alone, every glyph fan against the
// console's, in order.
pub fn compare_text(ours: &BrowserFrame, theirs: &Value, width: i32, height: i32, tally: &mut Tally) -> Result<(), String> {
    let console: Vec<Pass> = at(theirs, "passes").as_array().unwrap().iter().filter(|p| !p["card"].as_bool().unwrap_or(false)).map(read_pass).collect();
    let display = console.first().map(|p| p.target.clone()).ok_or("the frame has no pass")?;
    let mine = merge(from_scene(&ours.frame, &browser_layout(&display, width, height)).into_iter().filter(is_text).collect());
    let console = merge(console.into_iter().filter(is_text).collect());
    if mine.len() != console.len() {
        return Err(format!("text: {} scene passes, {} console passes", mine.len(), console.len()));
    }
    for (k, (a, b)) in mine.iter().zip(&console).enumerate() {
        if let Some(d) = pass_difference(a, b) {
            return Err(format!("text pass {k} ({}): {d}", a.name));
        }
        tally.fans += a.vertices.len() / 30;
    }
    tally.text_frames += 1;
    Ok(())
}

// One frame: the scene's passes (merged) against the console's non-card passes (merged), text by runs, in order.
pub fn compare(ours: &BrowserFrame, theirs: &Value, width: i32, height: i32, rom: bool, tally: &mut Tally) -> Result<(), String> {
    let mut console: Vec<Pass> = at(theirs, "passes").as_array().unwrap().iter().filter(|p| !p["card"].as_bool().unwrap_or(false)).map(read_pass).collect();
    // The core's clear of a display buffer in the module's first frames (facts/browser-chrome.md section 2, measured, not scheduled
    // by the facts): a black, transparent sprite over the screen at (0, 0), z 0, before the module's own clear; the scene omits it.
    while let Some(first) = console.first()
        && first.primitive == Primitive::Sprites
        && first.texture.is_none()
        && first.blend.is_none()
        && first.vertices.len() >= 2
        && first.vertices[0].a == 0.0
        && first.vertices[0].x == 0.0
        && first.vertices[1].x == width as f32
    {
        let head = console.remove(0);
        if head.vertices.len() > 2 {
            let mut rest = head.clone();
            rest.vertices.drain(..2);
            console.insert(0, rest);
        }
        tally.head_clears += 1;
    }
    tally.card += at(theirs, "passes").as_array().unwrap().iter().filter(|p| p["card"].as_bool().unwrap_or(false)).count();
    let display = console.first().map(|p| p.target.clone()).ok_or("the frame has no pass")?;
    let mine = from_scene(&ours.frame, &browser_layout(&display, width, height));
    // The scene draws its own glyphs (scene::browser::strings): every pass, the text's fans with them, against the console's.
    if mine.iter().any(is_text) {
        let mine = merge(mine);
        let console = merge(console);
        if mine.len() != console.len() {
            let names = |list: &[Pass]| -> Vec<String> {
                list.iter().map(|p| format!("{:?} {}", p.primitive, p.texture.as_ref().map_or("flat", |t| t.source.as_str()))).collect()
            };
            return Err(format!("{} scene passes {:?}, {} console passes {:?}", mine.len(), names(&mine), console.len(), names(&console)));
        }
        for (k, (a, b)) in mine.iter().zip(&console).enumerate() {
            if let Some(d) = pass_difference(a, b) {
                return Err(format!("pass {k} ({}): {d}", a.name));
            }
            if is_text(a) {
                tally.fans += a.vertices.len() / 30;
            }
            tally.vertices += a.vertices.len();
        }
        tally.passes += mine.len();
        tally.frames += 1;
        return Ok(());
    }
    let mut groups: Vec<(usize, Vec<&TextRun>)> = Vec::new();
    for run in &ours.texts {
        match groups.last_mut() {
            Some((at, list)) if *at == run.before_pass => list.push(run),
            _ => groups.push((run.before_pass, vec![run])),
        }
    }
    let mut console_blocks: Vec<(usize, Vec<&Pass>)> = Vec::new();
    let mut console_plain = Vec::new();
    let console = merge(console);
    for p in &console {
        if is_text(p) {
            match console_blocks.last_mut() {
                Some((at, list)) if *at == console_plain.len() => list.push(p),
                _ => console_blocks.push((console_plain.len(), vec![p])),
            }
        } else {
            console_plain.push(p.clone());
        }
    }
    let mine_plain = merge(mine);
    let console_plain = merge(console_plain);
    if mine_plain.len() != console_plain.len() {
        let names: Vec<String> =
            console_plain.iter().map(|p| format!("{:?} {}", p.primitive, p.texture.as_ref().map_or("flat", |t| t.source.as_str()))).collect();
        return Err(format!("{} scene passes, {} console passes: {names:?}", mine_plain.len(), console_plain.len()));
    }
    for (k, (a, b)) in mine_plain.iter().zip(&console_plain).enumerate() {
        if let Some(d) = pass_difference(a, b) {
            return Err(format!("pass {k} ({}): {d}", a.name));
        }
        tally.vertices += a.vertices.len();
    }
    tally.passes += mine_plain.len();
    // Text: each block of console glyph passes against the scene's runs placed before the same pass (merging can only join passes
    // that have no text between them, so the positions are counted on the merged lists).
    let position = |unmerged: usize, list: &[Pass]| -> usize { merge(list[..unmerged].to_vec()).len() };
    let mine_unmerged = from_scene(&ours.frame, &browser_layout(&display, width, height));
    let mut mine_blocks: BTreeMap<usize, Vec<&TextRun>> = BTreeMap::new();
    for (at, runs) in groups {
        mine_blocks.entry(position(at, &mine_unmerged)).or_default().extend(runs);
    }
    let mut joined: BTreeMap<usize, Vec<&Pass>> = BTreeMap::new();
    for (at, passes) in &console_blocks {
        let at = merge(console.iter().filter(|p| !is_text(p)).take(*at).cloned().collect()).len();
        joined.entry(at).or_default().extend(passes.iter().copied());
    }
    let mut seen = 0;
    for (at, passes) in &joined {
        let glyphs = glyph_colours(passes);
        let runs = mine_blocks.remove(at).unwrap_or_default();
        let runs: Vec<&TextRun> = runs.into_iter().filter(|r| scene::browser::chrome::glyph_count(&r.text) > 0).collect();
        seen += run_matches(&runs, &glyphs, rom).map_err(|e| format!("text before pass {at}: {e}"))?;
    }
    for (at, runs) in mine_blocks {
        if runs.iter().any(|r| scene::browser::chrome::glyph_count(&r.text) > 0) {
            return Err(format!("text before pass {at}: the console draws none, the scene has {:?}", runs.iter().map(|r| r.label).collect::<Vec<_>>()));
        }
    }
    tally.glyphs += seen;
    tally.frames += 1;
    Ok(())
}
