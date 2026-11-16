use crate::field::Field;
use crate::raster::Canvas;
use crate::trace::{Contour, Point, fair};

// Moves each contour point along its outward normal until the contours' exact area coverage of the picture's pixels is the
// picture's own coverage (`target`), the runs between corners kept fair. The console's pixels are the box average of the
// ideal outline (quantised to fifths), so this is the outline they were made from, as far as the picture tells it.
pub fn fit(contours: &mut [Contour], corners: &[Vec<bool>], target: &Field, iterations: usize, step: f64, sigma: f64, spacing: f64) {
    let pad = target.pad as f64;
    for _ in 0..iterations {
        let mut canvas = Canvas::new(target.width, target.height);
        canvas.fill(contours, |q| [q[0] + pad, q[1] + pad]);
        let coverage = canvas.coverage();
        let residual = |q: Point| -> f64 {
            let (x, y) = ((q[0] + pad).floor() as isize, (q[1] + pad).floor() as isize);
            if x < 0 || y < 0 || x >= target.width as isize || y >= target.height as isize {
                return 0.0;
            }
            let i = y as usize * target.width + x as usize;
            f64::from(coverage[i] - target.values[i])
        };
        for (c, corner) in contours.iter_mut().zip(corners) {
            let n = c.len();
            // The move is smoothed along the contour before it is made: the outline's shape follows the pixels' coverage at
            // the scale of strokes and bowls, never at the scale of one pixel's step.
            let push: Vec<Point> = (0..n)
                .map(|i| {
                    let (a, p, b) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
                    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                    let length = (dx * dx + dy * dy).sqrt().max(1e-9);
                    let r = residual(p);
                    [step * r * dy / length, -step * r * dx / length]
                })
                .collect();
            let push = fair_between_corners(&push, corner, sigma, spacing);
            let moved: Vec<Point> = (0..n).map(|i| [c[i][0] + push[i][0], c[i][1] + push[i][1]]).collect();
            *c = moved;
        }
    }
}

fn fair_between_corners(c: &[Point], corner: &[bool], sigma: f64, spacing: f64) -> Contour {
    let n = c.len();
    let Some(first) = corner.iter().position(|&k| k) else { return fair(c, sigma, spacing, true) };
    let mut out = c.to_vec();
    let mut start = first;
    loop {
        let mut end = (start + 1) % n;
        while !corner[end] {
            end = (end + 1) % n;
        }
        let length = (end + n - start) % n;
        let length = if length == 0 { n } else { length };
        let run: Vec<Point> = (0..=length).map(|k| c[(start + k) % n]).collect();
        let s = fair(&run, sigma, spacing, false);
        for k in 1..length {
            out[(start + k) % n] = s[k];
        }
        start = end;
        if start == first {
            break;
        }
    }
    out
}
