// Port of tests/scene/ArithmeticTest.cpp (CrystalOSD 039cc58) without its fixture-driven parts; every expected value is
// the C++ test's own.

use scene::arithmetic::{Arithmetic, EeArithmetic, MAX_FLOAT, MIN_NORMAL, NativeArithmetic, as_float, float_bits, s16, u16};
use scene::matrix::{Mat4, Matrix, Vec4};
use scene::ramp::{Ramp, tick_ramp};

type Ee = EeArithmetic;
type EeMatrix = Matrix<EeArithmetic>;
type NativeMatrix = Matrix<NativeArithmetic>;

// The generators of References/lib/ee-float.test.mjs.
struct Random(u32);

impl Random {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1103515245).wrapping_add(12345);
        self.0
    }
    fn single(&mut self, low: u32, high: u32) -> f32 {
        let exponent = low + self.next() % (high - low + 1);
        let sign = (self.next() & 1) << 31;
        as_float(sign | (exponent << 23) | (self.next() & 0x7fffff))
    }
    fn pair(&mut self) -> (f32, f32) {
        let a = self.single(60, 190);
        let gap = if self.next().is_multiple_of(4) { self.next() % 60 } else { self.next() % 30 };
        let ea = ((float_bits(a) >> 23) & 0xff) as i32;
        let eb = 1.max(254.min(ea - gap as i32));
        (a, self.single(eb as u32, eb as u32))
    }
}

fn fnv(values: &[f32]) -> u32 {
    let mut h = 0x811c9dc5u32;
    for v in values {
        let mut u = float_bits(*v);
        for _ in 0..4 {
            h ^= u & 0xff;
            h = h.wrapping_mul(0x01000193);
            u >>= 8;
        }
    }
    h
}

fn close(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance * 1.0f32.max(b.abs())
}

fn bits(x: f32) -> u32 {
    float_bits(x)
}

#[test]
fn fixed_cases() {
    assert_eq!(bits(Ee::cut(1.0 - 2f64.powi(-60))), 0x3f800000);
    assert_eq!(bits(Ee::add(1.0, -(2f32.powi(-60)))), 0x3f7fffff);
    assert_eq!(bits(Ee::mul(MIN_NORMAL, 0.5)), 0x00000000);
    assert_eq!(bits(Ee::mul(-MIN_NORMAL, 0.5)), 0x80000000);
    assert_eq!(bits(Ee::mul(MAX_FLOAT, 2.0)), 0x7f800000);
    assert_eq!(bits(Ee::add(MAX_FLOAT, MAX_FLOAT)), 0x7f800000);
    assert_eq!(bits(Ee::cut(3.4028235e38)), 0x7f7fffff);
    assert_eq!(bits(Ee::cut(3.4028236e38)), 0x7f800000);
    assert_eq!(bits(Ee::cut(-3.4028236e38)), 0xff800000);
    assert_eq!(bits(Ee::div(1.0, 0.0)), 0x7f800000);
    assert_eq!(bits(Ee::div(-1.0, 0.0)), 0xff800000);
    assert!(Ee::div(0.0, 0.0).is_nan());
    assert!(Ee::sqrt(-1.0).is_nan());
    assert_eq!(bits(Ee::vu0_quotient(1.0, 0.0)), 0x7f800000);
    assert_eq!(bits(Ee::vu0_root(0.0)), 0x00000000);
    let zero = EeMatrix::normalize(&[0.0, 0.0, 0.0]);
    assert!(zero[0].is_nan() && zero[1].is_nan() && zero[2].is_nan());

    let ints: [(u32, i32, u32); 6] =
        [(0x3fc00000, 1, 1), (0xbfc00000, -1, 0), (0x4f32d05e, 2147483647, 3000000000), (0xcf32d05e, i32::MIN, 0), (0x3f7d70a4, 0, 0), (0xbf7d70a4, 0, 0)];
    for (x, i, u) in ints {
        assert_eq!(Ee::to_int(as_float(x)), i);
        assert_eq!(Ee::to_unsigned(as_float(x)), u);
    }
    assert!(s16(0x18000) == -0x8000 && s16(0x7fff) == 0x7fff && u16(-1) == 0xffff);
}

#[test]
fn sweeps() {
    let mut random = Random(12345);
    let mut sums = Vec::new();
    for _ in 0..40000 {
        let (a, b) = random.pair();
        sums.push(Ee::add(a, b));
        sums.push(Ee::sub(a, b));
    }
    assert_eq!(fnv(&sums), 0x77104edd);

    let mut products = Vec::new();
    for _ in 0..20000 {
        let a = random.single(90, 160);
        let b = random.single(90, 160);
        products.extend([Ee::mul(a, b), Ee::div(a, b), Ee::sqrt(a.abs()), Ee::vu0_quotient(a.abs(), b.abs()), Ee::vu0_root(a)]);
    }
    assert_eq!(fnv(&products), 0x1c1172de);
}

