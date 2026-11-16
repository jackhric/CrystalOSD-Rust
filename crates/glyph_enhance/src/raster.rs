use crate::trace::{Contour, Point};

// Exact area coverage of closed contours (non-zero, clamped to 1), by accumulating each edge's signed area into the cells
// it crosses and summing along the rows (the method of font-rs). `transform` maps a contour point to canvas pixels.
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    cells: Vec<f32>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, cells: vec![0.0; width * height + 4] }
    }

    fn line(&mut self, p0: Point, p1: Point) {
        let (p0, p1) = ([p0[0] as f32, p0[1] as f32], [p1[0] as f32, p1[1] as f32]);
        if (p0[1] - p1[1]).abs() <= f32::EPSILON {
            return;
        }
        let (dir, p0, p1) = if p0[1] < p1[1] { (1.0f32, p0, p1) } else { (-1.0, p1, p0) };
        let dxdy = (p1[0] - p0[0]) / (p1[1] - p0[1]);
        let mut x = p0[0];
        let y0 = p0[1].max(0.0);
        if p0[1] < 0.0 {
            x -= p0[1] * dxdy;
        }
        let width = self.width;
        let limit = (self.height as f32).min(p1[1].ceil()) as usize;
        for y in (y0 as usize)..limit {
            let start = y * width;
            let dy = ((y + 1) as f32).min(p1[1]) - (y as f32).max(p0[1]);
            let next = x + dxdy * dy;
            let d = dy * dir;
            let (x0, x1) = if x < next { (x, next) } else { (next, x) };
            let x0 = x0.clamp(0.0, width as f32 - 1.0001);
            let x1 = x1.clamp(0.0, width as f32 - 1.0001);
            let x0_floor = x0.floor();
            let x0i = x0_floor as usize;
            let x1_ceil = x1.ceil();
            let x1i = x1_ceil as usize;
            if x1i <= x0i + 1 {
                let middle = 0.5 * (x0 + x1) - x0_floor;
                self.cells[start + x0i] += d - d * middle;
                self.cells[start + x0i + 1] += d * middle;
            } else {
                let s = (x1 - x0).recip();
                let x0f = x0 - x0_floor;
                let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
                let x1f = x1 - x1_ceil + 1.0;
                let am = 0.5 * s * x1f * x1f;
                self.cells[start + x0i] += d * a0;
                if x1i == x0i + 2 {
                    self.cells[start + x0i + 1] += d * (1.0 - a0 - am);
                } else {
                    let a1 = s * (1.5 - x0f);
                    self.cells[start + x0i + 1] += d * (a1 - a0);
                    for xi in x0i + 2..x1i - 1 {
                        self.cells[start + xi] += d * s;
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                    self.cells[start + x1i - 1] += d * (1.0 - a2 - am);
                }
                self.cells[start + x1i] += d * am;
            }
            x = next;
        }
    }

    pub fn fill(&mut self, contours: &[Contour], transform: impl Fn(Point) -> Point) {
        for c in contours {
            for i in 0..c.len() {
                self.line(transform(c[i]), transform(c[(i + 1) % c.len()]));
            }
        }
    }

    pub fn coverage(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.width * self.height);
        let mut sum = 0.0f32;
        for &cell in &self.cells[..self.width * self.height] {
            sum += cell;
            out.push(sum.abs().min(1.0));
        }
        out
    }
}
