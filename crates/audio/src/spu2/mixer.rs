use super::adpcm::{AdpcmState, decode_block};
use super::core::Core;
use super::envelope::Phase;
use super::gauss::interpolate;
use super::reverb::{ReverbPath, Stereo, run_reverb};
use super::spu2::{Spu2, StageSample, write_ram_word};
use super::voice::Voice;

const ADDRESS_MASK: u32 = 0xFFFFF;
const BLOCK_MASK: u32 = 7;
const FIFO_MASK: u32 = 31;
const REFILL_BELOW: u32 = 13;
const PITCH_TOP: u32 = 0x3FFF;
const VOICE_COUNT: usize = 24;
const RING_MASK: u16 = 0x1FF;

const CAPTURE_VOICE1: u32 = 0x400;
const CAPTURE_VOICE3: u32 = 0x600;
const CAPTURE_CORE1_INPUT: u32 = 0x800;
const CAPTURE_MIXED: u32 = 0x1000;
const MEMORY_INPUT: u32 = 0x2000;
const CORE_STRIDE: u32 = 0x800;
const MEMORY_INPUT_STRIDE: u32 = 0x400;
const RIGHT_OFFSET: u32 = 0x200;

fn scale(value: i32, volume: i32) -> i32 {
    volume.wrapping_mul(value) >> 15
}

fn clamp16(value: i32) -> i32 {
    value.clamp(-0x8000, 0x7FFF)
}

#[derive(Clone, Copy, Default)]
struct Sum {
    left: i32,
    right: i32,
}

struct Bus<'a> {
    ram: &'a mut [u8],
    spdif: &'a mut [u16; 8],
    out_pos: u16,
}

#[derive(Clone, Copy)]
struct Irq {
    enable: bool,
    address: u32,
    index: u32,
}

impl Irq {
    fn of(core: &Core) -> Irq {
        Irq { enable: core.irq_enable, address: core.irq_address, index: core.index }
    }
}

fn read(ram: &[u8], address: u32) -> u16 {
    let at = ((address & ADDRESS_MASK) * 2) as usize;
    u16::from(ram[at]) | u16::from(ram[at + 1]) << 8
}

fn touch(bus: &mut Bus, irq: Irq, address: u32) {
    if irq.enable && (address & ADDRESS_MASK) == irq.address {
        bus.spdif[1] |= (4u32 << irq.index) as u16;
    }
}

fn capture(bus: &mut Bus, irq: Irq, area: u32, value: i32) {
    let address = area.wrapping_add(irq.index.wrapping_mul(CORE_STRIDE)).wrapping_add(u32::from(bus.out_pos));
    touch(bus, irq, address);
    write_ram_word(bus.ram, address, value as u16);
}

fn apply_keys(core: &mut Core) {
    for v in 0..VOICE_COUNT {
        let bit = 1u32 << v;
        let voice = &mut core.voices[v];
        if core.key_off & bit != 0 {
            voice.envelope.key_off();
        }
        if core.key_on & bit == 0 {
            continue;
        }
        voice.envelope.key_on();
        voice.next_address = voice.start_address;
        voice.phase = 0;
        voice.prev1 = 0;
        voice.prev2 = 0;
        voice.fifo_read = 0;
        voice.fifo_write = 0;
        voice.fifo = [0; 32];
        voice.block_pending = false;
        core.endx &= !bit;
    }
    core.key_on = 0;
    core.key_off = 0;
}

// A block start is handled once its last word has been fetched and the voice output is done; the block itself
// is decoded when its first data word is fetched.
fn take_header(bus: &mut Bus, irq: Irq, voice: &mut Voice) {
    let at = ((voice.next_address & ADDRESS_MASK) * 2) as usize;
    touch(bus, irq, voice.next_address);
    voice.loop_flags = bus.ram[at + 1] as i8;
    if voice.loop_flags & 4 != 0 && voice.loop_mode == 0 {
        voice.loop_address = voice.next_address;
    }
    voice.next_address = voice.next_address.wrapping_add(1) & ADDRESS_MASK;
    voice.block_pending = true;
}

fn decode_block_ahead(ram: &[u8], voice: &mut Voice) {
    let at = ((voice.next_address & !BLOCK_MASK & ADDRESS_MASK) * 2) as usize;
    let mut state = AdpcmState { prev1: voice.prev1, prev2: voice.prev2 };
    voice.block = decode_block(&ram[at..at + 16], &mut state).pcm;
    voice.prev1 = state.prev1;
    voice.prev2 = state.prev2;
    voice.block_pending = false;
}

fn finish_block(endx: &mut u32, index: usize, voice: &mut Voice) {
    if voice.loop_flags & 1 == 0 {
        return;
    }
    *endx |= 1u32 << index;
    voice.next_address = voice.loop_address;
    if voice.loop_flags & 2 != 0 {
        return;
    }
    voice.envelope.phase = Phase::Off;
    voice.envelope.level = 0;
}

