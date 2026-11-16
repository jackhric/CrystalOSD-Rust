use assets::bytes::{le16, le32};

use super::core::{Core, CoreGates, ReverbRegs, StereoVolume};
use super::envelope::{Phase, VolumeSweep};
use super::snapshot::{Json, RAM_BYTES, REG_BYTES, Snapshot};
use super::spu2::Spu2;
use super::voice::Voice;
use crate::{Result, fail};

fn hex(text: &str) -> Result<u64> {
    let t = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']).as_bytes();
    let mut at = 0;
    let negative = match t.first() {
        Some(b'-') => {
            at += 1;
            true
        }
        Some(b'+') => {
            at += 1;
            false
        }
        _ => false,
    };
    if t.len() > at + 2 && t[at] == b'0' && (t[at + 1] == b'x' || t[at + 1] == b'X') && t[at + 2].is_ascii_hexdigit() {
        at += 2;
    }
    let digits = t[at..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
    if digits == 0 {
        return fail("spustate: stoul: no conversion");
    }
    let mut value = 0u64;
    for &c in &t[at..at + digits] {
        let d = u64::from((c as char).to_digit(16).unwrap_or(0));
        value = match value.checked_mul(16).and_then(|v| v.checked_add(d)) {
            Some(v) => v,
            None => return fail("spustate: stoul: out of range"),
        };
    }
    Ok(if negative { value.wrapping_neg() } else { value })
}

fn number(j: &Json) -> Result<u32> {
    match j {
        Json::String(s) => Ok(hex(s)? as u32),
        Json::Bool(b) => Ok(u32::from(*b)),
        Json::Integer(v) => Ok(*v as u32),
        Json::Unsigned(v) => Ok(*v as u32),
        _ => fail("spustate: not a number"),
    }
}

fn signed_number(j: &Json) -> Result<i32> {
    Ok(number(j)? as i32)
}

fn sweep(s: &mut VolumeSweep, j: &Json) -> Result<()> {
    s.reg = number(j.at("reg")?)? as u16;
    s.level = signed_number(j.at("value")?)?;
    s.counter = number(j.at("counter")?)?;
    Ok(())
}

fn gates(g: &mut CoreGates, j: &Json) -> Result<()> {
    g.inp_l = signed_number(j.at("inpL")?)?;
    g.inp_r = signed_number(j.at("inpR")?)?;
    g.snd_l = signed_number(j.at("sndL")?)?;
    g.snd_r = signed_number(j.at("sndR")?)?;
    g.ext_l = signed_number(j.at("extL")?)?;
    g.ext_r = signed_number(j.at("extR")?)?;
    Ok(())
}

fn restore_voice(v: &mut Voice, j: &Json) -> Result<()> {
    sweep(&mut v.left, j.at("volume")?.at("left")?)?;
    sweep(&mut v.right, j.at("volume")?.at("right")?)?;
    let a = j.at("adsr")?;
    v.envelope.reg1 = number(a.at("reg1")?)? as u16;
    v.envelope.reg2 = number(a.at("reg2")?)? as u16;
    v.envelope.level = signed_number(a.at("value")?)?;
    let phase = number(a.at("phase")?)?;
    let Some(phase) = Phase::from_u32(phase & 0xFF) else {
        return fail(format!("spustate: phase {phase} is not an envelope phase"));
    };
    v.envelope.phase = phase;
    v.envelope.counter = number(a.at("counter")?)?;
    v.pitch = number(j.at("pitch")?)? as u16;
    v.start_address = number(j.at("startA")?)?;
    v.loop_address = number(j.at("loopStartA")?)?;
    v.next_address = number(j.at("nextA")?)?;
    v.prev1 = signed_number(j.at("prev1")?)?;
    v.prev2 = signed_number(j.at("prev2")?)?;
    v.modulated = number(j.at("modulated")?)? != 0;
    v.noise = number(j.at("noise")?)? != 0;
    v.loop_mode = signed_number(j.at("loopMode")?)? as i8;
    v.loop_flags = signed_number(j.at("loopFlags")?)? as i8;
    v.phase = signed_number(j.at("sp")?)?;
    v.out_x = signed_number(j.at("outX")?)?;
    v.fifo_read = number(j.at("decPosRead")?)?;
    v.fifo_write = number(j.at("decPosWrite")?)?;
    let g = j.at("gates")?;
    v.dry_l = signed_number(g.at("dryL")?)?;
    v.dry_r = signed_number(g.at("dryR")?)?;
    v.wet_l = signed_number(g.at("wetL")?)?;
    v.wet_r = signed_number(g.at("wetR")?)?;
    v.fifo = [0; 32];
    v.block = [0; 28];
    Ok(())
}

fn restore_reverb(r: &mut ReverbRegs, j: &Json) -> Result<()> {
    r.in_coef_l = signed_number(j.at("inCoefL")?)? as i16;
    r.in_coef_r = signed_number(j.at("inCoefR")?)? as i16;
    r.apf1_size = number(j.at("apf1Size")?)?;
    r.apf2_size = number(j.at("apf2Size")?)?;
    r.apf1_vol = signed_number(j.at("apf1Vol")?)? as i16;
    r.apf2_vol = signed_number(j.at("apf2Vol")?)? as i16;
    r.iir_vol = signed_number(j.at("iirVol")?)? as i16;
    r.wall_vol = signed_number(j.at("wallVol")?)? as i16;
    r.comb1_vol = signed_number(j.at("comb1Vol")?)? as i16;
    r.comb2_vol = signed_number(j.at("comb2Vol")?)? as i16;
    r.comb3_vol = signed_number(j.at("comb3Vol")?)? as i16;
    r.comb4_vol = signed_number(j.at("comb4Vol")?)? as i16;
    r.same_l_src = number(j.at("sameLSrc")?)?;
    r.same_r_src = number(j.at("sameRSrc")?)?;
    r.diff_l_src = number(j.at("diffLSrc")?)?;
    r.diff_r_src = number(j.at("diffRSrc")?)?;
    r.same_l_dst = number(j.at("sameLDst")?)?;
    r.same_r_dst = number(j.at("sameRDst")?)?;
    r.diff_l_dst = number(j.at("diffLDst")?)?;
    r.diff_r_dst = number(j.at("diffRDst")?)?;
    r.comb1_l_src = number(j.at("comb1LSrc")?)?;
    r.comb1_r_src = number(j.at("comb1RSrc")?)?;
    r.comb2_l_src = number(j.at("comb2LSrc")?)?;
    r.comb2_r_src = number(j.at("comb2RSrc")?)?;
    r.comb3_l_src = number(j.at("comb3LSrc")?)?;
    r.comb3_r_src = number(j.at("comb3RSrc")?)?;
    r.comb4_l_src = number(j.at("comb4LSrc")?)?;
    r.comb4_r_src = number(j.at("comb4RSrc")?)?;
    r.apf1_l_dst = number(j.at("apf1LDst")?)?;
    r.apf1_r_dst = number(j.at("apf1RDst")?)?;
    r.apf2_l_dst = number(j.at("apf2LDst")?)?;
    r.apf2_r_dst = number(j.at("apf2RDst")?)?;
    Ok(())
}

fn stereo(j: &Json) -> Result<StereoVolume> {
    Ok(StereoVolume { left: signed_number(j.index(0)?)?, right: signed_number(j.index(1)?)? })
}

fn restore_core(k: &mut Core, j: &Json) -> Result<()> {
    k.transfer_address = number(j.at("tsa")?)?;
    k.active_transfer_address = number(j.at("activeTsa")?)?;
    k.irq_address = number(j.at("irqa")?)?;
    k.irq_enable = number(j.at("irqEnable")?)? != 0;
    k.fx_enable = number(j.at("fxEnable")?)? != 0;
    k.mute = number(j.at("mute")?)? != 0;
    k.auto_dma_control = number(j.at("autoDmaCtrl")?)? as u16;
    k.auto_dma_active = number(j.at("admaInProgress")?)? != 0;
    k.dma_bits = signed_number(j.at("dmaBits")?)? as i8;
    k.noise_clock = number(j.at("noiseClk")?)? as u8;
    k.noise_counter = number(j.at("noiseCnt")?)?;
    k.noise_out = number(j.at("noiseOut")?)?;
    k.key_on = number(j.at("keyOn")?)?;
    k.key_off = number(j.at("keyOff")?)?;
    let r = j.at("regs")?;
    k.pmon = number(r.at("pmon")?)?;
    k.non = number(r.at("non")?)?;
    k.vmixl = number(r.at("vmixl")?)?;
    k.vmixr = number(r.at("vmixr")?)?;
    k.vmixel = number(r.at("vmixel")?)?;
    k.vmixer = number(r.at("vmixer")?)?;
    k.endx = number(r.at("endx")?)?;
    k.mmix = number(r.at("mmix")?)? as u16;
    k.statx = number(r.at("statx")?)? as u16;
    k.attr = number(r.at("attr")?)? as u16;
    k.attr_bit0 = (k.attr & 1) as u8;
    k.dma_mode = ((k.attr >> 4) & 3) as u8;
    sweep(&mut k.master_left, j.at("masterVol")?.at("left")?)?;
    sweep(&mut k.master_right, j.at("masterVol")?.at("right")?)?;
    k.ext_vol = stereo(j.at("extVol")?)?;
    k.inp_vol = stereo(j.at("inpVol")?)?;
    k.fx_vol = stereo(j.at("fxVol")?)?;
    gates(&mut k.dry_gate, j.at("dryGate")?)?;
    gates(&mut k.wet_gate, j.at("wetGate")?)?;
    k.effects_start = number(j.at("effectsStartA")?)?;
    k.effects_end = number(j.at("effectsEndA")?)?;
    k.reverb_position = number(j.at("revbSampleBufPos")?)?;
    restore_reverb(&mut k.reverb, j.at("revb")?)?;
    let voices = j.at("voices")?;
    if voices.size() != 24 {
        return fail("spustate: not 24 voices");
    }
    for v in 0..24 {
        restore_voice(&mut k.voices[v], voices.index(v)?)?;
    }
    k.down = [[0; 128]; 2];
    k.up = [[0; 128]; 2];
    Ok(())
}

impl Spu2 {
    pub fn restore(&mut self, snapshot: &Snapshot) -> Result<()> {
        if snapshot.ram.len() != RAM_BYTES as usize || snapshot.regs.len() != REG_BYTES as usize {
            return fail("snapshot: wrong sizes");
        }
        self.ram.copy_from_slice(&snapshot.ram);
        self.raw.copy_from_slice(&snapshot.regs);
        let s = &snapshot.state;
        self.ticks = number(s.at("ticks")?)?;
        self.out_pos = number(s.at("outPos")?)? as u16;
        self.play_mode = signed_number(s.at("playMode")?)?;
        let sp = s.at("spdif")?;
        self.spdif = [0; 8];
        self.spdif[0] = number(sp.at("out")?)? as u16;
        self.spdif[1] = number(sp.at("info")?)? as u16;
        self.spdif[3] = number(sp.at("mode")?)? as u16;
        self.spdif[4] = number(sp.at("media")?)? as u16;
        self.spdif[6] = number(sp.at("protection")?)? as u16;
        if s.at("cores")?.size() != 2 {
            return fail("spustate: not 2 cores");
        }
        for c in 0..2 {
            self.core[c] = Core { index: c as u32, ..Core::default() };
            restore_core(&mut self.core[c], s.at("cores")?.index(c)?)?;
        }
        Ok(())
    }

    pub fn restore_history(&mut self, history: &[u8]) -> Result<()> {
        if history.len() != 2 * 4096 {
            return fail("history: not 8192 bytes");
        }
        let mut at = 0;
        for core in self.core.iter_mut() {
            for v in core.voices.iter_mut() {
                for sample in v.fifo.iter_mut() {
                    *sample = le32(history, at)? as i32;
                    at += 4;
                }
            }
            for buffers in [&mut core.down, &mut core.up] {
                for channel in buffers.iter_mut() {
                    for sample in channel.iter_mut() {
                        *sample = le16(history, at)? as i16;
                        at += 2;
                    }
                }
            }
        }
        Ok(())
    }
}
