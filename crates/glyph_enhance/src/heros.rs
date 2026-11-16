use std::collections::BTreeMap;

use crate::trace::{Contour, Point, signed_area};

// TeX Gyre Heros Regular 2.004 (GUST Font License, fonts/README.md), unmodified: its outlines are drawn, never the file
// passed on in another form.
pub const HEROS_REGULAR: &[u8] = include_bytes!("../../../fonts/texgyreheros-regular.otf");

// Flattened chords are at most this long, in font units (1000 an em): a twentieth of a console pixel's chord error.
const CHORD: f64 = 6.0;

// One glyph of the font: closed polygons in font units, y up, outer contours counter-clockwise (positive signed area), as
// CFF draws them; its advance and ink box.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub contours: Vec<Contour>,
    pub advance: f64,
    pub ink: [f64; 4],
}

pub struct HerosFont {
    pub units_per_em: f64,
    pub glyphs: BTreeMap<i32, Outline>,
}

#[derive(Default)]
struct Flatten {
    contours: Vec<Contour>,
    current: Contour,
}

impl Flatten {
    fn last(&self) -> Point {
        self.current.last().copied().unwrap_or([0.0, 0.0])
    }

    fn close_current(&mut self) {
        let mut c = std::mem::take(&mut self.current);
        if c.len() > 1 && c.first() == c.last() {
            c.pop();
        }
        if c.len() >= 3 {
            self.contours.push(c);
        }
    }

    fn curve(&mut self, p1: Point, p2: Point, p3: Point) {
        let p0 = self.last();
        let length = |a: Point, b: Point| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let n = ((length(p0, p1) + length(p1, p2) + length(p2, p3)) / CHORD).ceil().clamp(1.0, 256.0) as usize;
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.current.push([a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]]);
        }
    }
}

impl ttf_parser::OutlineBuilder for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_current();
        self.current.push([f64::from(x), f64::from(y)]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push([f64::from(x), f64::from(y)]);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p0 = self.last();
        let q = [f64::from(x1), f64::from(y1)];
        let p = [f64::from(x), f64::from(y)];
        self.curve(
            [p0[0] + 2.0 / 3.0 * (q[0] - p0[0]), p0[1] + 2.0 / 3.0 * (q[1] - p0[1])],
            [p[0] + 2.0 / 3.0 * (q[0] - p[0]), p[1] + 2.0 / 3.0 * (q[1] - p[1])],
            p,
        );
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.curve([f64::from(x1), f64::from(y1)], [f64::from(x2), f64::from(y2)], [f64::from(x), f64::from(y)]);
    }

    fn close(&mut self) {
        self.close_current();
    }
}

impl HerosFont {
    // The outlines of `codes` (Unicode scalar values) the font has.
    pub fn parse(data: &[u8], codes: &[i32]) -> Result<Self, String> {
        let face = ttf_parser::Face::parse(data, 0).map_err(|e| format!("TeX Gyre Heros: {e}"))?;
        let mut glyphs = BTreeMap::new();
        for &code in codes {
            let Some(ch) = u32::try_from(code).ok().and_then(char::from_u32) else { continue };
            let Some(id) = face.glyph_index(ch) else { continue };
            let mut f = Flatten::default();
            let Some(bbox) = face.outline_glyph(id, &mut f) else { continue };
            f.close_current();
            let mut contours = f.contours;
            if contours.iter().map(signed_area).sum::<f64>() < 0.0 {
                contours.iter_mut().for_each(|c| c.reverse());
            }
            let advance = f64::from(face.glyph_hor_advance(id).unwrap_or(0));
            glyphs.insert(code, Outline { contours, advance, ink: [bbox.x_min.into(), bbox.y_min.into(), bbox.x_max.into(), bbox.y_max.into()] });
        }
        Ok(Self { units_per_em: f64::from(face.units_per_em()), glyphs })
    }

    pub fn regular(codes: &[i32]) -> Self {
        Self::parse(HEROS_REGULAR, codes).expect("the bundled TeX Gyre Heros")
    }
}
