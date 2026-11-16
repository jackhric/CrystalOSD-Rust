mod common;

use std::sync::{Arc, Mutex};

use audio::clock_sound::{ClockSound, ClockSoundOptions, ClockSoundSources, SharedWriteTiming, Video, WriteContext, WriteTiming, clock_sound_sources};
use audio::driver::{Driver, EnvxFeed, SoundImages, build_sound_images_from_folder};
use audio::ee_sound_queue::SoundCommand;
use audio::live_audio::{LiveAudio, LiveAudioOptions};
use audio::spu_write::DriverCommand;
use common::{Hash, Rng};

// Digests of the C++ (CrystalOSD 039cc58) LiveAudio, ClockSound and Driver run on the scenarios below with HDD OSD 1.10U's
// SNDIMAGE and hddosd.elf: per scenario the digest of every frame (drained sends, samples, failure), of the finish() line or
// the command log, the counters, and one digest per 250 frames to localise a difference.
type LiveRun = (&'static str, u64, u64, u64, u32, u64, i32, &'static [u64]);
type ClockRun = (&'static str, u64, u64, u64, u64, &'static [u64]);

const LIVE: &[LiveRun] = &[
    (
        "boot-ntsc",
        0xe561c08a318a42a8,
        0xba400fa490644d88,
        4723919,
        16036,
        0,
        1,
        &[
            0xa7c4323eb6c66ff5,
            0x3b20fa8ee566b5b0,
            0x44f9f06e2e112ece,
            0xc5096bee03b62ebd,
            0x64ae1a892d1a4431,
            0x65af88d5f69e7c31,
            0x03be1cc24e20146d,
            0xd12ba2b5ec7d597e,
            0x2cd73a456e7d187d,
            0xc9391e53c8528078,
            0xf885a94ecf5dfd85,
            0x80a52449400f4d36,
            0xf2fc67a14bfac566,
            0x1badd66c5af4028b,
            0x379850b91949fb7f,
            0xeaf2159d2d208c1a,
            0x98f2609095ca7154,
            0x7da229c7c447060c,
            0xeab0160a7b3b407b,
            0xcb6945281fc218ba,
            0xf7aee9f987099d12,
            0x74d2095ba5b9d141,
            0xf6bd395ee91cd387,
            0xf926a25fb04358f3,
        ],
    ),
    (
        "boot-pal",
        0x473400af89936730,
        0xe282ffc15ad72e56,
        4004000,
        15704,
        0,
        0,
        &[
            0x63bbb0addedb5533,
            0x0a6f65f2e92718d4,
            0x355042056ab981cf,
            0xbe258bf2243e754c,
            0x426e500b603215f7,
            0xc2f87b106c40c5cb,
            0x70ddb32441bbe982,
            0x9c7d5040c26e463d,
            0x7987f06613efdb68,
            0x32c0b721eac05733,
            0xbea7e18f5de10a6a,
            0xeb53c226a009f31c,
            0xc0f1ca7ec47bf3f7,
            0x7c427d122a3ef825,
            0x2232567da1ac0eeb,
            0xdf2090498bbdd029,
            0x72c20ad2b6bb59a2,
            0xedf895500c75cd9b,
            0x7fcabba49c27caec,
            0xcc4895023424c9ca,
        ],
    ),
    (
        "opening",
        0x8d723e8e89b7bf11,
        0x46459047b70e35aa,
        3203200,
        7723,
        0,
        0,
        &[
            0xd2adca59744fcd23,
            0x4ea52737d787bd5f,
            0x127581c43f621917,
            0x950684d35a01ee53,
            0x546757b85724a0d9,
            0xdf11afd4309cf2d0,
            0x559b9f73ab8e5a55,
            0xb09f1dd4ff37eea6,
            0xe3443eaaaf1bc37a,
            0x4d26c9f72c98b7b0,
            0x762269191a0ae8b1,
            0x559b9f73ab8e5a55,
            0x559b9f73ab8e5a55,
            0x559b9f73ab8e5a55,
            0x559b9f73ab8e5a55,
            0x559b9f73ab8e5a55,
        ],
    ),
    (
        "fuzz-1",
        0xfb4a2f3850cc8bac,
        0x3fd658ea48b97288,
        2402400,
        16015,
        0,
        0,
        &[
            0x817559ecdc341bb3,
            0x2f89a9a3149bf740,
            0x3077d88ec5961151,
            0x9aaa9ab93c861709,
            0xbbb15cf4db53c18d,
            0xb5897119c1b7bbfe,
            0xbc5078893d310804,
            0x7cf904c5f6b170d0,
            0x798b5f9cce10c641,
            0xc8987bd8a96b996e,
            0x61a69e949f7dd1b6,
            0x14450aa87bda88c4,
        ],
    ),
    (
        "fuzz-2",
        0x7a7c64de444c63de,
        0x225d1f15f495ecbf,
        2402400,
        17585,
        0,
        0,
        &[
            0xe452f4c30eea350d,
            0xe236c0f173ce0ed2,
            0x4643724b722c17b7,
            0x8b73d80e501a2d7d,
            0xbf631238ce6ddeba,
            0x02ed7260c04bdb12,
            0x6464394b26573da3,
            0x5c38926f8974f5ac,
            0x457807acbac27362,
            0xd4d69fd5fbaa6104,
            0x3d2298c4dee95015,
            0x8b28cfe37b0e465e,
        ],
    ),
    (
        "fuzz-3",
        0xd305bf2cfb117cf0,
        0x08f683324f9674d7,
        2402400,
        17668,
        0,
        0,
        &[
            0x8662041c57a417be,
            0x4e264a65e6d06a4d,
            0x151e900fa050f1fd,
            0x17fb811666790f76,
            0xcdab73047ce7146d,
            0x5d25102b9ef3b62d,
            0xb84d1b4ccbb18693,
            0xc956332000f3251f,
            0xc3db3c4e88ce2590,
            0x6191282b23c3c6a0,
            0xce5e1dc5b69e23d8,
            0x3243fc68e7d65b95,
        ],
    ),
    (
        "fuzz-4",
        0xf6d76d3f43811e14,
        0x5f603ab8cf0fabd7,
        2402400,
        16674,
        0,
        0,
        &[
            0x608fa6d24bafb475,
            0x1b4c93ed3ea6f314,
            0xd170247142929cec,
            0x9e32ace3cdea3959,
            0x255b68602d5303e9,
            0x444fd60802eca6ca,
            0xc703450ef270e026,
            0xf6f44b0cbb34b45f,
            0xbc2db08ddd42ae1b,
            0x63ed073f01a680c0,
            0x3ff691b935bf8ce3,
            0x4112f35fa0bdacdb,
        ],
    ),
];
const CLOCK: &[ClockRun] = &[
    (
        "modelled-ntsc",
        0xf5e9cbee69e829f0,
        0xce7b264b49174beb,
        0xa8c7f832281a39c5,
        1266,
        &[0x81c7ffeda46e2dc9, 0xe6173024d4013a89, 0x62ac6fe038018183, 0x30e9c954da73e246, 0x0416dae931f69049],
    ),
    (
        "entry-pal",
        0xe771e2d973b3195a,
        0x25276499590c0112,
        0xa8c7f832281a39c5,
        1951,
        &[0x26a476d75068abc5, 0x9dac7ae0b5c9f5f4, 0x4966dc8653f72caf, 0xb7352c77d70b97e8, 0xd3a8a5d221a36ff4],
    ),
    ("delayed", 0xc4b4514b4f9ba16c, 0xc8f2459623ceb196, 0xa8c7f832281a39c5, 1571, &[0x68884d99553bfb8b, 0x5f232eaeec8724b2, 0xa3dfc8681b17bc5d]),
    ("manual-ticks", 0x3f2f1ab13ac1e251, 0xc8f2459623ceb196, 0xa8c7f832281a39c5, 1571, &[0xd21ed1c19db1bc34, 0x9d50d5e7c85304e8, 0xd0b8103762faed45]),
    ("cold", 0x4ade11462b0d8c3b, 0xf104947fbd2b8afb, 0xa8c7f832281a39c5, 136, &[0x043421ca68eaa0fa, 0xbcd0573708c18230]),
];
const DRIVER: &[(&str, u64, u32)] =
    &[("cold-timed", 0x2b955278996598cb, 133), ("cold-untimed", 0x1add869cf1486121, 135), ("warm-timed", 0x69ac99859a5b62b1, 146)];

