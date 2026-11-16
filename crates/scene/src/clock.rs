use std::sync::Arc;

use crate::Result;
use crate::arithmetic::Arithmetic;
use crate::camera::Camera;
use crate::clock_state::{ClockLogic, ClockState, ClockTime, Colour};
use crate::console_clock::DateWords;
use crate::cubes::{CubeDraw, CubeFrameInputs, CubeSend, Cubes};
use crate::font::Font;
use crate::frame::{BlendOp, CoordinateKind, DepthTest, Frame, GLYPH_TEXTURE, Material, Pass, PassTopology, Sampling, SourceKind, TargetName, Vertex};
use crate::frame_head::{AlphaMode, Coordinates, FrameHead, HeadDraw, HeadInputs, HeadState, HeadVertex, Part, Source, Target, Topology, TubeConstants};
use crate::menu_types::{ConfigItems, CubeState, MenuExternals, MenuWorld, MenusState, pad};
use crate::menus::{Menus, MenusOptions};
use crate::orbs::{ORB_COUNT, OrbDraw, OrbInputs, OrbSprite, OrbState, OrbVertex, Orbs};
use crate::program_image::ProgramImage;
use crate::rods::{DepthNode, RodMesh, RodRecord, RodSend, RodSendKind, RodVertex, Rods, RodsInput, insert_by_depth};
use crate::sound::SoundCommand;
use crate::text::{
    ClockItems, PagesInputs, StringRun, Text, TextDrawKind, TextFrame, TextFrameInputs, TextInputs, TextRamps, menus_at_pages, pages_of, text_ramps_of,
};

// What the menus add to the clock screen: their state, the cubes' state and mesh, the configuration items (item 0 is
// items[0]) and the options.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MenusInputs {
    pub menus: MenusState,
    pub cubes: CubeState,
    pub cube_mesh: RodMesh,
    pub items: ConfigItems,
    pub options: MenusOptions,
}

// The clock's state at a frame's entry: what the logic, the head, the rods and the orbs carry,
// and what stays constant (clear colours, tube constants, orb constants and colour, screen size).
// ClockState owns eased.fraction and vignetteRamp; the copies in OrbState and HeadState are not read.
// The display clear (clock_frame.mjs stateWriters display): the clock writes `clear_colour` into the clear of
// display environment 1 only; a frame drawn to buffer 0 is cleared with environment 0's own colour, set when
// the buffers were set up (`first_display_clear`).
#[derive(Debug, Clone)]
pub struct ClockInputs {
    pub state: ClockState,
    pub head: HeadState,
    pub rod_template: RodRecord,
    pub orbs: OrbState,
    pub mesh: RodMesh,
    pub clear_colour: Colour,
    pub first_display_clear: Colour,
    pub tube: TubeConstants,
    pub minute_factor: f32,
    pub fraction_easing: f32,
    pub orb_colour: Colour,
    pub has_entry_data: bool,
    pub wide: i32,
    pub orb_random: [i32; ORB_COUNT],
    pub orb_colours: [Colour; ORB_COUNT],
    pub width: i32,
    pub height: i32,
    // The text (facts/text.md): the font file FNTOSD and the program's ELF, and the font state carried. Without the
    // two files the frame has no text.
    pub font: Option<Arc<Font>>,
    pub program: Option<Arc<ProgramImage>>,
    pub text: TextInputs,
    pub menus: Option<MenusInputs>, // absent: the clock screen alone, as before
    pub pass_names: bool,           // false: the passes carry no names (labels, trace and parity read them)
}

impl Default for ClockInputs {
    fn default() -> Self {
        Self {
            state: ClockState::default(),
            head: HeadState::default(),
            rod_template: RodRecord::default(),
            orbs: OrbState::default(),
            mesh: RodMesh::default(),
            clear_colour: [0; 4],
            first_display_clear: [0; 4],
            tube: TubeConstants::default(),
            minute_factor: 0.0,
            fraction_easing: 0.0,
            orb_colour: [0; 4],
            has_entry_data: false,
            wide: 0,
            orb_random: [0; ORB_COUNT],
            orb_colours: [[0; 4]; ORB_COUNT],
            width: 640,
            height: 224,
            font: None,
            program: None,
            text: TextInputs::default(),
            menus: None,
            pass_names: true,
        }
    }
}

// What comes from outside the clock each frame: the time keeper, the field flag, the display buffer
// drawn to, configuration item 0, and configuration items 6 to 0xB (the date and time the text shows).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameInputs {
    pub time: ClockTime,
    pub field: i32,
    pub display_index: i32,
    pub item0: i32,
    pub items: ClockItems,
    pub menu: MenuExternals,               // pad, disc, configDirty, rtcMirror, mechaconParam
    pub config_items: Option<ConfigItems>, // items 1..0x13 external; item 0 the model's when the gate is modelled
    pub time_filled: Option<i32>,          // the time keeper's word (verify_frame.mjs EXTERNAL)
    pub text_ramps: Option<TextRamps>,     // the words of the text's alpha rules the menus' unmodelled code writes (panel flags, the dialog ramp)
    pub thread_step: bool,                 // run between() first; false for a frame recorded after it (frame 0)
}

