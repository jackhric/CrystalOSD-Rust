#![allow(clippy::approx_constant, clippy::type_complexity)]

mod opening_common;

#[path = "opening_common/vu0_vectors.rs"]
mod vectors;

use opening_common::program;
use scene::arithmetic::{Arithmetic, EeArithmetic, NativeArithmetic};
use scene::frame::{BlendOp, PassTopology, SourceKind, TargetName};
use scene::matrix::{Mat4, Vec4};
use scene::opening::*;

type Ee = EeArithmetic;
type EeVu0 = Vu0<EeArithmetic>;
type NativeVu0 = Vu0<NativeArithmetic>;

fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}
fn vec4(bits: &[u32]) -> Vec4 {
    [f(bits[0]), f(bits[1]), f(bits[2]), f(bits[3])]
}
fn mat4(bits: &[u32; 16]) -> Mat4 {
    [vec4(&bits[0..4]), vec4(&bits[4..8]), vec4(&bits[8..12]), vec4(&bits[12..16])]
}
fn bits_of(m: &Mat4) -> [u32; 16] {
    let mut out = [0u32; 16];
    for (i, x) in m.iter().flatten().enumerate() {
        out[i] = x.to_bits();
    }
    out
}
fn near(a: &Mat4, b: &Mat4, tolerance: f32) -> bool {
    a.iter().flatten().zip(b.iter().flatten()).all(|(x, y)| {
        let d = x - y;
        let limit = tolerance * 1.0f32.max(y.abs());
        d <= limit && -d <= limit
    })
}
fn close(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance * 1.0f32.max(b.abs())
}

// tests/scene/opening/Vu0Test.cpp vectors().
#[test]
fn vu0_vectors() {
    for c in vectors::SINE_COSINE {
        let (s, co) = Ee::sine_cosine(f(c[0]));
        assert_eq!((s.to_bits(), co.to_bits()), (c[1], c[2]), "sineCosine");
    }
    for (m, angles, out) in vectors::ROT_MATRIX {
        let m = mat4(m);
        let angles = vec4(angles);
        let got = EeVu0::rot_matrix(&m, &angles);
        assert_eq!(bits_of(&got), *out, "rotMatrix");
        if angles[0].abs() <= 3.14 && angles[1].abs() <= 3.14 && angles[2].abs() <= 3.14 {
            assert!(near(&NativeVu0::rot_matrix(&m, &angles), &got, 1e-4));
        }
        if m == EeVu0::unit_matrix() {
            assert_eq!(bits_of(&EeVu0::rot_matrix_unit(&angles)), *out, "rotMatrix of the unit matrix");
        }
    }
    for (a, b, out) in vectors::MUL_MATRIX {
        let got = EeVu0::mul_matrix(&mat4(a), &mat4(b));
        assert_eq!(bits_of(&got), *out, "mulMatrix");
        assert!(near(&NativeVu0::mul_matrix(&mat4(a), &mat4(b)), &got, 1e-4));
    }
    for (m, t, out) in vectors::TRANS_MATRIX {
        assert_eq!(bits_of(&EeVu0::trans_matrix(&mat4(m), &vec4(t))), *out, "transMatrix");
    }
    for (a, b, out) in vectors::SUB_VECTOR {
        assert_eq!(EeVu0::sub_vector(&vec4(a), &vec4(b)).map(f32::to_bits), *out, "subVector");
    }
    for (p, z, y, out) in vectors::CAMERA_MATRIX {
        let got = EeVu0::camera_matrix(&vec4(p), &vec4(z), &vec4(y));
        assert_eq!(bits_of(&got), *out, "cameraMatrix");
        assert!(near(&NativeVu0::camera_matrix(&vec4(p), &vec4(z), &vec4(y)), &got, 1e-3));
    }
    for (a, out) in vectors::VIEW_SCREEN_MATRIX {
        let g = |i: usize| f(a[i]);
        let got = EeVu0::view_screen_matrix(g(0), g(1), g(2), g(3), g(4), g(5), g(6), g(7), g(8));
        assert_eq!(bits_of(&got), *out, "viewScreenMatrix");
        assert!(near(&NativeVu0::view_screen_matrix(g(0), g(1), g(2), g(3), g(4), g(5), g(6), g(7), g(8)), &got, 1e-3));
    }
    for (l0, l1, l2, out) in vectors::NORMAL_LIGHT_MATRIX {
        assert_eq!(bits_of(&EeVu0::normal_light_matrix(&vec4(l0), &vec4(l1), &vec4(l2))), *out, "normalLightMatrix");
    }
    for (min, max, m, vertices, clipped) in vectors::CLIP_ALL {
        let vertices: Vec<Vec4> = vertices.chunks(4).map(vec4).collect();
        assert_eq!(EeVu0::clip_all(&vec4(min), &vec4(max), &mat4(m), &vertices), *clipped, "clipAll");
    }
}

