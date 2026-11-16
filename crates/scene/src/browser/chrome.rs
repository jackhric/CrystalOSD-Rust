// facts/browser-chrome.md: what the Browser's frame body (HDD OSD 1.10U 0x0024EB90, ROM 2.30 0x00247FD8) sends apart from the
// card row, as passes. The draw environment, texture environment and blend packets draw nothing and are not passes. Text is
// kept as runs (string, colour, alpha, where it falls among the passes): its glyph geometry is not stated by the facts.

use super::shell::Fade;
use super::strings::Place;
use crate::frame::{BlendOp, Frame, Material, Pass, PassTopology, Sampling, SourceKind, TargetName, TextureSet, Vertex};

// browser_texture_indices (0x002BD160): the TEXB sheets the chrome draws, as Browser texture numbers (TEXIMAGE members 53..60).
pub const SHEET_BUTTONS: i32 = 3;
pub const SHEET_TILES: i32 = 7;

// The tile layers' offsets, table HDD OSD 0x00366148 (ROM 0x0029BCD0), section 2 "Tile layers".
pub const TILE_OFFSETS: [i32; 8] = [0, 3, 9, 14, 1, 7, 12, 15];
// The guide pictures' x for English, table HDD OSD 0x002BD2E0 + lang * 16 (ROM 0x002AD860).
pub const GUIDE_X_ENGLISH: [i32; 3] = [0x18, 0xd5, 0x14f];

pub const TITLE_LEFT_COLOUR: u32 = 0x606060;
pub const TITLE_RIGHT_COLOUR: u32 = 0x006e6e;
pub const TEXT_COLOUR: u32 = 0x606060;

