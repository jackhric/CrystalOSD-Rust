use std::sync::OnceLock;

pub fn float_bits(x: f32) -> u32 {
    x.to_bits()
}

pub fn as_float(bits: u32) -> f32 {
    f32::from_bits(bits)
}

pub fn down(x: f32) -> f32 {
    as_float(float_bits(x).wrapping_sub(1))
}

pub fn s16(x: i32) -> i32 {
    i32::from(x as u16 as i16)
}

pub fn u16(x: i32) -> i32 {
    x & 0xffff
}

pub const MAX_FLOAT: f32 = f32::MAX;
pub const MIN_NORMAL: f32 = f32::MIN_POSITIVE;

pub trait Arithmetic {
    // The arithmetic the C++ runs under its EeRounding guard (coldStart and coldFont set it for NativeArithmetic only).
    type UnderEeRounding: Arithmetic;

    fn cut(x: f64) -> f32;
    fn add(a: f32, b: f32) -> f32;
    fn sub(a: f32, b: f32) -> f32;
    fn mul(a: f32, b: f32) -> f32;
    fn div(a: f32, b: f32) -> f32;
    fn sqrt(a: f32) -> f32;
    fn vu0_quotient(a: f32, b: f32) -> f32;
    fn vu0_root(a: f32) -> f32;
    fn to_int(x: f32) -> i32;
    fn to_unsigned(x: f32) -> u32;
    fn sin16(angle: i32) -> f32;
    fn cos16(angle: i32) -> f32;
    fn cosf(x: f32) -> f32;
    fn sinf(x: f32) -> f32;
    fn add_exact(a: f32, b: f32) -> f32;
    fn sub_exact(a: f32, b: f32) -> f32;
    fn quotient_exact(a: f32, b: f32) -> f32;
    fn root_exact(a: f32) -> f32;
    fn sine_cosine(radians: f32) -> (f32, f32);
    fn from_int(x: i32) -> f32 {
        x as f32
    }
}

// The EE's single precision as the JS model computes it (References/model/clock_math.mjs; facts
// page per caller): every result cut toward zero, no denormals.
#[derive(Debug, Clone, Copy, Default)]
pub struct EeArithmetic;

#[derive(Debug, Clone, Copy, Default)]
pub struct NativeArithmetic;

// NativeArithmetic as it computes under the C++ EeRounding guard (MXCSR round toward zero, flush to zero, denormals
// are zero), reproduced in software.
#[derive(Debug, Clone, Copy, Default)]
pub struct TruncatingArithmetic;

type Ee = EeArithmetic;

fn flush(x: f32) -> f32 {
    if x != 0.0 && x.abs() < MIN_NORMAL { if x < 0.0 { -0.0 } else { 0.0 } } else { x }
}

fn sine_table() -> &'static [f32; 0x4001] {
    static TABLE: OnceLock<[f32; 0x4001]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0.0f32; 0x4001];
        for (i, entry) in t.iter_mut().enumerate() {
            *entry = (i as f64 * std::f64::consts::FRAC_PI_2 / 16385.0).sin() as f32;
        }
        t
    })
}

// HDD OSD 1.10U libm (facts/config-cubes.md, References/model/ee_libm.mjs): cosf 0x00294B28,
// __kernel_cosf 0x00296CF8, __kernel_sinf 0x002977A0, __ieee754_rem_pio2f 0x00295850.
const C1: u32 = 0x3d2aaaab;
const C2: u32 = 0xbab60b61;
const C3: u32 = 0x37d00d01;
const C4: u32 = 0xb493f27c;
const C5: u32 = 0x310f74f6;
const C6: u32 = 0xad47d74e;
const S1: u32 = 0xbe2aaaab;
const S2: u32 = 0x3c088889;
const S3: u32 = 0xb9500d01;
const S4: u32 = 0x3638ef1b;
const S5: u32 = 0xb2d72f34;
const S6: u32 = 0x2f2ec9d3;
const INVPIO2: u32 = 0x3f22f984;
const PIO2_1: u32 = 0x3fc90f80;
const PIO2_1T: u32 = 0x37354443;
const PIO2_2: u32 = 0x37354400;
const PIO2_2T: u32 = 0x2e85a308;
const PIO2_3: u32 = 0x2e85a300;
const PIO2_3T: u32 = 0x248d3132;
const NPIO2_HW: [u32; 32] = [
    0x3fc90f00, 0x40490f00, 0x4096cb00, 0x40c90f00, 0x40fb5300, 0x4116cb00, 0x412fed00, 0x41490f00, 0x41623100, 0x417b5300, 0x418a3a00, 0x4196cb00, 0x41a35c00,
    0x41afed00, 0x41bc7e00, 0x41c90f00, 0x41d5a000, 0x41e23100, 0x41eec200, 0x41fb5300, 0x4203f200, 0x420a3a00, 0x42108300, 0x4216cb00, 0x421d1400, 0x42235c00,
    0x4229a500, 0x422fed00, 0x42363600, 0x423c7e00, 0x4242c700, 0x42490f00,
];

