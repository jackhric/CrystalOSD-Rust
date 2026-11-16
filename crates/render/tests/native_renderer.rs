// tests/render/NativeRendererTest.cpp: the parts that need no data. clockFrames, text, startFile and frozen drive
// scene::Clock, which is not in this crate's dependencies yet; tests/vectors.rs replays the golden gate's real frames.

mod support;

use render::{Error, NativeOutput, NativeRenderer};
use scene::frame::{
    BlendOp, DepthTest, GLYPH_LAYOUT_TEXTURE, GLYPH_TEXTURE, GlyphCache, Pass, PassTopology, Sampling, SourceKind, TargetName, TextureSet, Vertex,
};
use support::{HEIGHT, WIDTH, frame_of, near, pixel, sprite, sprite_at, vertex};

fn out(width: u32, height: u32, samples: u32) -> NativeOutput {
    NativeOutput { width, height, samples }
}

// Untextured colour is byte / 255, alpha byte / 128 (scene/frame.rs); expected values are written in bytes.
fn blends(renderer: &mut NativeRenderer<'_>) {
    renderer.configure(&out(WIDTH, HEIGHT, 1)).unwrap();
    let half = 0x40 as f64 / 128.0;
    let quarter = 0x20 as f64 / 128.0;

    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 200, 100, 50, 0x80), sprite(BlendOp::AlphaOver, 10, 220, 128, 0x40)])).unwrap();
    assert!(near(
        pixel(&renderer.read_target(TargetName::Display).unwrap(), 320, 224),
        10.0 * half + 200.0 * (1.0 - half),
        220.0 * half + 100.0 * (1.0 - half),
        128.0 * half + 50.0 * (1.0 - half),
        "AlphaOver"
    ));

    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 100, 100, 100, 0x80), sprite(BlendOp::Add, 60, 200, 10, 0x40)])).unwrap();
    assert!(near(pixel(&renderer.read_target(TargetName::Display).unwrap(), 17, 300), 100.0 + 60.0 * half, 100.0 + 200.0 * half, 100.0 + 10.0 * half, "Add"));

    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 100, 100, 100, 0x80), sprite(BlendOp::Subtract, 60, 200, 10, 0x40)])).unwrap();
    assert!(near(pixel(&renderer.read_target(TargetName::Display).unwrap(), 600, 10), 100.0 - 60.0 * half, 0.0, 100.0 - 10.0 * half, "Subtract"));

    let mut over = sprite(BlendOp::FixedOver, 10, 220, 128, 0x00);
    over.material.blend_constant = 0x20;
    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 200, 100, 50, 0x80), over])).unwrap();
    assert!(near(
        pixel(&renderer.read_target(TargetName::Display).unwrap(), 320, 224),
        10.0 * quarter + 200.0 * (1.0 - quarter),
        220.0 * quarter + 100.0 * (1.0 - quarter),
        128.0 * quarter + 50.0 * (1.0 - quarter),
        "FixedOver"
    ));

    let mut add = sprite(BlendOp::FixedAdd, 60, 200, 10, 0x10);
    add.material.blend_constant = 0x20;
    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 100, 100, 100, 0x80), add])).unwrap();
    assert!(near(
        pixel(&renderer.read_target(TargetName::Display).unwrap(), 320, 224),
        100.0 + 60.0 * quarter,
        100.0 + 200.0 * quarter,
        100.0 + 10.0 * quarter,
        "FixedAdd"
    ));

    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 1, 2, 3, 0x40)])).unwrap();
    assert_eq!(pixel(&renderer.read_target(TargetName::Display).unwrap(), 5, 5)[3], 0x40);
}