#[test]
fn sine_table() {
    assert_eq!(bits(Ee::sin16(0)), 0x00000000);
    assert_eq!(bits(Ee::sin16(1)), 0x38c90cb6);
    assert_eq!(bits(Ee::sin16(0x2000)), 0x3f3502bb);
    assert_eq!(bits(Ee::sin16(0x4000)), 0x3f800000);
    let table: Vec<f32> = (0..=0x4000).map(Ee::sin16).collect();
    assert_eq!(fnv(&table), 0x85b1f341);

    let cases: [(i32, u32, u32); 9] = [
        (0, 0x00000000, 0x3f800000),
        (0x4000, 0x3f800000, 0x80000000),
        (0x8000, 0x80000000, 0xbf800000),
        (-1, 0xb8c90cb6, 0x3f800000),
        (0x2000, 0x3f3502bb, 0x3f3502bb),
        (-0x2000, 0xbf3502bb, 0x3f3502bb),
        (0x6000, 0x3f3502bb, 0xbf3502bb),
        (12345, 0x3f6d09bb, 0x3ec155b6),
        (70000, 0x3ed47c47, 0x3f68e721),
    ];
    for (angle, sine, cosine) in cases {
        assert_eq!(bits(Ee::sin16(angle)), sine, "sin16({angle})");
        assert_eq!(bits(Ee::cos16(angle)), cosine, "cos16({angle})");
        assert!(close(NativeArithmetic::sin16(angle), Ee::sin16(angle), 1e-6));
        assert!(close(NativeArithmetic::cos16(angle), Ee::cos16(angle), 1e-6));
    }
    let mut sweep = Vec::new();
    let mut a = -0x9000;
    while a <= 0x9000 {
        sweep.push(Ee::sin16(a));
        sweep.push(Ee::cos16(a));
        a += 7;
    }
    assert_eq!(fnv(&sweep), 0xcba6b14a);
}

#[test]
fn libm() {
    let cases: [(u32, u32); 18] = [
        (0x00000000, 0x3f800000),
        (0x3f000000, 0x3f60a940),
        (0x3f800000, 0x3f0a5140),
        (0xbf800000, 0x3f0a5140),
        (0x3f490fda, 0x3f3504f4),
        (0x3fc90fdb, 0xb33bbd2e),
        (0xbfc90fdb, 0xb33bbd2e),
        (0x40000000, 0xbed51132),
        (0x40200000, 0xbf4d17bf),
        (0x40400000, 0xbf7d7025),
        (0x40490fdb, 0xbf7fffff),
        (0xc0490fdb, 0xbf7fffff),
        (0x40800000, 0xbf275530),
        (0x41200000, 0xbf56cd64),
        (0x42c80000, 0x3f5cc0ed),
        (0x43480000, 0x3ef970a9),
        (0x3727c5ac, 0x3f7fffff),
        (0x3e99999a, 0x3f7490ed),
    ];
    for (x, cosine) in cases {
        assert_eq!(bits(Ee::cosf(as_float(x))), cosine, "cosf({x:#010x})");
        assert!(close(NativeArithmetic::cosf(as_float(x)), Ee::cosf(as_float(x)), 1e-6));
    }
    let sines: [(u32, u32); 24] = [
        (0x00000000, 0x00000000),
        (0x3089705f, 0x3089705f),
        (0x3dcccccd, 0x3dcc7576),
        (0xbdcccccd, 0xbdcc7576),
        (0x3f000000, 0x3ef57743),
        (0x3f47ae14, 0x3f340a1e),
        (0x3f4ccccd, 0x3f37a4a6),
        (0x3f800000, 0x3f576aa3),
        (0x3fc90fda, 0x3f7fffff),
        (0x3fd1eb85, 0x3f7f6321),
        (0x40000000, 0x3f68c7b7),
        (0xc0200000, 0xbf193578),
        (0x40490fdb, 0xb3bbbd2e),
        (0x40800000, 0xbf41bdce),
        (0x40966666, 0xbf7ffaf8),
        (0x40b00000, 0xbf349e4a),
        (0x40c90fdb, 0x343bbd2e),
        (0xc0c00000, 0x3e8f0f8c),
        (0x41200000, 0xbf0b44f7),
        (0x42f6e979, 0xbf4dcee3),
        (0xbfd20c4a, 0xbf7f5e91),
        (0x3ea0d1b7, 0x3e9e3016),
        (0x402df854, 0x3ed251ef),
        (0x42c80000, 0xbf01a12d),
    ];
    for (x, sine) in sines {
        assert_eq!(bits(Ee::sinf(as_float(x))), sine, "sinf({x:#010x})");
        assert!(close(NativeArithmetic::sinf(as_float(x)), Ee::sinf(as_float(x)), 1e-6));
    }
    let cosines: Vec<f32> = (0..=20000).map(|i| Ee::cosf((-201.0 + f64::from(i) * 0.0201) as f32)).collect();
    assert_eq!(fnv(&cosines), 0x5605f7f8);

    let pairs: [(u32, u32, u32); 5] = [
        (0x3cfdf3b6, 0x3cfdebd6, 0x3f7fe083),
        (0x3e147ae1, 0x3e13f5ff, 0x3f7d5041),
        (0x00000000, 0x00000000, 0x3f800000),
        (0xbe147ae1, 0xbe13f5ff, 0x3f7d5041),
        (0x3fc00000, 0x3f7f5bd4, 0x3d90deae),
    ];
    for (x, sine, cosine) in pairs {
        let (s, c) = Ee::sine_cosine(as_float(x));
        assert_eq!((bits(s), bits(c)), (sine, cosine));
        let (ns, nc) = NativeArithmetic::sine_cosine(as_float(x));
        assert!(close(ns, s, 1e-5) && close(nc, c, 1e-5));
    }
}

