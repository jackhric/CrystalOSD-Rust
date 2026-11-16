use super::core::Core;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stereo {
    pub left: i32,
    pub right: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReverbPath {
    #[default]
    Avx,
    Sse,
}

const TAPS: u32 = 39;
const CENTRE: i32 = 19;
const RING: u32 = 64;
const HALF_BAND: [i32; 10] = [-1, 2, -10, 35, -103, 266, -616, 1332, -2960, 10246];

const fn tap_gain(index: i32) -> i32 {
    let distance = if index < CENTRE { CENTRE - index } else { index - CENTRE };
    if distance == 0 {
        return 16384;
    }
    if distance & 1 == 0 {
        return 0;
    }
    HALF_BAND[((19 - distance) / 2) as usize]
}

fn saturate(x: i32) -> i16 {
    x.clamp(-32768, 32767) as i16
}

fn weigh(volume: i32, value: i32) -> i32 {
    volume.wrapping_mul(value) >> 15
}

fn resample(history: &[i16; 128], position: u32, scale: i32) -> i16 {
    let first = if position < TAPS { position + (RING - TAPS) } else { position - TAPS };
    let mut sum = 0i32;
    for tap in 0..TAPS {
        let gain = tap_gain(tap as i32) * scale;
        if gain != 0 {
            sum += (gain * i32::from(history[(first + tap) as usize]) + 0x4000) >> 15;
        }
    }
    saturate(sum)
}

#[derive(Clone, Copy)]
struct Window {
    start: u32,
    size: u32,
    advance: u32,
}

fn locate(w: Window, offset: i64) -> u32 {
    let span = i64::from(w.size);
    w.start.wrapping_add((((offset + i64::from(w.advance)) % span + span) % span) as u32)
}

fn fetch(ram: &[u8], w: Window, offset: i64) -> i16 {
    let at = (locate(w, offset) * 2) as usize;
    (u16::from(ram[at]) | u16::from(ram[at + 1]) << 8) as i16
}

fn store(ram: &mut [u8], w: Window, offset: i64, value: i32) {
    let at = (locate(w, offset) * 2) as usize;
    let v = saturate(value);
    ram[at] = v as u8;
    ram[at + 1] = ((v as u16) >> 8) as u8;
}

pub fn reverb_filter(history: &[i16; 128], position: u32, up: bool, _path: ReverbPath) -> i16 {
    resample(history, position, if up { 2 } else { 1 })
}

// psx-spx "SPU Reverb Formula": left is computed on even cycles and right on odd ones, the work area advances one word per pair.
pub fn run_reverb(core: &mut Core, ram: &mut [u8], cycles: u32, input: Stereo, path: ReverbPath) -> Stereo {
    if core.effects_end <= core.effects_start {
        return Stereo::default();
    }
    let r = core.reverb;
    let window = Window { start: core.effects_start, size: core.effects_end.wrapping_sub(core.effects_start).wrapping_add(1), advance: cycles >> 1 };
    let right = cycles & 1 != 0;
    let channel = usize::from(right);
    let slot = (core.reverb_position & 63) as usize;

    let heard = [saturate(input.left), saturate(input.right)];
    for (ring, value) in core.down.iter_mut().zip(heard) {
        ring[slot] = value;
        ring[slot + 64] = value;
    }
    // The 48 kHz input is kept in the history ring and decimated by the half-band filter; the result is zero-stuffed into the output ring.
    let line_in = weigh(i32::from(if right { r.in_coef_r } else { r.in_coef_l }), i32::from(reverb_filter(&core.down[channel], slot as u32, false, path)));

    let same_src = if right { r.same_r_src } else { r.same_l_src };
    let same_dst = if right { r.same_r_dst } else { r.same_l_dst };
    let diff_src = if right { r.diff_l_src } else { r.diff_r_src };
    let diff_dst = if right { r.diff_r_dst } else { r.diff_l_dst };
    let writes = core.fx_enable;
    for (src, dst) in [(same_src, same_dst), (diff_src, diff_dst)] {
        let last = i32::from(fetch(ram, window, i64::from(dst) - 1));
        let wall = weigh(i32::from(r.wall_vol), i32::from(fetch(ram, window, i64::from(src))));
        let mut next = weigh(i32::from(r.iir_vol), line_in.wrapping_add(wall).wrapping_sub(last)).wrapping_add(last);
        if r.iir_vol == -0x8000 {
            next = next.wrapping_neg();
        }
        if writes {
            store(ram, window, i64::from(dst), next);
        }
    }

    let combs = [[r.comb1_l_src, r.comb2_l_src, r.comb3_l_src, r.comb4_l_src], [r.comb1_r_src, r.comb2_r_src, r.comb3_r_src, r.comb4_r_src]];
    let volumes = [r.comb1_vol, r.comb2_vol, r.comb3_vol, r.comb4_vol];
    let mut wave = 0i32;
    for tap in 0..4 {
        wave = wave.wrapping_add(weigh(i32::from(volumes[tap]), i32::from(fetch(ram, window, i64::from(combs[channel][tap])))));
    }
    wave = i32::from(saturate(wave));

    let stages = [
        (if right { r.apf1_r_dst } else { r.apf1_l_dst }, r.apf1_size, r.apf1_vol),
        (if right { r.apf2_r_dst } else { r.apf2_l_dst }, r.apf2_size, r.apf2_vol),
    ];
    for (dst, size, volume) in stages {
        let echo = i32::from(fetch(ram, window, i64::from(dst) - i64::from(size)));
        let entering = i32::from(saturate(wave.wrapping_sub(weigh(i32::from(volume), echo))));
        if writes {
            store(ram, window, i64::from(dst), entering);
        }
        wave = i32::from(saturate(weigh(i32::from(volume), entering).wrapping_add(echo)));
    }

    core.up[channel][slot] = wave as i16;
    core.up[channel][slot + 64] = wave as i16;
    core.up[1 - channel][slot] = 0;
    core.up[1 - channel][slot + 64] = 0;
    core.reverb_position = ((slot + 1) & 63) as u32;
    Stereo {
        left: i32::from(reverb_filter(&core.up[0], core.reverb_position, true, path)),
        right: i32::from(reverb_filter(&core.up[1], core.reverb_position, true, path)),
    }
}
