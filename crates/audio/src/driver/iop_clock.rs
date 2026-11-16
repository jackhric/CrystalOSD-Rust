use std::collections::{HashMap, HashSet};

use super::memory::{LIBSD_BASE, OSDSND_BASE, SOUND_IOP_BASE};
use crate::spu_write::SpuWrite;
use crate::{Result, fail};

pub const VBLANK_HANDLER_CYCLES: u32 = 8431;
pub const IOP_CYCLES_PER_SAMPLE: u32 = 768;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EnvxRead {
    pub core: u32,
    pub voice: u32,
    pub value: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimedWrite {
    pub address: u32,
    pub value: u16,
    pub cycle: u32,
    pub resume: u32,
}

pub type DmaCompletion = Box<dyn FnMut(u32, u32) -> Option<u32> + Send>;

// psx-spx documents the PS1 SPU only and gives no SPU2 DMA rate. 12622 is the shortest observed time of one 1 KiB chunk in Watson (PCSX2)
// boot-a, the emulator's floor; 751 is the unmodelled kernel-call time between the clock's wait entry and the transfer start (mode of 167 waits).
pub const DMA_CHUNK_CYCLES: u32 = 12622;
pub const DMA_START_CYCLES: u32 = 751;

pub fn nominal_dma_completion() -> DmaCompletion {
    Box::new(|_, entry| Some(entry.wrapping_add(DMA_START_CYCLES).wrapping_add(DMA_CHUNK_CYCLES)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandlerTiming {
    pub writes: Vec<TimedWrite>,
    pub instructions: u32,
    pub unmodelled_stubs: u32,
    pub unmeasured_ops: u32,
}

pub fn add_vblank_preemption(writes: &mut [SpuWrite], entry_cycle: u64, vblank_cycles: &[u64]) {
    let mut next = vblank_cycles.partition_point(|&v| v < entry_cycle);
    let mut shift = 0u64;
    let mut segment = 0u32;
    for w in writes.iter_mut() {
        if w.resume != segment {
            segment = w.resume;
            shift = 0;
            while next < vblank_cycles.len() && vblank_cycles[next].wrapping_sub(entry_cycle) <= u64::from(segment) {
                next += 1;
            }
        }
        let mut t = u64::from(w.cycle) + shift;
        while next < vblank_cycles.len() && vblank_cycles[next].wrapping_sub(entry_cycle) <= t {
            shift += u64::from(VBLANK_HANDLER_CYCLES);
            t += u64::from(VBLANK_HANDLER_CYCLES);
            next += 1;
        }
        w.cycle = t as u32;
    }
}

pub fn sample_of_cycle(cycle: u64, origin_cycle: u64) -> u64 {
    cycle.wrapping_sub(origin_cycle) / u64::from(IOP_CYCLES_PER_SAMPLE)
}

const RAM_SIZE: u32 = 0x200000;
const RETURN_ADDRESS: u32 = 0xfffffff0;
const STACK: u32 = 0x1f0000;
const TICK_ENTRY: u32 = OSDSND_BASE as u32 + 0x2470;
const COMMAND_ENTRY: u32 = 0x90868;
const COMMAND_BUFFER: u32 = 0x993d0;
const MEMSET: u32 = OSDSND_BASE as u32 + 0x8624;
const MEMCPY: u32 = OSDSND_BASE as u32 + 0x861c;
const DMA_CODE_FIRST: u32 = 0x8d200;
const DMA_CODE_END: u32 = 0x8d600;
const WAIT_EVENT_FLAG: u32 = 0xe090;
const KERNEL_STUB: u32 = 0x1000;
const IMPORT_MAGIC: u32 = 0x41e00000;
const JUMP_RETURN: u32 = 0x03e00008;
const EVENT_LIBRARY_WAIT: u32 = 10;
const DMA_RESUME_CYCLES: u32 = 793;
const MULTIPLY_EXTRA: u32 = 7;
const MEMSET_TICK_LENGTH: u32 = 0x3c;
const MEMSET_TICK_CYCLES: u32 = 67;
const INIT_COMMAND: u32 = 0x6010;
const STEP_LIMIT: u64 = 50_000_000;

struct InitWindow {
    ordinal: u32,
    cycles: u32,
}

const INIT_WINDOWS: [InitWindow; 4] = [
    InitWindow { ordinal: 5, cycles: 2159 },
    InitWindow { ordinal: 10, cycles: 540 },
    InitWindow { ordinal: 16, cycles: 371 },
    InitWindow { ordinal: 22, cycles: 409 },
];

fn is_io(a: u32) -> bool {
    (a & 0x1fffffff) >= 0x1f800000
}

fn phys(a: u32) -> u32 {
    a & 0x1fffffff
}

fn is_control(word: u32) -> bool {
    let op = word >> 26;
    if (1..=7).contains(&op) {
        return true;
    }
    op == 0 && ((word & 63) == 8 || (word & 63) == 9)
}

#[derive(Debug, Clone, Copy, Default)]
struct Counters {
    instructions: u32,
    multiplies: u32,
    divides: u32,
}

#[derive(Clone, Copy)]
struct Call {
    target: u32,
    length: u32,
}

pub struct IopClock {
    ram: Vec<u8>,
    r: [i32; 32],
    hi: i32,
    lo: i32,
    pc: u32,
    values: HashMap<u32, u16>,
    blocks: HashMap<u32, u32>,
    loop_heads: HashSet<u32>,
    block_end: u32,
    boundary: bool,
    count: Counters,
    seen: Counters,
    stub_cycles: u32,
    ordinal: u32,
    dma_ordinal: u32,
    resume: u32,
    dma: Option<DmaCompletion>,
    handler_is_init: u32,
    out: HandlerTiming,
    envx: HashMap<u32, Vec<u16>>,
    envx_last: HashMap<u32, u16>,
}

impl IopClock {
    pub fn new(libsd: &[u8], osdsnd: &[u8], iop: &[u8]) -> Result<IopClock> {
        let mut clock = IopClock {
            ram: vec![0; RAM_SIZE as usize],
            r: [0; 32],
            hi: 0,
            lo: 0,
            pc: 0,
            values: HashMap::new(),
            blocks: HashMap::new(),
            loop_heads: HashSet::new(),
            block_end: 0xffffffff,
            boundary: false,
            count: Counters::default(),
            seen: Counters::default(),
            stub_cycles: 0,
            ordinal: 0,
            dma_ordinal: 0,
            resume: 0,
            dma: None,
            handler_is_init: 0,
            out: HandlerTiming::default(),
            envx: HashMap::new(),
            envx_last: HashMap::new(),
        };
        let ram = &mut clock.ram;
        let mut place = |at: u32, data: &[u8]| -> Result<()> {
            if at as usize + data.len() > ram.len() {
                return fail("IOP clock image runs past RAM");
            }
            ram[at as usize..at as usize + data.len()].copy_from_slice(data);
            Ok(())
        };
        place(LIBSD_BASE as u32, libsd)?;
        place(OSDSND_BASE as u32, osdsnd)?;
        place(SOUND_IOP_BASE as u32, iop)?;
        let word = |ram: &[u8], a: u32| -> u32 {
            let a = a as usize;
            u32::from(ram[a]) | u32::from(ram[a + 1]) << 8 | u32::from(ram[a + 2]) << 16 | u32::from(ram[a + 3]) << 24
        };
        let link = |ram: &mut Vec<u8>, from: u32, length: usize| {
            let end = from as u64 + length as u64;
            let mut a = from;
            while u64::from(a) + 20 <= end {
                if word(ram, a) == IMPORT_MAGIC {
                    let events = &ram[a as usize + 12..a as usize + 19] == b"thevent";
                    let mut p = a + 20;
                    while u64::from(p) + 8 <= end && word(ram, p) == JUMP_RETURN {
                        let target = if events && (word(ram, p + 4) & 0xffff) == EVENT_LIBRARY_WAIT { WAIT_EVENT_FLAG } else { KERNEL_STUB };
                        let jump = 0x08000000u32 | (target >> 2);
                        ram[p as usize..p as usize + 4].copy_from_slice(&jump.to_le_bytes());
                        p += 8;
                    }
                }
                a += 4;
            }
        };
        let seed_handle = |ram: &mut Vec<u8>, a: u32, value: u32| {
            if word(ram, a) != 0 {
                return;
            }
            ram[a as usize..a as usize + 4].copy_from_slice(&value.to_le_bytes());
        };
        // libsd's module start creates its event flags and keeps the kernel's handles here; a start image built from the file has none, and any nonzero handle serves.
        seed_handle(&mut clock.ram, 0x90050, 0x01361521);
        seed_handle(&mut clock.ram, 0x90054, 0x01360f23);
        seed_handle(&mut clock.ram, 0x90058, 1);
        link(&mut clock.ram, LIBSD_BASE as u32, libsd.len());
        link(&mut clock.ram, OSDSND_BASE as u32, osdsnd.len());
        for (from, length) in [(LIBSD_BASE as u32, libsd.len()), (OSDSND_BASE as u32, osdsnd.len())] {
            let end = from as u64 + length as u64;
            let mut a = from;
            while u64::from(a) + 4 <= end {
                let w = word(&clock.ram, a);
                let op = w >> 26;
                if op == 1 || (4..=7).contains(&op) {
                    let target = a.wrapping_add(4).wrapping_add((i32::from(w as u16 as i16) * 4) as u32);
                    if target <= a {
                        clock.loop_heads.insert(target);
                    }
                }
                a += 4;
            }
        }
        for (address, value) in [(0x1f9007c0u32, 0xc032u16), (0x1f9007c6, 0x900), (0x1f9007c8, 0x200), (0x1f9007ca, 0x8)] {
            clock.values.insert(address, value);
        }
        Ok(clock)
    }

    pub fn seed(&mut self, address: u32, value: u16) {
        self.values.insert(address, value);
    }

    pub fn set_dma_completion(&mut self, completion: DmaCompletion) {
        self.dma = Some(completion);
    }

    pub fn tick(&mut self, envx: &[EnvxRead]) -> Result<HandlerTiming> {
        self.run(TICK_ENTRY, &[], 0, false, envx)
    }

    pub fn command(&mut self, id: u32, words: &[u32; 5], envx: &[EnvxRead]) -> Result<HandlerTiming> {
        let at = COMMAND_BUFFER as usize;
        self.ram[at..at + 4].fill(0);
        for (k, w) in words.iter().enumerate() {
            self.ram[at + 4 + 4 * k..at + 8 + 4 * k].copy_from_slice(&w.to_le_bytes());
        }
        let args = [id, COMMAND_BUFFER, 0x40];
        self.run(COMMAND_ENTRY, &args, id, true, envx)
    }

    fn run(&mut self, entry: u32, args: &[u32], handler_id: u32, is_command: bool, envx: &[EnvxRead]) -> Result<HandlerTiming> {
        self.out = HandlerTiming::default();
        self.envx.clear();
        self.envx_last.clear();
        for e in envx {
            self.envx.entry(e.core << 8 | e.voice).or_default().push(e.value);
        }
        for values in self.envx.values_mut() {
            values.reverse();
        }
        self.count = Counters::default();
        self.seen = Counters::default();
        self.stub_cycles = 0;
        self.ordinal = 0;
        self.dma_ordinal = 0;
        self.resume = 0;
        self.handler_is_init = u32::from(is_command && handler_id == INIT_COMMAND);
        self.boundary = true;
        self.r = [0; 32];
        self.r[29] = STACK as i32;
        self.r[31] = RETURN_ADDRESS as i32;
        for (i, &a) in args.iter().enumerate() {
            self.r[4 + i] = a as i32;
        }
        self.pc = entry;
        let mut steps = 0u64;
        while self.pc != RETURN_ADDRESS {
            self.step()?;
            steps += 1;
            if steps > STEP_LIMIT {
                return fail(format!("IOP clock step limit at pc {}", self.pc));
            }
        }
        let mut out = std::mem::take(&mut self.out);
        out.instructions = self.count.instructions;
        Ok(out)
    }

    fn enter_block(&mut self, at: u32) -> Result<()> {
        let end = match self.blocks.get(&at) {
            Some(&end) => end,
            None => {
                let end = self.compile(at)?;
                self.blocks.insert(at, end);
                end
            }
        };
        self.block_end = end;
        self.seen = self.count;
        Ok(())
    }

    fn compile(&self, start: u32) -> Result<u32> {
        let mut a = start;
        loop {
            if a != start && (self.blocks.contains_key(&a) || self.loop_heads.contains(&a)) {
                return Ok(a);
            }
            let i = self.ram_index(a, 4)? as usize;
            let w = u32::from(self.ram[i]) | u32::from(self.ram[i + 1]) << 8 | u32::from(self.ram[i + 2]) << 16 | u32::from(self.ram[i + 3]) << 24;
            if is_control(w) {
                return Ok(a.wrapping_add(8));
            }
            a = a.wrapping_add(4);
        }
    }

    fn ram_index(&self, address: u32, size: u32) -> Result<u32> {
        let i = address & (RAM_SIZE - 1);
        if i + size > RAM_SIZE {
            return fail("IOP clock RAM access out of range");
        }
        Ok(i)
    }

    fn step(&mut self) -> Result<()> {
        if self.pc < LIBSD_BASE as u32 {
            self.stub(self.pc);
            return Ok(());
        }
        if self.boundary || self.pc == self.block_end {
            self.boundary = false;
            self.enter_block(self.pc)?;
        }
        if self.pc == MEMSET || self.pc == MEMCPY {
            return self.host_fill(self.pc);
        }
        let at = self.pc;
        let word = self.load(at, 4, false)?;
        self.execute(word, at)?;
        self.r[0] = 0;
        Ok(())
    }

    fn stub(&mut self, at: u32) {
        if at == WAIT_EVENT_FLAG
            && let Some(dma) = &mut self.dma
        {
            let now = self.count.instructions.wrapping_add(MULTIPLY_EXTRA.wrapping_mul(self.count.multiplies)).wrapping_add(self.stub_cycles);
            let ordinal = self.dma_ordinal;
            self.dma_ordinal = self.dma_ordinal.wrapping_add(1);
            if let Some(done) = dma(ordinal, now) {
                let resume = done.wrapping_add(DMA_RESUME_CYCLES);
                if resume > now {
                    self.stub_cycles = self.stub_cycles.wrapping_add(resume - now);
                }
                self.resume = resume.max(now);
            }
            self.ordinal = self.ordinal.wrapping_add(1);
            self.r[2] = 0;
            self.pc = self.r[31] as u32;
            return;
        }
        let cost = self.stub_cost(Call { target: at, length: self.r[6] as u32 }, self.ordinal);
        if cost == 0 {
            self.out.unmodelled_stubs += 1;
        }
        self.stub_cycles = self.stub_cycles.wrapping_add(cost);
        self.ordinal = self.ordinal.wrapping_add(1);
        self.r[2] = 0;
        self.pc = self.r[31] as u32;
    }

    fn host_fill(&mut self, at: u32) -> Result<()> {
        let cost = self.stub_cost(Call { target: at, length: self.r[6] as u32 }, self.ordinal);
        if cost == 0 {
            self.out.unmodelled_stubs += 1;
        }
        self.stub_cycles = self.stub_cycles.wrapping_add(cost);
        self.ordinal = self.ordinal.wrapping_add(1);
        let dst = self.r[4] as u32;
        if at == MEMSET {
            let mut k = 0i32;
            while k < self.r[6] {
                self.store(dst.wrapping_add(k as u32), 1, self.r[5] as u32)?;
                k += 1;
            }
        } else {
            let src = self.r[5] as u32;
            let mut k = 0i32;
            while k < self.r[6] {
                let v = self.load(src.wrapping_add(k as u32), 1, false)?;
                self.store(dst.wrapping_add(k as u32), 1, v)?;
                k += 1;
            }
        }
        self.r[2] = self.r[4];
        self.pc = self.r[31] as u32;
        Ok(())
    }

    fn stub_cost(&self, call: Call, ordinal: u32) -> u32 {
        if call.target == MEMSET && call.length == MEMSET_TICK_LENGTH {
            return MEMSET_TICK_CYCLES;
        }
        if self.handler_is_init != 0 {
            for w in &INIT_WINDOWS {
                if w.ordinal == ordinal {
                    return w.cycles;
                }
            }
        }
        0
    }

    fn tally(&mut self, word: u32) {
        self.count.instructions = self.count.instructions.wrapping_add(1);
        if is_control(word) {
            self.boundary = true;
        }
        if (word >> 26) == 0 && ((word & 63) == 0x18 || (word & 63) == 0x19) {
            self.count.multiplies = self.count.multiplies.wrapping_add(1);
        }
    }

    fn mmio_read(&mut self, address: u32) -> u32 {
        let off = address & 0x7ff;
        let in_core = off & 0x3ff;
        if (address >> 16) == 0x1f90 && off < 0x800 && in_core < 0x180 && (in_core & 0xf) == 0xa {
            let core = (off >> 10) & 1;
            let voice = in_core >> 4;
            let key = core << 8 | voice;
            if let Some(values) = self.envx.get_mut(&key)
                && let Some(value) = values.pop()
            {
                self.envx_last.insert(key, value);
                return u32::from(value);
            }
            return self.envx_last.get(&key).map_or(0, |&v| u32::from(v));
        }
        self.values.get(&address).map_or(0, |&v| u32::from(v))
    }

    fn mmio_write(&mut self, address: u32, value: u32) {
        if (DMA_CODE_FIRST..DMA_CODE_END).contains(&self.pc) {
            return;
        }
        self.values.insert(address, value as u16);
        if self.count.divides != 0 {
            self.out.unmeasured_ops += 1;
        }
        if (address >> 16) == 0x1f90 {
            let cycle = self.seen.instructions.wrapping_add(MULTIPLY_EXTRA.wrapping_mul(self.seen.multiplies)).wrapping_add(self.stub_cycles);
            self.out.writes.push(TimedWrite { address, value: value as u16, cycle, resume: self.resume });
        }
    }

    fn load(&mut self, address: u32, size: u32, sign: bool) -> Result<u32> {
        let mut v = 0u32;
        if is_io(address) {
            v = self.mmio_read(phys(address));
            if size == 1 {
                v &= 0xff;
            } else if size == 2 {
                v &= 0xffff;
            }
        } else {
            let i = self.ram_index(address, size)?;
            for b in 0..size {
                v |= u32::from(self.ram[(i + b) as usize]) << (8 * b);
            }
        }
        if sign && size == 1 {
            return Ok(i32::from(v as u8 as i8) as u32);
        }
        if sign && size == 2 {
            return Ok(i32::from(v as u16 as i16) as u32);
        }
        Ok(v)
    }

    fn store(&mut self, address: u32, size: u32, value: u32) -> Result<()> {
        if is_io(address) {
            self.mmio_write(
                phys(address),
                if size == 1 {
                    value & 0xff
                } else if size == 2 {
                    value & 0xffff
                } else {
                    value
                },
            );
            return Ok(());
        }
        let i = self.ram_index(address, size)?;
        for b in 0..size {
            self.ram[(i + b) as usize] = (value >> (8 * b)) as u8;
        }
        Ok(())
    }

    fn delay_slot(&mut self, at: u32) -> Result<()> {
        let slot = at.wrapping_add(4);
        let word = self.load(slot, 4, false)?;
        let saved_pc = self.pc;
        self.execute(word, slot)?;
        self.pc = saved_pc;
        Ok(())
    }

    fn branch(&mut self, at: u32, taken: bool, to: u32) -> Result<()> {
        self.delay_slot(at)?;
        self.pc = if taken { to } else { at.wrapping_add(8) };
        Ok(())
    }

    fn execute(&mut self, word: u32, at: u32) -> Result<()> {
        self.tally(word);
        let op = word >> 26;
        let rs = ((word >> 21) & 31) as usize;
        let rt = ((word >> 16) & 31) as usize;
        let rd = ((word >> 11) & 31) as usize;
        let sh = (word >> 6) & 31;
        let imm = word & 0xffff;
        let simm = i32::from(imm as u16 as i16);
        let next = at.wrapping_add(4);
        let u = |r: &[i32; 32], reg: usize| r[reg] as u32;
        let branch_target = next.wrapping_add((simm as u32).wrapping_mul(4));
        match op {
            0 => {
                let r = &mut self.r;
                let func = word & 63;
                match func {
                    0x00 => r[rd] = (u(r, rt) << sh) as i32,
                    0x02 => r[rd] = (u(r, rt) >> sh) as i32,
                    0x03 => r[rd] = r[rt] >> sh,
                    0x04 => r[rd] = (u(r, rt) << (u(r, rs) & 31)) as i32,
                    0x06 => r[rd] = (u(r, rt) >> (u(r, rs) & 31)) as i32,
                    0x07 => r[rd] = r[rt] >> (u(r, rs) & 31),
                    0x08 => {
                        let to = u(r, rs);
                        self.delay_slot(at)?;
                        self.pc = to;
                        return Ok(());
                    }
                    0x09 => {
                        let to = u(r, rs);
                        let ra = at.wrapping_add(8) as i32;
                        self.delay_slot(at)?;
                        self.r[rd] = ra;
                        self.pc = to;
                        return Ok(());
                    }
                    0x10 => r[rd] = self.hi,
                    0x11 => self.hi = r[rs],
                    0x12 => r[rd] = self.lo,
                    0x13 => self.lo = r[rs],
                    0x18 => {
                        let p = i64::from(r[rs]) * i64::from(r[rt]);
                        self.lo = p as u64 as u32 as i32;
                        self.hi = ((p as u64) >> 32) as u32 as i32;
                    }
                    0x19 => {
                        let p = u64::from(u(r, rs)) * u64::from(u(r, rt));
                        self.lo = p as u32 as i32;
                        self.hi = (p >> 32) as u32 as i32;
                    }
                    0x1a => {
                        self.count.divides = self.count.divides.wrapping_add(1);
                        if r[rt] != 0 {
                            if r[rs] == i32::MIN && r[rt] == -1 {
                                self.lo = i32::MIN;
                                self.hi = 0;
                            } else {
                                self.lo = r[rs] / r[rt];
                                self.hi = r[rs] % r[rt];
                            }
                        }
                    }
                    0x1b => {
                        self.count.divides = self.count.divides.wrapping_add(1);
                        if r[rt] != 0 {
                            self.lo = (u(r, rs) / u(r, rt)) as i32;
                            self.hi = (u(r, rs) % u(r, rt)) as i32;
                        }
                    }
                    0x20 | 0x21 => r[rd] = u(r, rs).wrapping_add(u(r, rt)) as i32,
                    0x22 | 0x23 => r[rd] = u(r, rs).wrapping_sub(u(r, rt)) as i32,
                    0x24 => r[rd] = r[rs] & r[rt],
                    0x25 => r[rd] = r[rs] | r[rt],
                    0x26 => r[rd] = r[rs] ^ r[rt],
                    0x27 => r[rd] = !(r[rs] | r[rt]),
                    0x2a => r[rd] = i32::from(r[rs] < r[rt]),
                    0x2b => r[rd] = i32::from(u(r, rs) < u(r, rt)),
                    _ => return fail(format!("IOP clock: unsupported SPECIAL {func} at {at}")),
                }
            }
            1 => {
                let neg = (rt & 1) == 0;
                let taken = if neg { self.r[rs] < 0 } else { self.r[rs] >= 0 };
                let link = (rt & 0x10) != 0;
                let ra = at.wrapping_add(8) as i32;
                self.branch(at, taken, branch_target)?;
                if link {
                    self.r[31] = ra;
                }
                return Ok(());
            }
            2 => {
                self.delay_slot(at)?;
                self.pc = (next & 0xf0000000) | ((word & 0x3ffffff) << 2);
                return Ok(());
            }
            3 => {
                let ra = at.wrapping_add(8) as i32;
                self.delay_slot(at)?;
                self.r[31] = ra;
                self.pc = (next & 0xf0000000) | ((word & 0x3ffffff) << 2);
                return Ok(());
            }
            4 => return self.branch(at, self.r[rs] == self.r[rt], branch_target),
            5 => return self.branch(at, self.r[rs] != self.r[rt], branch_target),
            6 => return self.branch(at, self.r[rs] <= 0, branch_target),
            7 => return self.branch(at, self.r[rs] > 0, branch_target),
            8 | 9 => self.r[rt] = u(&self.r, rs).wrapping_add(simm as u32) as i32,
            10 => self.r[rt] = i32::from(self.r[rs] < simm),
            11 => self.r[rt] = i32::from(u(&self.r, rs) < simm as u32),
            12 => self.r[rt] = (u(&self.r, rs) & imm) as i32,
            13 => self.r[rt] = (u(&self.r, rs) | imm) as i32,
            14 => self.r[rt] = (u(&self.r, rs) ^ imm) as i32,
            15 => self.r[rt] = (imm << 16) as i32,
            32 => self.r[rt] = self.load(u(&self.r, rs).wrapping_add(simm as u32), 1, true)? as i32,
            33 => self.r[rt] = self.load(u(&self.r, rs).wrapping_add(simm as u32), 2, true)? as i32,
            35 => self.r[rt] = self.load(u(&self.r, rs).wrapping_add(simm as u32), 4, false)? as i32,
            36 => self.r[rt] = self.load(u(&self.r, rs).wrapping_add(simm as u32), 1, false)? as i32,
            37 => self.r[rt] = self.load(u(&self.r, rs).wrapping_add(simm as u32), 2, false)? as i32,
            40 => self.store(u(&self.r, rs).wrapping_add(simm as u32), 1, u(&self.r, rt))?,
            41 => self.store(u(&self.r, rs).wrapping_add(simm as u32), 2, u(&self.r, rt))?,
            43 => self.store(u(&self.r, rs).wrapping_add(simm as u32), 4, u(&self.r, rt))?,
            _ => return fail(format!("IOP clock: unsupported opcode {op} at {at}")),
        }
        self.r[0] = 0;
        self.pc = next;
        Ok(())
    }
}
