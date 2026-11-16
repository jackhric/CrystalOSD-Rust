mod common;

use audio::data::hd_bank::HdBank;
use audio::data::snd_image::{SOUND_MEMBERS, SndImage};
use audio::data::sq_sequence::SqSequence;
use audio::driver::build_sound_images_from_folder;
use audio::driver::iop_clock::{add_vblank_preemption, sample_of_cycle};
use audio::ee_sound_queue::{EeSoundQueue, SoundCommand};
use audio::spu_write::SpuWrite;
use audio::spu2::adpcm::{AdpcmState, decode_block};
use audio::spu2::core::Core;
use audio::spu2::envelope::{Envelope, Phase, VolumeSweep};
use audio::spu2::gauss::{gauss_tap, interpolate};
use audio::spu2::reverb::{ReverbPath, Stereo, reverb_filter, run_reverb};
use audio::spu2::snapshot::RAM_BYTES;
use audio::spu2::{Spu2, decode, is_status};
use common::{Hash, Rng, hash_core, hash_spu};

// FNV-1a 64 digests of the C++ (CrystalOSD 039cc58) driven with the same seeded inputs and HDD OSD 1.10U's SNDIMAGE and
// hddosd.elf; the probe is the Rust code below, line for line, in C++.
const ADPCM: u64 = 0x9cf74715b1fe34c7;
const GAUSS: u64 = 0x4d89dd454fbe51b5;
const ENVELOPE: u64 = 0x1a73b001975b4db8;
const REVERB: u64 = 0xa2c59cd31509efeb;
const SPU2_REGISTERS: u64 = 0xcaa69dcebf0de3b6;
const SND_IMAGE: u64 = 0xd1c8db2163438d89;
const HD_BANKS: u64 = 0x997cc2e7dcb433b5;
const SQ_SEQUENCES: u64 = 0xbf1933ea9830f4c8;
const SOUND_IMAGES: u64 = 0x848fdde87720a486;
const SOUND_QUEUE: u64 = 0x5925b7671394cde2;
const PREEMPTION: u64 = 0x14542688c9d114ad;

fn image() -> Option<(std::path::PathBuf, SndImage)> {
    let folder = common::resources()?;
    let image = SndImage::from_folder(&folder).unwrap();
    Some((folder, image))
}

#[test]
fn adpcm_matches_cpp() {
    let Some((_, image)) = image() else { return };
    let ram = image.spu_ram().unwrap();
    let mut h = Hash::default();
    let mut state = AdpcmState::default();
    for block in ram.as_chunks::<16>().0 {
        let b = decode_block(block, &mut state);
        b.pcm.iter().for_each(|&s| h.i16(s));
        h.flag(b.loop_start);
        h.flag(b.loop_end);
        h.flag(b.loop_repeat);
        h.i32(state.prev1);
        h.i32(state.prev2);
    }
    let mut rng = Rng(1);
    for _ in 0..20000 {
        let mut block = [0u8; 16];
        block.iter_mut().for_each(|x| *x = rng.next() as u8);
        let prev1 = rng.below(65536) as i32 - 32768;
        let prev2 = rng.below(65536) as i32 - 32768;
        let mut s = AdpcmState { prev1, prev2 };
        let b = decode_block(&block, &mut s);
        b.pcm.iter().for_each(|&x| h.i16(x));
        h.flag(b.loop_start);
        h.flag(b.loop_end);
        h.flag(b.loop_repeat);
        h.i32(s.prev1);
        h.i32(s.prev2);
    }
    assert_eq!(h.0, ADPCM);
}

#[test]
fn gauss_matches_cpp() {
    let mut h = Hash::default();
    (0..1024).for_each(|i| h.i16(gauss_tap(i)));
    let mut rng = Rng(2);
    for _ in 0..300 {
        let mut s = [0i16; 4];
        s.iter_mut().for_each(|x| *x = rng.next() as i16);
        for c in 0..0x1000u32 {
            h.i32(interpolate(&s, c | (rng.next() & 0xFFFF_0000)));
        }
    }
    assert_eq!(h.0, GAUSS);
}

