use std::collections::HashMap;

use crate::field::Field;

pub type Point = [f64; 2];

// A closed contour in picture coordinates, the inside on its left (y down).
pub type Contour = Vec<Point>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceParameters {
    pub spacing: f64,
    pub smoothing: f64,
    pub corner_window: f64,
    pub corner_angle: f64,
    pub corner_arm: f64,
    pub corner_reach: f64,
    pub straightness: f64,
    pub arm_bend: f64,
    pub fit_iterations: usize,
    pub fit_step: f64,
    pub fit_smoothing: f64,
}

impl Default for TraceParameters {
    fn default() -> Self {
        Self {
            spacing: 0.125,
            smoothing: 0.9,
            corner_window: 0.6,
            corner_angle: 40.0,
            corner_arm: 1.4,
            corner_reach: 1.2,
            straightness: 0.06,
            arm_bend: 15.0,
            fit_iterations: 20,
            fit_step: 0.4,
            fit_smoothing: 3.0,
        }
    }
}

// The zero level of `field` (negative inside), by marching squares over the pixel centres, each crossing placed by linear
// interpolation along the cell's side. A saddle is resolved by the mean of its four corners.
pub fn zero_contours(field: &Field) -> Vec<Contour> {
    let (w, h) = (field.width as isize, field.height as isize);
    let value = |x: isize, y: isize| -> f64 {
        let v = f64::from(field.at(x, y));
        if v == 0.0 { 1e-9 } else { v }
    };
    let pad = field.pad as f64;
    let point_of = |key: (u8, isize, isize)| -> Point {
        let (kind, x, y) = key;
        let (x1, y1) = if kind == 0 { (x + 1, y) } else { (x, y + 1) };
        let (a, b) = (value(x, y), value(x1, y1));
        let t = a / (a - b);
        let px = x as f64 + t * (x1 - x) as f64 + 0.5 - pad;
        let py = y as f64 + t * (y1 - y) as f64 + 0.5 - pad;
        [px, py]
    };
    let mut next: HashMap<(u8, isize, isize), (u8, isize, isize)> = HashMap::new();
    for y in -1..h {
        for x in -1..w {
            // Corners clockwise from the top left, sides: 0 top, 1 right, 2 bottom, 3 left.
            let corners = [value(x, y), value(x + 1, y), value(x + 1, y + 1), value(x, y + 1)];
            let side = |s: usize| -> (u8, isize, isize) {
                match s {
                    0 => (0, x, y),
                    1 => (1, x + 1, y),
                    2 => (0, x, y + 1),
                    _ => (1, x, y),
                }
            };
            let inside = corners.map(|c| c < 0.0);
            let mut entries = Vec::new();
            let mut exits = Vec::new();
            for s in 0..4 {
                let (a, b) = (inside[s], inside[(s + 1) % 4]);
                if a && !b {
                    exits.push(s);
                } else if !a && b {
                    entries.push(s);
                }
            }
            if entries.len() == 1 {
                next.insert(side(entries[0]), side(exits[0]));
            } else if entries.len() == 2 {
                let centre = corners.iter().sum::<f64>() / 4.0;
                for &e in &entries {
                    // From an entry, the exit next clockwise keeps the corner between them alone (the centre outside);
                    // the one after it joins the two inside corners through the centre.
                    let after: Vec<usize> = (1..4).map(|k| (e + k) % 4).filter(|s| exits.contains(s)).collect();
                    let exit = if centre < 0.0 { after[after.len() - 1] } else { after[0] };
                    next.insert(side(e), side(exit));
                }
            }
        }
    }
    let mut contours = Vec::new();
    let mut seen: HashMap<(u8, isize, isize), bool> = HashMap::new();
    let mut keys: Vec<_> = next.keys().copied().collect();
    keys.sort();
    for start in keys {
        if seen.contains_key(&start) {
            continue;
        }
        let mut contour = Vec::new();
        let mut at = start;
        loop {
            seen.insert(at, true);
            contour.push(point_of(at));
            match next.get(&at) {
                Some(&n) if n != start => at = n,
                _ => break,
            }
            if seen.contains_key(&at) {
                break;
            }
        }
        if contour.len() >= 3 {
            contours.push(contour);
        }
    }
    contours
}