// Every pass's written alpha is bounded by 0x80, which a target stores as 1.0: a target read back holds the bound
// of what was written to it, and in Debug a frame that would write more is refused before anything is recorded.
fn alpha_bounds(renderer: &mut NativeRenderer<'_>) {
    renderer.configure(&out(WIDTH, HEIGHT, 1)).unwrap();
    let mut low = sprite(BlendOp::Opaque, 1, 2, 3, 0x40);
    low.target = TargetName::RefractionSource;
    renderer.draw(&frame_of(vec![low])).unwrap();
    let mut read = sprite(BlendOp::AlphaOver, 0x80, 0x80, 0x80, 0x80);
    read.material.source = SourceKind::Target;
    read.material.source_target = TargetName::RefractionSource;
    read.material.sampling = Sampling::ClampToRegion;
    read.material.region = [0, 639, 0, 223];
    assert_eq!(renderer.written_alpha(&read, TextureSet::Clock), 0x40);
    assert_eq!(renderer.blend_alpha(&read, TextureSet::Clock), 0x40);

    let opaque = vec![0xffu8; 4 * 4 * 4];
    renderer.set_texture(21, 4, 4, &opaque).unwrap();
    let mut textured = sprite(BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80);
    textured.material.source = SourceKind::Texture;
    textured.material.texture = 21;
    assert_eq!(renderer.written_alpha(&textured, TextureSet::Clock), 0xff);
    let mut edge = textured.clone();
    edge.edge_smoothing = true;
    assert_eq!(renderer.written_alpha(&edge, TextureSet::Clock), 0x80);

    if cfg!(debug_assertions) {
        for high in [sprite(BlendOp::Opaque, 1, 2, 3, 0x90), textured] {
            let refused = renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 0, 0, 0, 0x80), high]));
            match refused {
                Err(Error::Logic(e)) => println!("refused: {e}"),
                other => panic!("not refused: {other:?}"),
            }
        }
    }
    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 9, 9, 9, 0x80)])).unwrap();
    assert_eq!(pixel(&renderer.read_target(TargetName::Display).unwrap(), 1, 1)[0], 9);
}

// A textured colour is texel * vertex / 128; ClampToRegion outside its region gives the region's edge texel.
fn sampling(renderer: &mut NativeRenderer<'_>) {
    renderer.configure(&out(WIDTH, HEIGHT, 1)).unwrap();
    let mut texture = vec![0u8; 4 * 4 * 4];
    for y in 0..4u32 {
        for x in 0..4u32 {
            let t = &mut texture[((y * 4 + x) * 4) as usize..][..4];
            t[0] = (10 + x * 50) as u8;
            t[1] = (10 + y * 50) as u8;
            t[2] = 200;
            t[3] = 0x80;
        }
    }
    renderer.set_texture(20, 4, 4, &texture).unwrap();

    let mut p = sprite(BlendOp::Opaque, 0x80, 0x80, 0x40, 0x80);
    p.material.source = SourceKind::Texture;
    p.material.texture = 20;
    p.material.sampling = Sampling::ClampToRegion;
    p.material.region = [1, 2, 1, 2];
    p.material.bilinear = true;
    p.vertices[0].u = -10.0;
    p.vertices[0].v = 20.0;
    p.vertices[1].u = -5.0;
    p.vertices[1].v = 30.0;
    renderer.draw(&frame_of(vec![p])).unwrap();
    let rgba = renderer.read_target(TargetName::Display).unwrap();
    assert!(near(pixel(&rgba, 3, 3), 60.0, 110.0, 100.0, "region clamp, top left"));
    assert!(near(pixel(&rgba, 636, 444), 60.0, 110.0, 100.0, "region clamp, bottom right"));
}

// Above the native size a copy sprite (u = x + 0.5, as the OSD's copies) still lands texel on pixel, and a
// sprite over the whole target covers every output pixel.
fn copies(renderer: &mut NativeRenderer<'_>) {
    renderer.configure(&out(1280, 896, 1)).unwrap();
    let mut background = sprite(BlendOp::Opaque, 20, 40, 60, 0x80);
    background.target = TargetName::RefractionSource;
    let block = sprite_at(TargetName::RefractionSource, BlendOp::Opaque, 250, 200, 150, 0x80, 100.0, 50.0, 300.5, 151.0);
    let mut copy = sprite(BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80);
    copy.material.source = SourceKind::Target;
    copy.material.source_target = TargetName::RefractionSource;
    copy.material.sampling = Sampling::ClampToRegion;
    copy.material.region = [0, 639, 0, 223];
    copy.material.bilinear = true;
    copy.vertices[0].u = 0.5;
    copy.vertices[0].v = 0.5;
    copy.vertices[1].u = 640.5;
    copy.vertices[1].v = 224.5;
    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 7, 7, 7, 0x80), background, block, copy])).unwrap();
    let source = renderer.read_target(TargetName::RefractionSource).unwrap();
    let copied = renderer.read_target(TargetName::Display).unwrap();
    let (mut differing, mut block_pixels) = (0usize, 0usize);
    for i in (0..source.len()).step_by(4) {
        for c in 0..3 {
            differing += usize::from((i32::from(source[i + c]) - i32::from(copied[i + c])).abs() > 1);
        }
        block_pixels += usize::from(source[i] == 250);
    }
    println!("copy at 1280x896: {differing} differing channels, {block_pixels} block pixels");
    assert!(source[0] == 20 && source[(896 * 1280 - 1) * 4] == 20);
    assert_eq!(block_pixels, 401 * 404);
    assert_eq!(differing, 0);
}

