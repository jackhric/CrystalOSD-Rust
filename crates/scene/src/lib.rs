#![allow(clippy::neg_cmp_op_on_partial_ord, clippy::needless_range_loop)]

pub mod arithmetic;
pub mod browser;
pub mod camera;
pub mod clock;
pub mod clock_date;
pub mod clock_state;
pub mod cold_camera;
pub mod cold_config;
pub mod cold_font;
pub mod cold_ramps;
pub mod cold_start;
pub mod cold_state;
pub mod cold_time;
pub mod cold_types;
pub mod console_clock;
pub mod cubes;
pub mod elf_records;
pub mod font;
pub mod frame;
pub mod frame_head;
pub mod matrix;
pub mod menu_types;
pub mod menus;
pub mod opening;
pub mod orbs;
pub mod program_image;
pub mod ramp;
pub mod rod_emitters;
pub mod rods;
pub mod sound;
pub mod text;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
