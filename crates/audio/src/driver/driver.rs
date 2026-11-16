use super::iop_clock::{DmaCompletion, HandlerTiming, IopClock};
use super::memory::Snapshot;
use super::state::State;
use crate::spu_write::{DriverCommand, WriteStream};
use crate::{Result, fail};

pub type EnvxFn = Box<dyn FnMut(u32, u32) -> u16 + Send>;

pub struct EnvxFeed {
    pub tick: EnvxFn,
    pub allocation: EnvxFn,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FillEvent {
    pub core: u32,
    pub start: u32,
    pub bytes: u64,
    pub transferred: u64,
}

pub const TICK_CYCLES_NTSC: u32 = 599776;

pub struct Driver {
    state: Box<State>,
}

fn stamp(s: &mut State, first: usize, timing: &HandlerTiming, what: &str) -> Result<()> {
    let log = &mut s.regs.log;
    if log.len() - first != timing.writes.len() {
        return fail(format!("{what}: the IOP clock ran {} writes, the driver {}", timing.writes.len(), log.len() - first));
    }
    for (k, t) in timing.writes.iter().enumerate() {
        let w = &mut log[first + k];
        if w.address != t.address || w.value != t.value {
            return fail(format!("{what}: the IOP clock write {k} differs from the driver's"));
        }
        w.cycle = t.cycle;
        w.resume = t.resume;
    }
    Ok(())
}

impl Driver {
    pub fn new(snapshot: &Snapshot, libsd: Vec<u8>, feed: EnvxFeed, cold_start: bool) -> Result<Driver> {
        Ok(Driver { state: Box::new(State::new(snapshot, libsd, feed, cold_start)?) })
    }

    pub fn seed_register(&mut self, address: u32, value: u16) {
        self.state.regs.seed(address, value);
        if let Some(clock) = &mut self.state.clock {
            clock.seed(address, value);
        }
    }

    pub fn enable_timing(&mut self) -> Result<()> {
        if self.state.clock.is_some() {
            return Ok(());
        }
        let s = &mut *self.state;
        if s.started {
            return fail("write timing is enabled before the first event");
        }
        let mut clock = IopClock::new(&s.libsd_start, &s.snapshot.osdsnd, &s.snapshot.iop)?;
        for (&address, &value) in &s.regs.values {
            clock.seed(address, value);
        }
        s.clock = Some(Box::new(clock));
        Ok(())
    }

    pub fn set_dma_completion(&mut self, completion: DmaCompletion) -> Result<()> {
        match &mut self.state.clock {
            Some(clock) => {
                clock.set_dma_completion(completion);
                Ok(())
            }
            None => fail("the DMA completion is set before write timing is enabled"),
        }
    }

    pub fn timing(&self) -> bool {
        self.state.clock.is_some()
    }

    pub fn begin_frame(&mut self, frame: u32) {
        self.state.regs.frame = frame;
    }

    pub fn command(&mut self, command: &DriverCommand) -> Result<()> {
        let s = &mut *self.state;
        let first = s.regs.log.len();
        s.started = true;
        s.envx_log.clear();
        if command.id == 0x6300 {
            s.effect(command)?;
        } else if command.id == 0x60d0 || command.id == 0x6120 {
            s.ramp(command)?;
        } else {
            s.init(command)?;
        }
        if let Some(clock) = &mut s.clock {
            let timing = clock.command(command.id, &command.words, &s.envx_log)?;
            stamp(s, first, &timing, &format!("command {}", command.id))?;
        }
        Ok(())
    }

    pub fn tick(&mut self) -> Result<()> {
        let s = &mut *self.state;
        let first = s.regs.log.len();
        s.started = true;
        s.envx_log.clear();
        s.tick()?;
        if let Some(clock) = &mut s.clock {
            let timing = clock.tick(&s.envx_log)?;
            stamp(s, first, &timing, "tick")?;
        }
        Ok(())
    }

    pub fn take_writes(&mut self) -> WriteStream {
        std::mem::take(&mut self.state.regs.log)
    }

    pub fn take_fills(&mut self) -> Vec<FillEvent> {
        std::mem::take(&mut self.state.fills)
    }
}