#[derive(Clone, Copy)]
struct Send {
    frame: u32,
    id: u16,
    a1: u16,
    a2: u16,
    a3: u16,
    c2: bool,
    c3: bool,
}

fn send(frame: u32, id: u16, a1: u16, a2: u16, a3: u16, c2: bool, c3: bool) -> Send {
    Send { frame, id, a1, a2, a3, c2, c3 }
}

fn fixed_boot() -> Vec<Send> {
    let mut s = vec![
        send(1, 0x6140, 0, 0, 0, false, false),
        send(123, 0x6140, 1, 0xB800, 0x8000, false, false),
        send(250, 0x6150, 1, 0, 0, false, false),
        send(281, 0x6150, 6, 0, 0xF, false, false),
        send(281, 0x6140, 2, 0, 0, true, true),
    ];
    for k in 0..16u16 {
        s.push(send(400 + 60 * u32::from(k), 0x6300, 1, k, if k == 3 { 0 } else { 0xF }, false, k == 3));
    }
    s.push(send(1400, 0x6300, 1, 2, 0x180, false, false));
    s.push(send(1400, 0x6300, 1, 3, 0, false, true));
    s.push(send(1460, 0x6300, 1, 0, 0x8000, false, false));
    s.push(send(1460, 0x6300, 1, 1, 0, false, true));
    for f in 1500..1531 {
        s.push(send(f, 0x6300, 1, 6, 1, false, false));
    }
    for _ in 0..3 {
        s.push(send(1550, 0x6300, 1, 6, 1, false, false));
    }
    s.push(send(1550, 0x6300, 1, 5, 0x57, false, false));
    s.push(send(1550, 0x6300, 1, 6, 0x57, false, false));
    s.push(send(1660, 0x6300, 1, 4, 0, false, false));
    s.push(send(1660, 0x6300, 1, 10, 0, false, false));
    s.push(send(5900, 0x6300, 0, 3, 0, false, false));
    s
}