#[test]
fn envelope_matches_cpp() {
    let mut h = Hash::default();
    let mut rng = Rng(3);
    for n in 0..1500 {
        let mut e = Envelope::default();
        let r1 = rng.next() as u16;
        let r2 = rng.next() as u16;
        if n % 3 == 0 {
            e.key_on();
        } else {
            e.phase = Phase::from_u32(rng.below(5)).unwrap();
            e.level = rng.below(0x8000) as i32;
            e.counter = rng.below(0x8000);
        }
        for k in 0..2500 {
            if k == 1200 {
                e.key_off();
            }
            let on = e.step_with(r1, r2);
            h.flag(on);
            h.byte(e.phase as u8);
            h.i32(e.level);
            h.u32(e.counter);
        }
    }
    for _ in 0..1500 {
        let mut s = VolumeSweep::default();
        s.write(rng.next() as u16);
        if rng.below(2) != 0 {
            s.level = rng.below(0x10000) as i32 - 0x8000;
        }
        for k in 0..2000 {
            if k == 1000 && rng.below(2) != 0 {
                s.write(rng.next() as u16);
            }
            s.step();
            h.u16(s.reg);
            h.i32(s.level);
            h.u32(s.counter);
        }
    }
    assert_eq!(h.0, ENVELOPE);
}

#[test]
fn reverb_matches_cpp() {
    let mut h = Hash::default();
    let mut rng = Rng(4);
    for round in 0..8 {
        let mut ram = vec![0u8; RAM_BYTES as usize];
        ram.iter_mut().for_each(|b| *b = rng.next() as u8);
        let mut core = Core { fx_enable: rng.below(4) != 0, ..Core::default() };
        let start = rng.below(0xF0000);
        core.effects_start = start;
        core.effects_end = if round == 7 { start } else { start + 0x100 + rng.below(0xFFFFF - start - 0x100) };
        let r = &mut core.reverb;
        for v in [
            &mut r.in_coef_l,
            &mut r.in_coef_r,
            &mut r.apf1_vol,
            &mut r.apf2_vol,
            &mut r.iir_vol,
            &mut r.wall_vol,
            &mut r.comb1_vol,
            &mut r.comb2_vol,
            &mut r.comb3_vol,
            &mut r.comb4_vol,
        ] {
            *v = rng.next() as i16;
        }
        if round == 3 {
            r.iir_vol = -0x8000;
        }
        for a in [
            &mut r.apf1_size,
            &mut r.apf2_size,
            &mut r.same_l_src,
            &mut r.same_r_src,
            &mut r.diff_l_src,
            &mut r.diff_r_src,
            &mut r.same_l_dst,
            &mut r.same_r_dst,
            &mut r.diff_l_dst,
            &mut r.diff_r_dst,
            &mut r.comb1_l_src,
            &mut r.comb1_r_src,
            &mut r.comb2_l_src,
            &mut r.comb2_r_src,
            &mut r.comb3_l_src,
            &mut r.comb3_r_src,
            &mut r.comb4_l_src,
            &mut r.comb4_r_src,
            &mut r.apf1_l_dst,
            &mut r.apf1_r_dst,
            &mut r.apf2_l_dst,
            &mut r.apf2_r_dst,
        ] {
            *a = if round == 5 { rng.next() } else { rng.below(0x4000) };
        }
        for cycles in 0..6000u32 {
            let left = (rng.next() % 140000) as i32 - 70000;
            let right = (rng.next() % 140000) as i32 - 70000;
            let at = cycles + rng.below(3);
            let o = run_reverb(&mut core, &mut ram, at, Stereo { left, right }, ReverbPath::Avx);
            h.i32(o.left);
            h.i32(o.right);
            h.u32(core.reverb_position);
        }
        h.bytes(&ram);
        hash_core(&mut h, &core);
    }
    for _ in 0..2000 {
        let mut hist = [0i16; 128];
        hist.iter_mut().for_each(|x| *x = rng.next() as i16);
        let pos = rng.below(64);
        h.i16(reverb_filter(&hist, pos, false, ReverbPath::Avx));
        h.i16(reverb_filter(&hist, pos, true, ReverbPath::Sse));
    }
    assert_eq!(h.0, REVERB);
}