impl Default for FrameInputs {
    fn default() -> Self {
        Self {
            time: ClockTime::default(),
            field: 0,
            display_index: 0,
            item0: 0,
            items: ClockItems::default(),
            menu: MenuExternals::default(),
            config_items: None,
            time_filled: None,
            text_ramps: None,
            thread_step: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Header {
    name: String,
    target: TargetName,
    topology: PassTopology,
    material: Material,
    edge_smoothing: bool,
    half_line: bool,
    prim_blend: bool,
}

#[allow(clippy::too_many_arguments)]
fn header(name: String, target: TargetName, topology: PassTopology, material: Material, edge_smoothing: bool, half_line: bool, prim_blend: bool) -> Header {
    Header { name, target, topology, material, edge_smoothing, half_line, prim_blend }
}

// Primitives into passes: a pass is a run of primitives under one state, as the GS draws them. A strip becomes
// its triangles (i, i + 1, i + 2) or lines (i, i + 1); a flat primitive takes its last vertex's colour and a
// sprite its second corner's colour and depth, as the GS does.
struct Builder {
    frame: Frame,
    names: bool,
    header: Header,
    cut: bool,
}

impl Builder {
    fn use_header(&mut self, header: Header) {
        self.header = header;
    }

    // What is drawn between (the text) keeps the next primitives out of the last pass.
    fn cut(&mut self) {
        self.cut = true;
    }

    fn triangles(&mut self, strip: &[Vertex]) {
        let mut i = 0;
        while i + 2 < strip.len() {
            self.put([strip[i], strip[i + 1], strip[i + 2]]);
            i += 1;
        }
    }

    fn lines(&mut self, strip: &[Vertex]) {
        let mut i = 0;
        while i + 1 < strip.len() {
            self.put([strip[i], strip[i + 1]]);
            i += 1;
        }
    }

    // A triangle fan: triangle i is the centre, vertex i and vertex i + 1.
    fn fan(&mut self, vertices: &[Vertex]) {
        let mut i = 1;
        while i + 1 < vertices.len() {
            self.put([vertices[0], vertices[i], vertices[i + 1]]);
            i += 1;
        }
    }

    fn sprite(&mut self, mut first: Vertex, second: &Vertex) {
        first.z = second.z;
        self.put([first, *second]);
    }

    fn put<const N: usize>(&mut self, mut primitive: [Vertex; N]) {
        if !self.header.material.gouraud || self.header.topology == PassTopology::Sprites {
            let last = primitive[N - 1];
            for v in primitive.iter_mut() {
                v.r = last.r;
                v.g = last.g;
                v.b = last.b;
                v.a = last.a;
            }
        }
        let h = &self.header;
        let fresh = match self.frame.passes.last() {
            None => true,
            Some(last) => {
                self.cut
                    || last.target != h.target
                    || last.topology != h.topology
                    || !(last.material == h.material)
                    || last.edge_smoothing != h.edge_smoothing
                    || last.half_line != h.half_line
                    || last.prim_blend != h.prim_blend
            }
        };
        if fresh {
            self.frame.passes.push(Pass {
                name: h.name.clone(),
                target: h.target,
                topology: h.topology,
                material: h.material,
                edge_smoothing: h.edge_smoothing,
                half_line: h.half_line,
                prim_blend: h.prim_blend,
                vertices: Vec::new(),
                scissor: None,
            });
            self.cut = false;
        }
        self.frame.passes.last_mut().unwrap().vertices.extend_from_slice(&primitive);
    }
}

fn lround(x: f32) -> i64 {
    to_i64(x.round())
}

fn to_i64(x: f32) -> i64 {
    if (i64::MIN as f32..-(i64::MIN as f32)).contains(&x) { x as i64 } else { i64::MIN }
}

fn byte_of(value: f32, scale: f32) -> u8 {
    lround(value * scale) as u8
}

fn depth_of(z: f32) -> u32 {
    (to_i64(z * 16.0) as u32) & 0xffffff
}

fn sampled(source: SourceKind, coordinates: CoordinateKind, sampling: Sampling) -> Material {
    Material { source, coordinates, sampling, bilinear: true, ..Default::default() }
}

fn from_target(target: TargetName, colour_only: bool, width: i32, height: i32) -> Material {
    let mut m = sampled(SourceKind::Target, CoordinateKind::Texel, Sampling::ClampToRegion);
    m.source_target = target;
    m.colour_only = colour_only;
    m.region = [0, width - 1, 0, height - 1];
    m
}

fn from_texture(texture: i32, coordinates: CoordinateKind, sampling: Sampling) -> Material {
    let mut m = sampled(SourceKind::Texture, coordinates, sampling);
    m.texture = texture;
    m
}

fn with_state(mut m: Material, blend: BlendOp, depth: DepthTest) -> Material {
    m.blend = blend;
    m.depth_test = depth;
    m
}

fn part_name(part: Part) -> &'static str {
    match part {
        Part::Background => "background",
        Part::Blur => "blur",
        Part::Copy => "copy",
        Part::Tint => "tint",
        Part::Vignette => "vignette",
        Part::Fade => "fade",
        Part::BlurAfter => "blur after",
        Part::Bars => "bars",
        Part::Column => "column",
        Part::CubeHalf => "cube half",
        Part::CubeAdded => "cubes added",
        Part::CubeChainShrink => "cubes chain shrink",
        Part::CubeChainStretch => "cubes chain stretch",
        Part::CubeToDisplay => "cubes to display",
    }
}

fn target_of(target: Target) -> TargetName {
    match target {
        Target::Display => TargetName::Display,
        Target::Work0 => TargetName::RefractionSource,
        Target::Work1 => TargetName::Work,
    }
}

// facts/clock-frame-rest.md: a head draw's state. Only the background is drawn with the field's half line
// (clock_rest.mjs head: the display environment with the clear); the others use offset 0.
fn head_header(d: &HeadDraw, width: i32, height: i32) -> Header {
    let mut m = Material::default();
    if d.textured {
        let coordinates = if d.coordinates == Coordinates::St { CoordinateKind::Projective } else { CoordinateKind::Texel };
        m = match d.source {
            Source::Background => from_texture(1, coordinates, Sampling::Repeat),
            Source::Frame => from_target(TargetName::Display, true, width, height),
            Source::Work0 => from_target(TargetName::RefractionSource, false, width, height),
            Source::Work1 => from_target(TargetName::Work, false, width, height),
            Source::None => panic!("a textured head draw without a source"),
        };
        m.coordinates = coordinates;
    }
    const MODES: [BlendOp; 5] = [BlendOp::Add, BlendOp::AlphaOver, BlendOp::Subtract, BlendOp::FixedOver, BlendOp::FixedAdd];
    m.blend = if d.blended { MODES[d.alpha as usize] } else { BlendOp::Opaque };
    m.blend_constant = if d.blended && d.alpha == AlphaMode::Constant { 0x28 } else { 0 };
    m.depth_test = if d.depth_test == 2 {
        DepthTest::GreaterEqual
    } else if d.depth_test == 3 {
        DepthTest::Greater
    } else {
        DepthTest::Always
    };
    m.gouraud = d.gouraud;
    header(
        part_name(d.part).into(),
        target_of(d.target),
        if d.topology == Topology::Sprite { PassTopology::Sprites } else { PassTopology::Triangles },
        m,
        false,
        d.part == Part::Background,
        false,
    )
}

fn head_vertex(h: &HeadVertex, coordinates: Coordinates) -> Vertex {
    let mut v = Vertex {
        x: h.x,
        y: h.y,
        z: h.z,
        u: 0.0,
        v: 0.0,
        q: 1.0,
        r: byte_of(h.r, 128.0),
        g: byte_of(h.g, 128.0),
        b: byte_of(h.b, 128.0),
        a: byte_of(h.a, 128.0),
    };
    if coordinates == Coordinates::St {
        v.u = h.s;
        v.v = h.t;
        v.q = h.q;
    } else if coordinates == Coordinates::Uv {
        v.u = h.u;
        v.v = h.v;
    }
    v
}

// The vignette's sectors continue one strip: its PRIM is sent once (clock_rest.mjs vignette).
fn emit_head(out: &mut Builder, draws: &[HeadDraw], width: i32, height: i32) {
    let mut vignette: Vec<Vertex> = Vec::new();
    for d in draws {
        let vertices: Vec<Vertex> = d.vertices.iter().map(|h| head_vertex(h, d.coordinates)).collect();
        if d.part == Part::Vignette {
            if vignette.is_empty() {
                out.use_header(head_header(d, width, height));
            }
            vignette.extend_from_slice(&vertices);
            continue;
        }
        if !vignette.is_empty() {
            out.triangles(&vignette);
        }
        vignette.clear();
        out.use_header(head_header(d, width, height));
        if d.topology == Topology::Sprite {
            let mut i = 0;
            while i + 1 < vertices.len() {
                out.sprite(vertices[i], &vertices[i + 1]);
                i += 2;
            }
        } else {
            out.triangles(&vertices);
        }
    }
    if !vignette.is_empty() {
        out.triangles(&vignette);
    }
}

// facts/clock-rod-draw.md "Per send" and facts/clock-extra-passes.md: the state of each send. The refracted
// faces have no blending; AA1 blends their edges with the ALPHA register as it stands.
fn label(names: bool, name: &str, suffix: &str) -> String {
    if names { format!("{name}{suffix}") } else { String::new() }
}

#[allow(clippy::too_many_arguments)]
fn rod_header(name: &str, names: bool, kind: RodSendKind, edge: bool, split: bool, grain_texture: i32, width: i32, height: i32) -> Header {
    let bent = if edge { BlendOp::AlphaOver } else { BlendOp::Opaque };
    let tri = PassTopology::Triangles;
    match kind {
        RodSendKind::RefractedFar => header(
            label(names, name, " refracted far"),
            TargetName::Work,
            tri,
            with_state(from_target(TargetName::RefractionSource, false, width, height), bent, DepthTest::Always),
            edge,
            true,
            false,
        ),
        RodSendKind::GrainSubtracted => header(
            label(names, name, " grain subtracted"),
            TargetName::Work,
            tri,
            with_state(from_texture(2, CoordinateKind::Projective, Sampling::Repeat), BlendOp::Subtract, DepthTest::GreaterEqual),
            false,
            true,
            false,
        ),
        RodSendKind::GrainAdded => header(
            label(names, name, " grain added"),
            TargetName::Work,
            tri,
            with_state(from_texture(2, CoordinateKind::Projective, Sampling::Repeat), BlendOp::Add, DepthTest::GreaterEqual),
            false,
            true,
            false,
        ),
        RodSendKind::RefractedNearToFrame => header(
            label(names, name, " refracted near to the frame"),
            TargetName::Display,
            tri,
            with_state(from_target(TargetName::Work, false, width, height), bent, DepthTest::GreaterEqual),
            edge,
            true,
            false,
        ),
        RodSendKind::RefractedNearToRefraction => header(
            label(names, name, " refracted near to the refraction buffer"),
            TargetName::RefractionSource,
            tri,
            with_state(from_target(TargetName::Work, false, width, height), bent, DepthTest::GreaterEqual),
            edge,
            true,
            false,
        ),
        RodSendKind::Reflection => header(
            label(names, name, " reflection"),
            TargetName::Work,
            tri,
            with_state(
                from_texture(0, CoordinateKind::Texel, Sampling::Clamp),
                BlendOp::Opaque,
                if split { DepthTest::GreaterEqual } else { DepthTest::Always },
            ),
            false,
            true,
            false,
        ),
        RodSendKind::Grain => header(
            label(names, name, " grain"),
            TargetName::Work,
            tri,
            with_state(from_texture(grain_texture, CoordinateKind::Projective, Sampling::Repeat), BlendOp::Subtract, DepthTest::Always),
            false,
            true,
            false,
        ),
    }
}

fn rod_vertex(r: &RodVertex) -> Vertex {
    Vertex {
        x: r.x,
        y: r.y,
        z: depth_of(r.z),
        u: r.u,
        v: r.v,
        q: r.q,
        r: byte_of(r.r, 128.0),
        g: byte_of(r.g, 128.0),
        b: byte_of(r.b, 128.0),
        a: byte_of(r.a, 128.0),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_rod_send(out: &mut Builder, name: &str, send: &RodSend, split: bool, grain_texture: i32, width: i32, height: i32) {
    for face in &send.faces {
        let names = out.names;
        out.use_header(rod_header(name, names, send.kind, face.edge_smoothing, split, grain_texture, width, height));
        let strip: Vec<Vertex> = face.strip.iter().map(rod_vertex).collect();
        out.triangles(&strip);
    }
}

// facts/clock-orbs.md: the trail (an antialiased line strip) and the two sprites, to the frame and to the refraction buffer.
fn emit_orb(out: &mut Builder, orb: &OrbDraw) {
    let names = out.names;
    let name = if names { format!("orb {}", orb.k) } else { String::new() };
    for (s, send) in orb.sends.iter().enumerate() {
        let target = if s == 0 { TargetName::Display } else { TargetName::RefractionSource };
        out.use_header(header(
            label(names, &name, " trail"),
            target,
            PassTopology::Lines,
            with_state(Material::default(), BlendOp::Add, DepthTest::Greater),
            true,
            true,
            false,
        ));
        let points: Vec<Vertex> = send
            .trail
            .points
            .iter()
            .map(|p| Vertex {
                x: p.x,
                y: p.y,
                z: depth_of(p.z),
                u: 0.0,
                v: 0.0,
                q: 1.0,
                r: byte_of(p.r, 255.0),
                g: byte_of(p.g, 255.0),
                b: byte_of(p.b, 255.0),
                a: byte_of(p.a, 128.0),
            })
            .collect();
        out.lines(&points);
        let corner = |c: &OrbVertex| Vertex {
            x: c.x,
            y: c.y,
            z: depth_of(c.z),
            u: c.u * 64.0,
            v: c.v * 64.0,
            q: 1.0,
            r: byte_of(c.r, 128.0),
            g: byte_of(c.g, 128.0),
            b: byte_of(c.b, 128.0),
            a: byte_of(c.a, 128.0),
        };
        let mut sprite = |what: &str, texture: i32, corners: &OrbSprite| {
            out.use_header(header(
                if names { format!("{name} {what}") } else { String::new() },
                target,
                PassTopology::Sprites,
                with_state(from_texture(texture, CoordinateKind::Texel, Sampling::Clamp), BlendOp::Add, DepthTest::Always),
                false,
                true,
                false,
            ));
            out.sprite(corner(&corners.first), &corner(&corners.second));
        };
        sprite("glow", 7, &send.glow);
        sprite("disc", 6, &send.disc);
    }
}

// facts/config-cubes.md "Drawing one cube" and "The pass", clock_cubes.mjs drawing(): the state of each send. The far faces
// sample the frame (gs.frameTexture) into the work buffer, the near ones the work buffer into the refraction buffer;
// the grain sends add with an offset and subtract without; the layer's reflection is clock texture 5.
fn cube_header(send: CubeSend, edge: bool, width: i32, height: i32) -> Header {
    let bent = if edge { BlendOp::AlphaOver } else { BlendOp::Opaque };
    let grain = from_texture(2, CoordinateKind::Projective, Sampling::Repeat);
    let tri = PassTopology::Triangles;
    let h = |name: &str, target: TargetName, material: Material, edge_smoothing: bool, prim_blend: bool| {
        header(name.into(), target, tri, material, edge_smoothing, true, prim_blend)
    };
    match send {
        CubeSend::RefractedFar => {
            h("cube refracted far", TargetName::Work, with_state(from_target(TargetName::Display, true, width, height), bent, DepthTest::Always), edge, false)
        }
        CubeSend::GrainOffsetFar => h("cube grain offset far", TargetName::Work, with_state(grain, BlendOp::Add, DepthTest::GreaterEqual), false, false),
        CubeSend::GrainPlainFar => h("cube grain plain far", TargetName::Work, with_state(grain, BlendOp::Subtract, DepthTest::GreaterEqual), false, false),
        CubeSend::EdgeColour => {
            h("cube edge colour", TargetName::RefractionSource, with_state(Material::default(), BlendOp::AlphaOver, DepthTest::Always), true, true)
        }
        CubeSend::Depth => h("cube depth", TargetName::RefractionSource, with_state(Material::default(), BlendOp::Add, DepthTest::GreaterEqual), true, true),
        CubeSend::RefractedNear => h(
            "cube refracted near",
            TargetName::RefractionSource,
            with_state(from_target(TargetName::Work, false, width, height), bent, DepthTest::Always),
            edge,
            false,
        ),
        CubeSend::GrainOffsetNear => {
            h("cube grain offset near", TargetName::RefractionSource, with_state(grain, BlendOp::Add, DepthTest::GreaterEqual), false, false)
        }
        CubeSend::GrainPlainNear => {
            h("cube grain plain near", TargetName::RefractionSource, with_state(grain, BlendOp::Subtract, DepthTest::GreaterEqual), false, false)
        }
        CubeSend::LayerReflection => h(
            "cube layer reflection",
            TargetName::Work,
            with_state(from_texture(5, CoordinateKind::Texel, Sampling::Clamp), bent, DepthTest::Always),
            edge,
            false,
        ),
        CubeSend::LayerAlpha => h("cube layer alpha", TargetName::Work, with_state(Material::default(), BlendOp::Add, DepthTest::GreaterEqual), edge, true),
        CubeSend::LayerClear | CubeSend::HalfBuffer | CubeSend::Added | CubeSend::ChainShrink | CubeSend::ChainStretch | CubeSend::ToDisplay => {
            panic!("a cube send that is not a face send")
        }
    }
}

fn emit_cubes(out: &mut Builder, draws: &[CubeDraw], width: i32, height: i32) {
    for d in draws {
        if d.send == CubeSend::LayerClear {
            let name = if out.names { "cubes layer clear" } else { "" };
            clear_target(out, name, TargetName::Work, &d.clear, width, height);
        } else if let Some(rectangle) = &d.rectangle {
            emit_head(out, std::slice::from_ref(rectangle), width, height);
        } else {
            for face in &d.faces {
                out.use_header(cube_header(d.send, face.edge_smoothing, width, height));
                let strip: Vec<Vertex> = face.strip.iter().map(rod_vertex).collect();
                out.triangles(&strip);
            }
        }
    }
}

// facts/text.md: the text's draws, all to the display with its field half line. A glyph is a Gouraud, textured,
// blended fan (PRIM 0x5D) of the glyph cache, ST, region clamp to its cell, bilinear (TEX1 0x60), blend 0x44, depth
// test always (Font_PutsPackets' TEST 0x30000); the hint's picture is DrawIcon's rectangle (func_00233770) of a clock
// texture bound with blend 0x44 and depth test always (func_002349E0).
fn emit_text(out: &mut Builder, text: &TextFrame) {
    for d in &text.draws {
        if d.kind == TextDrawKind::Glyph {
            let mut m = from_texture(GLYPH_TEXTURE, CoordinateKind::Projective, Sampling::ClampToRegion);
            m.region = d.glyph.region;
            m.blend = BlendOp::AlphaOver;
            m.gouraud = true;
            out.use_header(header("text".into(), TargetName::Display, PassTopology::Triangles, m, false, true, false));
            let fan: Vec<Vertex> = d
                .glyph
                .fan
                .iter()
                .map(|g| Vertex { x: g.x, y: g.y, z: 0, u: g.s, v: g.t, q: g.q, r: g.colour[0], g: g.colour[1], b: g.colour[2], a: g.colour[3] })
                .collect();
            out.fan(&fan);
            continue;
        }
        let r = &d.icon;
        out.use_header(header(
            "hint picture".into(),
            TargetName::Display,
            PassTopology::Sprites,
            with_state(from_texture(d.texture, CoordinateKind::Texel, Sampling::Clamp), BlendOp::AlphaOver, DepthTest::Always),
            false,
            true,
            false,
        ));
        let corner = |x: i32, y: i32, u: i32, v: i32| Vertex {
            x: x as f32 / 16.0,
            y: y as f32 / 16.0,
            z: (r.z as u32) & 0xffffff,
            u: (u & 0xffff) as f32 / 16.0,
            v: (v & 0xffff) as f32 / 16.0,
            q: 1.0,
            r: r.colour[0] as u8,
            g: r.colour[1] as u8,
            b: r.colour[2] as u8,
            a: r.colour[3] as u8,
        };
        out.sprite(corner(r.x0, r.y0, r.u0, r.v0), &corner(r.x1, r.y1, r.u1, r.v1));
    }
}

// clock_frame.mjs stateWriters work/display with a clear: a sprite over the whole target at depth 0.
fn clear_target(out: &mut Builder, name: &str, target: TargetName, colour: &Colour, width: i32, height: i32) {
    out.use_header(header(name.into(), target, PassTopology::Sprites, Material::default(), false, true, false));
    let first = Vertex { x: 0.0, y: 0.0, z: 0, u: 0.0, v: 0.0, q: 1.0, r: colour[0] as u8, g: colour[1] as u8, b: colour[2] as u8, a: colour[3] as u8 };
    let mut second = first;
    second.x = width as f32;
    second.y = height as f32;
    out.sprite(first, &second);
}

// facts/clock-frame.md "Order of a frame", the clock screen's parts: camera; head; rods, orbs and the
// two extra passes; overlay; trips after the rods; bars; text (date and time, button hint); column; then the state step.
// With menus: the externals in, between(), then after the trips the cubes, the menu ramp step, the menus and the pages' text,
// and endOfFrame last (clock_frame.mjs frame()).
#[derive(Debug, Clone)]
pub struct Clock<A> {
    state: ClockState,
    head: FrameHead<A>,
    rods: Rods<A>,
    orbs: OrbState,
    clear_colour: Colour,
    first_display_clear: Colour,
    tube: TubeConstants,
    minute_factor: f32,
    fraction_easing: f32,
    orb_colour: Colour,
    has_entry_data: bool,
    wide: i32,
    orb_random: [i32; ORB_COUNT],
    orb_colours: [Colour; ORB_COUNT],
    width: i32,
    height: i32,
    text: Option<Text<A>>,
    strings: Vec<StringRun>,
    menus: Option<Menus>,
    cubes: Option<Cubes<A>>,
    menus_state: MenusState,
    cube_state: CubeState,
    items: ConfigItems,
    external: MenuExternals,
    text_ramps: TextRamps,
    pass_names: bool,
    notes: Vec<String>,
    unmodelled: Vec<u32>,
    sounds: Vec<SoundCommand>,
    rtc_write: Option<DateWords>,
}

macro_rules! menu_world {
    ($s:expr) => {
        MenuWorld {
            clock: &mut $s.state,
            head: $s.head.state_mut(),
            sprite_fade: &mut $s.orbs.sprite_fade,
            menus: &mut $s.menus_state,
            cubes: &mut $s.cube_state,
            items: &mut $s.items,
            width: $s.width,
            height: $s.height,
            sounds: Some(&mut $s.sounds),
            rtc_write: Some(&mut $s.rtc_write),
        }
    };
}

impl<A: Arithmetic> Clock<A> {
    pub fn new(inputs: &ClockInputs) -> Result<Self> {
        let text = match (&inputs.font, &inputs.program) {
            (Some(font), Some(program)) => Some(Text::<A>::new(Arc::clone(font), Arc::clone(program), &inputs.text)?),
            _ => None,
        };
        let mut clock = Self {
            state: inputs.state,
            head: FrameHead::new(&inputs.head),
            rods: Rods::new(inputs.mesh.clone(), &inputs.rod_template),
            orbs: inputs.orbs,
            clear_colour: inputs.clear_colour,
            first_display_clear: inputs.first_display_clear,
            tube: inputs.tube,
            minute_factor: inputs.minute_factor,
            fraction_easing: inputs.fraction_easing,
            orb_colour: inputs.orb_colour,
            has_entry_data: inputs.has_entry_data,
            wide: inputs.wide,
            orb_random: inputs.orb_random,
            orb_colours: inputs.orb_colours,
            width: inputs.width,
            height: inputs.height,
            text,
            strings: Vec::new(),
            menus: None,
            cubes: None,
            menus_state: MenusState::default(),
            cube_state: CubeState::default(),
            items: [0; 20],
            external: MenuExternals::default(),
            text_ramps: inputs.text.ramps,
            pass_names: inputs.pass_names,
            notes: Vec::new(),
            unmodelled: Vec::new(),
            sounds: Vec::new(),
            rtc_write: None,
        };
        if let Some(menus) = &inputs.menus {
            clock.menus = Some(Menus::new(menus.options));
            clock.cubes = Some(Cubes::new(menus.cube_mesh.clone()));
            clock.menus_state = menus.menus.clone();
            clock.cube_state = menus.cubes;
            clock.items = menus.items;
        }
        Ok(clock)
    }

    pub fn state(&self) -> &ClockState {
        &self.state
    }

    pub fn head(&self) -> &HeadState {
        self.head.state()
    }

    pub fn orbs(&self) -> &OrbState {
        &self.orbs
    }

    pub fn rod_template(&self) -> &RodRecord {
        self.rods.rod_template()
    }

    pub fn text(&self) -> Option<&Text<A>> {
        self.text.as_ref()
    }

    // The strings of the last frame, as handed to the font code.
    pub fn strings(&self) -> &[StringRun] {
        &self.strings
    }

    pub fn menus(&self) -> Option<&MenusState> {
        self.menus.map(|_| &self.menus_state)
    }

    pub fn cubes(&self) -> Option<&CubeState> {
        self.menus.map(|_| &self.cube_state)
    }

    pub fn items(&self) -> &ConfigItems {
        &self.items
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn externals(&self) -> &MenuExternals {
        &self.external
    }

    // The model's notes of the last frame (what it does not cover).
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    pub fn sounds(&self) -> &[SoundCommand] {
        &self.sounds
    }

    pub fn rtc_write(&self) -> &Option<DateWords> {
        &self.rtc_write
    }

    // References/model/clock_frame.mjs frame(), the clock screen's parts, in its order.
    pub fn frame(&mut self, input: &FrameInputs) -> Frame {
        let frame = Frame { width: self.width, height: self.height, field: input.field, display_index: input.display_index, ..Default::default() };
        let mut out = Builder { frame, names: self.pass_names, header: Header::default(), cut: false };
        let (width, height) = (self.width, self.height);

        self.state.time = input.time;
        if self.menus.is_none() {
            self.state.time_filled = input.time_filled.unwrap_or(1);
        }
        self.state.scene.field = input.field;
        self.strings.clear();
        self.notes.clear();
        self.sounds.clear();
        self.rtc_write = None;
        let mut ext = MenuExternals::default();
        if self.menus.is_some() {
            ext = input.menu;
            if let Some(time_filled) = input.time_filled {
                self.state.time_filled = time_filled;
            }
            if let Some(config_items) = &input.config_items {
                let models_item0 = self.menus_state.config_gate.is_some() && ext.mechacon_param.is_some();
                let first = if models_item0 { 1 } else { 0 };
                self.items[first..].copy_from_slice(&config_items[first..]);
            }
            if input.thread_step {
                let mut world = menu_world!(self);
                Menus::between(&mut world, &ext, &mut self.notes);
            }
        }
        let camera = Camera::<A>::matrices(&mut self.state);

        let head = HeadInputs {
            width,
            height,
            mode: self.state.mode,
            overlay_level: self.state.overlay_level,
            level: self.state.level as u32,
            counter: self.state.counter,
            item0: if self.menus.is_some() { self.items[0] } else { input.item0 },
            proportion_x: self.state.proportions.ax,
            proportion_y: self.state.proportions.ay,
            clear_colour: self.clear_colour,
            tube: self.tube,
        };

        let clear = if input.display_index == 0 { self.first_display_clear } else { self.clear_colour };
        clear_target(&mut out, "clear", TargetName::Display, &clear, width, height);
        let draws = self.head.head(&head, &camera.view, &camera.screen);
        emit_head(&mut out, &draws, width, height);

        let mut rods = RodsInput {
            current_rod: self.state.state.current_rod,
            seconds_angle: i32::from(self.state.state.seconds_angle),
            rod_angle: i32::from(self.state.state.rod_angle),
            accent: self.state.state.accent,
            fourth: self.state.state.fourth,
            width,
            height,
            field: input.field,
            ..Default::default()
        };
        for i in 0..12 {
            let r = &self.state.state.rods[i];
            rods.rods[i] = crate::rods::RodLight { appearance: r.appearance, progress: r.progress, base: r.base, reflection: r.reflection };
        }
        let rod_frame = self.rods.frame(&rods, &camera.view, &camera.screen);

        let orbs = OrbInputs {
            milliseconds: self.state.time.milliseconds,
            seconds: self.state.time.seconds,
            minutes: self.state.time.minutes,
            second_hand: i32::from(self.state.eased.second_hand),
            hour_hand: i32::from(self.state.eased.hour_hand),
            progress: self.state.eased.progress,
            minute_factor: self.minute_factor,
            fraction_easing: self.fraction_easing,
            scene_scale: self.state.scene.scale,
            colour: self.orb_colour,
            width,
            height,
            mode: self.state.mode,
            overlay_level: self.state.overlay_level,
            has_entry_data: self.has_entry_data,
            wide: self.wide,
            random: self.orb_random,
            colours: self.orb_colours,
        };
        self.orbs.fraction = self.state.eased.fraction;
        let orb_frame = Orbs::<A>::frame(&mut self.orbs, &orbs, &camera.view, &camera.screen);
        self.state.eased.fraction = self.orbs.fraction;
        self.rods.rod_template_mut().local = orb_frame.orbs[ORB_COUNT - 1].local;

        // facts/clock-scene.md "The draw list": the rods, then the orbs 0 to 6, by the depth of their origin.
        let mut list = rod_frame.list.clone();
        for k in 0..ORB_COUNT {
            insert_by_depth(&mut list, &DepthNode { orb: true, index: k as i32, key: orb_frame.orbs[k].depth_key });
        }
        for node in &list {
            if node.orb {
                emit_orb(&mut out, &orb_frame.orbs[node.index as usize]);
                continue;
            }
            let rod = &rod_frame.rods[node.index as usize];
            if !rod.drawn {
                continue;
            }
            for send in &rod.sends {
                let name = if out.names { format!("rod {}", rod.record.number) } else { String::new() };
                emit_rod_send(&mut out, &name, send, !rod.pieces.is_empty(), 2, width, height);
            }
        }
        for (pass, draws) in rod_frame.extra_passes.iter().enumerate() {
            let names = out.names;
            let name = if names { format!("extra pass {pass}") } else { String::new() };
            clear_target(&mut out, &label(names, &name, " clear"), TargetName::Work, &[0, 0, 0, 0x80], width, height);
            for draw in draws {
                let rod = if names { format!("{name} rod {}", rod_frame.rods[draw.rod].record.number) } else { String::new() };
                emit_rod_send(&mut out, &rod, &draw.reflection, draw.split, draw.grain_texture, width, height);
                emit_rod_send(&mut out, &rod, &draw.grain, draw.split, draw.grain_texture, width, height);
            }
            out.use_header(header(
                label(names, &name, " added to the frame"),
                TargetName::Display,
                PassTopology::Sprites,
                with_state(from_target(TargetName::Work, false, width, height), BlendOp::Add, DepthTest::Always),
                false,
                false,
                false,
            ));
            let w = width as f32;
            let h = height as f32;
            out.sprite(
                Vertex { x: 0.0, y: 0.0, z: 0, u: 0.5, v: 0.5, q: 1.0, r: 0x80, g: 0x80, b: 0x80, a: 30 },
                &Vertex { x: w, y: h, z: 0, u: w + 0.5, v: h + 0.5, q: 1.0, r: 0x80, g: 0x80, b: 0x80, a: 30 },
            );
        }

        self.head.state_mut().vignette_ramp = self.state.vignette_ramp;
        let draws = self.head.overlay(&head);
        emit_head(&mut out, &draws, width, height);
        self.state.vignette_ramp = self.head.state().vignette_ramp;
        let draws = self.head.trips_after(&head);
        emit_head(&mut out, &draws, width, height);

        let mut at_menus = ClockState::default();
        if let Some(menus) = self.menus {
            let cube_draws = self.cubes.as_ref().unwrap().frame(
                &mut self.cube_state,
                self.head.state_mut(),
                &self.state,
                &CubeFrameInputs { width, height, field: input.field, body: self.menus_state.body },
            );
            emit_cubes(&mut out, &cube_draws, width, height);
            {
                let mut world = menu_world!(self);
                Menus::menu_step(&mut world, &ext);
            }
            at_menus = self.state;
            let at_items = self.items;
            let mut before = self.menus_state.clone();
            {
                // The pages' function runs before the page's input: its list crossfade is the step's without a move (clock_menus.mjs).
                let mut probe_clock = self.state;
                let mut probe_head = *self.head.state();
                let mut probe_fade = self.orbs.sprite_fade;
                let mut probe_menus = self.menus_state.clone();
                let mut probe_cubes = self.cube_state;
                let mut probe_items = self.items;
                let mut probe = MenuWorld {
                    clock: &mut probe_clock,
                    head: &mut probe_head,
                    sprite_fade: &mut probe_fade,
                    menus: &mut probe_menus,
                    cubes: &mut probe_cubes,
                    items: &mut probe_items,
                    width,
                    height,
                    sounds: None,
                    rtc_write: None,
                };
                let mut masked = ext;
                masked.pad.pressed &= !(pad::UP | pad::DOWN);
                let mut sink = Vec::new();
                menus.step(&mut probe, &mut masked, &mut sink);
                before.list_fade = probe_menus.list_fade;
            }
            let chosen = self.menus_state.entries[self.menus_state.page.selected.clamp(0, 8) as usize];
            if (ext.pad.pressed & pad::CROSS) != 0
                && self.menus_state.page.ramp.state == 2
                && self.menus_state.page.level == 0
                && self.unmodelled.contains(&chosen.string_callback)
            {
                ext.pad.pressed &= !pad::CROSS;
                self.notes.push("an entry whose value is not modelled cannot be entered".into());
            }
            {
                let mut world = menu_world!(self);
                menus.step(&mut world, &mut ext, &mut self.notes);
            }
            let constants = input.text_ramps.unwrap_or(self.text_ramps);
            let page_ramps = text_ramps_of(&menus_at_pages(&before, &self.menus_state), &constants);
            let pages: PagesInputs = pages_of(&menus_at_pages(&before, &self.menus_state), &at_menus, &at_items, &page_ramps, width, height);
            out.cut();
            if let Some(text) = self.text.as_mut() {
                let drawn = text.pages(&pages);
                emit_text(&mut out, &drawn.text);
                self.strings = drawn.text.strings;
                self.unmodelled = drawn.unmodelled;
                if let Some(title_width) = drawn.title_width {
                    self.menus_state.page.title_width = title_width;
                }
                out.cut();
            }
        }
        let draws = self.head.bars(&head);
        emit_head(&mut out, &draws, width, height);
        out.frame.text_at = out.frame.passes.len();
        out.cut();
        if let Some(writer) = &mut self.text {
            let mut text = TextFrameInputs {
                items: input.items,
                item0: input.item0,
                overlay_level: self.state.overlay_level,
                tail: self.state.tail,
                menu: self.state.menu_ramp,
                width,
                height,
                ramps: None,
            };
            if self.menus.is_some() {
                let i = &self.items;
                text.items = ClockItems { year: i[6], month: i[7], day: i[8], hour: i[9], minute: i[10], second: i[11] };
                text.item0 = i[0];
                text.overlay_level = at_menus.overlay_level;
                text.tail = at_menus.tail;
                text.menu = at_menus.menu_ramp;
                text.ramps = Some(text_ramps_of(&self.menus_state, &input.text_ramps.unwrap_or(self.text_ramps)));
            }
            let drawn = writer.frame(&text);
            emit_text(&mut out, &drawn);
            out.frame.glyphs = drawn.glyphs;
            self.strings.extend(drawn.strings);
            out.cut();
        }
        let draws = self.head.column(&head);
        emit_head(&mut out, &draws, width, height);

        ClockLogic::<A>::step(&mut self.state);
        if self.menus.is_some() {
            let mut world = menu_world!(self);
            Menus::end_of_frame(&mut world, &ext);
            self.external = ext;
        }
        out.frame
    }
}
