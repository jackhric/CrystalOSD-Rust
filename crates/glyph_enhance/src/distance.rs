use crate::field::Field;

const LIMIT: f32 = 16.0;

// Gustavson and Strand, "Anti-aliased Euclidean distance transform" (2011): the distance from a pixel's centre to the edge
// that crosses it, from its coverage `a` and the edge's normal (gx, gy), for an edge taken as straight within the pixel.
pub fn edge_distance(gx: f32, gy: f32, a: f32) -> f32 {
    if (gx == 0.0 && gy == 0.0) || gx == 0.0 || gy == 0.0 {
        return 0.5 - a;
    }
    let length = (gx * gx + gy * gy).sqrt();
    let (mut gx, mut gy) = ((gx / length).abs(), (gy / length).abs());
    if gx < gy {
        std::mem::swap(&mut gx, &mut gy);
    }
    let a1 = 0.5 * gy / gx;
    if a < a1 {
        0.5 * (gx + gy) - (2.0 * gx * gy * a).sqrt()
    } else if a < 1.0 - a1 {
        (0.5 - a) * gx
    } else {
        -0.5 * (gx + gy) + (2.0 * gx * gy * (1.0 - a)).sqrt()
    }
}

fn gradient(f: &Field, x: isize, y: isize) -> (f32, f32) {
    let s = std::f32::consts::SQRT_2;
    let gx = -f.at(x - 1, y - 1) - s * f.at(x - 1, y) - f.at(x - 1, y + 1) + f.at(x + 1, y - 1) + s * f.at(x + 1, y) + f.at(x + 1, y + 1);
    let gy = -f.at(x - 1, y - 1) - s * f.at(x, y - 1) - f.at(x + 1, y - 1) + f.at(x - 1, y + 1) + s * f.at(x, y + 1) + f.at(x + 1, y + 1);
    (gx, gy)
}

// The distance from every pixel centre to the shape of `coverage`, through the nearest pixel that holds an edge, plus the
// sub-pixel distance from that pixel's centre to its edge (edge_distance with the direction to it). Outside pixels only.
fn unsigned_outside(coverage: &Field) -> Vec<f32> {
    let (w, h) = (coverage.width as isize, coverage.height as isize);
    let mut edges = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let a = coverage.at(x, y);
            if a <= 0.0 {
                continue;
            }
            let border = a < 1.0 || [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&(dx, dy)| coverage.at(x + dx, y + dy) < 1.0);
            if border {
                let (gx, gy) = gradient(coverage, x, y);
                edges.push((x, y, a.min(1.0), gx, gy));
            }
        }
    }
    let mut out = vec![LIMIT; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let a = coverage.at(x, y);
            let i = (y * w + x) as usize;
            if a > 0.0 {
                let (gx, gy) = gradient(coverage, x, y);
                out[i] = if a < 1.0 { edge_distance(gx, gy, a) } else { -0.5 };
                continue;
            }
            let mut best = LIMIT;
            for &(ex, ey, ea, _, _) in &edges {
                let (dx, dy) = ((ex - x) as f32, (ey - y) as f32);
                let di = (dx * dx + dy * dy).sqrt();
                if di - 1.0 >= best {
                    continue;
                }
                let d = di + edge_distance(dx, dy, ea);
                if d < best {
                    best = d;
                }
            }
            out[i] = best;
        }
    }
    out
}

// Signed distance in pixels, positive outside the shape, at every pixel centre of the padded field.
pub fn signed_distance(coverage: &Field) -> Field {
    let outside = unsigned_outside(coverage);
    let inverse = Field { values: coverage.values.iter().map(|a| 1.0 - a.clamp(0.0, 1.0)).collect(), ..coverage.clone() };
    let inside = unsigned_outside(&inverse);
    let mut values = vec![0.0; outside.len()];
    for i in 0..values.len() {
        let a = coverage.values[i];
        values[i] = if a <= 0.0 {
            outside[i]
        } else if a >= 1.0 {
            -inside[i]
        } else {
            0.5 * (outside[i] - inside[i])
        };
    }
    Field { values, ..coverage.clone() }
}