fn cos_kernel(x: f32, y: f32) -> f32 {
    let ix = float_bits(x) & 0x7fffffff;
    if ix <= 0x31ffffff && Ee::to_int(x) == 0 {
        return 1.0;
    }
    let z = Ee::mul(x, x);
    let mut p = Ee::mul(z, as_float(C6));
    p = Ee::add(p, as_float(C5));
    p = Ee::mul(z, p);
    p = Ee::add(p, as_float(C4));
    p = Ee::mul(z, p);
    p = Ee::add(p, as_float(C3));
    p = Ee::mul(z, p);
    p = Ee::add(p, as_float(C2));
    p = Ee::mul(z, p);
    p = Ee::add(p, as_float(C1));
    let r = Ee::mul(z, p);
    if ix <= 0x3e999999 {
        return Ee::sub(1.0, Ee::sub(Ee::mul(z, 0.5), Ee::sub(Ee::mul(z, r), Ee::mul(x, y))));
    }
    let qx = if ix > 0x3f480000 { as_float(0x3e900000) } else { as_float(ix - 0x01000000) };
    let zr = Ee::sub(Ee::mul(z, r), Ee::mul(x, y));
    let hz = Ee::sub(Ee::mul(z, 0.5), qx);
    Ee::sub(Ee::sub(1.0, qx), Ee::sub(hz, zr))
}

fn sin_kernel(x: f32, y: f32, iy: bool) -> f32 {
    let ix = float_bits(x) & 0x7fffffff;
    if ix <= 0x31ffffff && Ee::to_int(x) == 0 {
        return x;
    }
    let z = Ee::mul(x, x);
    let v = Ee::mul(z, x);
    let mut p = Ee::mul(z, as_float(S6));
    p = Ee::add(p, as_float(S5));
    p = Ee::mul(z, p);
    p = Ee::add(p, as_float(S4));
    p = Ee::mul(z, p);
    p = Ee::add(p, as_float(S3));
    p = Ee::mul(z, p);
    let r = Ee::add(p, as_float(S2));
    if !iy {
        return Ee::add(x, Ee::mul(v, Ee::add(Ee::mul(z, r), as_float(S1))));
    }
    Ee::sub(x, Ee::sub(Ee::sub(Ee::mul(z, Ee::sub(Ee::mul(y, 0.5), Ee::mul(v, r))), y), Ee::mul(v, as_float(S1))))
}

struct Reduced {
    n: i32,
    y0: f32,
    y1: f32,
}

