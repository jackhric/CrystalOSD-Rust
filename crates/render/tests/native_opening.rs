// tests/render/NativeOpeningTest.cpp. The texture parts need the opening's PNG files
// (References/textures/opening): set CRYSTALOSD_OPENING_TEXTURES to that directory to run them.

mod support;

use std::path::Path;

use render::{Error, NativeOutput, NativeRenderer};
use scene::frame::{BlendOp, CoordinateKind, Sampling, SourceKind, TargetName, TextureSet};
use support::{HEIGHT, WIDTH, frame_in, near, pixel, sprite_at};

fn sprite(target: TargetName, blend: BlendOp, r: u8, g: u8, b: u8, a: u8) -> scene::frame::Pass {
    sprite_at(target, blend, r, g, b, a, 0.0, 0.0, 640.0, 224.0)
}

fn frame_of(passes: Vec<scene::frame::Pass>) -> scene::frame::Frame {
    frame_in(passes, TextureSet::Clock)
}

fn out(samples: u32) -> NativeOutput {
    NativeOutput { width: WIDTH, height: HEIGHT, samples }
}

// Cs x Ad + Cd: the destination's alpha is the target's stored alpha over 128.
fn add_destination_alpha(renderer: &mut NativeRenderer<'_>, samples: u32) {
    renderer.configure(&out(samples)).unwrap();
    let d = TargetName::Display;
    renderer.draw(&frame_of(vec![sprite(d, BlendOp::Opaque, 100, 100, 100, 0x40), sprite(d, BlendOp::AddDestinationAlpha, 60, 200, 10, 0x80)])).unwrap();
    assert!(near(pixel(&renderer.read_target(d).unwrap(), 320, 224), 100.0 + 60.0 * 0.5, 100.0 + 200.0 * 0.5, 100.0 + 10.0 * 0.5, "Ad 0.5"));
    renderer.draw(&frame_of(vec![sprite(d, BlendOp::Opaque, 100, 100, 100, 0x80), sprite(d, BlendOp::AddDestinationAlpha, 60, 100, 10, 0x20)])).unwrap();
    assert!(near(pixel(&renderer.read_target(d).unwrap(), 17, 300), 160.0, 200.0, 110.0, "Ad 1.0"));
    renderer.draw(&frame_of(vec![sprite(d, BlendOp::Opaque, 100, 100, 100, 0x00), sprite(d, BlendOp::AddDestinationAlpha, 60, 100, 10, 0x80)])).unwrap();
    assert!(near(pixel(&renderer.read_target(d).unwrap(), 600, 10), 100.0, 100.0, 100.0, "Ad 0"));
}

// Cd - Cs x constant / 128.
fn subtract_fixed(renderer: &mut NativeRenderer<'_>, samples: u32) {
    renderer.configure(&out(samples)).unwrap();
    let d = TargetName::Display;
    let mut full = sprite(d, BlendOp::SubtractFixed, 60, 20, 90, 0x00);
    full.material.blend_constant = 0x80;
    renderer.draw(&frame_of(vec![sprite(d, BlendOp::Opaque, 128, 128, 128, 0x80), full.clone()])).unwrap();
    assert!(near(pixel(&renderer.read_target(d).unwrap(), 320, 224), 128.0 - 60.0, 128.0 - 20.0, 128.0 - 90.0, "SubtractFixed 0x80"));
    let mut quarter = full;
    quarter.material.blend_constant = 0x20;
    renderer.draw(&frame_of(vec![sprite(d, BlendOp::Opaque, 128, 128, 128, 0x80), quarter])).unwrap();
    assert!(near(pixel(&renderer.read_target(d).unwrap(), 320, 224), 128.0 - 60.0 * 0.25, 128.0 - 20.0 * 0.25, 128.0 - 90.0 * 0.25, "SubtractFixed 0x20"));
}

