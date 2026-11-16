use std::ffi::{CStr, CString, c_int};
use std::path::PathBuf;

use audio::live_audio::AudioDevice;
use render::{ash, vk};
use sdl3_sys::audio::*;
use sdl3_sys::error::SDL_GetError;
use sdl3_sys::filesystem::SDL_GetBasePath;
use sdl3_sys::init::{SDL_INIT_AUDIO, SDL_InitSubSystem, SDL_QuitSubSystem};
use sdl3_sys::timer::{SDL_Delay, SDL_DelayPrecise, SDL_GetTicksNS};
use sdl3_sys::video::*;
use sdl3_sys::vulkan::{SDL_Vulkan_CreateSurface, SDL_Vulkan_GetInstanceExtensions};

pub fn error() -> String {
    // SAFETY: SDL_GetError returns a valid, NUL-terminated string.
    unsafe { CStr::from_ptr(SDL_GetError()).to_string_lossy().into_owned() }
}

pub fn ticks_ns() -> u64 {
    // SAFETY: no preconditions.
    unsafe { SDL_GetTicksNS() }
}

pub fn delay_precise(ns: u64) {
    // SAFETY: no preconditions.
    unsafe { SDL_DelayPrecise(ns) }
}

pub fn delay(ms: u32) {
    // SAFETY: no preconditions.
    unsafe { SDL_Delay(ms) }
}

pub fn base_path() -> Option<PathBuf> {
    // SAFETY: SDL_GetBasePath returns null or a string SDL owns.
    unsafe {
        let base = SDL_GetBasePath();
        if base.is_null() { None } else { Some(PathBuf::from(CStr::from_ptr(base).to_string_lossy().into_owned())) }
    }
}

#[derive(Clone, Copy)]
pub struct WindowHandle(pub *mut SDL_Window);

impl WindowHandle {
    pub fn set_size(self, w: c_int, h: c_int) {
        // SAFETY: the window is alive while the app runs.
        unsafe { SDL_SetWindowSize(self.0, w, h) };
    }

    pub fn minimize(self) {
        // SAFETY: as above.
        unsafe { SDL_MinimizeWindow(self.0) };
    }

    pub fn restore(self) {
        // SAFETY: as above.
        unsafe { SDL_RestoreWindow(self.0) };
    }

    pub fn refresh_rate(self) -> f64 {
        // SAFETY: as above; the mode is null or SDL's.
        unsafe {
            let mode = SDL_GetCurrentDisplayMode(SDL_GetDisplayForWindow(self.0));
            if mode.is_null() { 0.0 } else { f64::from((*mode).refresh_rate) }
        }
    }
}

impl render::Window for WindowHandle {
    fn instance_extensions(&self) -> Vec<CString> {
        let mut count = 0u32;
        // SAFETY: SDL returns `count` valid strings or null.
        unsafe {
            let names = SDL_Vulkan_GetInstanceExtensions(&mut count);
            if names.is_null() {
                return Vec::new();
            }
            (0..count as usize).map(|i| CStr::from_ptr(*names.add(i)).to_owned()).collect()
        }
    }

    fn create_surface(&self, _entry: &ash::Entry, instance: &ash::Instance) -> Result<vk::SurfaceKHR, String> {
        let mut surface = vk::SurfaceKHR::null();
        // SAFETY: a live window and a live instance created with SDL's extensions.
        if unsafe { SDL_Vulkan_CreateSurface(self.0, instance.handle(), std::ptr::null(), &mut surface) } { Ok(surface) } else { Err(error()) }
    }

    fn size_in_pixels(&self) -> (i32, i32) {
        let (mut w, mut h) = (0, 0);
        // SAFETY: a live window.
        unsafe { SDL_GetWindowSizeInPixels(self.0, &mut w, &mut h) };
        (w, h)
    }

    fn minimized(&self) -> bool {
        // SAFETY: a live window.
        unsafe { SDL_GetWindowFlags(self.0) & SDL_WINDOW_MINIMIZED != 0 }
    }
}

pub struct SdlAudio {
    stream: *mut SDL_AudioStream,
    resumed: bool,
}

// SAFETY: SDL audio streams are safe to use from any thread.
unsafe impl Send for SdlAudio {}

impl SdlAudio {
    pub fn open() -> Option<Box<dyn AudioDevice>> {
        // SAFETY: plain SDL calls; the stream is owned by the returned object.
        unsafe {
            if !SDL_InitSubSystem(SDL_INIT_AUDIO) {
                core_log::log!(Warn, Audio, "SDL audio init failed: {}", error());
                return None;
            }
            let spec = SDL_AudioSpec { format: SDL_AUDIO_S16, channels: 2, freq: audio::live_audio::RATE as c_int };
            let stream = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, None, std::ptr::null_mut());
            if stream.is_null() {
                core_log::log!(Warn, Audio, "no output device: {}", error());
                SDL_QuitSubSystem(SDL_INIT_AUDIO);
                return None;
            }
            Some(Box::new(SdlAudio { stream, resumed: false }))
        }
    }
}

impl AudioDevice for SdlAudio {
    fn queued_bytes(&self) -> i32 {
        // SAFETY: a live stream.
        unsafe { SDL_GetAudioStreamQueued(self.stream) }
    }

    fn put(&mut self, interleaved_stereo: &[i16]) {
        // SAFETY: a live stream and `interleaved_stereo.len() * 2` readable bytes.
        unsafe {
            SDL_PutAudioStreamData(self.stream, interleaved_stereo.as_ptr().cast(), (interleaved_stereo.len() * 2) as c_int);
            if !self.resumed {
                self.resumed = true;
                SDL_ResumeAudioStreamDevice(self.stream);
            }
        }
    }
}

impl Drop for SdlAudio {
    fn drop(&mut self) {
        // SAFETY: the stream is live and destroyed once.
        unsafe {
            SDL_DestroyAudioStream(self.stream);
            SDL_QuitSubSystem(SDL_INIT_AUDIO);
        }
    }
}
