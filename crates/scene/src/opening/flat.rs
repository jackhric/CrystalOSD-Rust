use std::marker::PhantomData;

use crate::arithmetic::{Arithmetic, as_float};
use crate::frame::{BlendOp, CoordinateKind, DepthTest, Material, Pass, PassTopology, Sampling, SourceKind, TargetName, Vertex};

// The arguments of one module frame's flat draws: the page drawn follows the counter's parity (func_0021CF38: odd is page 0;
// `displayIndex` is carried for the caller), `field` the video field, `fadeAlpha` the argument func_0021D848 receives (it caps it at
// 0x80), `blurLevel` func_0021D3D0's trip count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatInputs {
    pub counter: i32,
    pub display_index: i32,
    pub field: i32,
    pub logo_alpha: i32,
    pub fade_alpha: i32,
    pub blur_level: i32,
    pub fill_sprites: i32,
}

impl Default for FlatInputs {
    fn default() -> Self {
        Self { counter: 0, display_index: 0, field: 0, logo_alpha: 0, fade_alpha: 0, blur_level: 0, fill_sprites: 2 }
    }
}

// facts/opening.md 4.4, 4.5; verify_opening_flat.mjs, verify_opening_ghost.mjs, verify_opening_overlays_v2.mjs. NTSC 640 x 224.
// A call adds the passes of its packets that draw: the `Framebuffer` clear sprites land outside the scissor and are not here.
// OpeningProcess's SCISSOR_1 write (vif1SetSCISSOR_1 with 1, 1, W - 2, H - 2): the towers and the ghost are drawn under it.
pub const PROCESS_SCISSOR: [i32; 4] = [1, 1, 638, 222];

const WIDTH: i32 = 640;
const HEIGHT: i32 = 224;
const FARTHEST: u32 = 0xffffff;
const PIXEL: f32 = 0.0625;
// The copy reads the page through a 1024 x 512 texture (TEX0 of the dump's draws; the helper that sets it was not read), every other target read is 1024 x 256.
const COPY_SOURCE_HEIGHT: i32 = 512;

// verify_opening_flat.mjs: the texture colour at 0x00365118 and the bars' colour at 0x00365128
const TEXTURE_COLOUR: u8 = 0x80;
const BAR_COLOUR: [u8; 4] = [255, 255, 255, 128];
// 0x00370030: the view-screen shape (ax, ay) the bars' height comes from
const SHAPE_X: u32 = 0x3f800000;
const SHAPE_Y: u32 = 0x3eea4e16;
// facts/opening.md 4.4: D_00365158's NTSC rectangles (x, y, w, h) and D_003653A8's source rectangle
const LOGO_PLACE: [[i32; 4]; 2] = [[120, 105, 256, 16], [326, 105, 256, 16]];
const LOGO_SOURCE: [i32; 4] = [0, 1, 256, 30];

fn corner(x: f32, y: f32, z: u32, u: f32, v: f32, r: u8, g: u8, b: u8, a: u8) -> Vertex {
    Vertex { x, y, z, u, v, q: 1.0, r, g, b, a }
}

// pktSetTexRect (0x0021B7A0): half a texel in at the first corner, half a pixel in at the second.
fn textured_sprite(pass: &mut Pass, place: &[i32; 4], source: &[i32; 4], z: u32, grey: u8, alpha: u8) {
    pass.vertices.push(corner(place[0] as f32, place[1] as f32, z, source[0] as f32 + 0.5, source[1] as f32 + 0.5, grey, grey, grey, alpha));
    pass.vertices.push(corner(
        (place[0] + place[2]) as f32 - 0.5,
        (place[1] + place[3]) as f32 - 0.5,
        z,
        (source[0] + source[2]) as f32,
        (source[1] + source[3]) as f32,
        grey,
        grey,
        grey,
        alpha,
    ));
}

// pktSetFlatRect: the second corner one sixteenth in.
fn flat_sprite(pass: &mut Pass, rect: &[i32; 4], colour: &[u8; 4], z: u32) {
    pass.vertices.push(corner(rect[0] as f32, rect[1] as f32, z, 0.0, 0.0, colour[0], colour[1], colour[2], colour[3]));
    pass.vertices.push(corner((rect[0] + rect[2]) as f32 - PIXEL, (rect[1] + rect[3]) as f32 - PIXEL, z, 0.0, 0.0, colour[0], colour[1], colour[2], colour[3]));
}

