use crate::distance::signed_distance;
use crate::field::Field;
use crate::fit::fit;
use crate::layers::{Layers, combine};
use crate::raster::Canvas;
use crate::trace::{Contour, TraceParameters, offset, refine, zero_contours};

const PAD: usize = 4;

// How far out from the core's edge the alpha steps from the outline's 100 to the core's 125: the pictures give a pixel
// alpha 125 wherever the core touches it; 0.3 pixel is the offset whose pictures average back closest to the console's
// (tests/enhance.rs, over the Latin glyphs: 0.0489 against 0.0530 at 0 and 0.0552 at 0.7).
pub const STEP_OFFSET: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Technique {
    Bilinear,
    DistanceField,
    DistanceFieldBicubic,
    LevelSet,
    Traced,
    Fitted,
}

impl Technique {
    pub const ALL: [Technique; 6] = [Self::Bilinear, Self::DistanceField, Self::DistanceFieldBicubic, Self::LevelSet, Self::Traced, Self::Fitted];

    pub fn name(self) -> &'static str {
        match self {
            Self::Bilinear => "bilinear (today)",
            Self::DistanceField => "distance field, bilinear",
            Self::DistanceFieldBicubic => "distance field, bicubic",
            Self::LevelSet => "coverage level set, bicubic",
            Self::Traced => "traced outlines",
            Self::Fitted => "traced outlines fitted to coverage",
        }
    }
}

// One glyph picture made ready for every technique: its layers as padded fields, their distance fields and traced outlines.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub width: usize,
    pub height: usize,
    pub layers: Layers,
    pub grey: Field,
    pub alpha: Field,
    pub core: Field,
    pub outer: Field,
    pub core_distance: Field,
    pub outer_distance: Field,
    pub core_contours: Vec<Contour>,
    pub outer_contours: Vec<Contour>,
    pub core_fitted: Vec<Contour>,
    pub outer_fitted: Vec<Contour>,
}

pub fn prepare(layers: &Layers, parameters: &TraceParameters) -> Prepared {
    prepare_with(layers, parameters, true)
}

// The fields, distance fields and traced outlines; `fitted` adds the coverage fit (Phase 1 only: it is not used).
pub fn prepare_with(layers: &Layers, parameters: &TraceParameters, fitted: bool) -> Prepared {
    let (w, h) = (layers.width, layers.height);
    let recombined = layers.recombined();
    let grey: Vec<f32> = recombined.iter().map(|c| c.0).collect();
    let alpha: Vec<f32> = recombined.iter().map(|c| c.1).collect();
    let core = Field::padded(&layers.core, w, h, PAD, 0.0);
    let outer = Field::padded(&layers.outer, w, h, PAD, 0.0);
    let core_distance = signed_distance(&core);
    let outer_distance = signed_distance(&outer);
    let (core_contours, core_corners): (Vec<Contour>, Vec<Vec<bool>>) = zero_contours(&core_distance).iter().map(|c| refine(c, parameters)).unzip();
    let (outer_contours, outer_corners): (Vec<Contour>, Vec<Vec<bool>>) = zero_contours(&outer_distance).iter().map(|c| refine(c, parameters)).unzip();
    let mut core_fitted = core_contours.clone();
    let mut outer_fitted = outer_contours.clone();
    if fitted {
        fit(&mut core_fitted, &core_corners, &core, parameters.fit_iterations, parameters.fit_step, parameters.fit_smoothing, parameters.spacing);
        fit(&mut outer_fitted, &outer_corners, &outer, parameters.fit_iterations, parameters.fit_step, parameters.fit_smoothing, parameters.spacing);
    }
    Prepared {
        width: w,
        height: h,
        layers: layers.clone(),
        grey: Field::padded(&grey, w, h, PAD, 0.0),
        alpha: Field::padded(&alpha, w, h, PAD, 0.0),
        core,
        outer,
        core_distance,
        outer_distance,
        core_contours,
        outer_contours,
        core_fitted,
        outer_fitted,
    }
}

fn ramp(distance: f64, scale: f64) -> f32 {
    (0.5 - distance * scale).clamp(0.0, 1.0) as f32
}