// facts/text.md section 4, fontFilter (0x00212D78): the glyphs a string draws. UTF-8 characters draw one each; 0x0A and 0x09 move
// the pen; the escapes 07 c N, 07 a NNN, 07 p XX, 07 r N.NN, 07 y +HH and 07 s draw nothing; 07 o NNN draws the mark 0xD800 + NNN.
pub fn glyph_count(text: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < text.len() {
        let b = text[i];
        if b == 0x07 && i + 1 < text.len() {
            let skip = match text[i + 1] {
                b'c' => 3,
                b'a' => 5,
                b'p' => 4,
                b'r' => 6,
                b'y' => 5,
                b's' => 2,
                b'o' => {
                    n += 1;
                    5
                }
                _ => 2,
            };
            i += skip;
            continue;
        }
        if b == 0x0a || b == 0x09 {
            i += 1;
            continue;
        }
        n += 1;
        i += if b < 0x80 {
            1
        } else if b >> 5 == 0b110 {
            2
        } else if b >> 4 == 0b1110 {
            3
        } else {
            4
        };
    }
    n
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub label: &'static str,
    pub text: Vec<u8>,
    pub colour: u32,
    // None: the alpha the console uses here is not stated (an accept rule: one alpha up to 0x80 over the run).
    pub alpha: Option<i32>,
    pub before_pass: usize,
    // The caller that draws it (strings.rs); None where the glyphs are not drawn (ROM 2.30, a by-shape check).
    pub place: Option<Place>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BrowserFrame {
    pub frame: Frame,
    pub texts: Vec<TextRun>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogDraw {
    pub n: i32,
    pub step: i32,
    pub kind: i32,
    pub abort: bool,
    pub lines: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideSlot {
    pub s4: i32,
    pub counter: i32,
    pub label: Vec<u8>,
}

// `xs`: the pictures' x of slots 0..2 for the language; `slot3_x` the fourth slot's, width - width(label) - 0x34, which the
// console stores when it measures the label; `dy` 2 on HDD OSD, 1 on ROM 2.30.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guide {
    pub slots: Vec<GuideSlot>,
    pub swap: bool,
    pub xs: [i32; 3],
    pub slot3_x: i32,
    pub dy: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Titles {
    pub left: Vec<u8>,
    pub left_free: Option<Vec<u8>>,
    pub right_shown: bool,
    pub right: Vec<u8>,
    pub right_alpha: Option<i32>,
    pub right_second: Vec<u8>,
}

// `fade` as 0x0024CEB0 reads it; `tile_counter` the fade counter as the background 0x00248A90 reads it; `page_in` D_0037072C.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromeState {
    pub width: i32,
    pub height: i32,
    pub field: i32,
    pub display_index: i32,
    pub fade: Fade,
    pub tile_counter: i32,
    pub page_in: i32,
    pub titles: Titles,
    pub dialog: Option<DialogDraw>,
    pub guide: Guide,
    pub rom: bool,
}

// origin(size) = trunc((0x1000 - size) / 2) << 4: the screen's corner in GS primitive space, section 2 "Draw environment".
fn origin(size: i32) -> i32 {
    ((0x1000 - size) / 2) << 4
}

fn sixteenths(v: i32) -> f32 {
    v as f32 / 16.0
}

fn vertex(x: i32, y: i32, z: u32, rgba: [u8; 4], u: i32, v: i32) -> Vertex {
    Vertex { x: sixteenths(x), y: sixteenths(y), z, u: sixteenths(u), v: sixteenths(v), q: 1.0, r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] }
}

fn flat(blend: BlendOp, constant: u8) -> Material {
    Material { blend, blend_constant: constant, ..Material::default() }
}

fn sheet(texture: i32, blend: BlendOp, constant: u8, bilinear: bool) -> Material {
    Material { source: SourceKind::Texture, texture, sampling: Sampling::Clamp, bilinear, blend, blend_constant: constant, ..Material::default() }
}

struct Out {
    passes: Vec<Pass>,
    texts: Vec<TextRun>,
}

impl Out {
    fn pass(&mut self, name: &str, topology: PassTopology, material: Material, vertices: Vec<Vertex>) {
        self.passes.push(Pass { name: name.to_owned(), target: TargetName::Display, topology, material, half_line: true, vertices, ..Pass::default() });
    }

    fn text(&mut self, label: &'static str, text: &[u8], colour: u32, alpha: Option<i32>, place: Option<Place>) {
        self.texts.push(TextRun { label, text: text.to_vec(), colour, alpha, before_pass: self.passes.len(), place });
    }

    // A sprite in GS primitive space (12.4, the screen's origin added): both corners carry the second corner's depth, as the GS
    // draws a sprite.
    #[allow(clippy::too_many_arguments)]
    fn sprite(&mut self, name: &str, material: Material, corners: [i32; 4], uv: [i32; 4], z: u32, rgba: [u8; 4], o: (i32, i32)) {
        let v0 = vertex(corners[0] - o.0, corners[1] - o.1, z, rgba, uv[0], uv[1]);
        let v1 = vertex(corners[2] - o.0, corners[3] - o.1, z, rgba, uv[2], uv[3]);
        self.pass(name, PassTopology::Sprites, material, vec![v0, v1]);
    }
}

fn grey(alpha: i32) -> [u8; 4] {
    [0x80, 0x80, 0x80, alpha as u8]
}

// 0x0024C928: sceGsSetDefClear at (0x7FE - width / 2, 0x800 - height / 2), width x height, black, alpha 0x80, z 0.
fn clear(s: &ChromeState, out: &mut Out, o: (i32, i32)) {
    let x0 = 0x7fe - s.width / 2;
    let y0 = 0x800 - s.height / 2;
    out.sprite("browser clear", flat(BlendOp::Opaque, 0), [x0 << 4, y0 << 4, (x0 + s.width) << 4, (y0 + s.height) << 4], [0; 4], 0, [0, 0, 0, 0x80], o);
}

// 0x00248608 -> 0x0024F890: PRIM 0x10C (strip, Gouraud), z 4, corners (0,0) (w,0) (0,h) (w,h) in the colours AC AC AC, 52 52 48,
// 7A 7A 7A, 2A 2A 20, alpha 0x80.
fn gradient(s: &ChromeState, out: &mut Out) {
    let colours = [[0xac, 0xac, 0xac], [0x52, 0x52, 0x48], [0x7a, 0x7a, 0x7a], [0x2a, 0x2a, 0x20]];
    let corners = [(0, 0), (s.width, 0), (0, s.height), (s.width, s.height)];
    let v: Vec<Vertex> = corners.iter().zip(colours).map(|(&(x, y), c)| vertex(x << 4, y << 4, 4, [c[0], c[1], c[2], 0x80], 0, 0)).collect();
    let material = Material { gouraud: true, ..flat(BlendOp::Opaque, 0) };
    out.pass("browser gradient", PassTopology::Triangles, material, vec![v[0], v[1], v[2], v[1], v[2], v[3]]);
}

// 0x00248A90 -> 0x00248910 while the fade counter c is not 0: layer k additive (blend mode 6, Cs * 0x80 + Cd) when k < (c >> 2) + 4,
// else subtractive (mode 7, Cd - Cs * 0x80); four rows of ten 64-pixel sprites shifted by tile[k] + 8 sixteenths, UV 0..0x3F0,
// white at alpha 0x80, z 5, TEXBICHI with TEX1 1 (nearest).
fn tiles(s: &ChromeState, out: &mut Out, o: (i32, i32)) {
    let c = s.tile_counter;
    assert!(c >= 0, "a negative fade counter is not modelled");
    if c == 0 {
        return;
    }
    let n = (c >> 2) + 4;
    for (k, offset) in TILE_OFFSETS.iter().enumerate() {
        let blend = if (k as i32) < n { BlendOp::FixedAdd } else { BlendOp::SubtractFixed };
        let off = offset + 8;
        let mut vertices = Vec::with_capacity(80);
        for row in 0..4 {
            for col in 0..10 {
                let x0 = (col << 10) + o.0 + off;
                let y0 = (row << 10) + o.1 + off;
                vertices.push(vertex(x0 - o.0, y0 - o.1, 5, grey(0x80), 0, 0));
                vertices.push(vertex(x0 + 0x400 - o.0, y0 + 0x400 - o.1, 5, grey(0x80), 0x3f0, 0x3f0));
            }
        }
        out.pass("browser tiles", PassTopology::Sprites, sheet(SHEET_TILES, blend, 0x80, false), vertices);
    }
}

// 0x00250E20..0x00250E70: the top left title's alpha from the page fade counter (NTSC).
pub fn title_alpha(c: i32) -> i32 {
    if c < 0xb { 0 } else { ((c - 0xa) * 0x80 - (c - 0xa)) / (0x1a - 0xa) }
}

// 0x0025BD18, kind 0: n = clamp(n + step, 0, 0x1E); the panel's blend mode 3 with FIX = n * 0x54 / 0x1E, a full-screen black box
// (0x0024F6E8, alpha 0x80, z 4), the lines up to the first empty one at alpha (n << 7) / 0x1E. The abort draws nothing.
fn dialog(s: &ChromeState, d: &DialogDraw, out: &mut Out, o: (i32, i32)) {
    assert_eq!(d.kind, 0, "dialog kind {} is not modelled", d.kind);
    if d.abort {
        return;
    }
    let n = (d.n + d.step).min(0x1e);
    assert!(n >= 0, "a closing dialog is not modelled");
    let fix = (n * 0x54) / 0x1e;
    out.sprite("browser dialog", flat(BlendOp::FixedOver, fix as u8), [o.0, o.1, (s.width << 4) + o.0, (s.height << 4) + o.1], [0; 4], 4, [0, 0, 0, 0x80], o);
    let alpha = (n << 7) / 0x1e;
    let mut place = (!s.rom).then(|| Place::Dialog { lines: d.lines.clone(), n });
    for line in &d.lines {
        if line.is_empty() {
            break;
        }
        out.text("dialog line", line, TEXT_COLOUR, Some(alpha), place.take());
    }
}

// 0x0024D0C8: two black sprites (no blend), z 0xFFFFE, over the top and bottom 0x1C lines.
fn bands(s: &ChromeState, out: &mut Out, o: (i32, i32)) {
    let band = 0x1c;
    let material = flat(BlendOp::Opaque, 0);
    let z = 0xffffe;
    let black = [0, 0, 0, 0x80];
    let top = [vertex(0, 0, z, black, 0, 0), vertex(s.width << 4, band << 4, z, black, 0, 0)];
    let bottom = [vertex(0, (s.height - band) << 4, z, black, 0, 0), vertex(s.width << 4, s.height << 4, z, black, 0, 0)];
    let _ = o;
    out.pass("browser bands", PassTopology::Sprites, material, vec![top[0], top[1], bottom[0], bottom[1]]);
}

// 0x002524D0 -> 0x00252398: per slot with a nonzero counter, its label, then its picture: 24 x 12 at x = xs[s2] (s2 the picture,
// slots 1 and 2 swapped when the video mode is not 0), y = 0xC9 - dy + 0.5, UV of the quadrant (0.5, 0.5), (0.5, 32.5),
// (32.5, 32.5), (32.5, 0.5) + 31 texels, alpha min(counter * 8, 0x80), TEXBBTTN bilinear, blend mode 1, z 8.
fn guide(s: &ChromeState, out: &mut Out, o: (i32, i32)) {
    let g = &s.guide;
    for slot in &g.slots {
        let s2 = if g.swap && slot.s4 == 1 {
            2
        } else if g.swap && slot.s4 == 2 {
            1
        } else {
            slot.s4
        };
        let alpha = (slot.counter * 8).min(0x80);
        let place = (!s.rom).then(|| Place::Guide { s4: slot.s4, counter: slot.counter, label: slot.label.clone() });
        out.text("guide label", &slot.label, TEXT_COLOUR, Some(alpha), place);
        let u = if s2 >= 2 { 0x208 } else { 8 };
        let v = if s2 == 1 || s2 == 2 { 0x208 } else { 8 };
        let px = if s2 == 3 { g.slot3_x } else { g.xs[s2 as usize] };
        let x = o.0 + (px << 4);
        let y = o.1 + ((0xc9 - g.dy) << 4);
        out.sprite(
            "browser guide",
            sheet(SHEET_BUTTONS, BlendOp::AlphaOver, 0x80, true),
            [x, y + 8, x + 0x180, y + 0xc8],
            [u, v, u | 0x1f0, v | 0x1f0],
            8,
            [0x80, 0x80, 0x80, alpha as u8],
            o,
        );
    }
}

// 0x0024CEB0: blend mode 1, PRIM 0x146, colour k at the alpha of the pass, (0,0)-(w,h), z 0xFFFFF then 0xFFFFD.
fn fade(s: &ChromeState, alpha: i32, out: &mut Out, o: (i32, i32)) {
    let k = s.fade.colour as u8;
    out.sprite(
        "browser fade",
        flat(BlendOp::AlphaOver, 0x80),
        [o.0, o.1, (s.width << 4) + o.0, (s.height << 4) + o.1],
        [0; 4],
        0xffffd,
        [k, k, k, alpha as u8],
        o,
    );
}

// The frame in the master draw's order (0x00253860): clear, background, [card row], titles, dialog, bands, guide, fade.
// `fade_alpha` is the alpha 0x0024CEB0 draws with in this pass (None: it does not draw).
pub fn chrome_frame(s: &ChromeState, fade_alpha: Option<i32>) -> BrowserFrame {
    let o = (origin(s.width), origin(s.height));
    let mut out = Out { passes: Vec::new(), texts: Vec::new() };
    clear(s, &mut out, o);
    gradient(s, &mut out);
    tiles(s, &mut out, o);
    let t = &s.titles;
    if s.rom {
        out.text("titles", &t.left, TITLE_LEFT_COLOUR, Some(title_alpha(s.page_in)), None);
        out.text("titles", &t.right, TITLE_RIGHT_COLOUR, t.right_alpha, None);
    } else {
        let left_colour = if t.left_free.is_some() { TITLE_RIGHT_COLOUR } else { TITLE_LEFT_COLOUR };
        out.text(
            "title left",
            &t.left,
            left_colour,
            Some(title_alpha(s.page_in)),
            Some(Place::TitleLeft { record: t.left.clone(), free: t.left_free.clone() }),
        );
        if let Some(free) = &t.left_free {
            out.text("free line", free, TITLE_RIGHT_COLOUR, Some(title_alpha(s.page_in)), None);
        }
        if t.right_shown {
            let place = Place::TitleRight { first: t.right.clone(), second: t.right_second.clone(), alpha: t.right_alpha.unwrap_or(0x80) };
            out.text("title right", &t.right, TITLE_RIGHT_COLOUR, t.right_alpha, Some(place));
            out.text("title right, second string", &t.right_second, TITLE_RIGHT_COLOUR, t.right_alpha, None);
        }
    }
    if let Some(d) = &s.dialog {
        dialog(s, d, &mut out, o);
    }
    bands(s, &mut out, o);
    guide(s, &mut out, o);
    if let Some(alpha) = fade_alpha {
        fade(s, alpha, &mut out, o);
    }
    BrowserFrame {
        frame: Frame {
            width: s.width,
            height: s.height,
            field: s.field,
            display_index: s.display_index,
            passes: out.passes,
            texture_set: TextureSet::Browser,
            ..Frame::default()
        },
        texts: out.texts,
    }
}
