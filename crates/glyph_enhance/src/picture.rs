use scene::font::Font;

use crate::enhance::STEP_OFFSET;
use crate::field::Field;
use crate::layers::{Layers, combine};
use crate::outlines::GlyphOutline;
use crate::raster::Canvas;
use crate::trace::{Contour, offset};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Heros,
    Traced,
}

// One glyph's shape for the sharp text, in picture coordinates (a pixel spans [i, i + 1)): the white core and the dark
// outline's outer edge. The outer edge is always the one traced from the picture's alpha: the console grew it from the
// core's pixels, so it follows their grid, which a dilation of a smooth core does not (0.083 against 0.052 averaged back
// over the ASCII letters).
#[derive(Debug, Clone, PartialEq)]
pub struct SharpGlyph {
    pub source: Source,
    pub width: i32,
    pub height: i32,
    pub core: Vec<Contour>,
    pub outer: Vec<Contour>,
}

impl From<&GlyphOutline> for SharpGlyph {
    fn from(o: &GlyphOutline) -> Self {
        Self { source: Source::Traced, width: o.width, height: o.height, core: o.core.clone(), outer: o.outer.clone() }
    }
}

// One glyph's picture at `scale` texels per picture pixel, with the one-texel margin the cache keeps clear around it:
// (width + 2) x (height + 2) picture pixels, R G B A as the cache texture holds them (grey, alpha byte 0..125).
// With a shape: the core, the outline's outer edge and the alpha step (the core moved out by STEP_OFFSET), each by its
// exact area coverage. Without: the picture as the GS filters it (bilinear on the looked-up colours).
pub fn sharp_picture(font: &Font, code: i32, shape: Option<&SharpGlyph>, scale: u32) -> Option<(u32, u32, Vec<u8>)> {
    let glyph = font.glyph(code)?;
    let (w, h) = (glyph.block.width as usize, glyph.block.height as usize);
    let k = scale as usize;
    let (pw, ph) = ((w + 2) * k, (h + 2) * k);
    let mut rgba = vec![0u8; pw * ph * 4];
    let mut put = |i: usize, grey: f32, alpha: f32| {
        let g = (grey * 255.0).round().clamp(0.0, 255.0) as u8;
        rgba[4 * i..4 * i + 4].copy_from_slice(&[g, g, g, alpha.round().clamp(0.0, 255.0) as u8]);
    };
    match shape {
        Some(o) if o.width as usize == w && o.height as usize == h => {
            let s = f64::from(scale);
            let transform = |q: [f64; 2]| [(q[0] + 1.0) * s, (q[1] + 1.0) * s];
            let mut core = Canvas::new(pw, ph);
            core.fill(&o.core, transform);
            let mut outer = Canvas::new(pw, ph);
            outer.fill(&o.outer, transform);
            let mut step = Canvas::new(pw, ph);
            step.fill(&o.core.iter().map(|c| offset(c, STEP_OFFSET)).collect::<Vec<_>>(), transform);
            let (core, outer, step) = (core.coverage(), outer.coverage(), step.coverage());
            for i in 0..pw * ph {
                let (grey, alpha) = combine(core[i], outer[i], step[i]);
                put(i, grey, alpha);
            }
        }
        _ => {
            let original = Layers::of_glyph(font, &glyph).recombined();
            let grey = Field::padded(&original.iter().map(|c| c.0).collect::<Vec<_>>(), w, h, 2, 0.0);
            let alpha = Field::padded(&original.iter().map(|c| c.1).collect::<Vec<_>>(), w, h, 2, 0.0);
            for y in 0..ph {
                for x in 0..pw {
                    let u = (x as f64 + 0.5) / k as f64 - 1.0;
                    let v = (y as f64 + 0.5) / k as f64 - 1.0;
                    put(y * pw + x, grey.bilinear(u, v), alpha.bilinear(u, v));
                }
            }
        }
    }
    Some((pw as u32, ph as u32, rgba))
}

// A glyph's sharp picture at `scale` averaged back to the console picture's own pixels, against that picture: per pixel the
// larger of the premultiplied grey's and the alpha's difference (0..1). The sum over the pixels either inks, their count,
// and the largest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AveragedBack {
    pub sum: f64,
    pub inked: usize,
    pub worst: f64,
}

impl AveragedBack {
    pub fn mean(&self) -> f64 {
        self.sum / self.inked.max(1) as f64
    }

    pub fn add(&mut self, other: &Self) {
        self.sum += other.sum;
        self.inked += other.inked;
        self.worst = self.worst.max(other.worst);
    }
}

pub fn averaged_back(font: &Font, code: i32, shape: Option<&SharpGlyph>, scale: u32) -> Option<AveragedBack> {
    let k = scale as usize;
    let glyph = font.glyph(code)?;
    let original = Layers::of_glyph(font, &glyph).recombined();
    let (pw, _, rgba) = sharp_picture(font, code, shape, scale)?;
    let (w, h) = (glyph.block.width as usize, glyph.block.height as usize);
    let mut out = AveragedBack::default();
    for y in 0..h {
        for x in 0..w {
            let (mut pg, mut pa) = (0.0f64, 0.0f64);
            for j in 0..k {
                for i in 0..k {
                    let at = (((y + 1) * k + j) * pw as usize + (x + 1) * k + i) * 4;
                    pg += f64::from(rgba[at]) / 255.0 * f64::from(rgba[at + 3]) / 125.0;
                    pa += f64::from(rgba[at + 3]) / 125.0;
                }
            }
            let (g0, a0) = original[y * w + x];
            let d = (pg / (k * k) as f64 - f64::from(g0 * a0) / 125.0).abs().max((pa / (k * k) as f64 - f64::from(a0) / 125.0).abs());
            out.worst = out.worst.max(d);
            if a0 > 0.0 || pa > 0.0 {
                out.sum += d;
                out.inked += 1;
            }
        }
    }
    Some(out)
}

// The shapes `--text sharp` draws, by code; a code without one keeps its picture.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SharpFont {
    pub glyphs: std::collections::BTreeMap<i32, SharpGlyph>,
}

impl SharpFont {
    pub fn traced(outlines: &crate::outlines::OutlineFont) -> Self {
        Self { glyphs: outlines.glyphs.iter().map(|(&c, o)| (c, SharpGlyph::from(o))).collect() }
    }
}
