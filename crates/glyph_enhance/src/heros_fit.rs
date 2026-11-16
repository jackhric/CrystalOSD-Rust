use crate::heros::{HerosFont, Outline};
use crate::layers::Layers;
use crate::raster::Canvas;
use crate::trace::{Contour, Point};
use scene::font::Font;

// Helvetica's vertical zones (Adobe's published AFM: XHeight 523, CapHeight 718); Heros keeps Nimbus Sans', a little
// taller. With `Model::remap` the font's zones are moved onto these before scaling.
pub const HELVETICA_X_HEIGHT: f64 = 523.0;
pub const HELVETICA_CAP_HEIGHT: f64 = 718.0;

// One rasterisation run of the console's Latin glyphs: the outline scaled to pixels per em (x, y), its baseline row in the
// 40-row picture, emboldened per side (x, y), the coverage blurred vertically by [blur, 1 - 2 blur, blur] and quantised to
// fifths (the white core's five greys).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Model {
    pub scale_x: f64,
    pub scale_y: f64,
    pub baseline: f64,
    pub embolden_x: f64,
    pub embolden_y: f64,
    pub blur: f64,
    pub remap: bool,
}

// Where one glyph's outline was placed, after the run: `dx` from the font's origin at the console's originX, `dy` raised
// (negative) or lowered, `sx` and `sy` its own scales over the run's (about the origin), `descender` the part below the
// baseline scaled, `bold` emboldening per side added to the run's, `accent` the accent's own move (is_accent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub run: usize,
    pub dx: f64,
    pub dy: f64,
    pub sx: f64,
    pub sy: f64,
    pub descender: f64,
    pub bold: f64,
    pub accent: [f64; 2],
}

impl Placement {
    pub const IDENTITY: Self = Self { run: 1, dx: 0.0, dy: 0.0, sx: 1.0, sy: 1.0, descender: 1.0, bold: 0.0, accent: [0.0, 0.0] };

    pub fn get(&self, k: usize) -> f64 {
        [self.dx, self.dy, self.sx, self.sy, self.descender, self.bold, self.accent[0], self.accent[1]][k]
    }

