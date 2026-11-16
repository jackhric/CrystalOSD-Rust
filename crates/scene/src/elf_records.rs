use crate::clock_state::Colour;
use crate::frame_head::Rect;
use crate::matrix::{Mat4, Vec4};
use crate::program_image::ProgramImage;
use crate::ramp::Ramp;
use crate::rods::RodRecord;

pub fn colour_at(p: &ProgramImage, a: u32) -> Colour {
    [p.integer(a), p.integer(a + 4), p.integer(a + 8), p.integer(a + 12)]
}

pub fn ramp_at(p: &ProgramImage, a: u32) -> Ramp {
    Ramp { length: p.integer(a), counter: p.integer(a + 4), changed: p.integer(a + 8), state: p.integer(a + 12) }
}

pub fn vec4_at(p: &ProgramImage, a: u32) -> Vec4 {
    [p.single(a), p.single(a + 4), p.single(a + 8), p.single(a + 12)]
}

pub fn mat4_at(p: &ProgramImage, a: u32) -> Mat4 {
    [vec4_at(p, a), vec4_at(p, a + 16), vec4_at(p, a + 32), vec4_at(p, a + 48)]
}

pub fn rect_at(p: &ProgramImage, a: u32) -> Rect {
    Rect {
        colour: colour_at(p, a),
        x0: p.integer(a + 0x10),
        y0: p.integer(a + 0x14),
        u0: p.integer(a + 0x18),
        v0: p.integer(a + 0x1C),
        x1: p.integer(a + 0x20),
        y1: p.integer(a + 0x24),
        u1: p.integer(a + 0x28),
        v1: p.integer(a + 0x2C),
        z: p.integer(a + 0x30),
        blend: p.integer(a + 0x34),
        textured: p.integer(a + 0x38),
    }
}

pub fn rod_record_at(p: &ProgramImage, a: u32) -> RodRecord {
    RodRecord {
        number: p.integer(a),
        faces: p.integer(a + 4),
        local: mat4_at(p, a + 0x20),
        sx: p.single(a + 0x68),
        sy: p.single(a + 0x6C),
        sz: p.single(a + 0x70),
        base: colour_at(p, a + 0x80),
        strength: p.single(a + 0x90),
        textured: colour_at(p, a + 0xA0),
        pair: [p.single(a + 0xB0), p.single(a + 0xB4)],
        refraction: p.single(a + 0xB8),
        reflection: colour_at(p, a + 0xC0),
        extra: colour_at(p, a + 0xD0),
    }
}
