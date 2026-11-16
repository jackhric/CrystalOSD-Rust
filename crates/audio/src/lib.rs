pub mod clock_sound;
pub mod data;
pub mod driver;
pub mod ee_sound_queue;
pub mod lab;
pub mod live_audio;
pub mod output;
pub mod spu2;
pub mod spu_write;

pub use assets::{Error, Result};

pub(crate) fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error(message.into()))
}
