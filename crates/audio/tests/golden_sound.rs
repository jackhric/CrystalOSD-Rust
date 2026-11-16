mod common;

use std::path::PathBuf;

use audio::clock_sound::clock_sound_sources;
use audio::ee_sound_queue::SoundCommand;
use audio::live_audio::{LiveAudio, LiveAudioOptions};

// What the C++ app (CrystalOSD 039cc58) drained from its EE sound queue on each logical frame of its golden run
// (`--golden record --no-validation --mute --trace`): frame, id, a1, a2, a3. The 0x60D0 entries are the queue's own ramp.
const SENT: [(u32, u16, u16, u16, u16); 36] = [
    (1, 0x6140, 0, 0, 0),
    (2, 0x60D0, 1, 0, 0),
    (4, 0x60D0, 1, 344, 344),
    (6, 0x60D0, 1, 688, 688),
    (8, 0x60D0, 1, 1032, 1032),
    (10, 0x60D0, 1, 1376, 1376),
    (12, 0x60D0, 1, 1720, 1720),
    (14, 0x60D0, 1, 2064, 2064),
    (16, 0x60D0, 1, 2408, 2408),
    (18, 0x60D0, 1, 2752, 2752),
    (20, 0x60D0, 1, 3096, 3096),
    (22, 0x60D0, 1, 3440, 3440),
    (24, 0x60D0, 1, 3784, 3784),
    (26, 0x60D0, 1, 4128, 4128),
    (28, 0x60D0, 1, 4472, 4472),
    (30, 0x60D0, 1, 4816, 4816),
    (123, 0x6140, 1, 47104, 32768),
    (249, 0x6150, 1, 0, 0),
    (280, 0x6150, 6, 0, 15),
    (280, 0x6140, 2, 1772, 1768),
    (561, 0x6300, 1, 4, 15),
    (641, 0x6300, 1, 6, 87),
    (701, 0x6300, 1, 6, 1),
    (761, 0x6300, 1, 10, 0),
    (841, 0x6300, 1, 6, 15),
    (921, 0x6300, 1, 4, 4),
    (1041, 0x6300, 1, 5, 0),
    (1101, 0x6300, 1, 5, 0),
    (1161, 0x6300, 1, 5, 0),
    (1221, 0x6300, 1, 4, 0),
    (1321, 0x6300, 1, 10, 9),
    (1401, 0x6300, 1, 2, 384),
    (1401, 0x6300, 1, 3, 1872),
    (1501, 0x6300, 1, 0, 32768),
    (1501, 0x6300, 1, 1, 1888),
    (1601, 0x6300, 1, 10, 384),
];

fn golden_sounds() -> Vec<u64> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden/cpp-native.golden");
    let text = std::fs::read_to_string(&path).expect("golden/cpp-native.golden");
    text.lines()
        .map(|line| {
            let sound = line.split_whitespace().find_map(|w| w.strip_prefix("sound=")).expect("a sound= field");
            u64::from_str_radix(sound, 16).unwrap()
        })
        .collect()
}

// The golden run's sound column: LiveAudio fed with the scene's sends, one step per logical frame, each frame's samples
// hashed with FNV-1a 64 as Main.cpp does; then the --audio-wav file and its command log the C++ run wrote (size, FNV-1a 64).
#[test]
fn live_audio_reproduces_the_golden_sound_column() {
    let Some(folder) = common::resources() else { return };
    let want = golden_sounds();
    assert_eq!(want.len(), 1719);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("audio-golden");
    std::fs::create_dir_all(&dir).unwrap();
    let options = LiveAudioOptions { device: false, mute: true, wav: dir.join("run.wav"), command_log: dir.join("run.wav.commands.txt"), ..Default::default() };
    let mut live = LiveAudio::new(clock_sound_sources(&folder).unwrap(), &options, None).unwrap();
    let mut differ = Vec::new();
    for frame in 1..=want.len() as u32 {
        let sends: Vec<SoundCommand> = SENT.iter().filter(|e| e.0 == frame).map(|&(_, id, a1, a2, a3)| SoundCommand { id, a1, a2, a3 }).collect();
        for c in sends.iter().filter(|c| c.id != 0x60D0) {
            if c.id == 0x6140 && c.a1 == 2 {
                live.queue(SoundCommand { a2: 0, a3: 0, ..*c }, true, true);
            } else {
                live.queue(*c, false, false);
            }
        }
        assert_eq!(live.drain(), sends, "frame {frame}: drained sends");
        live.step();
        assert!(!live.stats().failed, "frame {frame}: {}", live.finish());
        if live.last_frame_hash() != want[frame as usize - 1] {
            differ.push(frame);
        }
    }
    assert!(differ.is_empty(), "{} frames differ, first {:?}", differ.len(), &differ[..differ.len().min(10)]);
    assert_eq!(live.stats().samples, 1376575);
    assert_eq!(live.stats().peak, 16036);
    assert_eq!(live.finish(), "audio: 1719 frames, 1376575 samples, peak 16036, 0 clipped, queue 0..0 frames, 0 underruns, 0 overruns, device none");
    let wav = std::fs::read(dir.join("run.wav")).unwrap();
    let log = std::fs::read(dir.join("run.wav.commands.txt")).unwrap();
    assert_eq!((wav.len(), common::fnv(&wav)), (5506344, 0x8eadb1c4c4709758));
    assert_eq!((log.len(), common::fnv(&log)), (1122, 0x3e147451aa824bae));
}
