#![allow(dead_code)]

use std::path::PathBuf;

pub fn resources() -> Option<PathBuf> {
    let folder =
        std::env::var_os("CRYSTALOSD_RESOURCES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Resources"));
    if folder.join("SNDIMAGE").is_file() && folder.join("hddosd.elf").is_file() {
        Some(folder)
    } else {
        eprintln!("skipped: no SNDIMAGE and hddosd.elf in {}", folder.display());
        None
    }
}

pub fn fnv(data: &[u8]) -> u64 {
    assets::bytes::hash_of(data)
}

pub fn samples_hash(samples: &[i16]) -> u64 {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    fnv(&bytes)
}

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }

    pub fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

pub struct Hash(pub u64);

impl Default for Hash {
    fn default() -> Self {
        Hash(assets::bytes::FNV_OFFSET)
    }
}

impl Hash {
    pub fn byte(&mut self, b: u8) {
        self.0 = (self.0 ^ u64::from(b)).wrapping_mul(assets::bytes::FNV_PRIME);
    }
    pub fn flag(&mut self, b: bool) {
        self.byte(u8::from(b));
    }
    pub fn u16(&mut self, v: u16) {
        v.to_le_bytes().iter().for_each(|&b| self.byte(b));
    }
    pub fn u32(&mut self, v: u32) {
        v.to_le_bytes().iter().for_each(|&b| self.byte(b));
    }
    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }
    pub fn i16(&mut self, v: i16) {
        self.u16(v as u16);
    }
    pub fn u64(&mut self, v: u64) {
        v.to_le_bytes().iter().for_each(|&b| self.byte(b));
    }
    pub fn bytes(&mut self, data: &[u8]) {
        data.iter().for_each(|&b| self.byte(b));
    }
}

use audio::spu2::Spu2;
use audio::spu2::core::Core;
use audio::spu2::voice::Voice;

pub fn hash_voice(h: &mut Hash, v: &Voice) {
    for s in [&v.left, &v.right] {
        h.u16(s.reg);
        h.i32(s.level);
        h.u32(s.counter);
    }
    h.byte(v.envelope.phase as u8);
    h.i32(v.envelope.level);
    h.u32(v.envelope.counter);
    h.u16(v.envelope.reg1);
    h.u16(v.envelope.reg2);
    h.u16(v.pitch);
    h.u32(v.start_address);
    h.u32(v.loop_address);
    h.u32(v.next_address);
    h.i32(v.prev1);
    h.i32(v.prev2);
    h.flag(v.modulated);
    h.flag(v.noise);
    h.byte(v.loop_mode as u8);
    h.byte(v.loop_flags as u8);
    h.i32(v.phase);
    h.i32(v.out_x);
    v.fifo.iter().for_each(|&f| h.i32(f));
    h.u32(v.fifo_write);
    h.u32(v.fifo_read);
    v.block.iter().for_each(|&b| h.i16(b));
    h.flag(v.block_pending);
    h.i32(v.dry_l);
    h.i32(v.dry_r);
    h.i32(v.wet_l);
    h.i32(v.wet_r);
}

pub fn hash_core(h: &mut Hash, k: &Core) {
    h.u32(k.index);
    k.voices.iter().for_each(|v| hash_voice(h, v));
    for g in [&k.dry_gate, &k.wet_gate] {
        for x in [g.inp_l, g.inp_r, g.snd_l, g.snd_r, g.ext_l, g.ext_r] {
            h.i32(x);
        }
    }
    for s in [&k.master_left, &k.master_right] {
        h.u16(s.reg);
        h.i32(s.level);
        h.u32(s.counter);
    }
    for s in [&k.ext_vol, &k.inp_vol, &k.fx_vol] {
        h.i32(s.left);
        h.i32(s.right);
    }
    h.u32(k.irq_address);
    h.u32(k.transfer_address);
    h.u32(k.active_transfer_address);
    h.flag(k.irq_enable);
    h.flag(k.fx_enable);
    h.flag(k.mute);
    h.flag(k.auto_dma_active);
    h.byte(k.dma_bits as u8);
    h.byte(k.noise_clock);
    h.u32(k.noise_counter);
    h.u32(k.noise_out);
    h.u16(k.auto_dma_control);
    h.byte(k.dma_mode);
    h.byte(k.attr_bit0);
    for x in [k.pmon, k.non, k.vmixl, k.vmixr, k.vmixel, k.vmixer, k.endx] {
        h.u32(x);
    }
    h.u16(k.mmix);
    h.u16(k.statx);
    h.u16(k.attr);
    h.u32(k.key_on);
    h.u32(k.key_off);
    h.u32(k.effects_start);
    h.u32(k.effects_end);
    h.u32(k.reverb_position);
    let r = &k.reverb;
    for x in [r.in_coef_l, r.in_coef_r, r.apf1_vol, r.apf2_vol, r.iir_vol, r.wall_vol, r.comb1_vol, r.comb2_vol, r.comb3_vol, r.comb4_vol] {
        h.i16(x);
    }
    for x in [
        r.apf1_size,
        r.apf2_size,
        r.same_l_src,
        r.same_r_src,
        r.diff_l_src,
        r.diff_r_src,
        r.same_l_dst,
        r.same_r_dst,
        r.diff_l_dst,
        r.diff_r_dst,
        r.comb1_l_src,
        r.comb1_r_src,
        r.comb2_l_src,
        r.comb2_r_src,
        r.comb3_l_src,
        r.comb3_r_src,
        r.comb4_l_src,
        r.comb4_r_src,
        r.apf1_l_dst,
        r.apf1_r_dst,
        r.apf2_l_dst,
        r.apf2_r_dst,
    ] {
        h.u32(x);
    }
    for ring in k.down.iter().chain(k.up.iter()) {
        ring.iter().for_each(|&x| h.i16(x));
    }
}

pub fn hash_spu(h: &mut Hash, spu: &Spu2) {
    h.bytes(spu.ram());
    for c in 0..2 {
        hash_core(h, spu.core(c));
    }
    for a in 0x1F90_0000u32..0x1F91_0000 {
        h.u16(spu.read(a).unwrap());
    }
    h.u32(spu.ticks());
    h.i16(spu.final_left());
    h.i16(spu.final_right());
}
