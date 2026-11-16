mod common;

use std::sync::{Arc, Mutex};

use audio::clock_sound::clock_sound_sources;
use audio::ee_sound_queue::SoundCommand;
use audio::live_audio::{AudioDevice, FRAME_SAMPLES, LiveAudio, LiveAudioOptions, PREFILL_FRAMES};

fn scaled(v: i16, volume: f32) -> i16 {
    ((f32::from(v) * volume) as i32).clamp(-32768, 32767) as i16
}

// The C++ (CrystalOSD 039cc58) device scaling int16_t(std::clamp(int32_t(float(v) * volume), -32768, 32767)) for every
// 16-bit sample at these volumes, hashed with FNV-1a 64.
const SCALED: u64 = 0xae09e23ec4637eb0;

#[test]
fn volume_scaling_matches_cpp() {
    let mut h = common::Hash::default();
    for volume in [0.0f32, 0.25, 0.5, 0.7, 0.999, 1.5, 3.0, 0.1, 0.333_333_34] {
        for x in i16::MIN..=i16::MAX {
            h.i16(scaled(x, volume));
        }
    }
    assert_eq!(h.0, SCALED);
}

#[derive(Default)]
struct Sink {
    queued: i32,
    puts: Vec<Vec<i16>>,
}

struct Fake(Arc<Mutex<Sink>>);

impl AudioDevice for Fake {
    fn queued_bytes(&self) -> i32 {
        self.0.lock().unwrap().queued
    }
    fn put(&mut self, interleaved_stereo: &[i16]) {
        self.0.lock().unwrap().puts.push(interleaved_stereo.to_vec());
    }
}

#[test]
fn device_path_follows_the_cpp_rules() {
    let Some(folder) = common::resources() else { return };
    let sources = clock_sound_sources(&folder).unwrap();
    let sink = Arc::new(Mutex::new(Sink::default()));
    let with = LiveAudioOptions::default();
    let without = LiveAudioOptions { device: false, ..Default::default() };
    let mut live = LiveAudio::new(sources.clone(), &with, Some(Box::new(Fake(sink.clone())))).unwrap();
    let mut bare = LiveAudio::new(sources.clone(), &without, None).unwrap();
    assert!(live.stats().device_open && !bare.stats().device_open);
    assert_eq!(sink.lock().unwrap().puts, vec![vec![0i16; 2 * (FRAME_SAMPLES * PREFILL_FRAMES) as usize]]);
    assert_eq!(bare.queued_frames(), -1);
    let mut expected_underruns = 0;
    let mut expected_overruns = 0;
    for frame in 0..400u32 {
        for audio in [&mut live, &mut bare] {
            if frame == 1 {
                audio.queue(SoundCommand { id: 0x6150, a1: 1, a2: 0, a3: 0 }, false, false);
            }
            if frame % 40 == 5 {
                audio.queue(SoundCommand { id: 0x6300, a1: 1, a2: (frame / 40 % 12) as u16, a3: 0 }, false, false);
            }
            audio.drain();
        }
        let queued = match frame % 9 {
            0 => 0,
            4 => (8 * FRAME_SAMPLES * 4 + 4) as i32,
            7 => -1,
            _ => (frame * 4) as i32,
        };
        sink.lock().unwrap().queued = queued;
        assert_eq!(live.queued_frames(), queued / 4);
        let volume = [1.0f32, 0.5, 0.25, 2.5][(frame / 50 % 4) as usize];
        live.set_volume(volume);
        live.set_muted(frame % 100 >= 90);
        let puts_before = sink.lock().unwrap().puts.len();
        live.step();
        bare.step();
        let raw = bare.last_frame().to_vec();
        let q = queued as u32 / 4;
        if q == 0 && live.stats().frames > 120 {
            expected_underruns += 1;
        }
        let put = sink.lock().unwrap().puts.len() > puts_before;
        if q > 8 * FRAME_SAMPLES {
            expected_overruns += 1;
            assert!(!put, "frame {frame}: an overrun puts nothing");
            assert_eq!(live.last_frame(), &raw[..]);
            continue;
        }
        assert!(put);
        let want: Vec<i16> = if live.muted() {
            vec![0; raw.len()]
        } else if volume != 1.0 {
            raw.iter().map(|&v| scaled(v, volume)).collect()
        } else {
            raw.clone()
        };
        assert_eq!(sink.lock().unwrap().puts.last().unwrap(), &want, "frame {frame}");
        assert_eq!(live.last_frame(), &want[..]);
    }
    let st = live.stats();
    assert_eq!((st.underruns, st.overruns), (expected_underruns, expected_overruns));
    assert_eq!((st.min_queued_frames, st.max_queued_frames), (0, u32::MAX / 4));
    assert!(bare.stats().peak > 0);
    assert_eq!(st.samples, bare.stats().samples);
}