fn fixed_opening() -> Vec<Send> {
    vec![
        send(1, 0x6150, 2, 0, 0, false, false),
        send(1, 0x6140, 6, 0, 0, true, true),
        send(123, 0x6140, 7, 0xB800, 0x8000, false, false),
        send(123, 0x6150, 0, 0, 0x11, false, false),
        send(250, 0x6150, 1, 0, 0, false, false),
        send(281, 0x6150, 6, 0, 0xF, false, false),
        send(281, 0x6140, 2, 0, 0, true, true),
        send(600, 0x6150, 2, 0, 0xE, false, false),
        send(900, 0x6150, 2, 0, 0xF, false, false),
        send(1200, 0x6150, 2, 0, 0, false, false),
        send(1500, 0x6150, 0, 0, 0xF, false, false),
        send(1800, 0x6140, 1, 0xB800, 0x8000, false, false),
        send(2100, 0x6150, 3, 0, 0, false, false),
        send(2101, 0x6150, 3, 0, 0, false, false),
        send(2400, 0x6140, 5, 0, 0, false, false),
        send(2700, 0x6150, 1, 0, 0x22, false, false),
    ]
}

fn fuzz(seed: u64, frames: u32) -> Vec<Send> {
    let mut rng = Rng(seed);
    let mut s = Vec::new();
    for f in 1..frames {
        if rng.below(40) != 0 {
            continue;
        }
        let n = 1 + rng.below(3);
        for _ in 0..n {
            let kind = rng.below(10);
            let mut c = send(f, 0, 0, 0, 0, false, false);
            if kind < 5 {
                c.id = 0x6300;
                c.a1 = 1;
                c.a2 = rng.below(16) as u16;
                c.a3 = rng.next() as u16;
                c.c3 = rng.below(3) == 0;
            } else if kind < 8 {
                c.id = 0x6140;
                c.a1 = rng.below(8) as u16;
                c.a2 = rng.next() as u16;
                c.a3 = rng.next() as u16;
                c.c2 = rng.below(2) == 0;
                c.c3 = rng.below(2) == 0;
            } else {
                c.id = 0x6150;
                c.a1 = rng.below(8) as u16;
                c.a3 = [0, 0xE, 0xF, 0x11, 0x10, 0x3F][rng.below(6) as usize];
            }
            s.push(c);
        }
    }
    s
}