#[test]
fn spu2_registers_and_mixer_match_cpp() {
    let Some((_, image)) = image() else { return };
    let ram = image.spu_ram().unwrap();
    let mut h = Hash::default();
    let mut rng = Rng(5);
    for a in 0x1F90_0000u32..0x1F91_0000 {
        let w = decode(a).unwrap();
        h.byte(w.core);
        h.byte(w.kind as u8);
        h.byte(w.voice);
        h.u16(w.reg);
        h.flag(is_status(a).unwrap());
    }
    let refused = [0x1F8F_FFFEu32, 0x1F91_0000, 0, 0xFFFF_FFFF].iter().filter(|&&a| decode(a).is_err()).count();
    h.u32(refused as u32);
    let mut spu = Spu2::new();
    spu.load_ram(&ram).unwrap();
    for n in 0..300000 {
        let pick = rng.below(10);
        let a = if pick < 7 {
            0x1F90_0000 + rng.below(0x800)
        } else if pick < 9 {
            0x1F90_0000 + (rng.below(0x800) & !1)
        } else {
            0x1F90_0000 + rng.below(0x10000)
        };
        let mut v = rng.next() as u16;
        if rng.below(8) == 0 {
            v &= 0x00FF;
        }
        spu.write(a, v).unwrap();
        h.u16(spu.read(a).unwrap());
        if n % 50 == 0 {
            spu.mix().field.iter().for_each(|&f| h.i32(f));
        }
        if n % 20011 == 0 {
            let words = [rng.next() as u16, rng.next() as u16, rng.next() as u16];
            let to = rng.next();
            spu.upload(to, &words);
            let from = rng.below(0x200100);
            let count = rng.below(300);
            spu.clear_ram(from, u64::from(count));
        }
    }
    spu.set_reverb_path(ReverbPath::Sse);
    let mut pcm = vec![0i16; 2 * 4000];
    spu.render(&mut pcm, 4000);
    pcm.iter().for_each(|&s| h.i16(s));
    spu.advance(1000);
    hash_spu(&mut h, &spu);
    assert_eq!(h.0, SPU2_REGISTERS);
}

#[test]
fn sound_data_matches_cpp() {
    let Some((folder, image)) = image() else { return };
    let mut h = Hash::default();
    for name in SOUND_MEMBERS {
        let m = image.member(name).unwrap();
        h.u64(m.len() as u64);
        h.bytes(m);
    }
    h.bytes(&image.spu_ram().unwrap());
    assert_eq!(h.0, SND_IMAGE);

    let mut b = Hash::default();
    for name in ["SNDBOOTH", "SNDOSDDH"] {
        let bank = HdBank::parse(image.member(name).unwrap()).unwrap();
        b.u64(bank.programs.len() as u64);
        for p in &bank.programs {
            b.byte(p.id);
            b.byte(p.head);
            b.byte(p.volume);
            b.byte(p.pan);
            b.u32(p.offset);
            b.u64(p.tones.len() as u64);
            for t in &p.tones {
                b.byte(t.key_low);
                b.byte(t.key_high);
                b.byte(t.root_key);
                b.byte(t.fine_tune as u8);
                b.u16(t.sample_start);
                b.u16(t.adsr1);
                b.u16(t.adsr2);
                b.byte(t.pan);
                b.byte(t.bend_range);
                b.byte(t.flags);
            }
        }
        b.bytes(&bank.velocity);
        for e in &bank.effects {
            for x in [e.left, e.right, e.pitch, e.flags_a, e.adsr1, e.adsr2, e.word12, e.word14] {
                b.u16(x);
            }
            b.u32(e.bd_offset);
        }
        b.flag(bank.effect_table.is_some());
        b.u32(bank.effect_table.unwrap_or(0));
        b.byte(bank.effect_master);
    }
    assert_eq!(b.0, HD_BANKS);

    let mut q = Hash::default();
    for name in ["SNDBOOTS", "SNDTNNLS", "SNDCLOKS", "SNDTM30S", "SNDTM60S", "SNDLOGOS", "SNDWARNS", "SNDRCLKS"] {
        let s = SqSequence::parse(image.member(name).unwrap()).unwrap();
        q.u16(s.volume);
        q.u16(s.resolution);
        q.u32(s.tempo);
        q.u64(s.channels.len() as u64);
        q.u64(s.events.len() as u64);
        for c in &s.channels {
            q.byte(c.channel);
            q.byte(c.program);
            q.byte(c.volume);
            q.byte(c.pan);
        }
        for e in &s.events {
            q.u32(e.tick);
            q.byte(e.status);
            q.byte(e.data1);
            q.byte(e.data2);
            q.flag(e.has_data2);
            q.u32(e.tempo);
        }
    }
    assert_eq!(q.0, SQ_SEQUENCES);
    let clock = SqSequence::parse(image.member("SNDCLOKS").unwrap()).unwrap();
    assert_eq!((clock.volume, clock.resolution, clock.tempo, clock.channels.len(), clock.events.len()), (100, 480, 120, 7, 278));

    let images = build_sound_images_from_folder(&folder).unwrap();
    let mut s = Hash::default();
    for part in [&images.snapshot.osdsnd, &images.snapshot.iop, &images.libsd] {
        s.u64(part.len() as u64);
        s.bytes(part);
    }
    s.bytes(&images.spu_ram);
    assert_eq!(s.0, SOUND_IMAGES);
}