// An additive line strip (the orb trail: AA1, so alpha 0x80) at 1280 x 896 (scale 2 across, 4 down) covers what
// the GS covers, one pixel across its minor axis: a row (4 output rows) for a gentle strip, a column (2 output
// columns) for a steep one, each pixel once. Returns the pixels lit once and more than once.
fn strip_coverage(renderer: &mut NativeRenderer<'_>, steep: bool) -> (usize, usize) {
    renderer.configure(&out(1280, 896, 1)).unwrap();
    let mut strip = Pass { name: "trail".into(), topology: PassTopology::Lines, edge_smoothing: true, ..Default::default() };
    strip.material.blend = BlendOp::Add;
    strip.material.depth_test = DepthTest::Always;
    let mut points: Vec<Vertex> = Vec::new();
    for i in 0..=44 {
        let along = 10.0f32 + 4.0 * i as f32;
        let across = 20.0f32 * (along / 30.0).sin();
        let x = if steep { 300.0 + across * 0.4 } else { 100.0 + 2.5 * along };
        let y = if steep { along } else { 112.0 + across };
        points.push(vertex(x, y, 0, 0.0, 0.0, 1.0, 0, 0, 200, 0x80));
    }
    for w in points.windows(2) {
        strip.vertices.push(w[0]);
        strip.vertices.push(w[1]);
    }
    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 0, 0, 0, 0x80), strip])).unwrap();
    let rgba = renderer.read_target(TargetName::Display).unwrap();
    let (mut once, mut twice) = (0usize, 0usize);
    for i in (0..rgba.len()).step_by(4) {
        once += usize::from(rgba[i + 2] >= 199 && rgba[i + 2] <= 201);
        twice += usize::from(rgba[i + 2] > 201);
    }
    (once, twice)
}

fn strips(renderer: &mut NativeRenderer<'_>) {
    let (gentle, gentle_twice) = strip_coverage(renderer, false);
    let (steep, steep_twice) = strip_coverage(renderer, true);
    println!("additive strips at 1280x896: gentle {gentle} pixels lit once, {gentle_twice} more; steep {steep} once, {steep_twice} more");
    // Gentle: 440 scene columns (880 output) by 4 rows. Steep: 176 scene rows (704 output) by 2 columns.
    assert!(gentle > 880 * 4 * 9 / 10 && gentle < 880 * 4 * 11 / 10 && gentle_twice == 0);
    assert!(steep > 704 * 2 * 9 / 10 && steep < 704 * 2 * 11 / 10 && steep_twice == 0);
}

fn edge_levels(renderer: &mut NativeRenderer<'_>, samples: u32) -> u32 {
    renderer.configure(&out(WIDTH, HEIGHT, samples)).unwrap();
    let triangle = Pass {
        name: "diagonal".into(),
        vertices: vec![
            vertex(0.0, 0.0, 0, 0.0, 0.0, 1.0, 255, 255, 255, 0x80),
            vertex(640.0, 0.0, 0, 0.0, 0.0, 1.0, 255, 255, 255, 0x80),
            vertex(0.0, 224.0, 0, 0.0, 0.0, 1.0, 255, 255, 255, 0x80),
        ],
        ..Default::default()
    };
    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 0, 0, 0, 0x80), triangle])).unwrap();
    let rgba = renderer.read_target(TargetName::Display).unwrap();
    let mut intermediate = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let r = pixel(&rgba, x, y)[0];
            if r > 0 && r < 255 {
                intermediate += 1;
            }
        }
    }
    intermediate
}

fn msaa(renderer: &mut NativeRenderer<'_>) {
    let single = edge_levels(renderer, 1);
    let four = edge_levels(renderer, 4);
    println!("diagonal edge: {single} intermediate pixels without MSAA, {four} with 4x");
    assert_eq!(single, 0);
    assert!(four > 400);
}