// The glyph drawn into `width` x `height` output pixels, picture point (u, v) at output (origin + (u, v) * scale): each
// pixel's straight grey (0..1) and alpha byte (0..125).
pub fn render(p: &Prepared, technique: Technique, scale: f64, origin: [f64; 2], width: usize, height: usize) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(width * height);
    let picture = |x: usize, y: usize| ((x as f64 + 0.5 - origin[0]) / scale, (y as f64 + 0.5 - origin[1]) / scale);
    match technique {
        Technique::Bilinear => {
            for y in 0..height {
                for x in 0..width {
                    let (u, v) = picture(x, y);
                    out.push((p.grey.bilinear(u, v), p.alpha.bilinear(u, v)));
                }
            }
        }
        Technique::DistanceField | Technique::DistanceFieldBicubic => {
            let cubic = technique == Technique::DistanceFieldBicubic;
            for y in 0..height {
                for x in 0..width {
                    let (u, v) = picture(x, y);
                    let (dc, d_outer) = if cubic {
                        (p.core_distance.bicubic(u, v), p.outer_distance.bicubic(u, v))
                    } else {
                        (p.core_distance.bilinear(u, v), p.outer_distance.bilinear(u, v))
                    };
                    let (dc, d_outer) = (f64::from(dc), f64::from(d_outer));
                    out.push(combine(ramp(dc, scale), ramp(d_outer, scale), ramp(dc - STEP_OFFSET, scale)));
                }
            }
        }
        Technique::LevelSet => {
            for y in 0..height {
                for x in 0..width {
                    let (u, v) = picture(x, y);
                    let c = 0.5 - f64::from(p.core.bicubic(u, v));
                    let o = 0.5 - f64::from(p.outer.bicubic(u, v));
                    out.push(combine(ramp(c, scale), ramp(o, scale), ramp(c - 0.4, scale)));
                }
            }
        }
        Technique::Traced | Technique::Fitted => {
            let (cc, oc) = if technique == Technique::Traced { (&p.core_contours, &p.outer_contours) } else { (&p.core_fitted, &p.outer_fitted) };
            let transform = |q: [f64; 2]| [origin[0] + q[0] * scale, origin[1] + q[1] * scale];
            let mut core = Canvas::new(width, height);
            core.fill(cc, transform);
            let mut outer = Canvas::new(width, height);
            outer.fill(oc, transform);
            let mut step = Canvas::new(width, height);
            step.fill(&cc.iter().map(|c| offset(c, STEP_OFFSET)).collect::<Vec<_>>(), transform);
            let (core, outer, step) = (core.coverage(), outer.coverage(), step.coverage());
            for i in 0..width * height {
                out.push(combine(core[i], outer[i], step[i]));
            }
        }
    }
    out
}

// The glyph at an integer `scale` over its own picture, averaged back to one value per picture pixel: (premultiplied grey,
// alpha), both 0..1, against the picture's own. Returns the mean and the largest absolute difference of each.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Faithfulness {
    pub grey_mean: f64,
    pub grey_max: f64,
    pub alpha_mean: f64,
    pub alpha_max: f64,
    pub edge_pixels: usize,
    pub grey_edge_mean: f64,
    pub alpha_edge_mean: f64,
}

pub fn faithfulness(p: &Prepared, technique: Technique, scale: usize) -> Faithfulness {
    let (w, h) = (p.width, p.height);
    let picture = render(p, technique, scale as f64, [0.0, 0.0], w * scale, h * scale);
    let original = p.layers.recombined();
    let mut f = Faithfulness::default();
    let area = (scale * scale) as f64;
    for y in 0..h {
        for x in 0..w {
            let (mut pg, mut pa) = (0.0f64, 0.0f64);
            for j in 0..scale {
                for i in 0..scale {
                    let (g, a) = picture[(y * scale + j) * w * scale + x * scale + i];
                    pg += f64::from(g) * f64::from(a) / 125.0;
                    pa += f64::from(a) / 125.0;
                }
            }
            let (g0, a0) = original[y * w + x];
            let dg = (pg / area - f64::from(g0) * f64::from(a0) / 125.0).abs();
            let da = (pa / area - f64::from(a0) / 125.0).abs();
            f.grey_mean += dg;
            f.alpha_mean += da;
            f.grey_max = f.grey_max.max(dg);
            f.alpha_max = f.alpha_max.max(da);
            let near_edge = (-1..=1isize).any(|dy| {
                (-1..=1isize).any(|dx| {
                    let (a, b) = (p.alpha.at((x + PAD) as isize + dx, (y + PAD) as isize + dy), a0);
                    a != b || p.grey.at((x + PAD) as isize + dx, (y + PAD) as isize + dy) != g0
                })
            });
            if near_edge {
                f.edge_pixels += 1;
                f.grey_edge_mean += dg;
                f.alpha_edge_mean += da;
            }
        }
    }
    let n = (w * h) as f64;
    f.grey_mean /= n;
    f.alpha_mean /= n;
    if f.edge_pixels > 0 {
        f.grey_edge_mean /= f.edge_pixels as f64;
        f.alpha_edge_mean /= f.edge_pixels as f64;
    }
    f
}