// tests/scene/ArithmeticTest.cpp openingVectors() (ArithmeticOpeningTest on the same vectors): the arithmetic the opening calls.
#[test]
fn arithmetic_opening_vectors() {
    for c in vectors::SIN_COS {
        let x = f(c[0]);
        assert_eq!(Ee::sinf(x).to_bits(), c[1], "sinf");
        assert_eq!(Ee::cosf(x).to_bits(), c[2], "cosf");
        assert!(close(NativeArithmetic::sinf(x), f(c[1]), 1e-5));
        assert!(close(NativeArithmetic::cosf(x), f(c[2]), 1e-5));
    }
    for c in vectors::SUMS {
        assert_eq!(Ee::add_exact(f(c[0]), f(c[1])).to_bits(), c[2], "addExact");
        assert_eq!(Ee::sub_exact(f(c[0]), f(c[1])).to_bits(), c[3], "subExact");
    }
    for c in vectors::QUOTIENTS {
        let (a, b) = (f(c[0]), f(c[1]));
        assert_eq!(Ee::quotient_exact(a, b).to_bits(), c[2], "quotientExact");
        let native = NativeArithmetic::quotient_exact(a, b);
        assert!(if b == 0.0 { native == f(c[2]) } else { close(native, f(c[2]), 1e-6) });
    }
    for c in vectors::ROOTS {
        assert_eq!(Ee::root_exact(f(c[0])).to_bits(), c[1], "rootExact");
        assert!(close(NativeArithmetic::root_exact(f(c[0])), f(c[1]), 1e-6));
    }
    for bits in [0x43490f81, 0xc3490f81, 0x44000000, 0x7f7fffff, 0x7f800000, 0xff800000, 0x7fc00000] {
        assert!(Ee::sinf(f(bits)).is_nan());
        assert!(Ee::cosf(f(bits)).is_nan());
    }
    assert_eq!(Ee::mul(1e-22, 1e-22).to_bits(), 0x00000000);
    assert_eq!(Ee::mul(-1e-22, 1e-22).to_bits(), 0x80000000);
    assert_eq!(Ee::quotient_exact(1e-30, 1e30).to_bits(), 0x00000000);
    assert_eq!(Ee::add_exact(1.0, -1e-22).to_bits(), 0x3f7fffff);
    assert_eq!(Ee::add_exact(1.0, 1e-22).to_bits(), 0x3f800000);
    assert_eq!(Ee::quotient_exact(1.0, 0.0).to_bits(), 0x7f7fffff);
    assert_eq!(Ee::quotient_exact(-1.0, 0.0).to_bits(), 0xff7fffff);
}

fn hand_off(module: i32, execute_app_type: i32, previous_was_opening: bool) -> HandOff {
    HandOff { module, execute_app_type, previous_was_opening }
}
fn inputs(snapshot: u32, clock_forced: bool, hdd_ready: bool, hdd_exec: i32, cdda: i32) -> HandOffInputs {
    HandOffInputs { snapshot, clock_forced, hdd_ready, hdd_exec, cdda }
}

