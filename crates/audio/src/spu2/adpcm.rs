#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdpcmState {
    pub prev1: i32,
    pub prev2: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdpcmBlock {
    pub pcm: [i16; 28],
    pub loop_start: bool,
    pub loop_end: bool,
    pub loop_repeat: bool,
}

const FORWARD: [i32; 8] = [0, 60, 115, 98, 122, 0, 0, 0];
const BACKWARD: [i32; 8] = [0, 0, -52, -55, -60, 0, 0, 0];

fn clamp16(x: i32) -> i16 {
    x.clamp(-32768, 32767) as i16
}

pub fn decode_block(block: &[u8], state: &mut AdpcmState) -> AdpcmBlock {
    let mut out = AdpcmBlock::default();
    let shift = i32::from(block[0] & 15);
    let filter = usize::from(block[0] >> 4 & 7);
    out.loop_end = block[1] & 1 != 0;
    out.loop_repeat = block[1] & 2 != 0;
    out.loop_start = block[1] & 4 != 0;
    let mut older = state.prev2;
    let mut newer = state.prev1;
    for i in 0..28 {
        let nibble = i32::from(block[2 + i / 2] >> (if i & 1 != 0 { 4 } else { 0 }) & 15);
        let raw = i32::from((nibble << 12) as i16) >> shift;
        let predicted = (newer.wrapping_mul(FORWARD[filter]).wrapping_add(older.wrapping_mul(BACKWARD[filter])).wrapping_add(32)) >> 6;
        let sample = clamp16(raw.wrapping_add(predicted));
        out.pcm[i] = sample;
        older = newer;
        newer = i32::from(sample);
    }
    state.prev1 = newer;
    state.prev2 = older;
    out
}