// The C++ test's main: one device and renderer through every part, in its order.
#[test]
fn native_renderer() {
    let Some(device) = support::device() else { return };
    let mut renderer = NativeRenderer::new(&device, support::shaders()).unwrap();
    blends(&mut renderer);
    sampling(&mut renderer);
    alpha_bounds(&mut renderer);
    copies(&mut renderer);
    strips(&mut renderer);
    msaa(&mut renderer);
    println!("validation errors: {}", device.validation_errors());
    assert_eq!(device.validation_errors(), 0);
}

// The refusals the C++ renderer throws for.
#[test]
fn refusals() {
    let Some(device) = support::device() else { return };
    let mut renderer = NativeRenderer::new(&device, support::shaders()).unwrap();
    assert!(matches!(renderer.draw(&frame_of(vec![])), Err(Error::Runtime(_))));
    assert!(matches!(renderer.read_target(TargetName::Display), Err(Error::Runtime(_))));
    assert!(matches!(renderer.configure(&out(0, 448, 1)), Err(Error::Runtime(_))));
    assert!(matches!(renderer.configure(&out(640, 448, 64)), Err(Error::Runtime(_))));
    assert!(matches!(renderer.set_texture(1, 4, 4, &[0; 15]), Err(Error::Runtime(_))));
    renderer.configure(&out(WIDTH, HEIGHT, 1)).unwrap();
    let mut own = sprite(BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80);
    own.material.source = SourceKind::Target;
    own.material.source_target = TargetName::Display;
    assert!(matches!(renderer.draw(&frame_of(vec![own])), Err(Error::Logic(_))));
    let mut missing = sprite(BlendOp::Opaque, 0x80, 0x80, 0x80, 0x80);
    missing.material.source = SourceKind::Texture;
    missing.material.texture = 99;
    assert!(matches!(renderer.draw(&frame_of(vec![missing])), Err(Error::Runtime(_))));
    assert_eq!(renderer.read_target(TargetName::Store).unwrap(), vec![0u8; (WIDTH * HEIGHT * 4) as usize]);
    renderer.draw(&frame_of(vec![sprite(BlendOp::Opaque, 9, 9, 9, 0x80)])).unwrap();
    assert_eq!(pixel(&renderer.read_target(TargetName::Display).unwrap(), 1, 1)[0], 9);
    assert_eq!(device.validation_errors(), 0);
}

// `--text sharp` covers the layouts a frame's cache passed through after its first (the Browser's), so a title drawn after
// the cache was laid out again is as sharp as the rest; at the console's own size every texture is the console's.
#[test]
fn sharp_text_covers_every_glyph_layout() {
    let Some(device) = support::device() else { return };
    let resources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Resources/FNTOSD");
    let Ok(file) = std::fs::read(&resources) else {
        eprintln!("skipped: no {}", resources.display());
        return;
    };
    let font = scene::font::Font::new(&file).unwrap();
    let traced = glyph_enhance::outlines::build(&font, glyph_enhance::outlines::font_hash(&file), &[0x41, 0x42], 2);
    let mut renderer = NativeRenderer::new(&device, support::shaders()).unwrap();
    renderer.set_sharp_text(Some(std::sync::Arc::new(glyph_enhance::picture::SharpFont::traced(&traced))));
    let cache = |cell_width: i32, cells: Vec<i32>| GlyphCache { address: 0xbc100, cell_width, cell_height: 44, width: 384, log_width: 9, log_height: 9, cells };
    let (first, later) = (cache(40, vec![0x41, 0x42]), [cache(48, vec![0, 0x42, 0x41]), cache(40, vec![0x42])]);
    for (output, texels) in [(out(1280, 896, 1), 1024), (out(1920, 1344, 1), 1536), (out(640, 448, 1), 512)] {
        renderer.configure(&output).unwrap();
        renderer.set_glyph_cache(&font, &first).unwrap();
        renderer.set_glyph_layouts(&font, &later).unwrap();
        for number in [GLYPH_TEXTURE, GLYPH_LAYOUT_TEXTURE, GLYPH_LAYOUT_TEXTURE + 1] {
            assert_eq!(renderer.texture_extent(number), Some([texels, texels]), "texture {number:#x} at {}x{}", output.width, output.height);
        }
    }
    assert_eq!(device.validation_errors(), 0);
}
