// facts/clock-frame.md: one clock frame as passes, in the OSD's order; facts/clock-gs-state.md: the state of each.
// RefractionSource is the OSD's work buffer 0, Work its work buffer 1. The opening adds Store (the half-width copy the
// ghost reads, GS 0x2300) and Extra (the buffer the cubes refract through, GS 0x1A40); there Work is unused.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum TargetName {
    #[default]
    Display,
    RefractionSource,
    Work,
    Store,
    Extra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum PassTopology {
    #[default]
    Triangles,
    Lines,
    Sprites,
}

// The ALPHA values the clock sends: Add Cs*As + Cd; AlphaOver (Cs - Cd)*As + Cd; Subtract Cd - Cs*As;
// FixedOver (Cs - Cd)*constant + Cd; FixedAdd Cs*constant + Cd. As and the constant are over 128. The opening adds
// AddDestinationAlpha Cs*Ad + Cd and SubtractFixed Cd - Cs*constant (facts/opening.md section 4; the list the audit closed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum BlendOp {
    #[default]
    Opaque,
    Add,
    AlphaOver,
    Subtract,
    FixedOver,
    FixedAdd,
    AddDestinationAlpha,
    SubtractFixed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum TextureSet {
    #[default]
    Clock,
    Opening,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum DepthTest {
    #[default]
    Always,
    GreaterEqual,
    Greater,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SourceKind {
    #[default]
    None,
    Texture,
    Target,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum CoordinateKind {
    #[default]
    Texel,
    Projective,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Sampling {
    #[default]
    Repeat,
    Clamp,
    ClampToRegion,
}

// x, y: pixels from the target's top-left corner, without the field's half line.
// z: the 24-bit depth the GS tests (greater is nearer).
// Texel coordinates: u, v in texels of the source, q = 1. Projective: u, v, q are S, T, Q (texel = u / q * size).
// r, g, b, a: the colour bytes. Textured, r / 128 multiplies the texel; untextured, r / 255 is the colour; a / 128 is the alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: u32,
    pub u: f32,
    pub v: f32,
    pub q: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for Vertex {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, z: 0, u: 0.0, v: 0.0, q: 1.0, r: 0, g: 0, b: 0, a: 0 }
    }
}

// `texture` is the clock's texture number (SourceKind::Texture); `source_target` the target read (SourceKind::Target),
// as 24-bit colour when `colour_only` (alpha 0x7f, 0 where the texel is black). `region`: min u, max u, min v,
// max v in texels (Sampling::ClampToRegion). `gouraud`: the colour is interpolated; otherwise each primitive's
// vertices already hold its last vertex's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Material {
    pub source: SourceKind,
    pub texture: i32,
    pub source_target: TargetName,
    pub colour_only: bool,
    pub coordinates: CoordinateKind,
    pub sampling: Sampling,
    pub region: [i32; 4],
    pub bilinear: bool,
    pub source_height: i32,
    pub blend: BlendOp,
    pub blend_constant: u8,
    pub depth_test: DepthTest,
    pub depth_write: bool,
    pub gouraud: bool,
    pub per_pixel_alpha: bool,
    pub alpha_correction: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            source: SourceKind::None,
            texture: 0,
            source_target: TargetName::Display,
            colour_only: false,
            coordinates: CoordinateKind::Texel,
            sampling: Sampling::Repeat,
            region: [0; 4],
            bilinear: false,
            source_height: 0,
            blend: BlendOp::Opaque,
            blend_constant: 0,
            depth_test: DepthTest::Always,
            depth_write: true,
            gouraud: false,
            per_pixel_alpha: false,
            alpha_correction: false,
        }
    }
}

// Vertices as lists: 3 per triangle, 2 per line, 2 per sprite (opposite corners). `edge_smoothing` is the
// OSD's AA1 (the edges' coverage becomes the alpha); `half_line`: the OSD draws it with the field's half-line offset;
// `prim_blend`: the primitive's own blend enable (PRIM ABE) on top of AA1 (the parity oracle cannot reproduce the pair on triangles).
// `scissor`: x0, y0, x1, y1 inclusive when the pass is drawn under a scissor other than the whole picture.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pass {
    pub name: String,
    pub target: TargetName,
    pub topology: PassTopology,
    pub material: Material,
    pub edge_smoothing: bool,
    pub half_line: bool,
    pub prim_blend: bool,
    pub vertices: Vec<Vertex>,
    pub scissor: Option<[i32; 4]>,
}

// facts/text.md section 2: the font library's glyph cache, sampled as texture GLYPH_TEXTURE. Cells of cell_width x
// cell_height, row by row across `width` texels, in a texture of 2^log_width x 2^log_height at GS word `address`;
// cells[n] is the code whose picture cell n holds (0 for none).
pub const GLYPH_TEXTURE: i32 = 10;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlyphCache {
    pub address: u32,
    pub cell_width: i32,
    pub cell_height: i32,
    pub width: i32,
    pub log_width: i32,
    pub log_height: i32,
    pub cells: Vec<i32>,
}

// `display_index`: which of the two display buffers is drawn; `text_at`: the first pass of the date, time and button
// hint (the passes before it are the bars); `glyphs`: the glyph cache the text's passes sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub width: i32,
    pub height: i32,
    pub field: i32,
    pub display_index: i32,
    pub passes: Vec<Pass>,
    pub text_at: usize,
    pub glyphs: GlyphCache,
    pub texture_set: TextureSet,
    pub depth_bits: i32,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            width: 640,
            height: 224,
            field: 0,
            display_index: 0,
            passes: Vec::new(),
            text_at: 0,
            glyphs: GlyphCache::default(),
            texture_set: TextureSet::Clock,
            depth_bits: 32,
        }
    }
}
