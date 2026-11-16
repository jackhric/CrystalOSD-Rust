use crate::clock_state::Proportions;
use crate::elf_records::rect_at;
use crate::frame_head::Rect;
use crate::program_image::ProgramImage;

const FADE_RECORD: u32 = 0x2B5F30;
const PROPORTION_PAL: u32 = 0x36FB84;
const PROPORTION_NTSC: u32 = 0x36FB88;
const CAMERA_OFFSET_BITS: u32 = 0xC2C80000;
const UNIT: f32 = 1.0;
const MODE: i32 = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ColdCameraOut {
    pub scale: f32,
    pub proportions: Proportions,
    pub camera_offset: f32,
    pub mode: i32,
    pub overlay_level: i32,
    pub fade: Rect,
    pub leaving: bool,
    pub field: i32,
}

// HDD OSD 1.10U 0x225D98, 0x225950, 0x234C28(2), 0x225DB0, 0x2259B8 verify_cold_camera.mjs
pub fn cold_camera(program: &ProgramImage, pal: bool, screen_width: u32, screen_height: u32, even_odd_field: u32) -> ColdCameraOut {
    let mut out = ColdCameraOut {
        scale: UNIT,
        proportions: Proportions { ax: UNIT, ay: program.single(if pal { PROPORTION_PAL } else { PROPORTION_NTSC }) },
        camera_offset: f32::from_bits(CAMERA_OFFSET_BITS),
        mode: MODE,
        overlay_level: 0,
        fade: rect_at(program, FADE_RECORD),
        leaving: false,
        field: even_odd_field as i32,
    };
    out.fade.colour[0] = 0;
    out.fade.colour[1] = 0;
    out.fade.colour[2] = 0;
    out.fade.x1 = (screen_width << 4) as i32;
    out.fade.y1 = (screen_height << 4) as i32;
    out
}