fn distance(a: Point, b: Point) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

pub fn signed_area(c: &Contour) -> f64 {
    let mut s = 0.0;
    for i in 0..c.len() {
        let (a, b) = (c[i], c[(i + 1) % c.len()]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * s
}

fn resample(c: &Contour, spacing: f64) -> Contour {
    let n = c.len();
    let mut total = 0.0;
    for i in 0..n {
        total += distance(c[i], c[(i + 1) % n]);
    }
    let count = ((total / spacing).round() as usize).max(8);
    let step = total / count as f64;
    let mut out = Vec::with_capacity(count);
    let (mut i, mut along, mut target) = (0usize, 0.0f64, 0.0f64);
    while out.len() < count {
        let (a, b) = (c[i % n], c[(i + 1) % n]);
        let length = distance(a, b);
        if along + length >= target || i >= 2 * n {
            let t = if length > 0.0 { ((target - along) / length).clamp(0.0, 1.0) } else { 0.0 };
            out.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
            target += step;
        } else {
            along += length;
            i += 1;
        }
    }
    out
}

pub(crate) fn smooth(c: &[Point], sigma: f64, spacing: f64, closed: bool) -> Vec<Point> {
    let n = c.len();
    if sigma <= 0.0 || n < 3 {
        return c.to_vec();
    }
    let radius = ((3.0 * sigma / spacing).ceil() as isize).max(1);
    let weights: Vec<f64> = (-radius..=radius).map(|k| (-(k as f64 * spacing).powi(2) / (2.0 * sigma * sigma)).exp()).collect();
    let mut out = Vec::with_capacity(n);
    for i in 0..n as isize {
        if !closed && (i == 0 || i == n as isize - 1) {
            out.push(c[i as usize]);
            continue;
        }
        let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
        for k in -radius..=radius {
            let j = i + k;
            let p = if closed {
                c[j.rem_euclid(n as isize) as usize]
            } else {
                // An open run is mirrored about its fixed ends, so the ends' tangents are kept.
                let end = n as isize - 1;
                if j < 0 {
                    let q = c[(-j).min(end) as usize];
                    [2.0 * c[0][0] - q[0], 2.0 * c[0][1] - q[1]]
                } else if j > end {
                    let q = c[(2 * end - j).max(0) as usize];
                    [2.0 * c[end as usize][0] - q[0], 2.0 * c[end as usize][1] - q[1]]
                } else {
                    c[j as usize]
                }
            };
            let wk = weights[(k + radius) as usize];
            sx += wk * p[0];
            sy += wk * p[1];
            sw += wk;
        }
        out.push([sx / sw, sy / sw]);
    }
    out
}

// Twicing (2 G - G G): a Gaussian low pass whose response is flat at low frequencies, so a curve is not shrunk as a plain
// Gaussian would shrink it, while the wobble the five grey levels leave along an edge (a period of 1 to 3 pixels) is removed.
pub(crate) fn fair(c: &[Point], sigma: f64, spacing: f64, closed: bool) -> Vec<Point> {
    let once = smooth(c, sigma, spacing, closed);
    let twice = smooth(&once, sigma, spacing, closed);
    (0..c.len()).map(|i| [2.0 * once[i][0] - twice[i][0], 2.0 * once[i][1] - twice[i][1]]).collect()
}

fn angle_of(a: Point, b: Point) -> f64 {
    (b[1] - a[1]).atan2(b[0] - a[0])
}

fn wrap(a: f64) -> f64 {
    let mut a = a;
    while a > std::f64::consts::PI {
        a -= 2.0 * std::f64::consts::PI;
    }
    while a < -std::f64::consts::PI {
        a += 2.0 * std::f64::consts::PI;
    }
    a
}

// The least-squares line through `points`: a point on it and its direction.
fn fit_line(points: &[Point]) -> (Point, Point) {
    let n = points.len() as f64;
    let (mx, my) = (points.iter().map(|p| p[0]).sum::<f64>() / n, points.iter().map(|p| p[1]).sum::<f64>() / n);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in points {
        let (dx, dy) = (p[0] - mx, p[1] - my);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    ([mx, my], [theta.cos(), theta.sin()])
}

fn line_residual(points: &[Point], origin: Point, direction: Point) -> f64 {
    points.iter().map(|p| ((p[0] - origin[0]) * direction[1] - (p[1] - origin[1]) * direction[0]).abs()).fold(0.0, f64::max)
}

fn intersect(p: Point, d: Point, q: Point, e: Point) -> Option<Point> {
    let det = d[0] * e[1] - d[1] * e[0];
    if det.abs() < 1e-6 {
        return None;
    }
    let t = ((q[0] - p[0]) * e[1] - (q[1] - p[1]) * e[0]) / det;
    Some([p[0] + t * d[0], p[1] + t * d[1]])
}

struct Corner {
    at: usize,
    from: usize,
    to: usize,
    point: Point,
}

// A traced contour made smooth along its curves and sharp at its corners: resampled evenly, its turning measured over
// `corner_window`; where it turns more than `corner_angle` between two straight arms, the arms' lines are met at their
// intersection (when it is within `corner_reach` of the contour), and the runs between corners are smoothed with their
// ends fixed. A run that stays within `straightness` of its chord becomes that chord.
pub fn refine(contour: &Contour, p: &TraceParameters) -> (Contour, Vec<bool>) {
    let (refined, corners) = refine_with_corners(contour, p);
    let traced = signed_area(contour);
    if refined.len() >= 8 && (signed_area(&refined) - traced).abs() <= 0.2 * traced.abs() {
        return (refined, corners);
    }
    // Corners too close together for their arms (a small shape): the contour is only smoothed.
    let perimeter: f64 = (0..contour.len()).map(|i| distance(contour[i], contour[(i + 1) % contour.len()])).sum();
    let s = fair(&resample(contour, p.spacing), p.smoothing.min(perimeter / 14.0), p.spacing, true);
    let n = s.len();
    (s, vec![false; n])
}

fn refine_with_corners(contour: &Contour, p: &TraceParameters) -> (Contour, Vec<bool>) {
    let perimeter: f64 = (0..contour.len()).map(|i| distance(contour[i], contour[(i + 1) % contour.len()])).sum();
    // A small contour (an accent, a dot) is smoothed less, or it would shrink away.
    let p = &TraceParameters { smoothing: p.smoothing.min(perimeter / 14.0), ..*p };
    let c = resample(contour, p.spacing);
    let n = c.len();
    let base = smooth(&c, p.smoothing * 0.5, p.spacing, true);
    let outer = signed_area(&c) < 0.0;
    let w = ((p.corner_window / p.spacing).round() as usize).max(1);
    let arm = ((p.corner_arm / p.spacing).round() as usize).max(2);
    // A counter's corners need longer straight sides: its bowls are small and nearly straight over a short arm.
    let arm = if signed_area(&c) < 0.0 { arm } else { arm * 3 / 2 };
    if n < 2 * (w + arm) + 4 {
        let s = fair(&c, p.smoothing, p.spacing, true);
        let n = s.len();
        return (s, vec![false; n]);
    }
    let at = |i: isize| base[i.rem_euclid(n as isize) as usize];
    let turn: Vec<f64> = (0..n as isize)
        .map(|i| {
            let a = angle_of(at(i - w as isize - 1), at(i - w as isize + 1));
            let b = angle_of(at(i + w as isize - 1), at(i + w as isize + 1));
            wrap(b - a)
        })
        .collect();
    let threshold = p.corner_angle.to_radians();
    let mut corners: Vec<Corner> = Vec::new();
    for i in 0..n {
        let t = turn[i].abs();
        if t < threshold {
            continue;
        }
        let peak = (1..=w).all(|k| turn[(i + k) % n].abs() <= t && turn[(i + n - k) % n].abs() < t);
        if !peak {
            continue;
        }
        let ii = i as isize;
        // An arm of a corner is straight: its two halves point the same way (a small bowl's arms turn with it). One arm
        // may be short (the end of a stem three pixels wide) when the other is a long straight side, on an outer contour
        // only: a counter's small bowl would take it for a corner.
        let bent = |arm: &[Point]| {
            let h = arm.len() / 2;
            wrap(angle_of(arm[0], arm[h]) - angle_of(arm[h], arm[arm.len() - 1])).abs() > p.arm_bend.to_radians()
        };
        let arm_of = |sign: isize, length: usize| -> Vec<Point> { (0..length as isize).map(|k| at(ii + sign * (w as isize + k))).collect() };
        let straight = |points: &[Point], long: bool| {
            let (o, d) = fit_line(points);
            if long { !bent(points) && line_residual(points, o, d) <= 0.12 } else { line_residual(points, o, d) <= 0.04 }
        };
        let short = (arm / 2).max(2);
        let (long_before, long_after) = (straight(&arm_of(-1, arm), true), straight(&arm_of(1, arm), true));
        let before = if long_before { arm_of(-1, arm) } else { arm_of(-1, short) };
        let after = if long_after { arm_of(1, arm) } else { arm_of(1, short) };
        let accepted = (long_before && long_after) || (outer && ((long_before && straight(&after, false)) || (long_after && straight(&before, false))));
        if !accepted {
            continue;
        }
        let (po, pd) = fit_line(&before);
        let (qo, qd) = fit_line(&after);
        let (ta, tb) = (angle_of(before[before.len() - 1], before[0]), angle_of(after[0], after[after.len() - 1]));
        if wrap(tb - ta).abs() < threshold {
            continue;
        }
        let Some(x) = intersect(po, pd, qo, qd) else { continue };
        if distance(x, at(ii)) > p.corner_reach {
            continue;
        }
        // The corner's points: from where the contour leaves the first arm's line to where it joins the second's.
        let mut from = ii - w as isize;
        while from < ii {
            let q = at(from + 1);
            if ((q[0] - po[0]) * pd[1] - (q[1] - po[1]) * pd[0]).abs() > 0.03 {
                break;
            }
            from += 1;
        }
        let mut to = ii + w as isize;
        while to > ii {
            let q = at(to - 1);
            if ((q[0] - qo[0]) * qd[1] - (q[1] - qo[1]) * qd[0]).abs() > 0.03 {
                break;
            }
            to -= 1;
        }
        corners.push(Corner { at: i, from: from.rem_euclid(n as isize) as usize, to: to.rem_euclid(n as isize) as usize, point: x });
    }
    corners.sort_by_key(|k| k.at);
    // Corners whose spans overlap: the sharper one is kept.
    let mut kept: Vec<Corner> = Vec::new();
    for k in corners {
        if let Some(last) = kept.last() {
            let gap = (k.at + n - last.at) % n;
            if gap < 2 * w {
                if turn[k.at].abs() > turn[last.at].abs() {
                    kept.pop();
                } else {
                    continue;
                }
            }
        }
        kept.push(k);
    }
    if kept.len() >= 2 {
        let (first, last) = (&kept[0], &kept[kept.len() - 1]);
        if (first.at + n - last.at) % n < 2 * w {
            if turn[first.at].abs() > turn[last.at].abs() {
                kept.pop();
            } else {
                kept.remove(0);
            }
        }
    }
    if kept.is_empty() {
        let s = fair(&c, p.smoothing, p.spacing, true);
        let n = s.len();
        return (s, vec![false; n]);
    }
    let mut out = Vec::new();
    let mut corner = Vec::new();
    for k in 0..kept.len() {
        let (a, b) = (&kept[k], &kept[(k + 1) % kept.len()]);
        out.push(a.point);
        corner.push(true);
        let mut run = vec![a.point];
        let mut i = a.to;
        let end = b.from;
        let length = (end + n - i) % n;
        for _ in 0..=length {
            run.push(c[i]);
            i = (i + 1) % n;
        }
        run.push(b.point);
        let s = fair(&run, p.smoothing, p.spacing, false);
        let s = straighten_open(&s, p.straightness);
        out.extend_from_slice(&s[1..s.len() - 1]);
        corner.resize(out.len(), false);
    }
    (out, corner)
}

fn chord_deviation(run: &[Point]) -> f64 {
    let (a, b) = (run[0], run[run.len() - 1]);
    let length = distance(a, b);
    if length < 1e-9 {
        return f64::INFINITY;
    }
    let d = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
    run.iter().map(|p| ((p[0] - a[0]) * d[1] - (p[1] - a[1]) * d[0]).abs()).fold(0.0, f64::max)
}

fn straighten_open(run: &[Point], tolerance: f64) -> Vec<Point> {
    if run.len() > 3 && chord_deviation(run) <= tolerance {
        let (a, b) = (run[0], run[run.len() - 1]);
        let m = run.len() - 1;
        return (0..=m)
            .map(|i| {
                let t = i as f64 / m as f64;
                [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
            })
            .collect();
    }
    run.to_vec()
}

// The contour moved `by` along its outward normal (the inside is on the left), each point by the mean of its two sides'
// normals, stretched at a corner so that both sides move by `by` (at most twice).
pub fn offset(c: &Contour, by: f64) -> Contour {
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
            [p[0] + by * stretch * ux, p[1] + by * stretch * uy]
        })
        .collect()
}

// Douglas-Peucker on a closed contour: the points that keep it within `tolerance`, the corners always kept.
pub fn simplify(c: &Contour, corners: &[bool], tolerance: f64) -> Contour {
    let n = c.len();
    if n < 8 {
        return c.clone();
    }
    let mut keep = vec![false; n];
    let anchors: Vec<usize> = {
        let mut a: Vec<usize> = (0..n).filter(|&i| corners.get(i).copied().unwrap_or(false)).collect();
        if a.len() < 3 {
            a.extend([0, n / 3, 2 * n / 3]);
            a.sort_unstable();
            a.dedup();
        }
        a
    };
    for &a in &anchors {
        keep[a] = true;
    }
    for k in 0..anchors.len() {
        let (from, to) = (anchors[k], anchors[(k + 1) % anchors.len()]);
        let length = (to + n - from) % n;
        let length = if length == 0 { n } else { length };
        let mut stack = vec![(0usize, length)];
        while let Some((i, j)) = stack.pop() {
            if j <= i + 1 {
                continue;
            }
            let (a, b) = (c[(from + i) % n], c[(from + j) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length = (dx * dx + dy * dy).sqrt();
            let mut worst = (0.0, i);
            for m in i + 1..j {
                let q = c[(from + m) % n];
                let d = if length < 1e-12 { distance(q, a) } else { ((q[0] - a[0]) * dy - (q[1] - a[1]) * dx).abs() / length };
                if d > worst.0 {
                    worst = (d, m);
                }
            }
            if worst.0 > tolerance {
                keep[(from + worst.1) % n] = true;
                stack.push((i, worst.1));
                stack.push((worst.1, j));
            }
        }
    }
    (0..n).filter(|&i| keep[i]).map(|i| c[i]).collect()
}
