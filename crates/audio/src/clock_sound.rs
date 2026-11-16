use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use assets::resources::AssetSet;

use crate::data::snd_image::SndImage;
use crate::driver::snapshot_builder::{build_sound_images, build_sound_images_from_folder};
use crate::driver::{Driver, EnvxFeed, Snapshot, TICK_CYCLES_NTSC, nominal_dma_completion};
use crate::spu_write::{DriverCommand, SpuWrite};
use crate::spu2::Spu2;
use crate::{Result, fail};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Video {
    #[default]
    Ntsc,
    Pal,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClockSoundSources {
    pub snapshot: Snapshot,
    pub libsd: Vec<u8>,
    pub spu_ram: Vec<u8>,
    pub spu_ticks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSoundOptions {
    pub video: Video,
    pub auto_ticks: bool,
    pub replay_init: bool,
    pub modelled_writes: bool,
    pub pre_roll_samples: u64,
}

impl Default for ClockSoundOptions {
    fn default() -> Self {
        Self { video: Video::Ntsc, auto_ticks: true, replay_init: true, modelled_writes: false, pre_roll_samples: 0 }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoggedCommand {
    pub sample: u64,
    pub command: DriverCommand,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteContext {
    pub tick: bool,
    pub id: u32,
    pub event_sample: u64,
    pub index: usize,
    pub write: SpuWrite,
}

pub trait WriteTiming: Send {
    fn sample(&mut self, context: &WriteContext) -> u64;
}

pub type SharedWriteTiming = Arc<Mutex<dyn WriteTiming>>;

#[derive(Debug, Clone, Copy, Default)]
pub struct EntryStamp;

impl WriteTiming for EntryStamp {
    fn sample(&mut self, context: &WriteContext) -> u64 {
        context.event_sample
    }
}

struct Timed {
    offset: u32,
    id: u32,
    words: [u32; 5],
}

const CYCLES_PER_SAMPLE: u64 = 768;
const FIRST_TICK_AFTER_INIT: u32 = 8808;

const INIT_COMMANDS: [Timed; 43] = [
    Timed { offset: 0, id: 0x6010, words: [4, 68, 4096, 0, 64] },
    Timed { offset: 47, id: 0x61a0, words: [965632, 20496, 65536, 0, 7] },
    Timed { offset: 50, id: 0x6070, words: [4, 68, 0, 2684493968, 64] },
    Timed { offset: 1082, id: 0x61a0, words: [965632, 86032, 65536, 0, 13] },
    Timed { offset: 1085, id: 0x6070, words: [4, 68, 0, 2684494736, 64] },
    Timed { offset: 2119, id: 0x61a0, words: [965632, 151568, 65536, 0, 19] },
    Timed { offset: 2123, id: 0x6070, words: [4, 68, 0, 2684495504, 64] },
    Timed { offset: 3154, id: 0x61a0, words: [965632, 217104, 65536, 0, 25] },
    Timed { offset: 3157, id: 0x6070, words: [4, 68, 0, 2684496272, 64] },
    Timed { offset: 4188, id: 0x61a0, words: [965632, 282640, 65536, 0, 0] },
    Timed { offset: 4192, id: 0x6070, words: [4, 68, 0, 2684493072, 64] },
    Timed { offset: 5223, id: 0x61a0, words: [965632, 348176, 65536, 0, 6] },
    Timed { offset: 5226, id: 0x6070, words: [4, 68, 0, 2684493840, 64] },
    Timed { offset: 6261, id: 0x61a0, words: [965632, 544784, 65536, 0, 12] },
    Timed { offset: 6264, id: 0x6070, words: [4, 68, 0, 2684494608, 64] },
    Timed { offset: 7299, id: 0x8070, words: [2, 1, 4096, 0, 5] },
    Timed { offset: 7301, id: 0x8070, words: [3, 1, 0, 2684493712, 64] },
    Timed { offset: 7304, id: 0x8010, words: [2049, 4095, 0, 2684493968, 64] },
    Timed { offset: 7306, id: 0x60c0, words: [0, 4, 0, 2684494224, 64] },
    Timed { offset: 7833, id: 0x60c0, words: [1, 4, 0, 2684494480, 64] },
    Timed { offset: 8354, id: 0x6060, words: [965632, 20496, 0, 2684494736, 64] },
    Timed { offset: 8357, id: 0x6060, words: [990208, 544784, 0, 2684494992, 64] },
    Timed { offset: 8359, id: 0x60a0, words: [60, 68, 0, 2684495248, 64] },
    Timed { offset: 8363, id: 0x6090, words: [0, 969728, 0, 2684495504, 64] },
    Timed { offset: 8365, id: 0x6090, words: [0, 973824, 0, 2684495760, 64] },
    Timed { offset: 8368, id: 0x6090, words: [0, 977920, 0, 2684496016, 64] },
    Timed { offset: 8371, id: 0x6090, words: [0, 982016, 0, 2684496272, 64] },
    Timed { offset: 8375, id: 0x6090, words: [0, 986112, 0, 2684496528, 64] },
    Timed { offset: 8379, id: 0x6090, words: [0, 994304, 0, 2684492816, 64] },
    Timed { offset: 8382, id: 0x6090, words: [0, 998400, 0, 2684493072, 64] },
    Timed { offset: 8384, id: 0x6090, words: [0, 1002496, 0, 2684493328, 64] },
    Timed { offset: 8388, id: 0x6130, words: [0, 66, 0, 2684493584, 64] },
    Timed { offset: 8391, id: 0x6130, words: [1, 42, 0, 2684493840, 64] },
    Timed { offset: 8394, id: 0x6130, words: [2, 45, 0, 2684494096, 64] },
    Timed { offset: 8397, id: 0x6130, words: [3, 27, 0, 2684494352, 64] },
    Timed { offset: 8400, id: 0x6130, words: [4, 27, 0, 2684494608, 64] },
    Timed { offset: 8403, id: 0x6130, words: [5, 27, 0, 2684494864, 64] },
    Timed { offset: 8406, id: 0x6130, words: [6, 54, 0, 2684495120, 64] },
    Timed { offset: 8409, id: 0x6130, words: [7, 54, 0, 2684495376, 64] },
    Timed { offset: 8412, id: 0x6310, words: [1, 28, 0, 2684495632, 64] },
    Timed { offset: 8414, id: 0x6120, words: [0, 16383, 16383, 2684495888, 64] },
    Timed { offset: 8417, id: 0x6120, words: [1, 16383, 16383, 2684496144, 64] },
    Timed { offset: 8419, id: 0x6200, words: [4, 68, 0, 2684496400, 64] },
];

struct ModelledStamp;

impl WriteTiming for ModelledStamp {
    fn sample(&mut self, context: &WriteContext) -> u64 {
        context.event_sample + u64::from(context.write.cycle) / CYCLES_PER_SAMPLE
    }
}

const WARMUP_TICKS: u32 = 14;
const WARMUP_TICK_CYCLES: u32 = 299872;
const WARMUP_END_CYCLES: u32 = 362012;

fn tick_cycles(video: Video, index: u32) -> u32 {
    if video == Video::Pal {
        return 603968;
    }
    if index < WARMUP_TICKS - 1 {
        return WARMUP_TICK_CYCLES;
    }
    if index == WARMUP_TICKS - 1 {
        return WARMUP_END_CYCLES;
    }
    TICK_CYCLES_NTSC
}

pub fn clock_sound_sources(resources: &Path) -> Result<ClockSoundSources> {
    let images = build_sound_images_from_folder(resources)?;
    Ok(ClockSoundSources { snapshot: images.snapshot, libsd: images.libsd, spu_ram: images.spu_ram, spu_ticks: 0 })
}

pub fn clock_sound_sources_from_decoded(decoded: &AssetSet) -> Result<ClockSoundSources> {
    let program = decoded.find("PROGRAM");
    let sounds = decoded.find("SNDIMAGE");
    let (Some(program), Some(sounds)) = (program, sounds) else {
        return fail(format!("the decoded set lacks {}: give --resources once to decode it", if program.is_some() { "SNDIMAGE" } else { "PROGRAM" }));
    };
    let images = build_sound_images(&program.data, &SndImage::from_container(&sounds.data)?)?;
    Ok(ClockSoundSources { snapshot: images.snapshot, libsd: images.libsd, spu_ram: images.spu_ram, spu_ticks: 0 })
}

#[derive(Debug, Clone, Copy, Default)]
struct Event {
    tick: bool,
    automatic: bool,
    frame: u32,
    command: DriverCommand,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    address: u32,
    value: u16,
}

fn locked(spu: &Mutex<Spu2>) -> MutexGuard<'_, Spu2> {
    spu.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct ClockSound {
    options: ClockSoundOptions,
    spu: Arc<Mutex<Spu2>>,
    driver: Driver,
    timing: SharedWriteTiming,
    events: BTreeMap<(u64, i32, u64), Event>,
    events_inserted: u64,
    log: Vec<LoggedCommand>,
    pending: BTreeMap<(u64, u64), Pending>,
    pending_inserted: u64,
    mixed: u64,
    next_tick_cycle: u64,
    tick_index: u32,
}

impl ClockSound {
    pub fn from_resources(resources: &Path, options: ClockSoundOptions) -> Result<ClockSound> {
        Self::new(clock_sound_sources(resources)?, options)
    }

    pub fn new(sources: ClockSoundSources, options: ClockSoundOptions) -> Result<ClockSound> {
        let mut spu = Spu2::new();
        spu.load_ram(&sources.spu_ram)?;
        spu.set_ticks(sources.spu_ticks);
        let spu = Arc::new(Mutex::new(spu));
        let own = |spu: Arc<Mutex<Spu2>>| -> crate::driver::EnvxFn {
            Box::new(move |core, voice| locked(&spu).voice(core as usize, voice as usize).envelope.level as u16)
        };
        let feed = EnvxFeed { tick: own(spu.clone()), allocation: own(spu.clone()) };
        let driver = Driver::new(&sources.snapshot, sources.libsd, feed, true)?;
        let mut sound = ClockSound {
            options,
            spu,
            driver,
            timing: Arc::new(Mutex::new(EntryStamp)),
            events: BTreeMap::new(),
            events_inserted: 0,
            log: Vec::new(),
            pending: BTreeMap::new(),
            pending_inserted: 0,
            mixed: 0,
            next_tick_cycle: 0,
            tick_index: 0,
        };
        if sound.options.modelled_writes {
            sound.driver.enable_timing()?;
            sound.driver.set_dma_completion(nominal_dma_completion())?;
            sound.timing = Arc::new(Mutex::new(ModelledStamp));
        }
        let mut first_tick = 0u64;
        if sound.options.replay_init {
            for t in &INIT_COMMANDS {
                let c = DriverCommand { id: t.id, words: t.words, ..Default::default() };
                sound.enqueue(u64::from(t.offset), c);
            }
            first_tick = u64::from(FIRST_TICK_AFTER_INIT);
            sound.log.clear();
        }
        if sound.options.auto_ticks {
            sound.next_tick_cycle = first_tick * CYCLES_PER_SAMPLE;
            sound.schedule_auto_tick();
        }
        if sound.options.pre_roll_samples > 0 {
            let mut discard = vec![0i16; 2 * sound.options.pre_roll_samples as usize];
            let frames = sound.options.pre_roll_samples as usize;
            sound.render(&mut discard, frames)?;
        }
        Ok(sound)
    }

    pub fn set_write_timing(&mut self, timing: Option<SharedWriteTiming>) {
        self.timing = timing.unwrap_or_else(|| Arc::new(Mutex::new(EntryStamp)));
    }

    fn frame_of(&self, sample: u64) -> u32 {
        let centi_hertz: u64 = if self.options.video == Video::Pal { 5000 } else { 5994 };
        (sample.wrapping_mul(centi_hertz) / (48000 * 100)) as u32
    }

    fn insert_event(&mut self, sample: u64, kind: i32, event: Event) {
        self.events.insert((sample, kind, self.events_inserted), event);
        self.events_inserted += 1;
    }

    fn enqueue(&mut self, sample: u64, command: DriverCommand) {
        let frame = if command.frame != 0 { command.frame } else { self.frame_of(sample) };
        let mut e = Event { command, frame, ..Default::default() };
        e.command.frame = e.frame;
        self.log.push(LoggedCommand { sample, command: e.command });
        self.insert_event(sample, 0, e);
    }

    fn schedule_auto_tick(&mut self) {
        let sample = self.next_tick_cycle / CYCLES_PER_SAMPLE;
        let e = Event { tick: true, automatic: true, frame: self.frame_of(sample), ..Default::default() };
        self.insert_event(sample, 1, e);
        self.next_tick_cycle += u64::from(tick_cycles(self.options.video, self.tick_index));
        self.tick_index += 1;
    }

    pub fn schedule_tick(&mut self, sample: u64, frame: u32) {
        self.insert_event(sample, 1, Event { tick: true, frame, ..Default::default() });
    }

    pub fn queue(&mut self, command: &DriverCommand) {
        self.enqueue(command.sample.max(self.mixed), *command);
    }

    pub fn command_log(&self) -> Vec<LoggedCommand> {
        self.log.clone()
    }

    pub fn position(&self) -> u64 {
        self.mixed
    }

    fn run(&mut self, sample: u64, event: &Event) -> Result<()> {
        self.driver.begin_frame(event.frame);
        if event.tick {
            self.driver.tick()?;
        } else {
            self.driver.command(&event.command)?;
        }
        {
            let mut spu = locked(&self.spu);
            for f in self.driver.take_fills() {
                spu.clear_ram(f.start, f.bytes);
            }
        }
        for (index, w) in self.driver.take_writes().into_iter().enumerate() {
            let context = WriteContext { tick: event.tick, id: event.command.id, event_sample: sample, index, write: w };
            let stamp = self.timing.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).sample(&context).max(self.mixed);
            self.pending.insert((stamp, self.pending_inserted), Pending { address: w.address, value: w.value });
            self.pending_inserted += 1;
        }
        Ok(())
    }

    pub fn render(&mut self, out: &mut [i16], frames: usize) -> Result<()> {
        for i in 0..frames {
            while let Some(entry) = self.events.first_entry() {
                if entry.key().0 > self.mixed {
                    break;
                }
                let at = entry.key().0;
                let event = entry.remove();
                if event.automatic {
                    self.schedule_auto_tick();
                }
                self.run(at, &event)?;
            }
            let mut spu = locked(&self.spu);
            while let Some(entry) = self.pending.first_entry() {
                if entry.key().0 > self.mixed {
                    break;
                }
                let p = entry.remove();
                spu.write(p.address, p.value)?;
            }
            spu.mix();
            out[2 * i] = spu.final_left();
            out[2 * i + 1] = spu.final_right();
            self.mixed += 1;
        }
        Ok(())
    }
}