// The reduction is carried as far as |x| <= 2^7 * pi/2, as the model carries it; beyond that the
// library calls __kernel_rem_pio2f, which the clock never reaches (NaN here).
fn rem_pio2(x: f32) -> Reduced {
    let hx = float_bits(x) as i32;
    let ix = (hx as u32) & 0x7fffffff;
    if ix <= 0x3f490fd8 {
        return Reduced { n: 0, y0: x, y1: 0.0 };
    }
    if ix <= 0x4016cbe3 {
        let positive = hx > 0;
        let step = |a: f32, b: f32| if positive { Ee::sub(a, b) } else { Ee::add(a, b) };
        let mut z = step(x, as_float(PIO2_1));
        let mut t = as_float(PIO2_1T);
        if (ix & 0xfffffff0) == 0x3fc90fd0 {
            z = step(z, as_float(PIO2_2));
            t = as_float(PIO2_2T);
        }
        let y0 = step(z, t);
        return Reduced { n: if positive { 1 } else { -1 }, y0, y1: step(Ee::sub(z, y0), t) };
    }
    if ix > 0x43490f80 {
        return Reduced { n: 0, y0: f32::NAN, y1: f32::NAN };
    }
    let t = x.abs();
    let n = Ee::to_int(Ee::add(Ee::mul(t, as_float(INVPIO2)), 0.5));
    let fn_ = n as f32;
    let mut r = Ee::sub(t, Ee::mul(fn_, as_float(PIO2_1)));
    let mut w = Ee::mul(fn_, as_float(PIO2_1T));
    let mut y0;
    if n < 32 && (ix & 0xffffff00) != NPIO2_HW[(n - 1) as usize] {
        y0 = Ee::sub(r, w);
    } else {
        let j = (ix >> 23) as i32;
        y0 = Ee::sub(r, w);
        if j - ((float_bits(y0) >> 23) & 0xff) as i32 > 8 {
            let mut before = r;
            w = Ee::mul(fn_, as_float(PIO2_2));
            r = Ee::sub(before, w);
            w = Ee::sub(Ee::mul(fn_, as_float(PIO2_2T)), Ee::sub(Ee::sub(before, r), w));
            y0 = Ee::sub(r, w);
            if j - ((float_bits(y0) >> 23) & 0xff) as i32 > 25 {
                before = r;
                w = Ee::mul(fn_, as_float(PIO2_3));
                r = Ee::sub(before, w);
                w = Ee::sub(Ee::mul(fn_, as_float(PIO2_3T)), Ee::sub(Ee::sub(before, r), w));
                y0 = Ee::sub(r, w);
            }
        }
    }
    let y1 = Ee::sub(Ee::sub(r, y0), w);
    if hx < 0 { Reduced { n: -n, y0: -y0, y1: -y1 } } else { Reduced { n, y0, y1 } }
}

// _sceVu0ecossin behind sceVu0RotMatrix* (facts/clock-camera.md): the coefficients S5432 as x, y, z, w.
const ECOSSIN: [u32; 4] = [0x362e9c14, 0xb94fb21f, 0x3c08873e, 0xbe2aaaa4];
const HALF_PI: u32 = 0x3fc90fdb;

impl Arithmetic for EeArithmetic {
    type UnderEeRounding = EeArithmetic;

    fn cut(x: f64) -> f32 {
        if x.is_nan() {
            return f32::NAN;
        }
        // 2^128 - 2^103: from here on fround gives Infinity, which the model keeps.
        if x.abs() >= 3.4028235677973366e38 {
            return if x > 0.0 { f32::INFINITY } else { f32::NEG_INFINITY };
        }
        let mut near = x as f32;
        if near.is_finite() && f64::from(near).abs() > x.abs() {
            near = down(near);
        }
        flush(near)
    }

    // The double sum can lose an addend far below the other; a sum that lands on a single with a
    // remainder of the opposite sign is one step nearer zero (two-sum remainder).
    fn add(a: f32, b: f32) -> f32 {
        let da = f64::from(a);
        let db = f64::from(b);
        let x = da + db;
        let near = x as f32;
        if f64::from(near) != x || !x.is_finite() {
            return Self::cut(x);
        }
        let part = x - da;
        let rest = (da - (x - part)) + (db - part);
        if rest == 0.0 || x == 0.0 {
            return Self::cut(x);
        }
        Self::cut(f64::from(if (rest > 0.0) == (x > 0.0) { near } else { down(near) }))
    }

    fn sub(a: f32, b: f32) -> f32 {
        Self::add(a, -b)
    }

    fn mul(a: f32, b: f32) -> f32 {
        Self::cut(f64::from(a) * f64::from(b))
    }

    fn div(a: f32, b: f32) -> f32 {
        Self::cut(f64::from(a) / f64::from(b))
    }