fn from_target(target: TargetName, colour_only: bool) -> Material {
    Material {
        source: SourceKind::Target,
        source_target: target,
        colour_only,
        coordinates: CoordinateKind::Texel,
        sampling: Sampling::Repeat,
        bilinear: true,
        depth_test: DepthTest::GreaterEqual,
        depth_write: false,
        ..Default::default()
    }
}

fn sprites(name: &str, target: TargetName, material: &Material, half_line: bool) -> Pass {
    Pass { name: name.into(), target, topology: PassTopology::Sprites, material: *material, half_line, ..Default::default() }
}

pub struct Flat<A>(PhantomData<A>);

impl<A: Arithmetic> Flat<A> {
    // The fill at the frame's start (OpeningProcess).
    // The black fill that opens every frame in the dump (measured, writer not read; two sprites; one in the first frame after a cold boot (opening-full counter 1; a saved-state run has no dump frame for its first call)); OpeningProcess's own SCISSOR_1 write (vif1SetSCISSOR_1 with 1, 1, W - 2, H - 2) is the ghost's Pass::scissor
    pub fn scissor(input: &FlatInputs, out: &mut Vec<Pass>) {
        let m = Material { depth_test: DepthTest::Always, depth_write: true, ..Default::default() };
        let mut pass = sprites("scissor", TargetName::Display, &m, true);
        for _ in 0..input.fill_sprites {
            pass.vertices.push(corner(0.0, 0.0, 0, 0.0, 0.0, 0, 0, 0, 0));
            pass.vertices.push(corner(WIDTH as f32, HEIGHT as f32, 0, 0.0, 0.0, 0, 0, 0, 0));
        }
        out.push(pass);
    }

    pub fn scissor_default(out: &mut Vec<Pass>) {
        Self::scissor(&FlatInputs::default(), out);
    }

    // func_0021D140(1, 2, 0x50, 0xFFFFFF, 0x80): the store over the frame, FixedOver 0x50.
    // verify_opening_ghost.mjs verify (ghost)
    pub fn ghost(_: &FlatInputs, out: &mut Vec<Pass>) {
        let mut m = from_target(TargetName::Store, true);
        m.blend = BlendOp::FixedOver;
        m.blend_constant = 0x50;
        m.depth_test = DepthTest::Always;
        let mut pass = sprites("ghost", TargetName::Display, &m, true);
        pass.scissor = Some(PROCESS_SCISSOR);
        let place = [0, 0, WIDTH, HEIGHT];
        let source = [0, 0, WIDTH >> 1, HEIGHT];
        textured_sprite(&mut pass, &place, &source, FARTHEST, 0x80, 0x80);
        out.push(pass);
    }

    // func_0021CF38: the page just drawn at half width into the store.
    // verify_opening_flat.mjs expect (frame copy)
    pub fn copy_to_store(_: &FlatInputs, out: &mut Vec<Pass>) {
        let mut material = from_target(TargetName::Display, false);
        material.source_height = COPY_SOURCE_HEIGHT;
        let mut pass = sprites("copy", TargetName::Store, &material, false);
        let place = [0, 0, WIDTH >> 1, HEIGHT];
        let source = [0, 0, WIDTH, HEIGHT];
        textured_sprite(&mut pass, &place, &source, FARTHEST, TEXTURE_COLOUR, TEXTURE_COLOUR);
        out.push(pass);
    }

    // func_0021D3D0: `blurLevel` round trips through Extra, two passes each.
    // verify_opening_flat.mjs expect (blur)
    pub fn blur(input: &FlatInputs, out: &mut Vec<Pass>) {
        let n = input.blur_level;
        let full = [0, 0, WIDTH, HEIGHT];
        for i in 0..n {
            let shrink = i * (n - 1);
            let small = [0, 0, ((WIDTH * 7) >> 3) - 1 - shrink, ((HEIGHT * 7) >> 3) - 1 - shrink];
            let mut into = sprites("blur into extra", TargetName::Extra, &from_target(TargetName::Display, false), false);
            textured_sprite(&mut into, &small, &full, FARTHEST, TEXTURE_COLOUR, TEXTURE_COLOUR);
            out.push(into);
            let mut back = sprites("blur back", TargetName::Display, &from_target(TargetName::Extra, false), false);
            textured_sprite(&mut back, &full, &small, FARTHEST, TEXTURE_COLOUR, TEXTURE_COLOUR);
            out.push(back);
        }
    }

