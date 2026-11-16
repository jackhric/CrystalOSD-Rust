mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use audio::clock_sound::{
    ClockSound, ClockSoundOptions, ClockSoundSources, SharedWriteTiming, Video, WriteContext, WriteTiming, clock_sound_sources,
    clock_sound_sources_from_decoded,
};
use audio::data::hd_bank::HdBank;
use audio::data::snd_image::SndImage;
use audio::data::sq_sequence::SqSequence;
use audio::driver::iop_clock::{VBLANK_HANDLER_CYCLES, add_vblank_preemption, sample_of_cycle};
use audio::driver::snapshot_builder::{LIBSD_IMAGE_SIZE, STAGED_MEMBERS};
use audio::driver::{Driver, EnvxFeed, build_sound_images, build_sound_images_from_folder};
use audio::ee_sound_queue::{EeSoundQueue, SoundCommand};
use audio::lab::wav_writer::{read_wav, write_wav};
use audio::live_audio::{LiveAudio, LiveAudioOptions};
use audio::output::{Output, OutputBackend};
use audio::spu_write::{DriverCommand, SpuWrite, first_difference};
use audio::spu2::adpcm::{AdpcmState, decode_block};
use audio::spu2::core::Core;
use audio::spu2::envelope::{Envelope, Phase, VolumeSweep};
use audio::spu2::gauss::{gauss_tap, interpolate};
use audio::spu2::reverb::{ReverbPath, Stereo, reverb_filter, run_reverb};
use audio::spu2::snapshot::RAM_BYTES;
use audio::spu2::{Kind, Spu2, StageSample, decode, is_status};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn core_first_difference() {
    let w = |frame, sample, address, value| SpuWrite { frame, sample, address, value, cycle: 0, resume: 0 };
    let a = vec![w(1, 10, 0x1F9005A4, 0x3100), w(1, 10, 0x1F9005A6, 0)];
    let mut b = a.clone();
    b[0].sample = 99;
    assert_eq!(first_difference(&a, &b), None);
    b[1].value = 1;
    assert_eq!(first_difference(&a, &b), Some(1));
    let mut b = a.clone();
    b.pop();
    assert_eq!(first_difference(&a, &b), Some(1));
}

#[test]
fn sound_queue_unit_cases() {
    let cmd = |id, a1, a2, a3| SoundCommand { id, a1, a2, a3 };
    {
        let mut q = EeSoundQueue::with_ramp_counter(EeSoundQueue::RAMP_STEPS);
        let accepted = (0..130).filter(|&i| q.enqueue(cmd(0x6300, 1, i as u16 + 1, 0))).count();
        assert_eq!(accepted, 128);
        let sends = q.drain();
        assert_eq!(sends.len(), 128);
        for (i, s) in sends.iter().enumerate() {
            assert_eq!(usize::from(s.a2), i + 1);
        }
        assert!(q.enqueue(cmd(0x6300, 1, 9, 0)));
    }
    {
        let mut q = EeSoundQueue::with_ramp_counter(EeSoundQueue::RAMP_STEPS);
        for _ in 0..3 {
            q.enqueue(cmd(0x6300, 1, 6, 0));
        }
        assert_eq!(q.drain().len(), 1);
        q.enqueue(cmd(0x6300, 1, 6, 0));
        assert_eq!(q.drain().len(), 1);
        q.enqueue(cmd(0x6300, 1, 5, 0));
        q.enqueue(cmd(0x6140, 1, 7, 0));
        q.enqueue(cmd(0x6300, 1, 7, 0));
        q.enqueue(cmd(0x6300, 1, 5, 0));
        let sends = q.drain();
        assert_eq!(sends.len(), 3);
        assert!(sends[0].a2 == 5 && sends[1].id == 0x6140 && sends[2].a2 == 5);
    }
    {
        let mut q = EeSoundQueue::default();
        assert!(q.drain().is_empty());
        assert_eq!(q.drain(), vec![cmd(0x60D0, 1, 0, 0)]);
        assert_eq!(q.drain(), vec![cmd(0x60D0, 1, 0x158, 0x158)]);
        let mut ramps = 2;
        for _ in 0..20 {
            let sends = q.drain();
            ramps += sends.len();
            if sends.is_empty() {
                continue;
            }
            assert!(sends[0].id == 0x60D0 && sends[0].a2 == sends[0].a3);
            if ramps == 15 {
                assert_eq!(sends[0].a2, 0x12D0);
            }
        }
        assert_eq!(ramps, 15);
    }
    {
        let mut q = EeSoundQueue::default();
        q.enqueue(cmd(0x6140, 0, 0, 0));
        let (a2, a3) = (q.carried_a2(), q.carried_a3());
        q.drain();
        q.drain();
        assert!(q.carried_a2() == a2 && q.carried_a3() == a3);
    }
    {
        let mut q = EeSoundQueue::with_ramp_counter(EeSoundQueue::RAMP_STEPS);
        q.enqueue(cmd(0x6300, 1, 4, 0));
        assert!(q.carried_a3() == 0x0658 && q.carried_a2() == 0x065C);
        q.enqueue(cmd(0x6300, 1, 4, 0));
        assert!(q.carried_a3() == 0x0660 && q.carried_a2() == 0x0664);
        for _ in 0..126 {
            q.enqueue(cmd(0x6140, 0, 0, 0));
        }
        let a3 = q.carried_a3();
        assert!(!q.enqueue(cmd(0x6140, 0, 0, 0)));
        assert!(q.carried_a3() == a3 && a3 == 0x0658 + 8 * 127);
    }
}

#[test]
fn spu2_decode() {
    let kill = decode(0x1F9005A4).unwrap();
    assert!(kill.core == 1 && kill.kind == Kind::Core && kill.reg == 0x1A4);
    let pitch = decode(0x1F900454).unwrap();
    assert!(pitch.core == 1 && pitch.kind == Kind::Voice && pitch.voice == 5 && pitch.reg == 4);
    let address = decode(0x1F9001C0 + 12 * 3 + 4).unwrap();
    assert!(address.core == 0 && address.kind == Kind::VoiceAddress && address.voice == 3 && address.reg == 4);
    assert_eq!(decode(0x1F9002E4).unwrap().kind, Kind::Reverb);
    assert!(decode(0x1F900760).unwrap().kind == Kind::Reverb && decode(0x1F900788).unwrap().core == 1);
    assert_eq!(decode(0x1F900348).unwrap().kind, Kind::Unknown);
    let status = |a| is_status(a).unwrap();
    assert!(status(0x1F90000A));
    assert!(!status(0x1F9001C0) && status(0x1F9001C8) && status(0x1F9001CA));
    assert!(status(0x1F900340) && status(0x1F9001A0) && status(0x1F900770));
    assert!(!status(0x1F900004) && !status(0x1F90019A));
    assert!(decode(0x1F910000).is_err());
}