    fn sqrt(a: f32) -> f32 {
        Self::cut(f64::from(a).sqrt())
    }

    fn vu0_quotient(a: f32, b: f32) -> f32 {
        let mut q = Self::cut(f64::from(a) / f64::from(b));
        while f64::from(q) * f64::from(b) > f64::from(a) {
            q = down(q);
        }
        q
    }

    fn vu0_root(a: f32) -> f32 {
        let x = f64::from(a).abs();
        let mut r = Self::cut(x.sqrt());
        while f64::from(r) * f64::from(r) > x {
            r = down(r);
        }
        r
    }

    fn to_int(x: f32) -> i32 {
        if x.is_nan() {
            return 0;
        }
        if x >= 2147483647.0 {
            return i32::MAX;
        }
        if x <= -2147483648.0 {
            return i32::MIN;
        }
        x as i32
    }

    fn to_unsigned(x: f32) -> u32 {
        if !(x > 0.0) || x.is_infinite() {
            return 0;
        }
        (f64::from(x).trunc() % 4294967296.0) as u64 as u32
    }

    // The clock's sine: a quarter wave of 0x4001 entries; the divisor is 16385 (facts/clock-camera.md).
    fn sin16(angle: i32) -> f32 {
        let a = s16(angle);
        let mut v = if a < 0 { -a } else { a };
        if v >= 0x4000 {
            v = 0x8000 - v;
        }
        let s = sine_table()[v as usize];
        if a < 0 { -s } else { s }
    }

    fn cos16(angle: i32) -> f32 {
        Self::sin16(s16(angle) + 0x4000)
    }

    fn cosf(x: f32) -> f32 {
        let ix = float_bits(x) & 0x7fffffff;
        if ix <= 0x3f490fd8 {
            return cos_kernel(x, 0.0);
        }
        if ix > 0x7f7fffff {
            return Self::sub(x, x);
        }
        let r = rem_pio2(x);
        match r.n & 3 {
            0 => cos_kernel(r.y0, r.y1),
            1 => -sin_kernel(r.y0, r.y1, true),
            2 => -cos_kernel(r.y0, r.y1),
            _ => sin_kernel(r.y0, r.y1, true),
        }
    }

    fn sinf(x: f32) -> f32 {
        let ix = float_bits(x) & 0x7fffffff;
        if ix <= 0x3f490fd8 {
            return sin_kernel(x, 0.0, false);
        }
        if ix > 0x7f7fffff {
            return f32::NAN;
        }
        let r = rem_pio2(x);
        match r.n & 3 {
            0 => sin_kernel(r.y0, r.y1, true),
            1 => cos_kernel(r.y0, r.y1),
            2 => -sin_kernel(r.y0, r.y1, true),
            _ => -cos_kernel(r.y0, r.y1),
        }
    }

    fn add_exact(a: f32, b: f32) -> f32 {
        Self::add(a, b)
    }

    fn sub_exact(a: f32, b: f32) -> f32 {
        Self::sub(a, b)
    }

    // opening-lib quotient: a division by zero gives the largest single, not infinity.
    fn quotient_exact(a: f32, b: f32) -> f32 {
        if b == 0.0 {
            return if (a < 0.0 || (a == 0.0 && a.is_sign_negative())) != b.is_sign_negative() { -MAX_FLOAT } else { MAX_FLOAT };
        }
        let mut q = Self::cut(f64::from(a) / f64::from(b));
        if (f64::from(q) * f64::from(b)).abs() > f64::from(a).abs() {
            q = down(q);
        }
        q
    }

    fn root_exact(a: f32) -> f32 {
        Self::vu0_root(a)
    }