fn blocks_differ(name: &str, got: &[u64], want: &[u64]) -> String {
    let first = got.iter().zip(want).position(|(a, b)| a != b);
    format!("{name}: first differing 250-frame block {first:?} of {}", want.len())
}

fn run_live(sources: &ClockSoundSources, video: Video, sends: &[Send], frames: u32, discard_at: u32) -> (u64, u64, u64, u32, u64, i32, Vec<u64>) {
    let options = LiveAudioOptions { device: false, video, ..Default::default() };
    let mut live = LiveAudio::new(sources.clone(), &options, None).unwrap();
    let mut total = Hash::default();
    let mut blocks = Vec::new();
    let mut block = Hash::default();
    let mut next = 0;
    for f in 1..=frames {
        while next < sends.len() && sends[next].frame == f {
            let c = sends[next];
            next += 1;
            live.queue(SoundCommand { id: c.id, a1: c.a1, a2: c.a2, a3: c.a3 }, c.c2, c.c3);
        }
        if f == discard_at {
            live.discard_pending();
        }
        let drained = live.drain();
        let mut fh = Hash::default();
        fh.u64(drained.len() as u64);
        for c in &drained {
            fh.u16(c.id);
            fh.u16(c.a1);
            fh.u16(c.a2);
            fh.u16(c.a3);
        }
        live.step();
        live.last_frame().iter().for_each(|&s| fh.i16(s));
        fh.flag(live.stats().failed);
        total.u64(fh.0);
        block.u64(fh.0);
        if f.is_multiple_of(250) || f == frames {
            blocks.push(block.0);
            block = Hash::default();
        }
    }
    let line = live.finish();
    let mut lh = Hash::default();
    lh.u64(line.len() as u64);
    lh.bytes(line.as_bytes());
    let st = *live.stats();
    (total.0, lh.0, st.samples, st.peak, st.clipped, i32::from(st.failed), blocks)
}

#[test]
fn live_audio_scenarios_match_cpp() {
    let Some(folder) = common::resources() else { return };
    let sources = clock_sound_sources(&folder).unwrap();
    let runs: Vec<(&str, Video, Vec<Send>, u32, u32)> = vec![
        ("boot-ntsc", Video::Ntsc, fixed_boot(), 6000, 0),
        ("boot-pal", Video::Pal, fixed_boot(), 5000, 0),
        ("opening", Video::Ntsc, fixed_opening(), 4000, 1201),
        ("fuzz-1", Video::Ntsc, fuzz(11, 3000), 3000, 0),
        ("fuzz-2", Video::Ntsc, fuzz(12, 3000), 3000, 0),
        ("fuzz-3", Video::Pal, fuzz(13, 3000), 3000, 0),
        ("fuzz-4", Video::Ntsc, fuzz(14, 3000), 3000, 0),
    ];
    for ((name, video, sends, frames, discard), want) in runs.into_iter().zip(LIVE) {
        assert_eq!(name, want.0);
        let got = run_live(&sources, video, &sends, frames, discard);
        assert_eq!(got.6, want.7, "{}", blocks_differ(name, &got.6, want.7));
        assert_eq!((got.0, got.1, got.2, got.3, got.4, got.5), (want.1, want.2, want.3, want.4, want.5, want.6), "{name}");
    }
}

struct Recorder {
    modelled: bool,
    delay: u64,
    h: Hash,
    writes: u64,
}

impl WriteTiming for Recorder {
    fn sample(&mut self, c: &WriteContext) -> u64 {
        let h = &mut self.h;
        h.flag(c.tick);
        h.u32(c.id);
        h.u64(c.event_sample);
        h.u64(c.index as u64);
        h.u32(c.write.frame);
        h.u32(c.write.sample);
        h.u32(c.write.address);
        h.u16(c.write.value);
        h.u32(c.write.cycle);
        h.u32(c.write.resume);
        self.writes += 1;
        c.event_sample + if self.modelled { u64::from(c.write.cycle) / 768 } else { 0 } + self.delay
    }
}

