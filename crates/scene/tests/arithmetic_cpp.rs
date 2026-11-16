mod common;

use common::{Hash, Rng, double_set, f, int_set, pair_set, single_set, trig_set};
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic, TruncatingArithmetic};

const N: u32 = 1 << 20;

// FNV-1a 64 of every result, from the C++ at commit 039cc58 (Source/Scene/Arithmetic.*, built with -O2 -ffp-contract=off
// -ffp-model=strict) fed the generators of tests/common: "truncating" is NativeArithmetic run under EeRounding.
// Columns: arithmetic, operation, input set, results, hash.
const EXPECTED: &str = include_str!("arithmetic_cpp.txt");

fn expected(tag: &str, group: &str) -> Vec<String> {
    EXPECTED
        .lines()
        .filter(|line| {
            let mut words = line.split(' ');
            words.next() == Some(tag) && group_of(words.next().unwrap_or("")) == group
        })
        .map(str::to_owned)
        .collect()
}

fn group_of(op: &str) -> &'static str {
    match op {
        "sin16" | "cos16" => "angles",
        "sinf" | "cosf" => "libm",
        "sineCosine" => "sine_cosine",
        _ => "basic",
    }
}

fn line(tag: &str, name: &str, h: &Hash) -> String {
    format!("{tag} {name} {} 0x{:016x}", h.count, h.h)
}

fn binary(tag: &str, out: &mut Vec<String>, op: &str, f2: impl Fn(f32, f32) -> f32) {
    for kind in 0..3 {
        let mut h = Hash::default();
        for (a, b) in pair_set(kind, N) {
            h.f32(f2(f(a), f(b)));
        }
        out.push(line(tag, &format!("{op} pairs{kind}"), &h));
    }
}

fn unary(tag: &str, out: &mut Vec<String>, op: &str, f1: impl Fn(f32) -> u32) {
    for kind in 0..3 {
        let mut h = Hash::default();
        for a in single_set(kind, N) {
            h.u32(f1(f(a)));
        }
        out.push(line(tag, &format!("{op} singles{kind}"), &h));
    }
}

fn basic<A: Arithmetic>(tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    binary(tag, &mut out, "add", A::add);
    binary(tag, &mut out, "sub", A::sub);
    binary(tag, &mut out, "mul", A::mul);
    binary(tag, &mut out, "div", A::div);
    binary(tag, &mut out, "vu0Quotient", |a, b| A::vu0_quotient(a.abs(), b.abs()));
    binary(tag, &mut out, "addExact", A::add_exact);
    binary(tag, &mut out, "subExact", A::sub_exact);
    binary(tag, &mut out, "quotientExact", A::quotient_exact);
    unary(tag, &mut out, "sqrt", |a| A::sqrt(a).to_bits());
    unary(tag, &mut out, "vu0Root", |a| A::vu0_root(a).to_bits());
    unary(tag, &mut out, "rootExact", |a| A::root_exact(a).to_bits());
    unary(tag, &mut out, "toInt", |a| A::to_int(a) as u32);
    unary(tag, &mut out, "toUnsigned", A::to_unsigned);
    for kind in 0..3 {
        let mut h = Hash::default();
        for x in double_set(kind, N) {
            h.f32(A::cut(f64::from_bits(x)));
        }
        out.push(line(tag, &format!("cut doubles{kind}"), &h));
    }
    let mut h = Hash::default();
    for x in int_set(N) {
        h.f32(A::from_int(x));
    }
    out.push(line(tag, "fromInt ints", &h));
    out
}

fn angles<A: Arithmetic>(tag: &str) -> Vec<String> {
    let mut s = Hash::default();
    let mut c = Hash::default();
    for a in -0x20000..0x20000 {
        s.f32(A::sin16(a));
        c.f32(A::cos16(a));
    }
    let mut r = Rng::new(0xa961e000);
    for _ in 0..65536 {
        let a = r.next32() as i32;
        s.f32(A::sin16(a));
        c.f32(A::cos16(a));
    }
    vec![line(tag, "sin16 angles", &s), line(tag, "cos16 angles", &c)]
}

fn libm<A: Arithmetic>(tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    for kind in 0..2 {
        let mut s = Hash::default();
        let mut c = Hash::default();
        for b in trig_set(kind, 4 << 20) {
            s.f32(A::sinf(f(b)));
            c.f32(A::cosf(f(b)));
        }
        out.push(line(tag, &format!("sinf trig{kind}"), &s));
        out.push(line(tag, &format!("cosf trig{kind}"), &c));
    }
    out
}

fn sine_cosine<A: Arithmetic>(tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    for kind in 2..4 {
        let mut h = Hash::default();
        for b in trig_set(kind, N) {
            let (s, c) = A::sine_cosine(f(b));
            h.f32(s);
            h.f32(c);
        }
        out.push(line(tag, &format!("sineCosine trig{kind}"), &h));
    }
    out
}

fn check(tag: &str, group: &str, got: Vec<String>) {
    let want = expected(tag, group);
    assert!(!want.is_empty());
    for (g, w) in got.iter().zip(&want) {
        assert_eq!(g, w);
    }
    assert_eq!(got.len(), want.len());
}

#[test]
fn ee_basic() {
    check("ee", "basic", basic::<EeArithmetic>("ee"));
}

#[test]
fn ee_angles() {
    check("ee", "angles", angles::<EeArithmetic>("ee"));
}

#[test]
fn ee_libm() {
    check("ee", "libm", libm::<EeArithmetic>("ee"));
}

#[test]
fn ee_sine_cosine() {
    check("ee", "sine_cosine", sine_cosine::<EeArithmetic>("ee"));
}

#[test]
fn native_basic() {
    check("native", "basic", basic::<NativeArithmetic>("native"));
}

#[test]
fn native_angles() {
    check("native", "angles", angles::<NativeArithmetic>("native"));
}

#[test]
fn native_libm() {
    check("native", "libm", libm::<NativeArithmetic>("native"));
}

#[test]
fn native_sine_cosine() {
    check("native", "sine_cosine", sine_cosine::<NativeArithmetic>("native"));
}

#[test]
fn truncating_basic() {
    check("truncating", "basic", basic::<TruncatingArithmetic>("truncating"));
}