    fn sine_cosine(radians: f32) -> (f32, f32) {
        let negative = radians < 0.0;
        let half_pi = as_float(HALF_PI);
        let t = if negative { Self::add(half_pi, radians) } else { Self::sub(half_pi, radians) };
        let t2 = Self::mul(t, t);
        let mut v = [0.0f32; 4];
        for i in 0..4 {
            v[i] = Self::mul(Self::mul(as_float(ECOSSIN[i]), t), t2);
        }
        for x in v.iter_mut().take(3) {
            *x = Self::mul(*x, t2);
        }
        let mut sum = Self::add(t, v[3]);
        for x in v.iter_mut().take(2) {
            *x = Self::mul(*x, t2);
        }
        sum = Self::add(sum, v[2]);
        v[0] = Self::mul(v[0], t2);
        sum = Self::add(sum, v[1]);
        sum = Self::add(sum, v[0]);
        let cosine = Self::add(0.0, sum);
        let q = Self::add(0.0, Self::vu0_root(Self::sub(1.0, Self::mul(cosine, cosine))));
        (if negative { Self::sub(0.0, q) } else { Self::add(0.0, q) }, cosine)
    }
}

fn native_to_int(x: f32) -> i32 {
    if x.is_nan() {
        return 0;
    }
    if x >= 2147483647.0 {
        return i32::MAX;
    }
    if x <= -2147483648.0 {
        return i32::MIN;
    }
    x as i32
}

fn native_to_unsigned(x: f32) -> u32 {
    if x > 0.0 && x < 4294967296.0 { x as u32 } else { 0 }
}

impl Arithmetic for NativeArithmetic {
    type UnderEeRounding = TruncatingArithmetic;

    fn cut(x: f64) -> f32 {
        x as f32
    }
    fn add(a: f32, b: f32) -> f32 {
        a + b
    }
    fn sub(a: f32, b: f32) -> f32 {
        a - b
    }
    fn mul(a: f32, b: f32) -> f32 {
        a * b
    }
    fn div(a: f32, b: f32) -> f32 {
        a / b
    }
    fn sqrt(a: f32) -> f32 {
        a.sqrt()
    }
    fn vu0_quotient(a: f32, b: f32) -> f32 {
        a / b
    }
    fn vu0_root(a: f32) -> f32 {
        a.abs().sqrt()
    }
    fn to_int(x: f32) -> i32 {
        native_to_int(x)
    }
    fn to_unsigned(x: f32) -> u32 {
        native_to_unsigned(x)
    }
    fn sin16(angle: i32) -> f32 {
        let a = s16(angle);
        let mut v = if a < 0 { -a } else { a };
        if v >= 0x4000 {
            v = 0x8000 - v;
        }
        let s = (v as f32 * (std::f32::consts::FRAC_PI_2 / 16385.0)).sin();
        if a < 0 { -s } else { s }
    }
    fn cos16(angle: i32) -> f32 {
        Self::sin16(s16(angle) + 0x4000)
    }
    fn cosf(x: f32) -> f32 {
        x.cos()
    }
    fn sinf(x: f32) -> f32 {
        x.sin()
    }
    fn add_exact(a: f32, b: f32) -> f32 {
        a + b
    }
    fn sub_exact(a: f32, b: f32) -> f32 {
        a - b
    }
    fn quotient_exact(a: f32, b: f32) -> f32 {
        if b == 0.0 {
            return if (a < 0.0 || (a == 0.0 && a.is_sign_negative())) != b.is_sign_negative() { -MAX_FLOAT } else { MAX_FLOAT };
        }
        a / b
    }
    fn root_exact(a: f32) -> f32 {
        a.abs().sqrt()
    }
    fn sine_cosine(radians: f32) -> (f32, f32) {
        (radians.sin(), radians.cos())
    }
}

fn denormals_are_zero(x: f32) -> f32 {
    if x.is_subnormal() { if x.is_sign_negative() { -0.0 } else { 0.0 } } else { x }
}

// A double cut toward zero to a single: past the largest single it stays the largest single; denormal results are
// zero of their sign.
fn truncate(x: f64) -> f32 {
    if !x.is_finite() {
        return x as f32;
    }
    let near = x as f32;
    let cut = if near.is_infinite() {
        if x > 0.0 { MAX_FLOAT } else { -MAX_FLOAT }
    } else if f64::from(near).abs() > x.abs() {
        down(near)
    } else {
        near
    };
    flush(cut)
}

