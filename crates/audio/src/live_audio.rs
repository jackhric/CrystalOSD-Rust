use std::fs;
use std::path::PathBuf;

use assets::bytes::hash_of;

use crate::Result;
use crate::clock_sound::{ClockSound, ClockSoundOptions, ClockSoundSources, Video};
use crate::ee_sound_queue::{EeSoundQueue, SoundCommand};
use crate::spu_write::DriverCommand;

pub const RATE: u32 = 48000;
const SAMPLES_NUMERATOR: u64 = 4004;
const SAMPLES_DENOMINATOR: u64 = 5;
pub const PREFILL_FRAMES: u32 = 4;
const MAX_QUEUED_FRAMES: u32 = 8;
pub const FRAME_SAMPLES: u32 = 801;
const UNMEASURED_SEND_SPACING: u64 = 26;
pub const EE_START_AFTER_INIT: u64 = 15000;
const RAMP_ID: u16 = 0x60D0;
const SPDIF_ID: u16 = 0x8070;

pub trait AudioDevice: Send {
    fn queued_bytes(&self) -> i32;
    fn put(&mut self, interleaved_stereo: &[i16]);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAudioOptions {
    pub mute: bool,
    pub device: bool,
    pub video: Video,
    pub wav: PathBuf,
    pub command_log: PathBuf,
}

impl Default for LiveAudioOptions {
    fn default() -> Self {
        Self { mute: false, device: true, video: Video::Ntsc, wav: PathBuf::new(), command_log: PathBuf::new() }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveAudioStats {
    pub frames: u64,
    pub samples: u64,
    pub underruns: u64,
    pub overruns: u64,
    pub clipped: u64,
    pub peak: u32,
    pub min_queued_frames: u32,
    pub max_queued_frames: u32,
    pub device_open: bool,
    pub failed: bool,
}

fn spacing_after(id: u16) -> u64 {
    match id {
        0x6140 => 3,
        0x6150 => 23,
        _ => UNMEASURED_SEND_SPACING,
    }
}

fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let bytes = (samples.len() * 2) as u32;
    let mut h = Vec::with_capacity(44 + samples.len() * 2);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + bytes).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&[1, 0, 2, 0]);
    h.extend_from_slice(&RATE.to_le_bytes());
    h.extend_from_slice(&(RATE * 4).to_le_bytes());
    h.extend_from_slice(&[4, 0, 16, 0]);
    h.extend_from_slice(b"data");
    h.extend_from_slice(&bytes.to_le_bytes());
    for s in samples {
        h.extend_from_slice(&s.to_le_bytes());
    }
    h
}

fn extend(halfword: u16) -> u32 {
    halfword as i16 as i32 as u32
}

pub struct LiveAudio {
    sound: ClockSound,
    queue: EeSoundQueue,
    skip_drain: bool,
    spdif_pending: bool,
    spdif_a3: u32,
    options: LiveAudioOptions,
    device: Option<Box<dyn AudioDevice>>,
    muted: bool,
    volume: f32,
    stats: LiveAudioStats,
    recorded: Vec<i16>,
    last: Vec<i16>,
    error: String,
}

impl LiveAudio {
    pub fn new(sources: ClockSoundSources, options: &LiveAudioOptions, device: Option<Box<dyn AudioDevice>>) -> Result<LiveAudio> {
        Self::with_device(sources, options, || device)
    }

    pub fn with_device(sources: ClockSoundSources, options: &LiveAudioOptions, open: impl FnOnce() -> Option<Box<dyn AudioDevice>>) -> Result<LiveAudio> {
        let sound = ClockSound::new(
            sources,
            ClockSoundOptions { video: options.video, auto_ticks: true, replay_init: true, modelled_writes: true, pre_roll_samples: EE_START_AFTER_INIT },
        )?;
        let mut live = LiveAudio {
            sound,
            queue: EeSoundQueue::default(),
            skip_drain: false,
            spdif_pending: true,
            spdif_a3: 0,
            options: options.clone(),
            device: None,
            muted: options.mute,
            volume: 1.0,
            stats: LiveAudioStats { min_queued_frames: u32::MAX, ..Default::default() },
            recorded: Vec::new(),
            last: Vec::new(),
            error: String::new(),
        };
        if !options.device {
            core_log::log!(Info, Audio, "device off");
            return Ok(live);
        }
        if let Some(mut device) = open() {
            let silence = vec![0i16; 2 * FRAME_SAMPLES as usize * PREFILL_FRAMES as usize];
            device.put(&silence);
            live.device = Some(device);
            live.stats.device_open = true;
        }
        core_log::log!(Info, Audio, "device {}", if live.stats.device_open { "open" } else { "none" });
        Ok(live)
    }

    fn guarded(&mut self, body: impl FnOnce(&mut ClockSound, &mut Vec<i16>) -> Result<()>) {
        if self.stats.failed {
            return;
        }
        if let Err(error) = body(&mut self.sound, &mut self.last) {
            self.stats.failed = true;
            self.error = error.0;
            core_log::log!(Error, Audio, "stopped: {}", self.error);
        }
    }

    pub fn discard_pending(&mut self) {
        self.queue.drain();
    }

    // facts/browser-shell.md section 7: the Browser's thread start stores D_001F0114 and the next drain pass sends
    // sceSdRemote(1, 0x8070, 0xA, a3), the same send as the boot's (a3 0 for S/PDIF mode 0, 2 for mode 1).
    pub fn push_spdif(&mut self, a3: u16) {
        self.spdif_pending = true;
        self.spdif_a3 = u32::from(a3);
    }