fn run_clock(
    sources: &ClockSoundSources,
    options: ClockSoundOptions,
    modelled: bool,
    delay: u64,
    manual_ticks: bool,
    chunks: u32,
) -> (u64, u64, u64, u64, Vec<u64>) {
    let mut sound = ClockSound::new(sources.clone(), options).unwrap();
    let recorder = Arc::new(Mutex::new(Recorder { modelled, delay, h: Hash::default(), writes: 0 }));
    sound.set_write_timing(Some(recorder.clone() as SharedWriteTiming));
    let mut commands: Vec<(u64, u32, u32, [u32; 5])> = vec![
        (15000, 0, 0x6140, [0, 0, 0, 0, 0]),
        (15003, 0, 0x8070, [10, 0, 2033250, 0, 0]),
        (15800, 0, 0x60d0, [1, 0, 0, 0, 0]),
        (17402, 21, 0x60d0, [1, 344, 344, 0, 0]),
        (112697, 140, 0x6140, [1, 0xFFFFB800, 0xFFFF8000, 0, 0]),
        (213598, 266, 0x6150, [1, 0, 0, 0, 0]),
        (238423, 297, 0x6150, [6, 0, 15, 0, 0]),
        (238446, 297, 0x6140, [2, 1772, 1768, 0, 0]),
    ];
    for k in 0..16u32 {
        commands.push((u64::from(250000 + 20000 * k), 0, 0x6300, [1, k, 0xF, 0, 0]));
    }
    commands.push((600000, 0, 0x6300, [1, 2, 384, 0, 0]));
    commands.push((600000, 0, 0x6300, [1, 3, 1872, 0, 0]));
    commands.push((600026, 0, 0x6150, [2, 0, 0xE, 0, 0]));
    commands.push((700000, 0, 0x6140, [7, 0, 0, 0, 0]));
    commands.push((700003, 0, 0x6150, [0, 0, 0x11, 0, 0]));
    commands.push((820000, 0, 0x6150, [3, 0, 0, 0, 0]));
    let mut total = Hash::default();
    let mut blocks = Vec::new();
    let mut block = Hash::default();
    let mut pcm = vec![0i16; 2 * 801];
    let mut next = 0;
    let mut error = String::new();
    let mut chunk = 0;
    while chunk < chunks && error.is_empty() {
        let at = u64::from(chunk) * 801;
        while next < commands.len() && commands[next].0 < at + 801 {
            let (sample, frame, id, words) = commands[next];
            sound.queue(&DriverCommand { frame, sample, id, words });
            next += 1;
        }
        if manual_ticks {
            sound.schedule_tick(at + 400, chunk);
        }
        let rendered = if chunk % 7 == 3 { sound.render(&mut pcm, 300).and_then(|_| sound.render(&mut pcm[600..], 501)) } else { sound.render(&mut pcm, 801) };
        if let Err(e) = rendered {
            error = e.0;
        }
        let mut fh = Hash::default();
        pcm.iter().for_each(|&s| fh.i16(s));
        fh.u64(recorder.lock().unwrap().h.0);
        fh.u64(sound.position());
        total.u64(fh.0);
        block.u64(fh.0);
        if (chunk + 1).is_multiple_of(250) || chunk + 1 == chunks {
            blocks.push(block.0);
            block = Hash::default();
        }
        chunk += 1;
    }
    let mut log = Hash::default();
    for c in sound.command_log() {
        log.u64(c.sample);
        log.u32(c.command.frame);
        log.u64(c.command.sample);
        log.u32(c.command.id);
        c.command.words.iter().for_each(|&w| log.u32(w));
    }
    let mut eh = Hash::default();
    eh.u64(error.len() as u64);
    eh.bytes(error.as_bytes());
    let writes = recorder.lock().unwrap().writes;
    (total.0, log.0, eh.0, writes, blocks)
}