#[test]
fn sound_queue_matches_cpp() {
    let mut h = Hash::default();
    let mut rng = Rng(6);
    for _ in 0..40 {
        let counter = rng.below(20) as i32 - 2;
        let mut q = EeSoundQueue::new(counter, rng.below(2) != 0);
        for _ in 0..400 {
            if rng.below(3) == 0 {
                let sends = q.drain();
                h.u64(sends.len() as u64);
                for c in sends {
                    h.u16(c.id);
                    h.u16(c.a1);
                    h.u16(c.a2);
                    h.u16(c.a3);
                }
            } else {
                let ids = [0x6300u16, 0x6300, 0x6140, 0x6150, 0x60D0, 0, 1];
                let id = ids[rng.below(7) as usize];
                let a1 = rng.below(3) as u16;
                let a2 = rng.below(4) as u16;
                let a3 = rng.next() as u16;
                let c = SoundCommand { id, a1, a2, a3 };
                let range = if rng.below(10) == 0 { 140 } else { 3 };
                let burst = 1 + rng.below(range);
                for _ in 0..burst {
                    h.flag(q.enqueue(c));
                }
            }
            h.u16(q.carried_a2());
            h.u16(q.carried_a3());
        }
    }
    assert_eq!(h.0, SOUND_QUEUE);
}

#[test]
fn preemption_matches_cpp() {
    let mut h = Hash::default();
    let mut rng = Rng(7);
    for _ in 0..3000 {
        let mut w = vec![SpuWrite::default(); rng.below(12) as usize];
        let mut cycle = 0u32;
        let mut resume = 0u32;
        for x in w.iter_mut() {
            cycle += rng.below(30000);
            if rng.below(4) == 0 {
                resume = cycle + rng.below(5000);
            }
            x.cycle = cycle;
            x.resume = resume;
        }
        let entry = u64::from(rng.next()) % 100000;
        let mut vb = Vec::new();
        let mut t = if entry > 20000 { entry - u64::from(rng.below(20000)) } else { 0 };
        for _ in 0..6 {
            t += u64::from(rng.below(40000));
            vb.push(t);
        }
        add_vblank_preemption(&mut w, entry, &vb);
        w.iter().for_each(|x| h.u32(x.cycle));
        h.u64(sample_of_cycle(entry + u64::from(rng.below(100000)), entry));
    }
    assert_eq!(h.0, PREEMPTION);
}