#[test]
fn spu2_quirks() {
    let mut spu = Spu2::new();
    let w = |spu: &mut Spu2, a, v| spu.write(a, v).unwrap();
    w(&mut spu, 0x1F9001C0, 0xFFFF);
    w(&mut spu, 0x1F9001C2, 0xFFFF);
    assert!(spu.read(0x1F9001C0).unwrap() == 0x000F && spu.read(0x1F9001C2).unwrap() == 0xFFF8);
    assert_eq!(spu.voice(0, 0).start_address, 0xFFFF8);
    w(&mut spu, 0x1F9001C8, 0x0001);
    w(&mut spu, 0x1F9001CA, 0x0012);
    assert_eq!(spu.voice(0, 0).next_address, 0x10011);
    w(&mut spu, 0x1F900004, 0x0123);
    assert!(spu.read(0x1F900004).unwrap() == 0x0123 && spu.voice(0, 0).pitch == 0x0123);
    w(&mut spu, 0x1F900000, 0x2000);
    assert_eq!(spu.voice(0, 0).left.level, 0x4000);
    w(&mut spu, 0x1F900002, 0x8000);
    assert!(spu.voice(0, 0).right.level == 0 && spu.voice(0, 0).right.reg == 0x8000);
    w(&mut spu, 0x1F9001A8, 0x0001);
    w(&mut spu, 0x1F9001AA, 0x2340);
    w(&mut spu, 0x1F9001AC, 0xBEEF);
    w(&mut spu, 0x1F9001AC, 0xCAFE);
    assert!(spu.ram()[2 * 0x12340] == 0xEF && spu.ram()[2 * 0x12340 + 1] == 0xBE);
    assert!(spu.ram()[2 * 0x12341] == 0xFE && spu.ram()[2 * 0x12341 + 1] == 0xCA);
    assert_eq!(spu.read(0x1F9001AA).unwrap(), 0x2342);
    w(&mut spu, 0x1F90033C, 0x000F);
    w(&mut spu, 0x1F90033E, 0x1234);
    assert!(spu.core(0).effects_end == 0x000FFFFF && spu.read(0x1F90033E).unwrap() == 0xFFFF);
    w(&mut spu, 0x1F900724, 0x0001);
    w(&mut spu, 0x1F900726, 0x0002);
    assert!(spu.core(1).reverb.diff_r_src == 0x00010002 && spu.core(1).reverb.diff_l_src == 0);
    assert!(spu.read(0x1F900728).unwrap() == 1 && spu.read(0x1F900724).unwrap() == 0);
    w(&mut spu, 0x1F900324, 0x0003);
    assert_eq!(spu.core(0).reverb.diff_l_src, 0x00030000);
    w(&mut spu, 0x1F900188, 0x0001);
    w(&mut spu, 0x1F90018A, 0x0002);
    assert!(spu.voice(0, 0).dry_l == -1 && spu.voice(0, 1).dry_l == 0 && spu.voice(0, 16).dry_l == 0 && spu.voice(0, 17).dry_l == -1);
    w(&mut spu, 0x1F90019A, 0xC080);
    assert!(spu.core(0).fx_enable && spu.core(0).attr == 0xC080);
    w(&mut spu, 0x1F900760, 0x3FFF);
    assert_eq!(spu.core(0).master_left.level, 0x7FFE);
    w(&mut spu, 0x1F900770, 0x1234);
    assert_eq!(spu.core(0).master_left.level, 0x7FFE);
    w(&mut spu, 0x1F900764, 0x8000);
    assert_eq!(spu.core(0).fx_vol.left, -32768);
}

#[test]
fn spu2_adpcm() {
    let mut zero = [0u8; 16];
    zero[..3].copy_from_slice(&[0x0C, 0x00, 0x87]);
    let a = decode_block(&zero, &mut AdpcmState::default());
    assert!(a.pcm[0] == 7 && a.pcm[1] == -8 && !a.loop_start && !a.loop_end && !a.loop_repeat);
    let mut flags = [0u8; 16];
    flags[..2].copy_from_slice(&[0x0C, 0x07]);
    let f = decode_block(&flags, &mut AdpcmState::default());
    assert!(f.loop_end && f.loop_repeat && f.loop_start);
    let mut shifted = [0u8; 16];
    shifted[..3].copy_from_slice(&[0x08, 0x00, 0x07]);
    assert_eq!(decode_block(&shifted, &mut AdpcmState::default()).pcm[0], 112);
    let mut filtered = [0u8; 16];
    filtered[..3].copy_from_slice(&[0x10, 0x00, 0x01]);
    let mut v = AdpcmState { prev1: 1000, prev2: 0 };
    let g = decode_block(&filtered, &mut v);
    assert!(g.pcm[0] == 5034 && g.pcm[1] == 4719);
    assert!(v.prev1 == i32::from(g.pcm[27]) && v.prev2 == i32::from(g.pcm[26]));
    let mut unknown = [0u8; 16];
    unknown[..3].copy_from_slice(&[0x70, 0x00, 0x01]);
    assert_eq!(decode_block(&unknown, &mut AdpcmState { prev1: 20000, prev2: -20000 }).pcm[0], 4096);
    let mut clamp = [0x77u8; 16];
    clamp[0] = 0x40;
    clamp[1] = 0;
    let c = decode_block(&clamp, &mut AdpcmState { prev1: 32767, prev2: 32767 });
    assert!(c.pcm[0] == 32767 && c.pcm[27] == 32767);
    let mut low = [0x88u8; 16];
    low[0] = 0x40;
    low[1] = 0;
    assert_eq!(decode_block(&low, &mut AdpcmState { prev1: -32768, prev2: -32768 }).pcm[27], -32768);
}

