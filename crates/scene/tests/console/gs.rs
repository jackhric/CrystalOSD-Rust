use std::collections::BTreeMap;

use serde_json::Value;

use scene::frame::{BlendOp, CoordinateKind, DepthTest, Frame, GlyphCache, Material, PassTopology, Sampling, SourceKind};

use super::inputs::{at, int, word};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    Triangles,
    Sprites,
    Lines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term {
    Source,
    Destination,
    Zero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Factor {
    SourceAlpha,
    DestinationAlpha,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blend {
    pub a: Term,
    pub b: Term,
    pub c: Factor,
    pub d: Term,
    pub fixed: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Test {
    Never,
    Always,
    GreaterEqual,
    Greater,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Repeat,
    Clamp,
    RegionClamp,
    RegionRepeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Address {
    pub mode: Mode,
    pub min: i32,
    pub max: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextureAlpha {
    pub constant: bool,
    pub value: u8,
    pub zero_when_black: bool,
    pub sixteen: bool,
    pub value_high: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    pub source: String,
    pub source_is_target: bool,
    pub width: u32,
    pub height: u32,
    pub projective: bool,
    pub address_u: Address,
    pub address_v: Address,
    pub bilinear: bool,
    pub alpha: TextureAlpha,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub depth: u32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub s: f32,
    pub t: f32,
    pub q: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pass {
    pub name: String,
    pub target: String,
    pub primitive: Primitive,
    pub scissor: [i32; 4],
    pub blend: Option<Blend>,
    pub antialias: bool,
    pub test: Test,
    pub write: bool,
    pub texture: Option<Texture>,
    pub skip: String,
    pub vertices: Vec<Vertex>,
}

pub struct TextureImage {
    pub id: String,
    pub width: u32,
    pub height: u32,
}

// Source/Parity/FromScene.hpp GsFrameLayout: where the frame's buffers and textures sit in GS memory.
pub struct Layout {
    pub targets: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub target_texture: [u32; 2],
    pub textures: BTreeMap<i32, TextureImage>,
}

pub fn block_name(block: u32) -> String {
    format!("fb{block:04x}")
}

// Source/Parity/FromScene.cpp glyphTextureId: TBP0, TBW, PSM 0x14, TW x TH as frame_passes.mjs names a texture.
pub fn glyph_texture_id(g: &GlyphCache) -> String {
    format!("t{:04x}-{}-20-{}x{}", g.address >> 6, g.width >> 6, g.log_width, g.log_height)
}

// Source/Parity/FromScene.cpp clockLayout (clock_frame.mjs stateWriters).
pub fn clock_layout(width: i32, height: i32, display_index: i32) -> Layout {
    let display = if display_index == 0 { ((width * height) >> 6) as u32 } else { 0 };
    let refraction = (((3 * width * height) >> 11) * 32) as u32;
    let work = (((width * height) >> 9) * 32) as u32;
    let mut textures = BTreeMap::new();
    let mut words = i64::from(width) * i64::from(height) * 5;
    for n in 0..10 {
        let log = if n == 1 { 7 } else { 6 };
        textures
            .insert(n, TextureImage { id: format!("t{:04x}-{}-0-{}x{}", (words >> 6) as u32, (1 << log) >> 6, log, log), width: 1 << log, height: 1 << log });
        words += if n == 1 { 128 * 128 } else { 64 * 64 };
    }
    Layout {
        targets: vec![block_name(display), block_name(refraction), block_name(work)],
        width: width as u32,
        height: height as u32,
        target_texture: [1 << 10, 1 << 8],
        textures,
    }
}

fn blend_of(op: BlendOp, constant: u8) -> Blend {
    use Term::*;
    let (a, b, c, d) = match op {
        BlendOp::Add => (Source, Zero, Factor::SourceAlpha, Destination),
        BlendOp::AlphaOver => (Source, Destination, Factor::SourceAlpha, Destination),
        BlendOp::Subtract => (Zero, Source, Factor::SourceAlpha, Destination),
        BlendOp::FixedOver => (Source, Destination, Factor::Fixed, Destination),
        BlendOp::FixedAdd => (Source, Zero, Factor::Fixed, Destination),
        BlendOp::AddDestinationAlpha => (Source, Zero, Factor::DestinationAlpha, Destination),
        BlendOp::SubtractFixed => (Zero, Source, Factor::Fixed, Destination),
        BlendOp::Opaque => panic!("an opaque pass has no blend"),
    };
    Blend { a, b, c, d, fixed: constant }
}

fn address_of(m: &Material, min: i32, max: i32) -> Address {
    match m.sampling {
        Sampling::Repeat => Address { mode: Mode::Repeat, min: 0, max: 0 },
        Sampling::Clamp => Address { mode: Mode::Clamp, min: 0, max: 0 },
        Sampling::ClampToRegion => Address { mode: Mode::RegionClamp, min, max },
    }
}

fn texture_of(m: &Material, layout: &Layout, frame: &Frame) -> Texture {
    let layouts = &frame.glyph_layouts;
    let later = m.texture - scene::frame::GLYPH_LAYOUT_TEXTURE;
    let (source, source_is_target, width, height) = if m.source == SourceKind::Target {
        let height = if m.source_height != 0 { m.source_height as u32 } else { layout.target_texture[1] };
        (layout.targets[m.source_target as usize].clone(), true, layout.target_texture[0], height)
    } else if m.texture == scene::frame::GLYPH_TEXTURE && !layout.textures.contains_key(&m.texture) {
        let glyphs = &frame.glyphs;
        (glyph_texture_id(glyphs), false, 1u32 << glyphs.log_width, 1u32 << glyphs.log_height)
    } else if later >= 0 && (later as usize) < layouts.len() {
        let glyphs = &layouts[later as usize];
        (glyph_texture_id(glyphs), false, 1u32 << glyphs.log_width, 1u32 << glyphs.log_height)
    } else {
        let image = layout.textures.get(&m.texture).unwrap_or_else(|| panic!("texture {} is not in the layout", m.texture));
        (image.id.clone(), false, image.width, image.height)
    };
    Texture {
        source,
        source_is_target,
        width,
        height,
        projective: m.coordinates == CoordinateKind::Projective,
        address_u: address_of(m, m.region[0], m.region[1]),
        address_v: address_of(m, m.region[2], m.region[3]),
        bilinear: m.bilinear,
        alpha: if m.colour_only { TextureAlpha { constant: true, value: 0x7f, zero_when_black: true, ..Default::default() } } else { TextureAlpha::default() },
    }
}

// Source/Parity/FromScene.cpp fromScene: the scene frame in the GS units of the parity rule.
pub fn from_scene(frame: &Frame, layout: &Layout) -> Vec<Pass> {
    frame
        .passes
        .iter()
        .map(|pass| {
            let m = &pass.material;
            let shift = if pass.half_line && frame.field != 0 { 0.5f32 } else { 0.0 };
            let skip = if pass.edge_smoothing && pass.prim_blend && pass.topology != PassTopology::Sprites {
                "antialiasing with alpha blending".to_owned()
            } else {
                String::new()
            };
            Pass {
                name: pass.name.clone(),
                target: layout.targets[pass.target as usize].clone(),
                primitive: match pass.topology {
                    PassTopology::Triangles => Primitive::Triangles,
                    PassTopology::Lines => Primitive::Lines,
                    PassTopology::Sprites => Primitive::Sprites,
                },
                scissor: pass.scissor.unwrap_or([0, 0, layout.width as i32 - 1, layout.height as i32 - 1]),
                blend: if m.blend == BlendOp::Opaque { None } else { Some(blend_of(m.blend, m.blend_constant)) },
                antialias: pass.edge_smoothing,
                test: match m.depth_test {
                    DepthTest::Always => Test::Always,
                    DepthTest::GreaterEqual => Test::GreaterEqual,
                    DepthTest::Greater => Test::Greater,
                },
                write: m.depth_write,
                texture: if m.source == SourceKind::None { None } else { Some(texture_of(m, layout, frame)) },
                skip,
                vertices: pass
                    .vertices
                    .iter()
                    .map(|v| Vertex {
                        x: v.x,
                        y: v.y - shift,
                        depth: v.z,
                        r: f32::from(v.r),
                        g: f32::from(v.g),
                        b: f32::from(v.b),
                        a: f32::from(v.a),
                        s: v.u,
                        t: v.v,
                        q: v.q,
                    })
                    .collect(),
            }
        })
        .collect()
}

fn term(v: &Value) -> Term {
    match v.as_str().unwrap() {
        "Source" => Term::Source,
        "Destination" => Term::Destination,
        "Zero" => Term::Zero,
        other => panic!("unknown blend term {other}"),
    }
}

fn address(j: &Value) -> Address {
    let mode = match at(j, "mode").as_str().unwrap() {
        "Repeat" => Mode::Repeat,
        "Clamp" => Mode::Clamp,
        "RegionClamp" => Mode::RegionClamp,
        "RegionRepeat" => Mode::RegionRepeat,
        other => panic!("unknown address mode {other}"),
    };
    Address { mode, min: int(at(j, "min")), max: int(at(j, "max")) }
}

fn number(v: &Value) -> f32 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) as f32
}

// Source/Parity/Fixture.cpp readPass, with SceneGate.cpp dumpPass: the paletted glyph texture's skip is cleared.
pub fn read_pass(p: &Value) -> Pass {
    let primitive = match at(p, "primitive").as_str().unwrap() {
        "Triangles" => Primitive::Triangles,
        "Sprites" => Primitive::Sprites,
        "Lines" => Primitive::Lines,
        other => panic!("unknown primitive {other}"),
    };
    let s = at(p, "scissor");
    let blend = match at(p, "blend") {
        Value::Null => None,
        b => Some(Blend {
            a: term(at(b, "a")),
            b: term(at(b, "b")),
            c: match at(b, "c").as_str().unwrap() {
                "SourceAlpha" => Factor::SourceAlpha,
                "DestinationAlpha" => Factor::DestinationAlpha,
                "Fixed" => Factor::Fixed,
                other => panic!("unknown blend factor {other}"),
            },
            d: term(at(b, "d")),
            fixed: int(at(b, "fixed")) as u8,
        }),
    };
    let depth = at(p, "depth");
    let test = match at(depth, "test").as_str().unwrap() {
        "Never" => Test::Never,
        "Always" => Test::Always,
        "GreaterEqual" => Test::GreaterEqual,
        "Greater" => Test::Greater,
        other => panic!("unknown depth test {other}"),
    };
    let texture = match at(p, "texture") {
        Value::Null => None,
        t => {
            let source = at(t, "source");
            let source_is_target = source.get("target").is_some();
            let a = at(t, "alpha");
            let alpha = match at(a, "mode").as_str().unwrap() {
                "Constant" => TextureAlpha {
                    constant: true,
                    value: int(at(a, "value")) as u8,
                    zero_when_black: at(a, "zeroWhenBlack").as_bool().unwrap(),
                    ..Default::default()
                },
                "Texel16" => TextureAlpha {
                    constant: false,
                    value: int(at(a, "value")) as u8,
                    zero_when_black: at(a, "zeroWhenBlack").as_bool().unwrap(),
                    sixteen: true,
                    value_high: int(at(a, "valueHigh")) as u8,
                },
                "Texel" => TextureAlpha::default(),
                other => panic!("unknown texture alpha mode {other}"),
            };
            Some(Texture {
                source: at(source, if source_is_target { "target" } else { "image" }).as_str().unwrap().to_owned(),
                source_is_target,
                width: word(at(t, "width")),
                height: word(at(t, "height")),
                projective: at(t, "coordinates").as_str() == Some("Projective"),
                address_u: address(at(t, "addressU")),
                address_v: address(at(t, "addressV")),
                bilinear: at(t, "filter").as_str() == Some("Bilinear"),
                alpha,
            })
        }
    };
    let mut skip = at(p, "skip").as_str().unwrap_or("").to_owned();
    if skip == "texture format 0x14" {
        skip.clear();
    }
    Pass {
        name: at(p, "name").as_str().unwrap().to_owned(),
        target: at(p, "target").as_str().unwrap().to_owned(),
        primitive,
        scissor: [int(&s[0]), int(&s[1]), int(&s[2]), int(&s[3])],
        blend,
        antialias: at(p, "antialias").as_bool().unwrap(),
        test,
        write: at(depth, "write").as_bool().unwrap(),
        texture,
        skip,
        vertices: at(p, "vertices")
            .as_array()
            .unwrap()
            .iter()
            .map(|v| Vertex {
                x: number(&v[0]),
                y: number(&v[1]),
                depth: word(&v[2]),
                r: number(&v[3]),
                g: number(&v[4]),
                b: number(&v[5]),
                a: number(&v[6]),
                s: number(&v[7]),
                t: number(&v[8]),
                q: number(&v[9]),
            })
            .collect(),
    }
}

fn region(mode: Mode) -> bool {
    mode == Mode::RegionClamp || mode == Mode::RegionRepeat
}

fn same_address(a: &Address, b: &Address) -> bool {
    a.mode == b.mode && (!region(a.mode) || (a.min == b.min && a.max == b.max))
}

pub fn vertex_text(v: &Vertex) -> String {
    format!("({}, {}, z {}, rgba {} {} {} {}, stq {} {} {})", v.x, v.y, v.depth, v.r, v.g, v.b, v.a, v.s, v.t, v.q)
}

pub fn same_vertex(a: &Vertex, b: &Vertex, textured: bool) -> bool {
    let position = a.x == b.x && a.y == b.y && a.depth == b.depth && a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a;
    position && (!textured || (a.s == b.s && a.t == b.t && a.q == b.q))
}

// tests/scene/SceneGate.cpp stateDifference: the first difference of state, or None.
pub fn state_difference(ours: &Pass, theirs: &Pass) -> Option<String> {
    if ours.target != theirs.target {
        return Some(format!("target {} vs {}", ours.target, theirs.target));
    }
    if ours.primitive != theirs.primitive {
        return Some(format!("primitive {:?} vs {:?}", ours.primitive, theirs.primitive));
    }
    if ours.scissor != theirs.scissor {
        return Some(format!("scissor {:?} vs {:?}", ours.scissor, theirs.scissor));
    }
    if ours.blend != theirs.blend {
        return Some(format!("blend {:?} vs {:?}", ours.blend, theirs.blend));
    }
    if ours.antialias != theirs.antialias {
        return Some("antialias".into());
    }
    if ours.test != theirs.test || ours.write != theirs.write {
        return Some(format!("depth {:?} {} vs {:?} {}", ours.test, ours.write, theirs.test, theirs.write));
    }
    match (&ours.texture, &theirs.texture) {
        (None, None) => {}
        (Some(a), Some(b)) => {
            if a.source != b.source || a.source_is_target != b.source_is_target {
                return Some(format!("texture source {} vs {}", a.source, b.source));
            }
            if a.width != b.width || a.height != b.height {
                return Some("texture size".into());
            }
            if a.projective != b.projective {
                return Some("texture coordinates".into());
            }
            if !same_address(&a.address_u, &b.address_u) || !same_address(&a.address_v, &b.address_v) {
                return Some(format!("texture address {:?} {:?} vs {:?} {:?}", a.address_u, a.address_v, b.address_u, b.address_v));
            }
            if a.bilinear != b.bilinear {
                return Some("texture filter".into());
            }
            if a.alpha.constant != b.alpha.constant || a.alpha.value != b.alpha.value || a.alpha.zero_when_black != b.alpha.zero_when_black {
                return Some(format!("texture alpha {:?} vs {:?}", a.alpha, b.alpha));
            }
        }
        _ => return Some("texture presence".into()),
    }
    if ours.skip != theirs.skip {
        return Some(format!("skip '{}' vs '{}'", ours.skip, theirs.skip));
    }
    None
}

pub fn pass_difference(ours: &Pass, theirs: &Pass) -> Option<String> {
    if let Some(state) = state_difference(ours, theirs) {
        return Some(state);
    }
    if ours.vertices.len() != theirs.vertices.len() {
        return Some(format!("vertex count {} vs {}", ours.vertices.len(), theirs.vertices.len()));
    }
    for (k, (a, b)) in ours.vertices.iter().zip(&theirs.vertices).enumerate() {
        if !same_vertex(a, b, ours.texture.is_some()) {
            return Some(format!("vertex {k} {} vs {}", vertex_text(a), vertex_text(b)));
        }
    }
    None
}

pub fn label(pass: &Pass) -> String {
    let source = pass.texture.as_ref().map_or("flat".to_owned(), |t| t.source.clone());
    format!("{} {:?} {}", pass.name, pass.primitive, source)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TextCount {
    pub font: usize,
    pub hint: usize,
    pub absorbed: usize,
}

impl TextCount {
    pub fn is_text(pass: &Pass) -> bool {
        let Some(t) = &pass.texture else { return false };
        if t.source_is_target {
            return false;
        }
        if pass.primitive == Primitive::Triangles {
            return t.source.ends_with("-20-9x9");
        }
        pass.primitive == Primitive::Sprites && (t.source == "t2ec0-1-0-6x6" || t.source == "t2e80-1-0-6x6")
    }

    fn take(&mut self, pass: &Pass) {
        if !Self::is_text(pass) {
            return;
        }
        if pass.primitive == Primitive::Triangles {
            self.font += 1;
        } else {
            self.hint += 1;
        }
    }
}

// tests/scene/SceneGate.cpp compareFrame: every dump pass is a scene pass, in order, state and every vertex equal.
// `absorb_text`: a text pass of the dump the scene does not produce is counted, not failed.
pub fn compare_frame(ours: &[Pass], dump: &Value, count: &mut TextCount, at_frame: &str, absorb_text: bool) -> Result<usize, String> {
    let mut j = 0;
    for p in at(dump, "passes").as_array().unwrap() {
        let theirs = read_pass(p);
        if absorb_text && TextCount::is_text(&theirs) && (j >= ours.len() || pass_difference(&ours[j], &theirs).is_some()) {
            count.absorbed += 1;
            continue;
        }
        let Some(mine) = ours.get(j) else { return Err(format!("{at_frame}: dump {} after the last scene pass", label(&theirs))) };
        if let Some(difference) = pass_difference(mine, &theirs) {
            return Err(format!("{at_frame}: scene pass {j} ({}) differs from dump {}: {difference}", mine.name, label(&theirs)));
        }
        count.take(mine);
        j += 1;
    }
    if j != ours.len() {
        return Err(format!("{at_frame}: scene pass {j} ({}) is not in the dump", ours[j].name));
    }
    Ok(j)
}