// A stopped voice keeps walking through the sample memory but no longer decodes it.
fn refill(bus: &mut Bus, irq: Irq, voice: &mut Voice) {
    if voice.next_address & BLOCK_MASK == 0 {
        take_header(bus, irq, voice);
    }
    if voice.fifo_write.wrapping_sub(voice.fifo_read) >= REFILL_BELOW {
        return;
    }
    if voice.envelope.phase == Phase::Off {
        for _ in 0..4 {
            voice.fifo[(voice.fifo_write & FIFO_MASK) as usize] = 0;
            voice.fifo_write = voice.fifo_write.wrapping_add(1);
        }
    } else {
        if voice.block_pending {
            decode_block_ahead(bus.ram, voice);
        }
        touch(bus, irq, voice.next_address);
        let first = ((voice.next_address & BLOCK_MASK).wrapping_sub(1)).wrapping_mul(4);
        for k in 0..4 {
            voice.fifo[(voice.fifo_write & FIFO_MASK) as usize] = i32::from(voice.block[(first + k) as usize]);
            voice.fifo_write = voice.fifo_write.wrapping_add(1);
        }
    }
    voice.next_address = voice.next_address.wrapping_add(1) & ADDRESS_MASK;
}

fn interpolated(voice: &Voice) -> i32 {
    let mut taps = [0i16; 4];
    for (k, tap) in taps.iter_mut().enumerate() {
        *tap = voice.fifo[(voice.fifo_read.wrapping_add(k as u32) & FIFO_MASK) as usize] as i16;
    }
    interpolate(&taps, voice.phase as u32)
}

fn pitch_of(voice: &Voice, previous_out_x: Option<i32>) -> u32 {
    let Some(previous) = previous_out_x.filter(|_| voice.modulated) else {
        return u32::from(voice.pitch).min(PITCH_TOP);
    };
    let modulated = (i64::from(voice.pitch) * (0x8000 + i64::from(previous))) >> 15;
    modulated.clamp(0, i64::from(PITCH_TOP)) as u32
}

fn step_voice(bus: &mut Bus, core: &mut Core, index: usize, dry: &mut Sum, wet: &mut Sum) {
    let irq = Irq::of(core);
    let previous_out_x = if index > 0 { Some(core.voices[index - 1].out_x) } else { None };
    let noise_out = core.noise_out;
    let Core { voices, endx, .. } = core;
    let voice = &mut voices[index];
    voice.left.step();
    voice.right.step();
    refill(bus, irq, voice);
    let raw = if voice.noise { i32::from(noise_out as i16) } else { interpolated(voice) };
    let running = voice.envelope.phase != Phase::Off;
    voice.envelope.step();
    let shaped = scale(raw, voice.envelope.level);
    if running {
        voice.out_x = shaped;
    }
    let left = scale(shaped, voice.left.level);
    let right = scale(shaped, voice.right.level);
    dry.left = dry.left.wrapping_add(voice.dry_l & left);
    dry.right = dry.right.wrapping_add(voice.dry_r & right);
    wet.left = wet.left.wrapping_add(voice.wet_l & left);
    wet.right = wet.right.wrapping_add(voice.wet_r & right);

    voice.phase = voice.phase.wrapping_add(pitch_of(voice, previous_out_x) as i32);
    voice.fifo_read = voice.fifo_read.wrapping_add((voice.phase >> 12) as u32);
    voice.phase &= 0xFFF;
    if voice.next_address & BLOCK_MASK == 0 {
        finish_block(endx, index, voice);
        take_header(bus, irq, voice);
    }

    if index == 1 {
        capture(bus, irq, CAPTURE_VOICE1, voice.out_x);
    }
    if index == 3 {
        capture(bus, irq, CAPTURE_VOICE3, voice.out_x);
    }
}

// psx-spx "SPU Noise Generator": the timer advances by the step each cycle and the register shifts when it overflows.
fn step_noise(core: &mut Core) {
    let step = 4 + u32::from(core.noise_clock & 3);
    let period = 0x8000_0000u32 >> (core.noise_clock >> 2);
    core.noise_counter = core.noise_counter.wrapping_add(step << 14);
    let mut pass = 0;
    while pass < 2 && core.noise_counter >= period {
        let x = core.noise_out;
        let parity = ((x >> 15) ^ (x >> 12) ^ (x >> 11) ^ (x >> 10) ^ 1) & 1;
        core.noise_out = (x << 1) | parity;
        core.noise_counter -= period;
        pass += 1;
    }
}