#[test]
fn spu2_envelope() {
    let mut e = Envelope::default();
    e.key_on();
    assert_eq!(e.phase, Phase::Attack);
    let mut last = -1;
    let mut i = 0;
    while i < 100000 && e.phase == Phase::Attack {
        e.step_with(0x00FF, 0x1FC0);
        assert!(e.level >= last);
        last = e.level;
        i += 1;
    }
    assert!(e.phase != Phase::Attack && e.level <= 0x7FFF);
    e.key_off();
    let mut i = 0;
    while i < 1000000 && e.phase != Phase::Off {
        e.step_with(0x00FF, 0x1FC0);
        i += 1;
    }
    assert!(e.phase == Phase::Off && e.level == 0);

    let mut linear = Envelope::default();
    linear.key_on();
    linear.step_with(0x00FF, 0x1FC0);
    assert!(linear.level == 7 << 11 && linear.counter == 0);
    let mut slow = Envelope::default();
    slow.key_on();
    slow.step_with(0x3C00 | 0x00FF, 0x1FC0);
    assert!(slow.level == 0 && slow.counter == (0x8000 >> 4));
    let mut sustain = Envelope { phase: Phase::Sustain, level: 100, ..Default::default() };
    assert!(sustain.step_with(0x00FF, 0x1FC0));
    sustain.level = 0;
    sustain.counter = 0;
    assert!(!sustain.step_with(0x00FF, 0x1FC0) && sustain.phase == Phase::Off);

    let mut sweep = VolumeSweep::default();
    sweep.write(0x3FFF);
    assert_eq!(sweep.level, 0x7FFE);
    sweep.write(0xA000);
    sweep.level = 0x4000;
    let mut i = 0;
    while i < 100000 && sweep.level > 0 {
        sweep.step();
        i += 1;
    }
    assert_eq!(sweep.level, 0);
    let mut exponential = Envelope { phase: Phase::Attack, level: 0x6001, ..Default::default() };
    exponential.step_with(0x80FF, 0x1FC0);
    assert!(exponential.level == 0x6001 && exponential.counter == 0x2000);
    exponential.level = 0x6000;
    exponential.counter = 0;
    exponential.step();
    assert!(exponential.level == 0x7FFF && exponential.counter == 0 && exponential.phase == Phase::Decay);

    let mut rise = VolumeSweep::default();
    rise.write(0xC000);
    rise.level = 0x6001;
    rise.step();
    assert!(rise.level == 0x6001 && rise.counter == 0x2000);
    rise.level = 0x6000;
    rise.counter = 0;
    rise.step();
    assert!(rise.level == 0x7FFF && rise.counter == 0);
    let mut fall = VolumeSweep::default();
    fall.write(0xE000);
    fall.level = 0x4000;
    fall.step();
    assert!(fall.level == 0x2000 && fall.counter == 0);
    let mut mirrored = VolumeSweep::default();
    mirrored.write(0x9000);
    mirrored.level = 0;
    mirrored.step();
    assert_eq!(mirrored.level, -(7 << 11));
}

#[test]
fn spu2_gauss() {
    assert!(gauss_tap(0) == -1 && gauss_tap(0x0FF) == 0x12C7 && gauss_tap(0x100) == 0x1307 && gauss_tap(0x1FF) == 0x59B3);
    for i in 0..256 {
        let sum = i32::from(gauss_tap(0x0FF - i)) + i32::from(gauss_tap(0x1FF - i)) + i32::from(gauss_tap(0x100 + i)) + i32::from(gauss_tap(i));
        assert!((0x7F7F..=0x7F81).contains(&sum));
    }
    assert_eq!(interpolate(&[1000; 4], 0), 993);
    for counter in [0u32, 0x10, 0xFF0, 0x1230] {
        assert!((990..=1000).contains(&interpolate(&[1000; 4], counter)));
    }
    assert_eq!(interpolate(&[0; 4], 0x800), 0);
    assert_eq!(interpolate(&[0, 32767, 0, 0], 0), (0x59B3 * 32767) >> 15);
    assert_eq!(interpolate(&[0, 0, 0, 32767], 0x10), (i32::from(gauss_tap(1)) * 32767) >> 15);
}

#[test]
fn spu2_gauss_table_equals_psx_spx() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../CrystalOSD/tests/audio/psx_spx_gauss.txt");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("skipped: no {}", path.display());
        return;
    };
    let taps: Vec<i32> = text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.trim().parse().unwrap()).collect();
    assert_eq!(taps.len(), 512);
    for (i, &t) in taps.iter().enumerate() {
        assert_eq!(i32::from(gauss_tap(i as u32)), t);
    }
}

#[test]
fn spu2_reverb_units() {
    let mut ram = vec![0u8; RAM_BYTES as usize];
    let mut core = Core { fx_enable: true, effects_end: 0, ..Core::default() };
    let none = run_reverb(&mut core, &mut ram, 100, Stereo { left: 1000, right: -1000 }, ReverbPath::Avx);
    assert!(none.left == 0 && none.right == 0 && core.reverb_position == 0);

    core.effects_start = 0x20000;
    core.effects_end = 0x2FFFF;
    core.reverb.apf2_l_dst = 0x40;
    core.reverb.apf2_size = 0x10;
    ram[2 * 0x20062] = 0xD2;
    ram[2 * 0x20062 + 1] = 0x04;
    run_reverb(&mut core, &mut ram, 100, Stereo::default(), ReverbPath::Avx);
    assert!(core.up[0][0] == 1234 && core.up[1][0] == 0 && core.up[0][64] == 1234);
    assert_eq!(core.reverb_position, 1);
    assert!(ram[2 * 0x20072] == 0 && ram[2 * 0x20072 + 1] == 0);

    core.reverb_position = 0;
    core.reverb.apf2_r_dst = 0x80;
    core.reverb.apf2_size = 0x10;
    ram[2 * (0x20000 + 25 + 0x70)] = 0x0A;
    ram[2 * (0x20000 + 25 + 0x70) + 1] = 0x00;
    run_reverb(&mut core, &mut ram, 51, Stereo::default(), ReverbPath::Avx);
    assert!(core.up[1][0] == 10 && core.up[0][0] == 0);

    core.effects_start = 0xFFF00;
    core.effects_end = 0xFFFFF;
    core.reverb.apf2_l_dst = 0xFFFF0000;
    core.reverb.same_l_src = 0xFFFFF000;
    for cycles in 0..600 {
        run_reverb(&mut core, &mut ram, cycles, Stereo { left: 32767, right: -32768 }, ReverbPath::Avx);
    }
    assert!(core.reverb_position < 64);

    let constant = [1000i16; 128];
    for path in [ReverbPath::Avx, ReverbPath::Sse] {
        for position in [0u32, 17, 63] {
            assert!((996..=1004).contains(&reverb_filter(&constant, position, false, path)));
            assert!((1992..=2008).contains(&reverb_filter(&constant, position, true, path)));
        }
    }
    assert_eq!(reverb_filter(&[0; 128], 5, false, ReverbPath::Avx), 0);
    let mut pulse = [0i16; 128];
    pulse[19] = 10000;
    assert_eq!(reverb_filter(&pulse, 39, false, ReverbPath::Avx), 5000);
    assert_eq!(reverb_filter(&pulse, 39, false, ReverbPath::Sse), 5000);
    assert_eq!(reverb_filter(&pulse, 39, true, ReverbPath::Avx), 10000);
    pulse[19] = 0;
    pulse[18] = 10000;
    assert_eq!(reverb_filter(&pulse, 39, false, ReverbPath::Avx), 3127);
}