    pub fn set(&mut self, k: usize, v: f64) {
        match k {
            0 => self.dx = v,
            1 => self.dy = v.clamp(-4.0, 4.0),
            2 => self.sx = v.clamp(0.8, 1.6),
            3 => self.sy = v.clamp(0.8, 1.6),
            4 => self.descender = v.clamp(0.6, 1.2),
            5 => self.bold = v.clamp(-0.3, 0.6),
            6 => self.accent[0] = v.clamp(-3.0, 3.0),
            _ => self.accent[1] = v.clamp(-4.0, 4.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Letter,
    Descender,
    Digit,
    Mark,
}

impl Class {
    pub fn of(code: i32, outline: &Outline) -> Self {
        let ch = u32::try_from(code).ok().and_then(char::from_u32).unwrap_or(' ');
        if ch.is_ascii_digit() {
            Self::Digit
        } else if ch.is_alphabetic() {
            if outline.ink[1] < -100.0 { Self::Descender } else { Self::Letter }
        } else {
            Self::Mark
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Descender => "descender",
            Self::Digit => "digit",
            Self::Mark => "mark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zones {
    pub x_height: f64,
    pub cap_height: f64,
}

impl Zones {
    pub fn of(heros: &HerosFont) -> Self {
        let top = |c: char| heros.glyphs.get(&(c as i32)).map_or(0.0, |o| o.ink[3]);
        Self { x_height: top('x'), cap_height: top('H') }
    }

    fn remap(&self, v: f64) -> f64 {
        if v <= 0.0 {
            v
        } else if v <= self.x_height {
            v * HELVETICA_X_HEIGHT / self.x_height
        } else if v <= self.cap_height {
            HELVETICA_X_HEIGHT + (v - self.x_height) * (HELVETICA_CAP_HEIGHT - HELVETICA_X_HEIGHT) / (self.cap_height - self.x_height)
        } else {
            v - self.cap_height + HELVETICA_CAP_HEIGHT
        }
    }
}

// Each point moved along its corner's bisector, `x` and `y` per side: the outline emboldened as a rasteriser's
// emboldening does (FreeType's FT_Outline_EmboldenXY), outward for the picture's orientation (filled contours clockwise
// on screen, negative signed area with y down).
pub fn embolden(c: &Contour, x: f64, y: f64) -> Contour {
    let n = c.len();
    let normal = |a: Point, b: Point| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = (dx * dx + dy * dy).sqrt().max(1e-12);
        [-dy / length, dx / length]
    };
    (0..n)
        .map(|i| {
            let (a, p, b) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
            let (m0, m1) = (normal(a, p), normal(p, b));
            let (sx, sy) = (m0[0] + m1[0], m0[1] + m1[1]);
            let length = (sx * sx + sy * sy).sqrt();
            if length < 1e-9 {
                return p;
            }
            let (ux, uy) = (sx / length, sy / length);
            let stretch = (1.0 / (ux * m0[0] + uy * m0[1]).max(0.5)).min(2.0);
            [p[0] + x * stretch * ux, p[1] + y * stretch * uy]
        })
        .collect()
}

// A contour of an accent: wholly above the x-height's overshoot, or wholly below the baseline, in a glyph with others.
pub fn is_accent(outline: &Outline, zones: &Zones, contour: &Contour) -> bool {
    let low = contour.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let high = contour.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    outline.contours.len() > 1 && (low >= zones.x_height + 30.0 || high <= 0.0)
}

pub fn has_accent(outline: &Outline, zones: &Zones) -> bool {
    outline.contours.iter().any(|c| is_accent(outline, zones, c))
}

// The glyph's outline in picture coordinates (a pixel spans [i, i + 1)), placed and emboldened as the run drew it.
pub fn place(outline: &Outline, zones: &Zones, model: &Model, placement: &Placement, origin_x: f64) -> Vec<Contour> {
    let sx = model.scale_x * placement.sx / 1000.0;
    let sy = model.scale_y * placement.sy / 1000.0;
    outline
        .contours
        .iter()
        .map(|c| {
            let [ax, ay] = if is_accent(outline, zones, c) { placement.accent } else { [0.0, 0.0] };
            let moved: Contour = c
                .iter()
                .map(|&[u, v]| {
                    let v = if model.remap { zones.remap(v) } else { v };
                    let v = if v < 0.0 { v * placement.descender } else { v };
                    [origin_x + placement.dx + ax + u * sx, model.baseline + placement.dy + ay - v * sy]
                })
                .collect();
            embolden(&moved, model.embolden_x + placement.bold, model.embolden_y + placement.bold)
        })
        .collect()
}

// The run's 1x picture of placed contours: exact area coverage, blurred vertically, quantised to fifths when `fifths`.
pub fn rasterise(contours: &[Contour], width: usize, height: usize, blur: f64, fifths: bool) -> Vec<f32> {
    let mut canvas = Canvas::new(width, height);
    canvas.fill(contours, |q| q);
    let coverage = canvas.coverage();
    let blur = blur as f32;
    let at = |x: usize, y: isize| if y < 0 || y >= height as isize { 0.0 } else { coverage[y as usize * width + x] };
    let mut out = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let v = blur * at(x, y as isize - 1) + (1.0 - 2.0 * blur) * at(x, y as isize) + blur * at(x, y as isize + 1);
            out[y * width + x] = if fifths { (v * 5.0).round() / 5.0 } else { v };
        }
    }
    out
}

// Sum of absolute differences, and the pixels either inks.
pub fn difference(model: &[f32], console: &[f32]) -> (f64, usize) {
    let mut sum = 0.0;
    let mut inked = 0;
    for (a, b) in model.iter().zip(console) {
        if *a > 0.0 || *b > 0.0 {
            sum += f64::from((a - b).abs());
            inked += 1;
        }
    }
    (sum, inked)
}

// One console glyph to fit: its core coverage and its originX.
pub struct Target<'a> {
    pub code: i32,
    pub outline: &'a Outline,
    pub class: Class,
    pub core: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub origin_x: f64,
}

impl<'a> Target<'a> {
    pub fn new(font: &Font, heros: &'a HerosFont, code: i32) -> Option<Self> {
        let outline = heros.glyphs.get(&code)?;
        let glyph = font.glyph(code)?;
        let layers = Layers::of_glyph(font, &glyph);
        Some(Self {
            code,
            outline,
            class: Class::of(code, outline),
            core: layers.core,
            width: layers.width,
            height: layers.height,
            origin_x: f64::from(glyph.metrics[0]),
        })
    }

