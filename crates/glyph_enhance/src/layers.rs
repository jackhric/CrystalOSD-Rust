use scene::font::{Font, FontBlock, Glyph, Rgba};

// facts/text.md section 1: the sixteen colours every block of FNTOSD has. Grey is the white core's coverage; alpha is 0
// outside, up to 100 in the dark outline and 125 under the core. The GS filters the colour and the alpha of the looked-up
// texels separately, so the picture is exactly two fields: grey (the core) and alpha (the outline's outer edge, then a
// step from 100 to 125 under the core).
pub const CONSOLE_TABLE: [Rgba; 16] = [
    [0, 0, 0, 0],
    [0, 0, 0, 25],
    [0, 0, 0, 50],
    [0, 0, 0, 75],
    [0, 0, 0, 100],
    [51, 51, 51, 50],
    [51, 51, 51, 75],
    [51, 51, 51, 100],
    [51, 51, 51, 125],
    [102, 102, 102, 75],
    [102, 102, 102, 100],
    [102, 102, 102, 125],
    [153, 153, 153, 100],
    [153, 153, 153, 125],
    [204, 204, 204, 125],
    [255, 255, 255, 125],
];

pub const OUTLINE_ALPHA: f32 = 100.0;
pub const CORE_ALPHA: f32 = 125.0;

pub fn is_console_table(table: &[Rgba; 16]) -> bool {
    *table == CONSOLE_TABLE
}

// One glyph picture as its layers: `core` = grey / 255, `outer` = min(alpha, 100) / 100, `step` = max(alpha - 100, 0) / 25.
#[derive(Debug, Clone, PartialEq)]
pub struct Layers {
    pub width: usize,
    pub height: usize,
    pub core: Vec<f32>,
    pub outer: Vec<f32>,
    pub step: Vec<f32>,
}

pub fn split(colour: Rgba) -> (f32, f32, f32) {
    let alpha = f32::from(colour[3]);
    (f32::from(colour[0]) / 255.0, alpha.min(OUTLINE_ALPHA) / OUTLINE_ALPHA, (alpha - OUTLINE_ALPHA).max(0.0) / (CORE_ALPHA - OUTLINE_ALPHA))
}

// The straight grey (0..1) and alpha byte (0..125) of the layers at one point.
pub fn combine(core: f32, outer: f32, step: f32) -> (f32, f32) {
    let step = step.clamp(0.0, 1.0);
    let outer = outer.clamp(0.0, 1.0).max(step);
    (core.clamp(0.0, 1.0), OUTLINE_ALPHA * outer + (CORE_ALPHA - OUTLINE_ALPHA) * step)
}

impl Layers {
    pub fn of_glyph(font: &Font, glyph: &Glyph<'_>) -> Self {
        let block: &FontBlock = glyph.block;
        let table = font.table(block);
        let (width, height) = (block.width as usize, block.height as usize);
        let mut layers = Self { width, height, core: vec![0.0; width * height], outer: vec![0.0; width * height], step: vec![0.0; width * height] };
        for y in 0..height {
            for x in 0..width {
                let (core, outer, step) = split(table[font.pixel(glyph, x as i32, y as i32) as usize]);
                let i = y * width + x;
                layers.core[i] = core;
                layers.outer[i] = outer;
                layers.step[i] = step;
            }
        }
        layers
    }

    pub fn indices(font: &Font, glyph: &Glyph<'_>) -> Vec<u8> {
        let block = glyph.block;
        let mut out = Vec::with_capacity((block.width * block.height) as usize);
        for y in 0..block.height {
            for x in 0..block.width {
                out.push(font.pixel(glyph, x, y) as u8);
            }
        }
        out
    }

    pub fn recombined(&self) -> Vec<(f32, f32)> {
        (0..self.core.len()).map(|i| combine(self.core[i], self.outer[i], self.step[i])).collect()
    }
}