#[test]
fn spu2_resampler_paths() {
    let mut seed = 12345u32;
    for round in 0..200u32 {
        let mut history = [0i16; 128];
        for h in history.iter_mut() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *h = ((seed >> 16) as i32 % 1500 - 750) as i16;
        }
        let position = round & 63;
        for up in [false, true] {
            assert_eq!(reverb_filter(&history, position, up, ReverbPath::Avx), reverb_filter(&history, position, up, ReverbPath::Sse));
        }
    }
}

#[test]
fn spu2_clamp() {
    for sign in [1, -1] {
        let mut spu = Spu2::new();
        let mut block = [0x0300u16, 0x7777, 0x7777, 0x7777, 0x7777, 0x7777, 0x7777, 0x7777];
        if sign < 0 {
            block[1..].fill(0x8888);
        }
        spu.upload(0x3000, &block);
        let base = 0x1F900400;
        let mut w = |a: u32, v: u16| spu.write(a, v).unwrap();
        for v in 0..3 {
            w(base + 16 * v, 0x3FFF);
            w(base + 16 * v + 2, 0x3FFF);
            w(base + 16 * v + 4, 0x1000);
            w(base + 16 * v + 6, 0x00FF);
            w(base + 16 * v + 8, 0x1FC0);
            w(0x1F9005C0 + 12 * v, 0);
            w(0x1F9005C0 + 12 * v + 2, 0x3000);
            w(0x1F9005C0 + 12 * v + 4, 0);
            w(0x1F9005C0 + 12 * v + 6, 0x3000);
        }
        for (a, v) in [(0x1F900588, 0xFFFF), (0x1F90058A, 0x00FF), (0x1F900590, 0xFFFF), (0x1F900592, 0x00FF), (0x1F900788, 0x3FFF), (0x1F90078A, 0x3FFF)] {
            w(a, v);
        }
        w(0x1F9005A0, 0x0007);
        w(0x1F9005A2, 0x0000);
        let mut last = StageSample::default();
        for _ in 0..200 {
            last = spu.mix();
        }
        assert!(last.field[10] == if sign > 0 { 32767 } else { -32768 } && last.field[18] == last.field[10]);
        let edge = if sign > 0 { 32765 } else { -32766 };
        assert!(spu.final_left() == edge && spu.final_right() == edge);
        assert_eq!(last.field[22], i32::from(spu.final_left()));
    }
}

#[test]
fn spu2_cold_end() {
    let mut spu = Spu2::new();
    assert!(spu.core(0).effects_end == 0xFFFFF && spu.core(1).effects_end == 0xFFFFF);
    spu.write(0x1F90073C, 0xE).unwrap();
    assert_eq!(spu.core(1).effects_end, 0xEFFFF);
    spu.write(0x1F90073E, 0x1234).unwrap();
    assert_eq!(spu.core(1).effects_end, 0xEFFFF);
    spu.write(0x1F90033C, 0x10F).unwrap();
    assert_eq!(spu.core(0).effects_end, 0xFFFFF);
}

#[test]
fn spu2_ram_image() {
    let mut spu = Spu2::new();
    let image = vec![0x5Au8; spu.ram().len()];
    spu.load_ram(&image).unwrap();
    assert_eq!(spu.ram(), &image[..]);
    assert!(spu.load_ram(&[0; 10]).is_err());
    spu.clear_ram(0x100, 4);
    assert!(spu.ram()[0xFF] == 0x5A && spu.ram()[0x100] == 0 && spu.ram()[0x103] == 0 && spu.ram()[0x104] == 0x5A);
    spu.clear_ram((image.len() - 2) as u32, 100);
    assert!(spu.ram()[image.len() - 3] == 0x5A && spu.ram()[image.len() - 1] == 0);
    spu.set_ticks(4003);
    assert_eq!(spu.ticks(), 4003);
}

#[test]
fn spu2_edges() {
    let mut spu = Spu2::new();
    spu.upload(0xFFFFE, &[0x1111, 0x2222, 0x3333, 0x4444]);
    assert!(spu.ram()[2 * 0xFFFFE] == 0x11 && spu.ram()[2 * 0xFFFFF + 1] == 0x22 && spu.ram()[0] == 0x33 && spu.ram()[3] == 0x44);
    spu.upload(0xFFFF8, &[0x0100, 0x7777, 0x7777, 0x7777, 0x7777, 0x7777, 0x7777, 0x7777]);
    let base = 0x1F900400;
    for (a, v) in [
        (base, 0x3FFF),
        (base + 2, 0x3FFF),
        (base + 4, 0x3FFF),
        (base + 6, 0x00FF),
        (base + 8, 0x1FC0),
        (0x1F9005C0, 0x000F),
        (0x1F9005C2, 0xFFF8),
        (0x1F9005A0, 0x0001),
    ] {
        spu.write(a, v).unwrap();
    }
    for _ in 0..400 {
        spu.mix();
    }
    let v = spu.voice(1, 0);
    assert!(v.next_address < 0x100000);
    assert_eq!(spu.core(1).endx & 1, 1);
    assert!(v.envelope.phase == Phase::Off && v.loop_address == 0x2800);
    assert!(v.next_address > 0x2800 && v.next_address < 0x2800 + 0x400);
}