    pub fn error(&self, zones: &Zones, model: &Model, placement: &Placement, fifths: bool) -> (f64, usize) {
        let contours = place(self.outline, zones, model, placement, self.origin_x);
        difference(&rasterise(&contours, self.width, self.height, model.blur, fifths), &self.core)
    }

    // The mean over the inked pixels of the quantised model's difference: the reported error.
    pub fn mean_error(&self, zones: &Zones, model: &Model, placement: &Placement) -> f64 {
        let (sum, inked) = self.error(zones, model, placement, true);
        sum / inked.max(1) as f64
    }

    // The placement that puts the font's ink left `ink_offset` pixels right of originX.
    fn initial(&self, model: &Model, run: usize, ink_offset: f64) -> Placement {
        Placement { run, dx: ink_offset - self.outline.ink[0] * model.scale_x / 1000.0, ..Placement::IDENTITY }
    }

    // The best `dx` alone: a scan in tenths of a pixel, then golden-section within the best tenth.
    pub fn fit_dx(&self, zones: &Zones, model: &Model, start: Placement) -> (Placement, f64) {
        let cost = |dx: f64| self.error(zones, model, &Placement { dx, ..start }, false).0;
        let (mut best, mut best_cost) = (start.dx, cost(start.dx));
        for i in -25..=25 {
            let dx = start.dx + f64::from(i) * 0.1;
            let c = cost(dx);
            if c < best_cost {
                (best, best_cost) = (dx, c);
            }
        }
        let (dx, c) = golden(cost, best - 0.1, best + 0.1, 24);
        let (dx, c) = if c < best_cost { (dx, c) } else { (best, best_cost) };
        (Placement { dx, ..start }, c)
    }

    // The glyph's own residual placement by its class: dx always; the descender's scale for letters that descend; the
    // accent's move for accented letters past ASCII; dy and both scales for digits; dy, both scales and the weight for marks
    // (the console's are raised, and some resized). Bounded, so a glyph the font does not draw cannot fit something else.
    pub fn fit(&self, zones: &Zones, model: &Model, run: usize) -> Placement {
        let (start, _) = self.fit_dx(zones, model, self.initial(model, run, 1.5));
        let accent = self.code >= 0x80 && has_accent(self.outline, zones);
        let free: &[usize] = match (self.class, accent) {
            (Class::Letter, false) => return start,
            (Class::Letter, true) => &[0, 6, 7],
            (Class::Descender, false) => &[0, 4],
            (Class::Descender, true) => &[0, 4, 6, 7],
            (Class::Digit, _) => &[0, 1, 2, 3],
            (Class::Mark, _) => &[0, 1, 2, 3, 5],
        };
        let steps = [0.3, 0.5, 0.05, 0.05, 0.1, 0.1, 0.4, 0.4];
        let step: Vec<f64> = free.iter().map(|&k| steps[k]).collect();
        let make = |x: &[f64]| {
            let mut p = start;
            for (i, &k) in free.iter().enumerate() {
                p.set(k, x[i]);
            }
            p
        };
        let cost = |p: &Placement| self.error(zones, model, p, false).0;
        let mut best = (start, cost(&start));
        // Two starts for the vertical: the punctuation the console raised sits two to three pixels up.
        for dy in [0.0, -2.5] {
            let x0: Vec<f64> = free.iter().map(|&k| if k == 1 { dy } else { start.get(k) }).collect();
            let mut x = nelder_mead(|x| cost(&make(x)), &x0, &step, 200);
            x = nelder_mead(|x| cost(&make(x)), &x, &step.iter().map(|s| s * 0.3).collect::<Vec<_>>(), 120);
            let p = make(&x);
            let c = cost(&p);
            if c < best.1 {
                best = (p, c);
            }
        }
        best.0
    }
}

pub fn golden(f: impl Fn(f64) -> f64, mut a: f64, mut b: f64, iterations: usize) -> (f64, f64) {
    let r = (5f64.sqrt() - 1.0) / 2.0;
    let mut c = b - r * (b - a);
    let mut d = a + r * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..iterations {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - r * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + r * (b - a);
            fd = f(d);
        }
    }
    if fc < fd { (c, fc) } else { (d, fd) }
}