// Per-pixel alpha only appears with blending off in the opening: it draws as a plain pass, and with blending on the
// frame is refused.
fn per_pixel_alpha(renderer: &mut NativeRenderer<'_>, samples: u32) {
    renderer.configure(&out(samples)).unwrap();
    let d = TargetName::Display;
    let mut copy = sprite(d, BlendOp::Opaque, 60, 200, 10, 127);
    copy.material.per_pixel_alpha = true;
    renderer.draw(&frame_of(vec![sprite(d, BlendOp::Opaque, 100, 100, 100, 0x80), copy.clone()])).unwrap();
    let rgba = renderer.read_target(d).unwrap();
    assert!(near(pixel(&rgba, 100, 224), 60.0, 200.0, 10.0, "PABE without blending"));
    assert_eq!(pixel(&rgba, 100, 224)[3], 127);
    let mut plain = copy.clone();
    plain.material.per_pixel_alpha = false;
    renderer.draw(&frame_of(vec![sprite(d, BlendOp::Opaque, 100, 100, 100, 0x80), plain])).unwrap();
    assert_eq!(pixel(&renderer.read_target(d).unwrap(), 100, 224), pixel(&rgba, 100, 224));
    for op in [BlendOp::Add, BlendOp::AlphaOver, BlendOp::AddDestinationAlpha] {
        let mut blended = copy.clone();
        blended.material.blend = op;
        assert!(matches!(renderer.draw(&frame_of(vec![blended])), Err(Error::Logic(_))));
    }
}

// The Store holds 320 x 224 of content: a sprite reading it across 640 texels finds the right half black.
fn store(renderer: &mut NativeRenderer<'_>, samples: u32) {
    renderer.configure(&out(samples)).unwrap();
    let d = TargetName::Display;
    let mut copy = sprite(d, BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80);
    copy.material.source = SourceKind::Target;
    copy.material.source_target = TargetName::Store;
    copy.material.sampling = Sampling::ClampToRegion;
    copy.material.region = [0, 639, 0, 223];
    copy.vertices[0].u = 0.0;
    copy.vertices[0].v = 0.0;
    copy.vertices[1].u = 640.0;
    copy.vertices[1].v = 224.0;
    renderer
        .draw(&frame_of(vec![
            sprite(d, BlendOp::Opaque, 9, 9, 9, 0x80),
            sprite_at(TargetName::Store, BlendOp::Opaque, 200, 150, 100, 0x80, 0.0, 0.0, 320.0, 224.0),
            copy,
        ]))
        .unwrap();
    let rgba = renderer.read_target(d).unwrap();
    assert!(near(pixel(&rgba, 100, 224), 200.0, 150.0, 100.0, "store left half"));
    assert!(near(pixel(&rgba, 500, 224), 0.0, 0.0, 0.0, "store right half"));
    assert!(near(pixel(&renderer.read_target(TargetName::Store).unwrap(), 500, 224), 0.0, 0.0, 0.0, "store target"));
}

// The Extra target is 1024 x 256 as a texture with its 640 x 224 picture at the corner: S reaches 0.625 and T 0.875
// at the picture's far edge.
fn extra(renderer: &mut NativeRenderer<'_>, samples: u32) {
    renderer.configure(&out(samples)).unwrap();
    let mut read = sprite(TargetName::Display, BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80);
    read.material.source = SourceKind::Target;
    read.material.source_target = TargetName::Extra;
    read.material.coordinates = CoordinateKind::Projective;
    read.material.sampling = Sampling::ClampToRegion;
    read.material.region = [0, 639, 0, 223];
    read.vertices[0].u = 0.0;
    read.vertices[0].v = 0.0;
    read.vertices[1].u = 0.625;
    read.vertices[1].v = 0.875;
    let e = TargetName::Extra;
    renderer
        .draw(&frame_in(
            vec![
                sprite_at(e, BlendOp::Opaque, 10, 200, 10, 0x80, 0.0, 0.0, 320.0, 112.0),
                sprite_at(e, BlendOp::Opaque, 10, 10, 200, 0x80, 320.0, 0.0, 640.0, 112.0),
                sprite_at(e, BlendOp::Opaque, 200, 10, 10, 0x80, 0.0, 112.0, 640.0, 224.0),
                read,
            ],
            TextureSet::Opening,
        ))
        .unwrap();
    let rgba = renderer.read_target(TargetName::Display).unwrap();
    assert!(near(pixel(&rgba, 280, 100), 10.0, 200.0, 10.0, "extra top left"));
    assert!(near(pixel(&rgba, 360, 100), 10.0, 10.0, 200.0, "extra top right"));
    assert!(near(pixel(&rgba, 100, 350), 200.0, 10.0, 10.0, "extra bottom"));
    assert!(near(pixel(&rgba, 600, 440), 200.0, 10.0, 10.0, "extra bottom right"));
}