#[test]
fn data_robust() {
    assert!(SndImage::from_container(&[0; 64]).is_err());
    assert!(SndImage::from_folder(Path::new("does-not-exist")).is_err());
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn get16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

#[test]
fn data_hd_robust() {
    let mut bank = vec![0u8; 0x30];
    bank[0xC..0x10].copy_from_slice(b"SShd");
    bank[0x10] = 0x80;
    bank[0x14..0x18].fill(0xFF);
    bank[0x2C..0x30].fill(0xFF);
    assert!(HdBank::parse(&bank).is_err());
    assert!(HdBank::parse(&[0; 8]).is_err());
    let mut late = vec![0u8; 0x40];
    late[0xC..0x10].copy_from_slice(b"SShd");
    late[0x10] = 0x30;
    late[0x32] = 0x0A;
    late[0x14..0x18].fill(0xFF);
    late[0x2C..0x30].fill(0xFF);
    late[0x38] = 0xFF;
    assert!(HdBank::parse(&late).is_err());
    let mut skipped = vec![0u8; 0x40];
    skipped[0xC..0x10].copy_from_slice(b"SShd");
    skipped[0x10] = 0x30;
    skipped[0x14..0x18].fill(0xFF);
    skipped[0x2C..0x30].fill(0xFF);
    skipped[0x32] = 0xFF;
    skipped[0x33] = 0xFF;
    let none = HdBank::parse(&skipped).unwrap();
    assert!(none.programs.is_empty() && none.effects.is_empty() && none.velocity.is_empty());
}

fn sequence_with(body: &[u8], pad_to: usize) -> Vec<u8> {
    let mut data = vec![0u8; 0x110];
    data[0xC..0x10].copy_from_slice(b"SSsq");
    data.extend_from_slice(body);
    let len = data.len().max(pad_to);
    data.resize(len, 0);
    data
}

#[test]
fn data_sq_robust() {
    let mut seq = vec![0u8; 0x20];
    seq[0xC..0x10].copy_from_slice(b"SSsq");
    assert!(SqSequence::parse(&seq).is_err());
    for body in
        [&[0x90u8, 60, 100][..], &[0x90, 60, 100, 0x00, 0x90, 60], &[0x90, 60, 100, 0x81, 0x81, 0x81, 0x81, 0x01], &[0xFF, 0x7F], &[0x00], &[0xFF, 0x51, 1]]
    {
        assert!(SqSequence::parse(&sequence_with(body, 0)).is_err());
    }
    let ok = SqSequence::parse(&sequence_with(&[0x90, 60, 100, 0x10, 0x80, 60, 0, 0x00, 0xFF, 0x2F], 0)).unwrap();
    assert!(ok.events.len() == 3 && ok.events[1].tick == 0x10 && ok.events[2].status == 0xFF && ok.events[2].data1 == 0x2F);
}

fn empty_bank(size: usize) -> Vec<u8> {
    let mut b = vec![0u8; size];
    b[0xC..0x10].copy_from_slice(b"SShd");
    put32(&mut b, 0x10, 0xFFFFFFFF);
    put32(&mut b, 0x14, 0xFFFFFFFF);
    put32(&mut b, 0x2C, 0xFFFFFFFF);
    b
}

#[test]
fn data_synth_hd_fields() {
    let mut b = empty_bank(0x700);
    put32(&mut b, 0x10, 0x100);
    put32(&mut b, 0x14, 0x200);
    put32(&mut b, 0x2C, 0x300);
    put16(&mut b, 0x100, 2);
    put16(&mut b, 0x102, 0x10);
    put16(&mut b, 0x104, 0xFFFF);
    put16(&mut b, 0x106, 0x50);
    b[0x110] = 0x01;
    b[0x111] = 0x21;
    b[0x112] = 0x22;
    for t in 0..2 {
        for i in 0..16 {
            b[0x118 + 16 * t + i] = (if t == 1 { 0x60 } else { 0x30 }) + i as u8;
        }
    }
    b[0x118 + 3] = 0xF0;
    b[0x128 + 3] = 0x05;
    b[0x150] = 0x80;
    b[0x151] = 0x41;
    b[0x152] = 0x42;
    for i in 0..16 {
        b[0x158 + i] = 0x90 + i as u8;
    }
    for i in 0..128 {
        b[0x202 + i] = (i * 3 + 1) as u8;
    }
    b[0x300] = 0x64;
    for n in 0..15 {
        for i in 0..20 {
            b[0x320 + 64 * n + i] = (n * 20 + i * 7 + 1) as u8;
        }
    }
    let bank = HdBank::parse(&b).unwrap();
    assert_eq!(bank.programs.len(), 2);
    let p0 = &bank.programs[0];
    assert!(p0.id == 0 && p0.head == 0x01 && p0.volume == 0x21 && p0.pan == 0x22 && p0.offset == 0x110 && p0.tones.len() == 2);
    for (t, x) in p0.tones.iter().enumerate() {
        let o = 0x118 + 16 * t;
        assert!(x.key_low == b[o] && x.key_high == b[o + 1] && x.root_key == b[o + 2]);
        assert!(x.sample_start == get16(&b, o + 4) && x.adsr1 == get16(&b, o + 6) && x.adsr2 == get16(&b, o + 8));
        assert!(x.pan == b[o + 11] && x.bend_range == b[o + 13] && x.flags == b[o + 15]);
    }
    assert!(p0.tones[0].fine_tune == -16 && p0.tones[1].fine_tune == 5);
    assert!(p0.tones[0].key_low == 0x30 && p0.tones[0].pan == 0x3B && p0.tones[0].bend_range == 0x3D && p0.tones[0].flags == 0x3F);
    assert!(bank.program(1).is_none());
    let p2 = bank.program(2).unwrap();
    assert!(p2.head == 0x80 && p2.volume == 0x41 && p2.pan == 0x42 && p2.offset == 0x150 && p2.tones.len() == 1);
    assert!(p2.tones[0].key_low == 0x90 && p2.tones[0].bend_range == 0x9D && p2.tones[0].flags == 0x9F && p2.tones[0].fine_tune == 0x93u8 as i8);
    assert_eq!(bank.velocity.len(), 128);
    for (i, &v) in bank.velocity.iter().enumerate() {
        assert_eq!(v, (i * 3 + 1) as u8);
    }
    assert!(bank.effect_table == Some(0x300) && bank.effect_master == 0x64);
    assert_eq!(bank.effects.len(), 14);
    for (n, e) in bank.effects.iter().enumerate() {
        let o = 0x320 + 64 * n;
        assert!(e.left == get16(&b, o) && e.right == get16(&b, o + 2) && e.pitch == get16(&b, o + 4) && e.flags_a == get16(&b, o + 6));
        assert!(e.adsr1 == get16(&b, o + 8) && e.adsr2 == get16(&b, o + 10) && e.word12 == get16(&b, o + 12) && e.word14 == get16(&b, o + 14));
        assert_eq!(e.bd_offset, u32::from(get16(&b, o + 16)) | u32::from(get16(&b, o + 18)) << 16);
    }
}

#[test]
fn data_synth_hd_bounds() {
    let refused = |b: &[u8]| HdBank::parse(b).is_err();
    let mut b = empty_bank(0x38);
    put32(&mut b, 0x10, 0x20);
    put16(&mut b, 0x20, 0);
    put16(&mut b, 0x22, 0x08);
    b[0x28] = 0x01;
    assert!(refused(&b));
    b[0x28] = 0x00;
    assert!(refused(&b));
    b.resize(0x3C, 0);
    b[0x28] = 0x01;
    assert!(refused(&b));
    b[0x28] = 0x00;
    assert!(refused(&b));
    b.resize(0x40, 0);
    assert!(!refused(&b));

    let mut b = empty_bank(0x40);
    put32(&mut b, 0x14, 0x20 + 0x10 - 2);
    assert!(refused(&b));
    let mut big = empty_bank(0x100);
    put32(&mut big, 0x14, 0x20);
    assert!(!refused(&big));

    let mut b = empty_bank(0x40);
    put32(&mut b, 0x2C, 0x40);
    assert!(refused(&b));
    put32(&mut b, 0x2C, 0x3F);
    let bank = HdBank::parse(&b).unwrap();
    assert!(bank.effects.is_empty() && bank.effect_table == Some(0x3F));

    let mut b = empty_bank(0x40);
    put32(&mut b, 0x10, 0x20);
    put16(&mut b, 0x20, 0xFFFF);
    assert!(refused(&b));
    put32(&mut b, 0x10, 0x40);
    assert!(refused(&b));
}

fn sequence(events: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 0x110];
    b[0xC..0x10].copy_from_slice(b"SSsq");
    put16(&mut b, 0, 0x1234);
    put16(&mut b, 2, 0x01E0);
    put32(&mut b, 4, 0x000A1B2C);
    for c in 0..16 {
        b[0x10 + 16 * c + 2] = 0xFF;
    }
    b[0x11..0x15].copy_from_slice(&[5, 0x20, 0x30, 0x40]);
    b[0x31..0x35].copy_from_slice(&[7, 0x21, 0x31, 0x41]);
    b.extend_from_slice(events);
    b
}

