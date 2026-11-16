#![allow(dead_code)]

use super::Hash;
use scene::clock_state::{ClockState, Colour};
use scene::frame_head::{HeadState, Rect};
use scene::matrix::{Mat4, Vec4};
use scene::menu_types::{ConfigEntry, VersionPage, VersionRow};
use scene::ramp::Ramp;
use scene::rods::RodRecord;
use scene::text::{FontCache, FontState, TextRamps};

pub fn h4(h: &mut Hash, v: &Vec4) {
    for &x in v {
        h.f32(x);
    }
}
pub fn hm(h: &mut Hash, m: &Mat4) {
    for r in m {
        h4(h, r);
    }
}
pub fn hc(h: &mut Hash, c: &Colour) {
    for &x in c {
        h.i32(x);
    }
}
pub fn hramp(h: &mut Hash, r: &Ramp) {
    for x in [r.length, r.counter, r.changed, r.state] {
        h.i32(x);
    }
}
pub fn hbytes(h: &mut Hash, s: &[u8]) {
    h.u32(s.len() as u32);
    for &c in s {
        h.u32(c.into());
    }
}
pub fn hrect(h: &mut Hash, r: &Rect) {
    hc(h, &r.colour);
    for x in [r.x0, r.y0, r.u0, r.v0, r.x1, r.y1, r.u1, r.v1, r.z, r.blend, r.textured] {
        h.i32(x);
    }
}
pub fn hrecord(h: &mut Hash, r: &RodRecord) {
    h.i32(r.number);
    h.i32(r.faces);
    hm(h, &r.local);
    h.f32(r.sx);
    h.f32(r.sy);
    h.f32(r.sz);
    hc(h, &r.base);
    h.f32(r.strength);
    hc(h, &r.textured);
    h.f32(r.pair[0]);
    h.f32(r.pair[1]);
    h.f32(r.refraction);
    hc(h, &r.reflection);
    hc(h, &r.extra);
}
pub fn hclock(h: &mut Hash, s: &ClockState) {
    h.f32(s.time.milliseconds);
    h.i32(s.time.seconds);
    h.i32(s.time.minutes);
    h.i32(s.time.hours);
    h.i32(s.eased.second_hand.into());
    h.i32(s.eased.hour_hand.into());
    h.f32(s.eased.progress);
    h.f32(s.eased.fraction);
    h.i32(s.state.current_rod);
    h.i32(s.state.seconds_angle.into());
    h.i32(s.state.rod_angle.into());
    for r in &s.state.rods {
        h.f32(r.appearance);
        h.f32(r.progress);
        hc(h, &r.base);
        hc(h, &r.reflection);
    }
    hc(h, &s.state.accent);
    hc(h, &s.state.fourth);
    h.f32(s.state.progress_target);
    hc(h, &s.state.accent_frame);
    hc(h, &s.state.current_target);
    hramp(h, &s.appearance);
    for c in [&s.colours.base, &s.colours.reflection, &s.colours.fourth, &s.colours.current_reflection] {
        hc(h, c);
    }
    let cc = &s.cycle_counters;
    for x in [cc.base.index, cc.base.wait, cc.accent.index, cc.accent.wait, cc.last] {
        h.i32(x);
    }
    for c in s.cycle_tables.base.iter().chain(&s.cycle_tables.accent) {
        hc(h, c);
    }
    h.f32(s.logic_constants.rod_easing);
    h.f32(s.logic_constants.seconds_easing);
    h.f32(s.logic_constants.progress_step);
    h.f32(s.scene.scale);
    h.i32(s.scene.leaving);
    h.i32(s.scene.field);
    h.f32(s.scale_target);
    h.f32(s.scale_factor);
    h.i32(s.time_filled);
    h.boolean(s.spin.is_some());
    h.u32(u32::from(s.spin.unwrap_or(0)));
    h.i32(s.mode);
    h.i32(s.level);
    h.i32(s.overlay_level);
    hramp(h, &s.vignette_ramp);
    h.i32(s.vignette_length);
    hramp(h, &s.menu_ramp);
    h.i32(s.tail);
    h.i32(s.counter);
    h.f32(s.proportions.ax);
    h.f32(s.proportions.ay);
    for v in [&s.position, &s.direction, &s.up, &s.rotation] {
        h4(h, v);
    }
    h.f32(s.camera_offset);
    h.f32(s.zmax);
    h.f32(s.camera_factor);
}
pub fn hhead(h: &mut Hash, s: &HeadState) {
    hramp(h, &s.grey_ramp);
    for g in s.greys {
        h.i32(g);
    }
    hramp(h, &s.vignette_ramp);
    for x in [s.ring.alpha, s.ring.cx, s.ring.cy, s.ring.rx, s.ring.ry, s.ring.z] {
        h.i32(x);
    }
    for r in [&s.tint, &s.blur, &s.copy, &s.fade, &s.bars, &s.column] {
        hrect(h, r);
    }
}
pub fn hcache(h: &mut Hash, c: &FontCache) {
    h.u32(c.list.len() as u32);
    for e in &c.list {
        h.i32(e.code);
        h.i32(e.loaded);
        h.i32(e.cell);
        h.u32(e.block);
    }
    for x in [c.format, c.cell_w, c.cell_h, c.cells, c.width, c.height, c.log_w, c.log_h, c.set_up] {
        h.i32(x);
    }
    for x in [c.block, c.memory, c.texture, c.table] {
        h.u32(x);
    }
}
pub fn hstate(h: &mut Hash, s: &FontState) {
    for x in [s.line_height, s.fixed, s.percent, s.pitch, s.decoration, s.clip, s.blank, s.ascent, s.dirty] {
        h.i32(x);
    }
    h.f32(s.tv);
    h.f32(s.ratio);
    h.f32(s.locate[0]);
    h.f32(s.locate[1]);
    for c in s.colour {
        h.f32(c);
    }
    hm(h, &s.matrix);
}
pub fn hentry(h: &mut Hash, e: &ConfigEntry) {
    for x in [e.id, e.value_count, e.value_index, e.item] {
        h.i32(x);
    }
    for x in [e.value_table, e.enter, e.string_callback, e.frame_callback, e.confirm, e.cancel, e.focus] {
        h.u32(x);
    }
    for x in e.rest {
        h.i32(x);
    }
}
pub fn hrow(h: &mut Hash, r: &VersionRow) {
    h.i32(r.label);
    hbytes(h, &r.value);
    h.i32(r.sub_rows);
    h.i32(r.id);
}
pub fn hversion(h: &mut Hash, v: &VersionPage) {
    for x in [v.title, v.count, v.shown, v.selected, v.first] {
        h.i32(x);
    }
    for r in &v.rows {
        hrow(h, r);
    }
    for x in [v.job, v.panel_on, v.panel_alpha, v.arrows] {
        h.i32(x);
    }
    for x in v.hints {
        h.i32(x);
    }
    h.i32(v.polls);
}
pub fn hramps(h: &mut Hash, r: &TextRamps) {
    for x in [&r.config, &r.main_menu, &r.version, &r.dialog_closing, &r.first_run, &r.dialog] {
        hramp(h, x);
    }
    for x in [r.lead, r.body, r.panel7, r.panel8_on, r.panel8, r.adjust_row] {
        h.i32(x);
    }
    for x in r.panel8_ids {
        h.i32(x);
    }
}