#[test]
fn clock_sound_write_streams_match_cpp() {
    let Some(folder) = common::resources() else { return };
    let sources = clock_sound_sources(&folder).unwrap();
    let options = |video, auto_ticks, replay_init, modelled_writes, pre_roll_samples| ClockSoundOptions {
        video,
        auto_ticks,
        replay_init,
        modelled_writes,
        pre_roll_samples,
    };
    let runs = [
        ("modelled-ntsc", options(Video::Ntsc, true, true, true, 15000), true, 0, false, 1200),
        ("entry-pal", options(Video::Pal, true, true, false, 0), false, 0, false, 1200),
        ("delayed", options(Video::Ntsc, true, true, false, 0), false, 9, false, 600),
        ("manual-ticks", options(Video::Ntsc, false, true, true, 0), true, 0, true, 600),
        ("cold", options(Video::Ntsc, true, false, false, 0), false, 0, false, 400),
    ];
    for ((name, o, modelled, delay, manual, chunks), want) in runs.into_iter().zip(CLOCK) {
        assert_eq!(name, want.0);
        let got = run_clock(&sources, o, modelled, delay, manual, chunks);
        assert_eq!(got.4, want.5, "{}", blocks_differ(name, &got.4, want.5));
        assert_eq!((got.0, got.1, got.2, got.3), (want.1, want.2, want.3, want.4), "{name}");
    }
}