#[test]
fn data_synth_sq_fields() {
    let data = sequence(&[
        0xB0, 0x63, 0x14, 0x00, 0x90, 0x3C, 0x64, 0x83, 0x60, 0x3E, 0x65, 0x81, 0x80, 0x00, 0xC0, 0x2A, 0x7F, 0xD1, 0x55, 0x05, 0xFF, 0x51, 0x11, 0x22, 0x33,
        0xB0, 0x63, 0x1E, 0x00, 0x64, 0x28, 0x00, 0xFF, 0x2F,
    ]);
    let seq = SqSequence::parse(&data).unwrap();
    assert!(seq.volume == 0x1234 && seq.resolution == 0x01E0 && seq.tempo == 0x000A1B2C);
    assert_eq!(seq.channels.len(), 2);
    let c = &seq.channels;
    assert!(c[0].channel == 5 && c[0].program == 0x20 && c[0].volume == 0x30 && c[0].pan == 0x40);
    assert!(c[1].channel == 7 && c[1].program == 0x21 && c[1].volume == 0x31 && c[1].pan == 0x41);
    let e = &seq.events;
    assert_eq!(e.len(), 9);
    assert!(e[0].tick == 0 && e[0].status == 0xB0 && e[0].data1 == 0x63 && e[0].data2 == 0x14 && e[0].has_data2);
    assert!(e[1].tick == 0 && e[1].status == 0x90 && e[1].data1 == 0x3C && e[1].data2 == 0x64);
    assert!(e[2].tick == 480 && e[2].status == 0x90 && e[2].data1 == 0x3E && e[2].data2 == 0x65 && e[2].has_data2);
    assert!(e[3].tick == 480 + 16384 && e[3].status == 0xC0 && e[3].data1 == 0x2A && !e[3].has_data2 && e[3].data2 == 0);
    assert!(e[4].tick == 480 + 16384 + 127 && e[4].status == 0xD1 && e[4].data1 == 0x55 && !e[4].has_data2);
    assert!(e[5].tick == 480 + 16384 + 127 + 5 && e[5].status == 0xFF && e[5].data1 == 0x51 && e[5].tempo == 0x332211);
    assert!(e[6].tick == e[5].tick && e[6].status == 0xB0 && e[6].data1 == 0x63 && e[6].data2 == 0x1E);
    assert!(e[7].tick == e[5].tick && e[7].status == 0xB0 && e[7].data1 == 0x64 && e[7].data2 == 0x28);
    assert!(e[8].tick == e[5].tick && e[8].status == 0xFF && e[8].data1 == 0x2F);
    let long_vlq = SqSequence::parse(&sequence(&[0x90, 0x3C, 0x64, 0x81, 0x80, 0x80, 0x00, 0x80, 0x3C, 0x00, 0x00, 0xFF, 0x2F])).unwrap();
    assert!(long_vlq.events.len() == 3 && long_vlq.events[1].tick == 1 << 21);
    for events in [
        &[0x90u8, 0x3C, 0x64, 0x81, 0x80, 0x80, 0x80, 0x00, 0xFF, 0x2F][..],
        &[0xFF, 0x7F, 0xAA, 0xBB, 0xCC, 0xFF, 0x2F],
        &[0xFF, 0x51, 0x11, 0x22],
        &[0x90, 0x3C, 0x64, 0x00],
    ] {
        assert!(SqSequence::parse(&sequence(events)).is_err());
    }
}

#[test]
fn write_time_preempt() {
    let mut w = vec![SpuWrite::default(); 4];
    for (i, x) in w.iter_mut().enumerate() {
        x.cycle = 10 * (i as u32 + 1);
    }
    let frames = [500u64, 1015, 700000];
    add_vblank_preemption(&mut w, 1000, &frames);
    assert_eq!(w[0].cycle, 10);
    assert_eq!(w[1].cycle, 20 + VBLANK_HANDLER_CYCLES);
    assert!(w[2].cycle == 30 + VBLANK_HANDLER_CYCLES && w[3].cycle == 40 + VBLANK_HANDLER_CYCLES);
    let mut waited = vec![SpuWrite { cycle: 10, ..Default::default() }, SpuWrite { cycle: 5000, resume: 4000, ..Default::default() }];
    add_vblank_preemption(&mut waited, 1000, &[1500, 3000, 5200]);
    assert_eq!(waited[0].cycle, 10);
    assert_eq!(waited[1].cycle, 5000 + VBLANK_HANDLER_CYCLES);
    let mut before = vec![SpuWrite { cycle: 5, ..Default::default() }];
    add_vblank_preemption(&mut before, 1000, &frames);
    assert_eq!(before[0].cycle, 5);
    assert_eq!(sample_of_cycle(1000 + 768 * 5 + 767, 1000), 5);
}