    // func_0021D848: the black rectangle with alpha capped at 0x80.
    // verify_opening_flat.mjs expect (fade rectangle, mode 'B')
    pub fn fade(input: &FlatInputs, out: &mut Vec<Pass>) {
        let m = Material { blend: BlendOp::AlphaOver, depth_test: DepthTest::Always, depth_write: false, ..Default::default() };
        let mut pass = sprites("fade", TargetName::Display, &m, true);
        let alpha = (if input.fade_alpha >= 0 && input.fade_alpha < 0x81 { input.fade_alpha } else { 0x80 }) as u8;
        let rect = [0, 0, WIDTH, HEIGHT];
        let colour = [0, 0, 0, alpha];
        flat_sprite(&mut pass, &rect, &colour, FARTHEST);
        out.push(pass);
    }

    // func_0021D990: texture 0, rows 1..30 and 33..62 as the NTSC rectangles 0 and 1.
    // verify_opening_overlays_v2.mjs verify (logo)
    pub fn logo(input: &FlatInputs, out: &mut Vec<Pass>) {
        let m = Material {
            source: SourceKind::Texture,
            texture: 0,
            coordinates: CoordinateKind::Texel,
            sampling: Sampling::Repeat,
            bilinear: true,
            blend: BlendOp::AlphaOver,
            depth_test: DepthTest::GreaterEqual,
            depth_write: false,
            blend_constant: input.logo_alpha as u8,
            ..Default::default()
        };
        let mut pass = sprites("logo", TargetName::Display, &m, true);
        let alpha = input.logo_alpha as u8;
        textured_sprite(&mut pass, &LOGO_PLACE[0], &LOGO_SOURCE, 0xfffffe, 0x80, alpha);
        let lower = [LOGO_SOURCE[0], LOGO_SOURCE[1] + 0x20, LOGO_SOURCE[2], LOGO_SOURCE[3]];
        textured_sprite(&mut pass, &LOGO_PLACE[1], &lower, 0xfffffe, 0x80, alpha);
        out.push(pass);
    }

    // func_0021D6C0: the letterbox bars, SubtractFixed 0x80.
    // verify_opening_flat.mjs expect (letterbox bars)
    pub fn bars(_: &FlatInputs, out: &mut Vec<Pass>) {
        let m = Material { blend: BlendOp::SubtractFixed, blend_constant: 0x80, depth_test: DepthTest::Always, depth_write: false, ..Default::default() };
        let mut pass = sprites("bars", TargetName::Display, &m, true);
        let open = Self::picture_height();
        let bar = Self::bar_rows();
        let above = [0, 0, WIDTH, bar];
        let below = [0, bar + open, WIDTH, bar];
        flat_sprite(&mut pass, &above, &BAR_COLOUR, FARTHEST);
        flat_sprite(&mut pass, &below, &BAR_COLOUR, FARTHEST);
        out.push(pass);
    }

    // The helpers' own arguments, for tests: the page address in 64-word blocks (0 or 640 x 224 / 64), the picture's height, a bar's rows.
    pub fn page(input: &FlatInputs) -> i32 {
        if (input.counter & 1) != 0 { 0 } else { (WIDTH * HEIGHT) >> 6 }
    }

    pub fn blur_which(input: &FlatInputs) -> i32 {
        input.counter & 1
    }

    pub fn picture_height() -> i32 {
        let width = (WIDTH * 9) as f32;
        A::to_int(A::div(A::mul(width, as_float(SHAPE_Y)), A::mul(as_float(SHAPE_X), 16.0)))
    }

    pub fn bar_rows() -> i32 {
        let x = HEIGHT - Self::picture_height() + 1;
        (x + ((x as u32) >> 31) as i32) >> 1
    }
}