fn run_driver(images: &SoundImages, cold_start: bool, timing: bool, seed: u64) -> (u64, u32) {
    let mut n = 0u32;
    let tick = Box::new(move |c: u32, v: u32| {
        n = n.wrapping_add(1);
        let x = n.wrapping_mul(2654435761) >> 9;
        (if x.is_multiple_of(7) { x & 3 } else { (x ^ (c << 8) ^ v) & 0x7fff }) as u16
    });
    let mut m = 0u32;
    let allocation = Box::new(move |c: u32, v: u32| {
        m = m.wrapping_add(1);
        let x = m.wrapping_mul(40503).wrapping_add(v.wrapping_mul(977)).wrapping_add(c) >> 3;
        (if x.is_multiple_of(5) { x & 1 } else { x & 0x7fff }) as u16
    });
    let mut d = Driver::new(&images.snapshot, images.libsd.clone(), EnvxFeed { tick, allocation }, cold_start).unwrap();
    d.seed_register(0x1F90019A, 0xC080);
    if timing {
        d.enable_timing().unwrap();
        d.set_dma_completion(Box::new(
            |ordinal, entry| if ordinal % 3 == 2 { None } else { Some(entry.wrapping_add(500).wrapping_add(1000u32.wrapping_mul(ordinal))) },
        ))
        .unwrap();
    }
    let mut rng = Rng(seed);
    let mut script: Vec<(u32, [u32; 5])> = vec![
        (0x6010, [4, 68, 4096, 0, 64]),
        (0x61a0, [965632, 20496, 65536, 0, 7]),
        (0x6070, [4, 68, 0, 0, 64]),
        (0x8070, [2, 1, 4096, 0, 5]),
        (0x8070, [3, 1, 0, 0, 64]),
        (0x8010, [2049, 4095, 0, 0, 64]),
        (0x60c0, [0, 4, 0, 0, 64]),
        (0x60c0, [1, 4, 0, 0, 64]),
        (0x6060, [965632, 20496, 0, 0, 64]),
        (0x6060, [990208, 544784, 0, 0, 64]),
        (0x60a0, [60, 68, 0, 0, 64]),
    ];
    for k in 0..8u32 {
        script.push((0x6090, [0, 969728 + 4096 * k + if k >= 5 { 4096 } else { 0 }, 0, 0, 64]));
    }
    for k in 0..8u32 {
        script.push((0x6130, [k, 27 + 5 * k, 0, 0, 64]));
    }
    script.push((0x6310, [1, 28, 0, 0, 64]));
    script.push((0x6120, [0, 16383, 16383, 0, 64]));
    script.push((0x6120, [1, 16383, 16383, 0, 64]));
    script.push((0x6200, [4, 68, 0, 0, 64]));
    script.push((0x6140, [0, 0, 0, 0, 0]));
    let mut h = Hash::default();
    let mut errors = 0u32;
    let mut event = |d: &mut Driver, tick: bool, id: u32, words: [u32; 5], frame: u32| {
        d.begin_frame(frame);
        let result = if tick { d.tick() } else { d.command(&DriverCommand { id, words, ..Default::default() }) };
        let error = match result {
            Ok(()) => String::new(),
            Err(e) => {
                errors += 1;
                e.0
            }
        };
        h.u64(error.len() as u64);
        h.bytes(error.as_bytes());
        for w in d.take_writes() {
            h.u32(w.frame);
            h.u32(w.sample);
            h.u32(w.address);
            h.u16(w.value);
            h.u32(w.cycle);
            h.u32(w.resume);
        }
        for f in d.take_fills() {
            h.u32(f.core);
            h.u32(f.start);
            h.u64(f.bytes);
            h.u64(f.transferred);
        }
    };
    let mut frame = 0;
    for (id, words) in script {
        event(&mut d, false, id, words, frame);
        frame += 1;
    }
    let refusals: [(u32, [u32; 5]); 21] = [
        (0x6ff0, [0; 5]),
        (0x1234, [0; 5]),
        (0x60c0, [0, 7, 0, 0, 0]),
        (0x60c0, [1, 9, 0, 0, 0]),
        (0x60c0, [0, 10, 0, 0, 0]),
        (0x8070, [0xa, 1, 0, 0, 0]),
        (0x8070, [0xa, 0, 0, 0, 0]),
        (0x6120, [2, 1, 1, 0, 0]),
        (0x60d0, [0xffff, 1, 1, 0, 0]),
        (0x6300, [1, 0x7fff, 0, 0, 0]),
        (0x6300, [1, 0xffff, 0, 0, 0]),
        (0x8070, [8, 0x15, 0, 0, 0]),
        (0x8070, [4, 1, 0, 0, 0]),
        (0x8070, [5, 0, 0, 0, 0]),
        (0x60c0, [0, 0xFFFF, 0, 0, 0]),
        (0x6060, [0, 0, 0, 0, 0]),
        (0x6090, [0x80, 969728, 0, 0, 0]),
        (0x6090, [0, 0, 0, 0, 0]),
        (0x6130, [0x20, 3, 0, 0, 0]),
        (0x6150, [0x30, 0, 0, 0, 0]),
        (0x6140, [0xFFFF, 0, 0, 0, 0]),
    ];
    for (id, words) in refusals {
        event(&mut d, false, id, words, frame);
        frame += 1;
    }
    for _ in 0..2500 {
        let pick = rng.below(20);
        if pick < 12 {
            event(&mut d, true, 0, [0; 5], frame);
            frame += 1;
            continue;
        }
        let (id, words) = if pick < 15 {
            let a = rng.below(3);
            let b = rng.below(18);
            (0x6300, [a, b, 0, 0, 0])
        } else if pick < 17 {
            (0x6140, [rng.below(8), 0, 0, 0, 0])
        } else if pick < 19 {
            let a = rng.below(8);
            let b = rng.below(5);
            let c = if rng.below(2) != 0 { 0 } else { 0x11 };
            (0x6150, [a, b, c, 0, 0])
        } else {
            let id = [0x60d0u32, 0x6120, 0x8010, 0x60a0, 0x6130][rng.below(5) as usize];
            let a = rng.below(2);
            let b = rng.below(0x4000);
            let e = rng.below(0x4000);
            let first = if id == 0x8010 {
                2049 + 2 * rng.below(8)
            } else if id == 0x60a0 {
                60
            } else {
                a
            };
            (id, [first, b, e, 0, 0])
        };
        event(&mut d, false, id, words, frame);
        frame += 1;
    }
    (h.0, errors)
}

#[test]
fn driver_write_streams_match_cpp() {
    let Some(folder) = common::resources() else { return };
    let images = build_sound_images_from_folder(&folder).unwrap();
    let runs = [("cold-timed", true, true, 21), ("cold-untimed", true, false, 22), ("warm-timed", false, true, 23)];
    for ((name, cold, timing, seed), want) in runs.into_iter().zip(DRIVER) {
        assert_eq!(name, want.0);
        assert_eq!(run_driver(&images, cold, timing, seed), (want.1, want.2), "{name}");
    }
}