#[test]
fn lab_wav_round_trip() {
    let dir = scratch("audio-lab");
    let mut tone = vec![0i16; 2 * 4800];
    for i in 0..4800 {
        let v = (10000.0 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48000.0).sin()) as i16;
        tone[2 * i] = v;
        tone[2 * i + 1] = v;
    }
    write_wav(&dir.join("tone.wav"), &tone, 48000).unwrap();
    assert_eq!(read_wav(&dir.join("tone.wav")).unwrap(), tone);
    assert_eq!(std::fs::metadata(dir.join("tone.wav")).unwrap().len(), 44 + tone.len() as u64 * 2);
    std::fs::write(dir.join("junk.wav"), [0u8; 50]).unwrap();
    assert!(read_wav(&dir.join("junk.wav")).is_err());
    assert!(read_wav(&dir.join("absent.wav")).is_err());
}

struct FakeBackend {
    available: bool,
    opened: Arc<Mutex<u32>>,
}

impl OutputBackend for FakeBackend {
    fn available(&self) -> bool {
        self.available
    }
    fn open(&mut self) -> bool {
        *self.opened.lock().unwrap() += 1;
        true
    }
    fn close(&mut self) {}
    fn put(&mut self, _: &[i16]) {}
}

#[test]
fn output_never_opens_muted() {
    let calls = Arc::new(Mutex::new(0u32));
    let opened = Arc::new(Mutex::new(0u32));
    {
        let c = calls.clone();
        let mut out = Output::new(
            Some(Box::new(move |_: &mut [i16], _| *c.lock().unwrap() += 1)),
            true,
            Some(Box::new(FakeBackend { available: true, opened: opened.clone() })),
        );
        assert!(out.available() && out.muted() && !out.open());
        out.set_muted(true);
        assert!(out.muted() && !out.open());
        out.feed(400);
    }
    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(*opened.lock().unwrap(), 0);
    let mut missing = Output::new(None, true, Some(Box::new(FakeBackend { available: false, opened: opened.clone() })));
    assert!(!missing.available() && missing.muted() && !missing.open());
    missing.set_muted(false);
    assert!(missing.muted());
    let c = calls.clone();
    let mut live = Output::new(
        Some(Box::new(move |chunk: &mut [i16], frames| *c.lock().unwrap() += (chunk.len() / 2 == frames) as u32)),
        false,
        Some(Box::new(FakeBackend { available: true, opened: opened.clone() })),
    );
    assert!(live.open() && !live.muted());
    live.feed(400);
    assert_eq!(*calls.lock().unwrap(), 1);
}

struct Delay {
    samples: u64,
    writes: u32,
    tick_writes: u32,
}

impl WriteTiming for Delay {
    fn sample(&mut self, context: &WriteContext) -> u64 {
        self.writes += 1;
        if context.tick {
            self.tick_writes += 1;
        }
        context.event_sample + self.samples
    }
}

fn render(sources: &ClockSoundSources, timing: Option<SharedWriteTiming>, frames: usize) -> Vec<i16> {
    let mut sound = ClockSound::new(sources.clone(), ClockSoundOptions::default()).unwrap();
    if timing.is_some() {
        sound.set_write_timing(timing);
    }
    let mut out = vec![0i16; 2 * frames];
    let mut at = 0;
    while at < frames {
        let n = 1000.min(frames - at);
        sound.render(&mut out[2 * at..], n).unwrap();
        at += n;
    }
    out
}

#[test]
fn clock_sound_deterministic_and_chunking_independent() {
    let Some(folder) = common::resources() else { return };
    let sources = clock_sound_sources(&folder).unwrap();
    const FRAMES: usize = 48000;
    let a = render(&sources, None, FRAMES);
    assert_eq!(a, render(&sources, None, FRAMES));
    let delay = Arc::new(Mutex::new(Delay { samples: 0, writes: 0, tick_writes: 0 }));
    assert_eq!(render(&sources, Some(delay.clone()), FRAMES), a);
    let (writes, ticks) = {
        let d = delay.lock().unwrap();
        (d.writes, d.tick_writes)
    };
    assert!(writes > 0 && ticks <= writes);
    let late = Arc::new(Mutex::new(Delay { samples: 9, writes: 0, tick_writes: 0 }));
    let shifted = render(&sources, Some(late.clone()), FRAMES);
    assert_eq!(late.lock().unwrap().writes, writes);
    let mut chunked = ClockSound::new(sources.clone(), ClockSoundOptions::default()).unwrap();
    let mut one = vec![0i16; 2 * FRAMES];
    chunked.render(&mut one, FRAMES).unwrap();
    assert_eq!(one, a);
    assert_eq!(chunked.position(), FRAMES as u64);
    eprintln!("a 9-sample delay {} the samples", if shifted != a { "changes" } else { "leaves" });
}

fn start_clock_thread(sound: &mut ClockSound) {
    sound.queue(&DriverCommand { sample: 0, id: 0x6150, words: [1, 0, 0, 0, 0], ..Default::default() });
    sound.queue(&DriverCommand { sample: 31 * 4004 / 5, id: 0x6140, words: [2, 0x06DC, 0x06D8, 0, 0], ..Default::default() });
}

fn render_started(sources: ClockSoundSources, frames: usize) -> Vec<i16> {
    let mut sound = ClockSound::new(sources, ClockSoundOptions::default()).unwrap();
    start_clock_thread(&mut sound);
    let mut out = vec![0i16; 2 * frames];
    let mut at = 0;
    while at < frames {
        let n = 1000.min(frames - at);
        sound.render(&mut out[2 * at..], n).unwrap();
        at += n;
    }
    out
}

#[test]
fn decoded_sources_equal_the_folder() {
    let Some(folder) = common::resources() else { return };
    const FRAMES: usize = 48000;
    let from_folder = render_started(clock_sound_sources(&folder).unwrap(), FRAMES);
    assert!(from_folder.iter().any(|&v| v != 0));
    let decoded = assets::resources::decode_folder(&folder).unwrap();
    assert!(decoded.find("PROGRAM").is_some() && decoded.find("SNDIMAGE").is_some());
    assert_eq!(render_started(clock_sound_sources_from_decoded(&decoded).unwrap(), FRAMES), from_folder);
    let mut bare = decoded.clone();
    bare.assets.retain(|a| a.name != "SNDIMAGE");
    let refused = clock_sound_sources_from_decoded(&bare).unwrap_err();
    assert_eq!(refused.0, "the decoded set lacks SNDIMAGE: give --resources once to decode it");
}

