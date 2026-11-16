// A scalar field on a pixel grid, sample (x, y) at the pixel's centre (x + 0.5, y + 0.5). `pad` pixels of margin surround
// the picture it was made from, so picture coordinates are field coordinates minus `pad`.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub width: usize,
    pub height: usize,
    pub pad: usize,
    pub values: Vec<f32>,
}

impl Field {
    pub fn padded(picture: &[f32], width: usize, height: usize, pad: usize, outside: f32) -> Self {
        let (w, h) = (width + 2 * pad, height + 2 * pad);
        let mut values = vec![outside; w * h];
        for y in 0..height {
            for x in 0..width {
                values[(y + pad) * w + x + pad] = picture[y * width + x];
            }
        }
        Self { width: w, height: h, pad, values }
    }

    pub fn at(&self, x: isize, y: isize) -> f32 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.values[y * self.width + x]
    }

    // (u, v) in picture coordinates (a pixel spans [i, i + 1)).
    pub fn bilinear(&self, u: f64, v: f64) -> f32 {
        let x = u + self.pad as f64 - 0.5;
        let y = v + self.pad as f64 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let top = self.at(x0, y0) * (1.0 - fx) + self.at(x0 + 1, y0) * fx;
        let bottom = self.at(x0, y0 + 1) * (1.0 - fx) + self.at(x0 + 1, y0 + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    }

    pub fn bicubic(&self, u: f64, v: f64) -> f32 {
        let x = u + self.pad as f64 - 0.5;
        let y = v + self.pad as f64 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let weights = |t: f32| {
            // Catmull-Rom.
            let t2 = t * t;
            let t3 = t2 * t;
            [-0.5 * t3 + t2 - 0.5 * t, 1.5 * t3 - 2.5 * t2 + 1.0, -1.5 * t3 + 2.0 * t2 + 0.5 * t, 0.5 * t3 - 0.5 * t2]
        };
        let (wx, wy) = (weights(fx), weights(fy));
        let mut sum = 0.0;
        for (j, wj) in wy.iter().enumerate() {
            let mut row = 0.0;
            for (i, wi) in wx.iter().enumerate() {
                row += wi * self.at(x0 - 1 + i as isize, y0 - 1 + j as isize);
            }
            sum += wj * row;
        }
        sum
    }
}