// Nelder and Mead's simplex search from `x0` with initial steps `step`, deterministic.
pub fn nelder_mead(f: impl Fn(&[f64]) -> f64, x0: &[f64], step: &[f64], iterations: usize) -> Vec<f64> {
    let n = x0.len();
    let mut simplex: Vec<(Vec<f64>, f64)> = Vec::with_capacity(n + 1);
    simplex.push((x0.to_vec(), f(x0)));
    for i in 0..n {
        let mut x = x0.to_vec();
        x[i] += step[i];
        let v = f(&x);
        simplex.push((x, v));
    }
    for _ in 0..iterations {
        simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
        let centroid: Vec<f64> = (0..n).map(|j| simplex[..n].iter().map(|s| s.0[j]).sum::<f64>() / n as f64).collect();
        let along = |t: f64| -> Vec<f64> { (0..n).map(|j| centroid[j] + t * (simplex[n].0[j] - centroid[j])).collect() };
        let reflected = along(-1.0);
        let fr = f(&reflected);
        if fr < simplex[0].1 {
            let expanded = along(-2.0);
            let fe = f(&expanded);
            simplex[n] = if fe < fr { (expanded, fe) } else { (reflected, fr) };
        } else if fr < simplex[n - 1].1 {
            simplex[n] = (reflected, fr);
        } else {
            let contracted = if fr < simplex[n].1 { along(-0.5) } else { along(0.5) };
            let fc = f(&contracted);
            if fc < simplex[n].1.min(fr) {
                simplex[n] = (contracted, fc);
            } else {
                let best = simplex[0].0.clone();
                for s in simplex.iter_mut().skip(1) {
                    s.0 = (0..n).map(|j| best[j] + 0.5 * (s.0[j] - best[j])).collect();
                    s.1 = f(&s.0);
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
    simplex.swap_remove(0).0
}

// The summed error of `targets` under `model`, each at its best placement (Target::fit), on `threads` threads (summed in
// the targets' order, so the result does not depend on the threads).
pub fn model_cost(targets: &[&Target<'_>], zones: &Zones, model: &Model, run: usize, threads: usize) -> f64 {
    let chunk = targets.len().div_ceil(threads.max(1)).max(1);
    let costs: Vec<f64> = std::thread::scope(|scope| {
        let handles: Vec<_> = targets
            .chunks(chunk)
            .map(|part| scope.spawn(move || part.iter().map(|t| t.error(zones, model, &t.fit(zones, model, run), false).0).collect::<Vec<f64>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a fitting thread")).collect()
    });
    costs.iter().sum()
}

// The run's model fitted to `targets` (letters: placed by dx, and their accents moved), from `start`.
pub fn fit_model(targets: &[&Target<'_>], zones: &Zones, start: &Model, run: usize, threads: usize, iterations: usize) -> Model {
    let make =
        |x: &[f64]| Model { scale_x: x[0], scale_y: x[1], baseline: x[2], embolden_x: x[3], embolden_y: x[4], blur: x[5].clamp(0.0, 0.33), remap: start.remap };
    let x0 = [start.scale_x, start.scale_y, start.baseline, start.embolden_x, start.embolden_y, start.blur];
    let step = [0.4, 0.4, 0.2, 0.05, 0.05, 0.03];
    make(&nelder_mead(|x| model_cost(targets, zones, &make(x), run, threads), &x0, &step, iterations))
}