fn live(sources: &ClockSoundSources, log: &Path, menu_frame: u32, a2: u16) -> Vec<i16> {
    let options = LiveAudioOptions { device: false, command_log: log.to_owned(), ..Default::default() };
    let mut audio = LiveAudio::new(sources.clone(), &options, None).unwrap();
    let mut out = Vec::new();
    let cmd = |id, a1, a2, a3| SoundCommand { id, a1, a2, a3 };
    for frame in 0..60 {
        if frame == 0 {
            audio.queue(cmd(0x6150, 1, 0, 0), false, false);
        }
        if frame == 2 {
            audio.queue(cmd(0x6150, 6, 0, 0xF), false, false);
            audio.queue(cmd(0x6140, 2, 0, 0), true, true);
        }
        if frame == menu_frame {
            audio.queue(cmd(0x6300, 1, a2, 0), false, false);
        }
        audio.drain();
        audio.step();
        out.extend_from_slice(audio.last_frame());
    }
    audio.finish();
    out
}

fn replay(sources: &ClockSoundSources, log: &Path) -> Vec<i16> {
    let text = std::fs::read_to_string(log).unwrap();
    let mut words = text.split_whitespace();
    let mut header = || words.next().unwrap().to_owned();
    let (_, total, _, video, _, preroll) = (header(), header(), header(), header(), header(), header());
    let total: usize = total.parse().unwrap();
    let options = ClockSoundOptions {
        video: if video == "pal" { Video::Pal } else { Video::Ntsc },
        modelled_writes: true,
        pre_roll_samples: preroll.parse().unwrap(),
        ..Default::default()
    };
    let mut sound = ClockSound::new(sources.clone(), options).unwrap();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let mut c =
            DriverCommand { sample: f[0].parse().unwrap(), frame: f[1].parse().unwrap(), id: u32::from_str_radix(f[2], 16).unwrap(), ..Default::default() };
        for (w, text) in c.words.iter_mut().zip(&f[3..]) {
            *w = text.parse().unwrap();
        }
        sound.queue(&c);
    }
    let mut samples = vec![0i16; 2 * total];
    sound.render(&mut samples, total).unwrap();
    samples
}

#[test]
fn live_and_replay_agree() {
    let Some(folder) = common::resources() else { return };
    let sources = clock_sound_sources(&folder).unwrap();
    let log = scratch("audio-live-replay").join("live-replay.commands.txt");
    let bare = live(&sources, &log, u32::MAX, 0);
    let mut audible = false;
    for frame in [5u32, 7, 13, 27] {
        for a2 in 1..=12 {
            let out = live(&sources, &log, frame, a2);
            assert!(out == replay(&sources, &log), "live and replay differ (frame {frame}, a2 {a2})");
            audible |= out != bare;
        }
    }
    assert!(audible);
}

#[test]
fn snapshot_refuses_a_changed_program() {
    let Some(folder) = common::resources() else { return };
    let sound = SndImage::from_folder(&folder).unwrap();
    let mut program = std::fs::read(folder.join("hddosd.elf")).unwrap();
    assert_eq!(build_sound_images(&program, &sound).unwrap().libsd.len(), LIBSD_IMAGE_SIZE);
    let middle = program.len() / 2;
    program[middle] ^= 0xff;
    assert!(build_sound_images(&program, &sound).is_err());
    let names: Vec<&str> = STAGED_MEMBERS.iter().map(|m| m.name).collect();
    assert!(names.iter().filter(|n| n.ends_with('H')).count() == 2 && names.len() == 10);
}

fn idle_driver(folder: &Path) -> Driver {
    let images = build_sound_images_from_folder(folder).unwrap();
    let feed = EnvxFeed { tick: Box::new(|_, _| 0), allocation: Box::new(|_, _| 0x7fff) };
    let mut d = Driver::new(&images.snapshot, images.libsd, feed, true).unwrap();
    d.command(&DriverCommand { id: 0x6010, words: [4, 68, 4096, 0, 64], ..Default::default() }).unwrap();
    d.command(&DriverCommand { id: 0x6060, words: [965632, 20496, 0, 0, 64], ..Default::default() }).unwrap();
    d.command(&DriverCommand { id: 0x6060, words: [990208, 544784, 0, 0, 64], ..Default::default() }).unwrap();
    d.take_writes();
    d
}

fn command(id: u32, words: [u32; 5]) -> DriverCommand {
    DriverCommand { id, words, ..Default::default() }
}

#[test]
fn driver_refusals() {
    let Some(folder) = common::resources() else { return };
    let mut d = idle_driver(&folder);
    for (id, words) in [
        (0x6ff0, [0; 5]),
        (0x1234, [0; 5]),
        (0x60c0, [0, 7, 0, 0, 0]),
        (0x60c0, [1, 9, 0, 0, 0]),
        (0x8070, [0xa, 1, 0, 0, 0]),
        (0x6120, [2, 1, 1, 0, 0]),
        (0x60d0, [0xffff, 1, 1, 0, 0]),
    ] {
        assert!(d.command(&command(id, words)).is_err(), "{id:x} {words:?}");
    }
    assert_eq!(d.command(&command(0x6ff0, [0; 5])).unwrap_err().0, "command 0x6ff0 is not modelled");
    let mut d = idle_driver(&folder);
    d.command(&command(0x60c0, [0, 10, 0, 0, 0])).unwrap();
    assert!(d.take_writes().is_empty());
    d.command(&command(0x8070, [0xa, 0, 0, 0, 0])).unwrap();
    assert_eq!(d.take_writes().len(), 3);
    let mut clean = idle_driver(&folder);
    let mut after = idle_driver(&folder);
    after.command(&command(0x6300, [1, 0x7fff, 0, 0, 0])).unwrap();
    after.command(&command(0x6300, [1, 0xffff, 0, 0, 0])).unwrap();
    assert!(after.take_writes().is_empty());
    clean.command(&command(0x6300, [1, 0, 0, 0, 0])).unwrap();
    after.command(&command(0x6300, [1, 0, 0, 0, 0])).unwrap();
    let a = clean.take_writes();
    assert!(!a.is_empty());
    assert_eq!(first_difference(&a, &after.take_writes()), None);
}
