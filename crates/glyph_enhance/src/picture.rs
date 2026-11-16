use scene::font::Font;

use crate::enhance::STEP_OFFSET;
use crate::field::Field;
use crate::layers::{Layers, combine};
use crate::outlines::GlyphOutline;
use crate::raster::Canvas;
use crate::trace::offset;

// One glyph's picture at `scale` texels per picture pixel, with the one-texel margin the cache keeps clear around it:
// (width + 2) x (height + 2) picture pixels, R G B A as the cache texture holds them (grey, alpha byte 0..125).
// With outlines: the core, the outline's outer edge and the alpha step (the core moved out by STEP_OFFSET), each by its
// exact area coverage. Without: the picture as the GS filters it (bilinear on the looked-up colours).
pub fn sharp_picture(font: &Font, code: i32, outline: Option<&GlyphOutline>, scale: u32) -> Option<(u32, u32, Vec<u8>)> {
    let glyph = font.glyph(code)?;
    let (w, h) = (glyph.block.width as usize, glyph.block.height as usize);
    let k = scale as usize;
    let (pw, ph) = ((w + 2) * k, (h + 2) * k);
    let mut rgba = vec![0u8; pw * ph * 4];
    let mut put = |i: usize, grey: f32, alpha: f32| {
        let g = (grey * 255.0).round().clamp(0.0, 255.0) as u8;
        rgba[4 * i..4 * i + 4].copy_from_slice(&[g, g, g, alpha.round().clamp(0.0, 255.0) as u8]);
    };
    match outline {
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