// tests/scene/opening/HandoffTest.cpp branches().
#[test]
fn handoff_branches() {
    assert_eq!(decide(&inputs(0x6C, true, false, 0, 0)), hand_off(2, -1, true));
    assert_eq!(decide(&inputs(0x6C, true, false, 0, 0)), decide(&inputs(0, true, false, 0, 0)));
    assert_eq!(decide(&inputs(0x6C, true, true, 0, 0)), hand_off(1, 1, false));
    assert_eq!(decide(&inputs(0x6C, true, false, 1, 0)), hand_off(1, 1, false));
    assert_eq!(decide(&inputs(0x6A, false, false, 0, 0)), hand_off(1, 2, false));
    assert_eq!(decide(&inputs(0x6B, false, false, 0, 0)), hand_off(1, 2, false));
    assert_eq!(decide(&inputs(0x6C, false, false, 0, 0)), hand_off(1, 1, false));
    assert_eq!(decide(&inputs(0x6D, false, false, 0, 0)), hand_off(1, 1, false));
    assert_eq!(decide(&inputs(0x6E, false, false, 0, 0)), hand_off(1, 0, false));
    assert_eq!(decide(&inputs(0x6F, false, false, 0, 0)), hand_off(1, 5, false));
    assert_eq!(decide(&inputs(0x70, false, false, 0, 0)), hand_off(1, 4, false));
    assert_eq!(decide(&inputs(0x71, false, false, 0, 0)), hand_off(2, -1, true));
    assert_eq!(decide(&inputs(0x72, false, false, 0, 1)), hand_off(5, -1, true));
    assert_eq!(decide(&inputs(0x72, false, false, 0, 0)), hand_off(2, -1, true));
    assert_eq!(decide(&inputs(0x72, false, false, 0, -1)), hand_off(2, -1, true));
    assert_eq!(decide(&inputs(0x73, false, false, 0, 0)), hand_off(1, 3, false));
    assert_eq!(decide(&inputs(0x74, false, false, 0, 0)), hand_off(4, -1, true));
    for s in [0, 0x64, 0x65, 0x69, 0x75, 0xFFFFFFFF] {
        assert_eq!(decide(&inputs(s, false, false, 0, 0)), hand_off(2, -1, true));
    }
    assert_eq!(decide(&inputs(0x64, false, true, 1, 0)), hand_off(0, 6, true));
    assert_eq!(decide(&inputs(0x6A, false, true, 1, 0)), hand_off(0, 6, true));
    assert_eq!(decide(&inputs(0x64, false, true, 0, 0)), hand_off(2, -1, true));
    assert_eq!(FRAMES_TO_CLOCK, 34);
}

// tests/scene/opening/TimelineTest.cpp constants(): the timeline's constants are the program's words.
#[test]
fn timeline_constants_are_the_programs() {
    let Some(image) = program() else { return };
    use timeline_constant::*;
    let table = [
        (0x36f9d4, HDD_ROLL_VELOCITY),
        (0x36f9d8, HDD_WAIT_VZ),
        (0x36f9dc, HDD_LATE_VZ),
        (0x36f9e0, HDD_EXEC_VZ),
        (0x36f9e4, STAGE_ONE_AZ),
        (0x36f9e8, HDD_AZ),
        (0x36f9ec, HDD_ROLL_ACC),
        (0x36f9f0, PLAIN_VZ),
        (0x36f9f4, PLAIN_ROLL_ACC),
        (0x36f9f8, PI),
        (0x36f9fc, TWO_PI),
        (0x36fa04, TWO_PI),
    ];
    for (address, value) in table {
        assert_eq!(image.word(address), value.to_bits(), "constant at {address:#010x}");
    }
    assert_eq!(image.word(0x36fa00), (-PI).to_bits());
}