// The opening's textures: CT16 ones keep TEXA's alpha (127 or 129); a frame of the opening set draws them.
fn textures(renderer: &mut NativeRenderer<'_>, directory: &Path, samples: u32) {
    renderer.load_opening_textures(directory).unwrap();
    renderer.configure(&NativeOutput { width: WIDTH, height: 224, samples }).unwrap();
    let (w, h, png) = render::texture_file::load_rgba(&directory.join("tex6-256x256.png")).unwrap();
    assert!(w == 256 && h == 256);
    let mut p = sprite_at(TargetName::Display, BlendOp::Opaque, 0x80, 0x80, 0x80, 0x40, 0.0, 0.0, 256.0, 224.0);
    p.material.source = SourceKind::Texture;
    p.material.texture = 6;
    p.vertices[0].u = 0.5;
    p.vertices[0].v = 0.5;
    p.vertices[1].u = 256.5;
    p.vertices[1].v = 224.5;
    renderer.draw(&frame_in(vec![sprite(TargetName::Display, BlendOp::Opaque, 0, 0, 0, 0x80), p], TextureSet::Opening)).unwrap();
    let rgba = renderer.read_target(TargetName::Display).unwrap();
    let (mut checked, mut wrong) = (0usize, 0usize);
    for y in (4..220u32).step_by(17) {
        for x in (4..252u32).step_by(13) {
            let t = &png[(y as usize * 256 + x as usize) * 4..];
            let at = (y as usize * WIDTH as usize + x as usize) * 4;
            for c in 0..3 {
                wrong += usize::from((i32::from(rgba[at + c]) - i32::from(t[c])).abs() > 1);
            }
            checked += 1;
        }
    }
    println!("opening texture 6: {checked} texels checked, {wrong} channels differ");
    assert_eq!(wrong, 0);

    renderer.draw(&frame_of(vec![sprite(TargetName::Display, BlendOp::Opaque, 1, 2, 3, 0x80)])).unwrap();
    let mut clock_texture = sprite(TargetName::Display, BlendOp::Opaque, 0x80, 0x80, 0x80, 0x40);
    clock_texture.material.source = SourceKind::Texture;
    clock_texture.material.texture = 6;
    assert!(matches!(renderer.draw(&frame_of(vec![clock_texture])), Err(Error::Runtime(_))));
}

// A texture alpha of 254 (the light sprites) times a vertex alpha of 0x80 is above the 0x80 a target stores: refused.
fn alpha_above(renderer: &mut NativeRenderer<'_>, samples: u32) {
    renderer.configure(&out(samples)).unwrap();
    let mut p = sprite(TargetName::Display, BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80);
    p.material.source = SourceKind::Texture;
    p.material.texture = 8;
    assert_eq!(renderer.written_alpha(&p, TextureSet::Opening), 254);
    match renderer.draw(&frame_in(vec![p.clone()], TextureSet::Opening)) {
        Err(Error::Logic(e)) => println!("refused: {e}"),
        other => panic!("not refused: {other:?}"),
    }
    p.vertices[0].a = 0x40;
    p.vertices[1].a = 0x40;
    renderer.draw(&frame_in(vec![p], TextureSet::Opening)).unwrap();
}

#[test]
fn native_opening() {
    let Some(device) = support::device() else { return };
    let mut renderer = NativeRenderer::new(&device, support::shaders()).unwrap();
    let directory = std::env::var_os("CRYSTALOSD_OPENING_TEXTURES").map(std::path::PathBuf::from);
    if directory.is_none() {
        eprintln!("CRYSTALOSD_OPENING_TEXTURES is not set: the texture parts are skipped");
    }
    for samples in [1u32, 4] {
        add_destination_alpha(&mut renderer, samples);
        subtract_fixed(&mut renderer, samples);
        per_pixel_alpha(&mut renderer, samples);
        store(&mut renderer, samples);
        extra(&mut renderer, samples);
        if let Some(dir) = &directory {
            textures(&mut renderer, dir, samples);
            if cfg!(debug_assertions) {
                alpha_above(&mut renderer, samples);
            }
        }
        println!("samples {samples}: opening paths equal");
    }
    println!("validation errors: {}", device.validation_errors());
    assert_eq!(device.validation_errors(), 0);
}