fn truncated_sum(a: f32, b: f32) -> f32 {
    let da = f64::from(a);
    let db = f64::from(b);
    let x = da + db;
    let near = x as f32;
    if f64::from(near) != x {
        return truncate(x);
    }
    let part = x - da;
    let rest = (da - (x - part)) + (db - part);
    if rest == 0.0 || x == 0.0 {
        return truncate(x);
    }
    truncate(f64::from(if (rest > 0.0) == (x > 0.0) { near } else { down(near) }))
}

fn truncated_quotient(a: f32, b: f32) -> f32 {
    let a = denormals_are_zero(a);
    let b = denormals_are_zero(b);
    if !a.is_finite() || !b.is_finite() || b == 0.0 {
        return a / b;
    }
    truncate(f64::from(a) / f64::from(b))
}

fn truncated_root(a: f32) -> f32 {
    let a = denormals_are_zero(a);
    if !a.is_finite() || a <= 0.0 {
        return a.sqrt();
    }
    truncate(f64::from(a).sqrt())
}

impl Arithmetic for TruncatingArithmetic {
    type UnderEeRounding = TruncatingArithmetic;

    fn cut(x: f64) -> f32 {
        truncate(x)
    }
    fn add(a: f32, b: f32) -> f32 {
        let a = denormals_are_zero(a);
        let b = denormals_are_zero(b);
        if !a.is_finite() || !b.is_finite() {
            return a + b;
        }
        truncated_sum(a, b)
    }
    fn sub(a: f32, b: f32) -> f32 {
        let a = denormals_are_zero(a);
        let b = denormals_are_zero(b);
        if !a.is_finite() || !b.is_finite() {
            return a - b;
        }
        truncated_sum(a, -b)
    }
    fn mul(a: f32, b: f32) -> f32 {
        let a = denormals_are_zero(a);
        let b = denormals_are_zero(b);
        if !a.is_finite() || !b.is_finite() {
            return a * b;
        }
        truncate(f64::from(a) * f64::from(b))
    }
    fn div(a: f32, b: f32) -> f32 {
        truncated_quotient(a, b)
    }
    fn sqrt(a: f32) -> f32 {
        truncated_root(a)
    }
    fn vu0_quotient(a: f32, b: f32) -> f32 {
        truncated_quotient(a, b)
    }
    fn vu0_root(a: f32) -> f32 {
        truncated_root(a.abs())
    }
    fn to_int(x: f32) -> i32 {
        native_to_int(x)
    }
    fn to_unsigned(x: f32) -> u32 {
        native_to_unsigned(x)
    }
    fn sin16(_: i32) -> f32 {
        panic!("TruncatingArithmetic::sin16: glibc sinf under round toward zero is not reproduced")
    }
    fn cos16(_: i32) -> f32 {
        panic!("TruncatingArithmetic::cos16: glibc sinf under round toward zero is not reproduced")
    }
    fn cosf(_: f32) -> f32 {
        panic!("TruncatingArithmetic::cosf: glibc cosf under round toward zero is not reproduced")
    }
    fn sinf(_: f32) -> f32 {
        panic!("TruncatingArithmetic::sinf: glibc sinf under round toward zero is not reproduced")
    }
    fn add_exact(a: f32, b: f32) -> f32 {
        Self::add(a, b)
    }
    fn sub_exact(a: f32, b: f32) -> f32 {
        Self::sub(a, b)
    }
    fn quotient_exact(a: f32, b: f32) -> f32 {
        let compared_a = denormals_are_zero(a);
        if denormals_are_zero(b) == 0.0 {
            return if (compared_a < 0.0 || (compared_a == 0.0 && a.is_sign_negative())) != b.is_sign_negative() { -MAX_FLOAT } else { MAX_FLOAT };
        }
        truncated_quotient(a, b)
    }
    fn root_exact(a: f32) -> f32 {
        truncated_root(a.abs())
    }
    fn sine_cosine(_: f32) -> (f32, f32) {
        panic!("TruncatingArithmetic::sine_cosine: glibc sinf and cosf under round toward zero are not reproduced")
    }
    fn from_int(x: i32) -> f32 {
        truncate(f64::from(x))
    }
}