fn memory_input(bus: &Bus, core: &Core) -> Sum {
    let at = MEMORY_INPUT.wrapping_add(core.index.wrapping_mul(MEMORY_INPUT_STRIDE)).wrapping_add(u32::from(bus.out_pos));
    Sum { left: i32::from(read(bus.ram, at) as i16), right: i32::from(read(bus.ram, at.wrapping_add(RIGHT_OFFSET)) as i16) }
}

fn mix_core(bus: &mut Bus, core: &mut Core, external: Sum, stage: &mut StageSample, ticks: u32, path: ReverbPath) -> Sum {
    let base = (core.index * 10) as usize;
    let mut dry = Sum::default();
    let mut wet = Sum::default();
    for v in 0..VOICE_COUNT {
        step_voice(bus, core, v, &mut dry, &mut wet);
    }
    core.master_left.step();
    core.master_right.step();
    step_noise(core);

    let dry = Sum { left: clamp16(dry.left), right: clamp16(dry.right) };
    let wet = Sum { left: clamp16(wet.left), right: clamp16(wet.right) };
    stage.field[base] = dry.left;
    stage.field[base + 1] = dry.right;
    stage.field[base + 2] = wet.left;
    stage.field[base + 3] = wet.right;
    let irq = Irq::of(core);
    capture(bus, irq, CAPTURE_MIXED, dry.left);
    capture(bus, irq, CAPTURE_MIXED + RIGHT_OFFSET, dry.right);
    capture(bus, irq, CAPTURE_MIXED + 2 * RIGHT_OFFSET, wet.left);
    capture(bus, irq, CAPTURE_MIXED + 3 * RIGHT_OFFSET, wet.right);

    let memory = memory_input(bus, core);
    let in_l = scale(memory.left, core.inp_vol.left);
    let in_r = scale(memory.right, core.inp_vol.right);
    let ext_l = scale(external.left, core.ext_vol.left);
    let ext_r = scale(external.right, core.ext_vol.right);
    let dg = core.dry_gate;
    let wg = core.wet_gate;
    let td = Sum {
        left: clamp16((dry.left & dg.snd_l).wrapping_add(in_l & dg.inp_l).wrapping_add(ext_l & dg.ext_l)),
        right: clamp16((dry.right & dg.snd_r).wrapping_add(in_r & dg.inp_r).wrapping_add(ext_r & dg.ext_r)),
    };
    let tw = Sum {
        left: clamp16((wet.left & wg.snd_l).wrapping_add(in_l & wg.inp_l).wrapping_add(ext_l & wg.ext_l)),
        right: clamp16((wet.right & wg.snd_r).wrapping_add(in_r & wg.inp_r).wrapping_add(ext_r & wg.ext_r)),
    };
    stage.field[base + 4] = tw.left;
    stage.field[base + 5] = tw.right;

    let reverb = run_reverb(core, bus.ram, ticks.wrapping_add(1), Stereo { left: tw.left, right: tw.right }, path);
    stage.field[base + 6] = reverb.left;
    stage.field[base + 7] = reverb.right;

    let mixed = Sum {
        left: clamp16(td.left.wrapping_add(scale(reverb.left, core.fx_vol.left))),
        right: clamp16(td.right.wrapping_add(scale(reverb.right, core.fx_vol.right))),
    };
    stage.field[base + 8] = mixed.left;
    stage.field[base + 9] = mixed.right;
    Sum { left: clamp16(scale(mixed.left, core.master_left.level)), right: clamp16(scale(mixed.right, core.master_right.level)) }
}

impl Spu2 {
    pub fn mix(&mut self) -> StageSample {
        let mut stage = StageSample::default();
        for core in self.core.iter_mut() {
            apply_keys(core);
        }
        let ticks = self.ticks;
        let path = self.reverb_path;
        let out_pos = self.out_pos;
        let mut bus = Bus { ram: &mut self.ram, spdif: &mut self.spdif, out_pos };

        let first = mix_core(&mut bus, &mut self.core[0], Sum::default(), &mut stage, ticks, path);
        stage.field[20] = first.left;
        stage.field[21] = first.right;
        write_ram_word(bus.ram, CAPTURE_CORE1_INPUT + u32::from(out_pos), first.left as u16);
        write_ram_word(bus.ram, CAPTURE_CORE1_INPUT + RIGHT_OFFSET + u32::from(out_pos), first.right as u16);

        let silent = (self.play_mode & 4) != 0 || self.core[1].mute;
        let second = mix_core(&mut bus, &mut self.core[1], if silent { Sum::default() } else { first }, &mut stage, ticks, path);
        stage.field[22] = second.left;
        stage.field[23] = second.right;
        self.final_ = (second.left as i16, second.right as i16);

        self.out_pos = self.out_pos.wrapping_add(1) & RING_MASK;
        self.ticks = self.ticks.wrapping_add(1);
        stage
    }
}