// tests/scene/opening/TimelineTest.cpp whole(): the totals it checks of the default intro carried from the module's first call
// (they hold for the opening-full capture there; here they are checked on the computation alone).
#[test]
fn timeline_whole_intro_totals() {
    let options = BootOptions { disc: DiscSchedule { first: 0x65, later: 0x64 }, ..Default::default() };
    let mut timeline = Timeline::<Ee>::new(&options, &Timeline::<Ee>::initial_state(1));
    let initial = *timeline.state();
    assert!(initial.counter == 1 && initial.direction == [0.0; 4] && initial.lights[0] == [0.0; 4]);
    let (mut drawn, mut dive, mut logo_draws) = (0, 0, 0);
    let mut first_blur = [0; 4];
    let (mut first_fade, mut last_fade, mut last_result, mut last_logo) = (0, 0, -1, -1);
    let (mut dive_z, mut blur_z) = (0.0f32, 0.0f32);
    let mut last_counter = 0;
    let mut heard = Vec::new();
    for k in 0..247 {
        let counter = timeline.state().counter;
        assert_eq!(counter, k + 1);
        let step = timeline.step(if k == 0 { options.disc.first } else { options.disc.later });
        if k == 0 {
            timeline.scene_set_up();
        }
        heard.extend_from_slice(&step.sounds);
        if step.result == 0 {
            drawn += 1;
        }
        last_result = step.result;
        last_counter = counter;
        if timeline.state().go && dive == 0 {
            dive = counter;
            dive_z = timeline.state().camera[2];
        }
        if step.blur_level > 0 && first_blur[step.blur_level as usize] == 0 {
            first_blur[step.blur_level as usize] = counter;
            if step.blur_level == 1 {
                blur_z = timeline.state().camera[2];
            }
        }
        if step.fade_alpha >= 0 && counter > 2 {
            if first_fade == 0 {
                first_fade = counter;
            }
            last_fade = step.fade_alpha;
        }
        if step.logo_alpha >= 0 {
            logo_draws += 1;
            if logo_draws == 1 {
                assert_eq!(counter, 51);
            }
            let expected = if logo_draws <= 60 { 4 * logo_draws } else { 4 * (120 - logo_draws) };
            assert!(step.logo_value == expected && step.logo_alpha == 0x70.min(expected));
            last_logo = counter;
        }
    }
    assert!(drawn == 246 && last_result == 1 && last_counter == 247);
    assert!(dive == 121 && (dive_z - 20.9).abs() < 0.05);
    assert!(first_blur[1] == 214 && (blur_z - 68.6).abs() < 0.05 && first_blur[2] == 226 && first_blur[3] == 236);
    assert!(first_fade == 218 && last_fade == 131);
    assert!(logo_draws == 120 && last_logo == 170);
    assert_eq!(heard, vec![SoundEvent { id: 0x6140, argument: 1, counter: 122 }]);

    let mut native = Timeline::<NativeArithmetic>::new(&options, &Timeline::<NativeArithmetic>::initial_state(1));
    let mut native_drawn = 0;
    let mut native_last = 0;
    for i in 0..247 {
        let step = native.step(if i == 0 { options.disc.first } else { options.disc.later });
        if i == 0 {
            native.scene_set_up();
        }
        native_drawn += i32::from(step.result == 0);
        native_last = step.result;
    }
    assert!(native_drawn == 246 && native_last == 1);
}

fn opening_rand<A: Arithmetic>() {
    let Some(image) = program() else { return };
    let mut opening = Opening::<A>::new(&BootOptions::default(), &image);
    let mut display = 0;
    let (mut frames, mut first_logo, mut first_logo_alpha) = (0, 0, 0);
    while !opening.ended() {
        let frame = opening.frame(display, 0);
        if first_logo == 0 {
            for pass in &frame.passes {
                if pass.name == "logo" {
                    first_logo = frames + 1;
                    first_logo_alpha = pass.material.blend_constant;
                }
            }
        }
        display ^= 1;
        frames += 1;
        assert!(frames < 100000);
    }
    assert_eq!(frames, 246);
    assert_eq!(first_logo, 51);
    assert_eq!(first_logo_alpha, 4);
    assert_eq!(opening.rand_calls(), 27314);
    assert_eq!(opening.rand_state(), 0x60440537);
}

// tests/scene/opening/OpeningRandTest.cpp.
#[test]
fn opening_rand_ee() {
    opening_rand::<EeArithmetic>();
}

#[test]
fn opening_rand_native() {
    opening_rand::<NativeArithmetic>();
}