    pub fn queue(&mut self, command: SoundCommand, carried_a2: bool, carried_a3: bool) {
        let mut c = command;
        if carried_a2 {
            c.a2 = self.queue.carried_a2();
        }
        if carried_a3 {
            c.a3 = self.queue.carried_a3();
        }
        self.queue.enqueue(c);
    }

    pub fn drain(&mut self) -> Vec<SoundCommand> {
        if self.skip_drain {
            self.skip_drain = false;
            return Vec::new();
        }
        let sent = self.queue.drain();
        if sent.iter().any(|c| c.id == RAMP_ID) {
            self.skip_drain = true;
        }
        if self.stats.failed {
            return sent;
        }
        let mut at = self.sound.position();
        let mut previous = 0u16;
        let mut send = |live: &mut LiveAudio, d: DriverCommand, id: u16| {
            if previous != 0 {
                at += spacing_after(previous);
            }
            let c = DriverCommand { sample: at, ..d };
            live.guarded(|sound, _| {
                sound.queue(&c);
                Ok(())
            });
            previous = id;
        };
        for c in &sent {
            let d = DriverCommand { id: u32::from(c.id), words: [extend(c.a1), extend(c.a2), extend(c.a3), 0, 0], ..Default::default() };
            core_log::log!(Debug, Audio, "send {:x} {} {} {}", c.id, c.a1, c.a2, c.a3);
            send(self, d, c.id);
        }
        if self.spdif_pending {
            self.spdif_pending = false;
            let d = DriverCommand { id: u32::from(SPDIF_ID), words: [10, self.spdif_a3, 0x1F0662, 0, 0], ..Default::default() };
            core_log::log!(Debug, Audio, "send {:x} 10 {} 0", SPDIF_ID, self.spdif_a3);
            send(self, d, SPDIF_ID);
        }
        sent
    }

    pub fn step(&mut self) {
        if self.stats.failed {
            return;
        }
        let count = ((self.stats.frames + 1) * SAMPLES_NUMERATOR) / SAMPLES_DENOMINATOR - (self.stats.frames * SAMPLES_NUMERATOR) / SAMPLES_DENOMINATOR;
        self.stats.frames += 1;
        self.last.clear();
        self.last.resize(2 * count as usize, 0);
        self.guarded(|sound, out| sound.render(out, count as usize));
        if self.stats.failed {
            return;
        }
        self.stats.samples += count;
        for &v in &self.last {
            let magnitude = i32::from(v).unsigned_abs();
            self.stats.peak = self.stats.peak.max(magnitude);
            if v == i16::MAX || v == i16::MIN {
                self.stats.clipped += 1;
            }
        }
        if !self.options.wav.as_os_str().is_empty() {
            self.recorded.extend_from_slice(&self.last);
        }
        let Some(device) = &mut self.device else {
            return;
        };
        let queued = device.queued_bytes() as u32 / 4;
        self.stats.min_queued_frames = self.stats.min_queued_frames.min(queued);
        self.stats.max_queued_frames = self.stats.max_queued_frames.max(queued);
        if queued == 0 && self.stats.frames > 120 {
            self.stats.underruns += 1;
        }
        if queued > MAX_QUEUED_FRAMES * FRAME_SAMPLES {
            self.stats.overruns += 1;
            return;
        }
        if self.muted {
            self.last.fill(0);
        } else if self.volume != 1.0 {
            for v in self.last.iter_mut() {
                *v = ((f32::from(*v) * self.volume) as i32).clamp(-32768, 32767) as i16;
            }
        }
        device.put(&self.last);
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn stats(&self) -> &LiveAudioStats {
        &self.stats
    }

    pub fn queued_frames(&self) -> i32 {
        match &self.device {
            Some(device) => device.queued_bytes() / 4,
            None => -1,
        }
    }

    pub fn last_frame(&self) -> &[i16] {
        &self.last
    }

    pub fn last_frame_bytes(&self) -> Vec<u8> {
        self.last.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    pub fn last_frame_hash(&self) -> u64 {
        hash_of(&self.last_frame_bytes())
    }

    pub fn finish(&mut self) -> String {
        if !self.options.wav.as_os_str().is_empty() {
            let _ = fs::write(&self.options.wav, wav_bytes(&self.recorded));
        }
        if !self.options.command_log.as_os_str().is_empty() {
            let mut log = format!(
                "samples {} video {} preroll {}\n",
                self.stats.samples,
                if self.options.video == Video::Pal { "pal" } else { "ntsc" },
                EE_START_AFTER_INIT
            );
            for c in self.sound.command_log() {
                log += &format!("{} {} {:x}", c.sample, c.command.frame, c.command.id);
                for w in c.command.words {
                    log += &format!(" {w}");
                }
                log += "\n";
            }
            let _ = fs::write(&self.options.command_log, log);
        }
        let mut line = format!(
            "audio: {} frames, {} samples, peak {}, {} clipped, queue {}..{} frames, {} underruns, {} overruns, device {}{}{}",
            self.stats.frames,
            self.stats.samples,
            self.stats.peak,
            self.stats.clipped,
            if self.stats.min_queued_frames == u32::MAX { 0 } else { self.stats.min_queued_frames },
            self.stats.max_queued_frames,
            self.stats.underruns,
            self.stats.overruns,
            if self.stats.device_open { "open" } else { "none" },
            if self.stats.failed { ", FAILED: " } else { "" },
            self.error
        );
        if line.len() > 255 {
            let mut end = 255;
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
        }
        line
    }
}