#[test]
fn matrices() {
    let mut chain = Vec::new();
    for i in 0..12 {
        let mut m = EeMatrix::rotate_y(&EeMatrix::rotate_z(&EeMatrix::identity(), 21845), 29976);
        m = EeMatrix::rotate_z(&m, s16((i << 16) / 12 - 0x8000));
        m = EeMatrix::move_(&m, 0.0, 20.0, 0.0);
        m = EeMatrix::rotate_y(&m, s16(s16(29976) << 2));
        m = EeMatrix::rotate_x(&m, -1234 * i);
        for row in &m {
            chain.extend_from_slice(row);
        }
    }
    let first: [u32; 16] =
        [0xbeb1c373, 0x3f19f63e, 0xbf382cf4, 0, 0x3f5db2cb, 0x3efff8bf, 0, 0, 0x3eb827bc, 0xbf1f7f70, 0xbf31c87c, 0, 0x418a8fbe, 0x411ffb77, 0, 0x3f800000];
    for k in 0..16 {
        assert_eq!(bits(chain[k]), first[k], "chain[{k}]");
    }
    assert_eq!(fnv(&chain), 0xe560d076);

    let n = EeMatrix::normalize(&[3.5, -1.25, 0.1]);
    assert_eq!((bits(n[0]), bits(n[1]), bits(n[2])), (0x3f70ffab, 0xbeac2455, 0x3cdc5778));
    assert_eq!(bits(EeMatrix::dot(&n, &[0.3, 2.0, -7.7])), 0xbf18dcad);

    let screen = EeMatrix::view_screen(512.0, 1.0, 0.47, 2048.0, 2048.0, 1.0, 16777215.0, 1.0, 65536.0);
    let expected: [[u32; 4]; 4] = [[0x44000000, 0, 0, 0], [0, 0x4370a3d7, 0, 0], [0x45000000, 0x45000000, 0xc37f00fe, 0x3f800000], [0, 0, 0x4b80007f, 0]];
    let as_bits = |m: &Mat4| m.map(|row: Vec4| row.map(bits));
    assert_eq!(as_bits(&screen), expected);
    let native_screen = NativeMatrix::view_screen(512.0, 1.0, 0.47, 2048.0, 2048.0, 1.0, 16777215.0, 1.0, 65536.0);
    for r in 0..4 {
        for c in 0..4 {
            assert!(close(native_screen[r][c], screen[r][c], 1e-5));
        }
    }

    let mut ramp = Ramp { length: 3, counter: 1, changed: 1, state: 1 };
    tick_ramp(&mut ramp);
    assert!(ramp.counter == 2 && ramp.changed == 0 && ramp.state == 1);
    tick_ramp(&mut ramp);
    assert!(ramp.counter == 3 && ramp.changed == 1 && ramp.state == 2);
    ramp.state = 3;
    ramp.counter = 1;
    tick_ramp(&mut ramp);
    assert!(ramp.counter == 0 && ramp.changed == 1 && ramp.state == 0);
}

// From the C++ at 039cc58: vu0Quotient of a negative quotient steps down through every single to -0, then to NaN.
#[test]
fn signed_vu0_quotient() {
    assert_eq!(bits(Ee::vu0_quotient(-1.0, -3.0)), 0xffffffff);
}