// tests/scene/opening/FlatTest.cpp run(): the geometry it checks of each helper, without the probed records.
#[test]
fn flat_geometry() {
    type F = Flat<EeArithmetic>;
    let (width, height) = (640, 224);
    for level in 1..=3 {
        let input = FlatInputs { blur_level: level, ..Default::default() };
        let mut passes = Vec::new();
        F::blur(&input, &mut passes);
        assert_eq!(passes.len(), level as usize * 2);
        for i in 0..level {
            let into = &passes[i as usize * 2];
            let back = &passes[i as usize * 2 + 1];
            assert!(into.target == TargetName::Extra && into.material.source_target == TargetName::Display);
            assert!(back.target == TargetName::Display && back.material.source_target == TargetName::Extra);
            assert!(into.topology == PassTopology::Sprites && into.vertices.len() == 2 && back.vertices.len() == 2);
            let shrink = i * (level - 1);
            let w = (((width * 7) >> 3) - 1 - shrink) as f32;
            let h = (((height * 7) >> 3) - 1 - shrink) as f32;
            assert!(into.vertices[1].x == w - 0.5 && into.vertices[1].y == h - 0.5);
            assert!(into.vertices[1].u == 640.0 && into.vertices[1].v == 224.0);
            assert!(back.vertices[1].x == 639.5 && back.vertices[1].y == 223.5);
            assert!(back.vertices[1].u == w && back.vertices[1].v == h);
            assert!(!into.half_line && !back.half_line);
        }
    }
    for counter in [1, 2, 3, 246] {
        let input = FlatInputs { counter, ..Default::default() };
        assert_eq!(F::page(&input), if counter & 1 != 0 { 0 } else { 640 * 224 / 64 });
        let mut passes = Vec::new();
        F::copy_to_store(&input, &mut passes);
        assert_eq!(passes.len(), 1);
        let p = &passes[0];
        assert!(p.target == TargetName::Store && p.material.source_target == TargetName::Display && !p.material.colour_only);
        assert!(p.vertices.len() == 2 && p.vertices[1].x == 319.5 && p.vertices[1].y == 223.5 && p.vertices[1].u == 640.0);
        assert_eq!(p.vertices[0].r, 0x80);
    }
    for fade in [0, 1, 0x40, 0x80, 0x81, 0xff, 0x1000] {
        let mut passes = Vec::new();
        F::fade(&FlatInputs { fade_alpha: fade, ..Default::default() }, &mut passes);
        assert!(passes.len() == 1 && passes[0].vertices.len() == 2);
        let capped = if fade < 0x81 { fade } else { 0x80 } as u8;
        assert!(passes[0].vertices[0].a == capped && passes[0].vertices[1].a == capped);
        assert_eq!(passes[0].material.blend, BlendOp::AlphaOver);
        assert!(passes[0].vertices[1].x == 639.9375 && passes[0].vertices[1].y == 223.9375);
    }
    let (open, bar) = (F::picture_height(), F::bar_rows());
    assert!(open == 164 && bar == 30);
    assert!(Flat::<NativeArithmetic>::picture_height() == open && Flat::<NativeArithmetic>::bar_rows() == bar);
    let mut passes = Vec::new();
    F::bars(&FlatInputs::default(), &mut passes);
    assert!(passes.len() == 1 && passes[0].vertices.len() == 4);
    let v = &passes[0].vertices;
    assert!(v[0].y == 0.0 && v[1].y == bar as f32 - 0.0625);
    assert!(v[2].y == (bar + open) as f32 && v[3].y == height as f32 - 0.0625);
    assert!(v[1].x == 639.9375 && v[3].x == 639.9375);
    for vertex in v {
        assert_eq!((vertex.r, vertex.g, vertex.b, vertex.a), (255, 255, 255, 128));
    }
    assert!(passes[0].material.blend == BlendOp::SubtractFixed && passes[0].material.blend_constant == 0x80);
    for alpha in [4, 0x70] {
        let mut passes = Vec::new();
        F::logo(&FlatInputs { logo_alpha: alpha, ..Default::default() }, &mut passes);
        assert!(passes.len() == 1 && passes[0].vertices.len() == 4);
        let v = &passes[0].vertices;
        for vertex in v {
            assert!(i32::from(vertex.a) == alpha && vertex.r == 0x80);
        }
        assert!(passes[0].material.source == SourceKind::Texture && passes[0].material.texture == 0);
        assert!(v[0].x == 120.0 && v[0].y == 105.0 && v[0].v == 1.5 && v[1].u == 256.0 && v[1].v == 31.0 && v[1].x == 375.5 && v[1].y == 120.5);
        assert!(v[2].x == 326.0 && v[2].v == 33.5 && v[3].v == 63.0 && v[3].x == 581.5);
    }
}
