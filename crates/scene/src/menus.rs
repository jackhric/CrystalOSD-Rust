use crate::clock_date::date_check;
use crate::console_clock::{local_of, rtc_of, zone_of};
use crate::menu_types::{ConfigEntry, ListFade, MenuExternals, MenuWorld, MenusState, VERSION_ROWS, pad};
use crate::ramp::{Ramp, tick_ramp};
use crate::sound::{PLACEHOLDER_A3, SoundCommand};

const STEP: i32 = 3000;
const ASPECT_CONFIRM: u32 = 0x00227d30;
const ADJUST_ENTER: u32 = 0x00226fd0;
const ADJUST_CONFIRM: u32 = 0x00227b90;
const ADJUST_CANCEL: u32 = 0x00227be8;
const VALUE_LIST: u32 = 0x00228660;
const EDITOR: u32 = 0x00227ad0;
const ADJUST_STRING: u32 = 0x00227420;
const ITEM_STRING: u32 = 0x00228470;
const CONFIG_ENTRIES: u32 = 0x002b2bf0;
const VERSION_JOB: i32 = 0x0020aad8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Enter,
    Confirm,
    Cancel,
}

const MENU_SOUND: u16 = 0x6300;

fn send(sounds: &mut Option<&mut Vec<SoundCommand>>, a2: u16, a3: u16, carried_a3: bool) {
    if let Some(sounds) = sounds {
        sounds.push(SoundCommand { id: MENU_SOUND, a1: 1, a2, a3, carried_a2: false, carried_a3 });
    }
}

fn send2(sounds: &mut Option<&mut Vec<SoundCommand>>, a2: u16) {
    send(sounds, a2, PLACEHOLDER_A3, false);
}

fn sprites_hide(w: &mut MenuWorld<'_>) {
    let r = &mut *w.sprite_fade;
    if r.state == 2 {
        Menus::hide(r);
    } else if r.state == 1 {
        r.state = 3;
    }
}

fn sprites_show(w: &mut MenuWorld<'_>) {
    let r = &mut *w.sprite_fade;
    if r.state == 0 {
        Menus::show(r);
    } else if r.state == 3 {
        r.state = 1;
    }
}

fn appearance(w: &mut MenuWorld<'_>, up: bool) {
    let ramp = &mut w.clock.appearance;
    if ramp.state != (if up { 0 } else { 2 }) {
        return;
    }
    if up {
        ramp.counter = 0;
        ramp.changed = 0;
        ramp.state = 0;
        Menus::show(ramp);
    } else {
        Menus::hide(ramp);
    }
    let state = &mut w.clock.state;
    for i in 0..12 {
        state.rods[((i + state.current_rod) % 12) as usize].appearance = 0.0;
    }
}

fn aspect_of(ext: &MenuExternals) -> i32 {
    let Some(param) = ext.mechacon_param else { panic!("mechaconParam is not in the inputs") };
    let ratio = ((param[0] >> 1) & 3) as i32;
    if ratio < 3 { ratio } else { 0 }
}

fn reload_item0(w: &mut MenuWorld<'_>, ext: &MenuExternals) {
    if w.menus.config_gate.is_some() && ext.mechacon_param.is_some() && w.menus.config_gate == Some(1) {
        w.items[0] = aspect_of(ext);
    }
}

fn dirty(ext: &MenuExternals) -> bool {
    let Some(d) = ext.config_dirty else { panic!("configDirty is not in the inputs") };
    d[0] != 0 || d[1] != 0 || d[2] != 0
}

fn set_gate(w: &mut MenuWorld<'_>, value: i32) {
    if w.menus.config_gate.is_some() {
        w.menus.config_gate = Some(value);
    }
}

fn entry_index(w: &MenuWorld<'_>) -> usize {
    let index = (i64::from(w.menus.page.entries) - i64::from(CONFIG_ENTRIES)) / 0x38 + i64::from(w.menus.page.selected);
    if !(0..=8).contains(&index) {
        panic!("the selected entry is outside the list");
    }
    index as usize
}

fn time_from_items(w: &mut MenuWorld<'_>) {
    w.clock.time.milliseconds = 0.0;
    w.clock.time.seconds = w.items[0xb];
    w.clock.time.minutes = w.items[0xa];
    w.clock.time.hours = w.items[9];
    w.clock.time_filled = 0;
}

fn time_from_clock(w: &mut MenuWorld<'_>, ext: &MenuExternals, notes: &mut Vec<String>) {
    let (Some(rtc), Some(param)) = (ext.rtc_mirror, ext.mechacon_param) else {
        notes.push("the console clock is not in the capture: the time after a cancel is not modelled".into());
        return;
    };
    let moved = local_of(&rtc, zone_of(param[0]));
    w.clock.time.milliseconds = 0.0;
    w.clock.time.seconds = moved[5];
    w.clock.time.minutes = moved[4];
    w.clock.time.hours = moved[3];
    w.clock.time_filled = 0;
}

fn order_fields(w: &mut MenuWorld<'_>, ext: &MenuExternals) {
    let Some(param) = ext.mechacon_param else { return };
    struct Field {
        item: i32,
        low: i32,
        high: i32,
    }
    const YEAR: Field = Field { item: 6, low: 2000, high: 2099 };
    const MONTH: Field = Field { item: 7, low: 1, high: 12 };
    const DAY: Field = Field { item: 8, low: 1, high: 31 };
    let order: [&Field; 3] = match param[1] & 3 {
        1 => [&MONTH, &DAY, &YEAR],
        2 => [&DAY, &MONTH, &YEAR],
        0 => [&YEAR, &MONTH, &DAY],
        _ => return,
    };
    for (k, field) in order.iter().enumerate() {
        w.menus.adjust_fields[k].item = field.item;
        w.menus.adjust_fields[k].lowest = field.low;
        w.menus.adjust_fields[k].highest = field.high;
    }
}

fn check_date(w: &mut MenuWorld<'_>, ext: &MenuExternals, value_count: i32) {
    let Some(param) = ext.mechacon_param else { return };
    let now = [w.items[6], w.items[7], w.items[8], w.items[9], w.items[10], w.items[11]];
    let check = date_check(&now, param[0]);
    w.items[6..12].copy_from_slice(&check.items);
    for k in 0..value_count {
        let field = &mut w.menus.adjust_fields[k as usize];
        if field.item < 6 || field.item > 11 {
            continue;
        }
        field.lowest = check.ranges[(field.item - 6) as usize][0];
        field.highest = check.ranges[(field.item - 6) as usize][1];
    }
}

fn adjustment_opens(w: &mut MenuWorld<'_>, ext: &MenuExternals, entry: &ConfigEntry) {
    order_fields(w, ext);
    w.clock.scale_target = 0.0;
    sprites_hide(w);
    w.items[0xb] = 0;
    time_from_items(w);
    check_date(w, ext, entry.value_count);
}

fn callback(w: &mut MenuWorld<'_>, ext: &mut MenuExternals, slot: Slot, notes: &mut Vec<String>) {
    let entry = w.menus.entries[entry_index(w)];
    let address = match slot {
        Slot::Enter => entry.enter,
        Slot::Confirm => entry.confirm,
        Slot::Cancel => entry.cancel,
    };
    match address {
        ASPECT_CONFIRM => {
            if ext.config_dirty.is_some()
                && w.items[0] != aspect_of(ext)
                && let Some(dirty) = ext.config_dirty.as_mut()
            {
                dirty[0] = 1;
            }
        }
        ADJUST_ENTER => adjustment_opens(w, ext, &entry),
        ADJUST_CONFIRM => {
            if let (Some(rtc_write), Some(param)) = (w.rtc_write.as_deref_mut(), ext.mechacon_param) {
                *rtc_write = Some(rtc_of(&[w.items[6], w.items[7], w.items[8], w.items[9], w.items[10], w.items[11]], zone_of(param[0])));
            }
            w.clock.scale_target = 1.0;
            sprites_show(w);
        }
        ADJUST_CANCEL => {
            time_from_clock(w, ext, notes);
            w.clock.scale_target = 1.0;
            sprites_show(w);
        }
        _ => {}
    }
}

fn adjustment_frame(w: &mut MenuWorld<'_>, ext: &MenuExternals) {
    let index = entry_index(w);
    let entry = w.menus.entries[index];
    if entry.frame_callback == VALUE_LIST {
        if w.clock.menu_ramp.state == 0 {
            if ext.pad.pressed & pad::LEFT != 0 {
                send(&mut w.sounds, 6, 0xFFFF, false);
            }
            if ext.pad.pressed & pad::RIGHT != 0 {
                send(&mut w.sounds, 6, 0xFFFF, false);
            }
        }
        return;
    }
    if entry.frame_callback != EDITOR {
        return;
    }
    if w.clock.menu_ramp.state == 0 {
        let pressed = ext.pad.pressed;
        let repeat = ext.pad.repeating;
        let count = entry.value_count;
        let mut cursor = entry.value_index;
        if pressed & pad::LEFT != 0 {
            send2(&mut w.sounds, 6);
            cursor -= 1;
            if cursor < 0 {
                cursor += count;
            }
        }
        if pressed & pad::RIGHT != 0 {
            send2(&mut w.sounds, 6);
            cursor += 1;
            if cursor == count {
                cursor = 0;
            }
        }
        w.menus.entries[index].value_index = cursor;
        let field = w.menus.adjust_fields[cursor as usize];
        let item = field.item as usize;
        if repeat & pad::UP != 0 {
            send2(&mut w.sounds, 6);
            let next = w.items[item].wrapping_add(1);
            w.items[item] = if next > field.highest { field.lowest } else { next };
            check_date(w, ext, count);
        }
        if repeat & pad::DOWN != 0 {
            if repeat & pad::UP == 0 {
                send2(&mut w.sounds, 6);
            }
            let next = w.items[item].wrapping_sub(1);
            w.items[item] = if next < field.lowest { field.highest } else { next };
            check_date(w, ext, count);
        }
    }
    time_from_items(w);
}

fn value_index(entry: &mut ConfigEntry, item: i32) {
    const VALUES: [i32; 3] = [0, 1, 2];
    let count = entry.value_count;
    if count <= 0 || (entry.value_index >= 0 && entry.value_index < 3 && item == VALUES[entry.value_index as usize]) {
        return;
    }
    entry.value_index = 0;
    for n in 0..count {
        if item == VALUES[n as usize] {
            entry.value_index = n;
            return;
        }
    }
}

fn string_callback(w: &mut MenuWorld<'_>, ext: &MenuExternals) {
    let index = entry_index(w);
    let entry = w.menus.entries[index];
    if entry.string_callback == ADJUST_STRING {
        order_fields(w, ext);
    } else if entry.string_callback == ITEM_STRING && entry.item == 0 {
        let item0 = w.items[0];
        value_index(&mut w.menus.entries[index], item0);
    }
}

fn list_move(w: &mut MenuWorld<'_>, direction: i32) {
    let pal = w.menus.video_mode == 2;
    let rate = if pal { 50 } else { w.menus.list_constants.rate as i32 };
    let half = rate.wrapping_add(((rate as u32) >> 31) as i32) >> 1;
    if half == 0 {
        panic!("the list's rate is zero");
    }
    let list = &mut w.cubes.list;
    let left = list.left.wrapping_add(direction.wrapping_mul(STEP));
    list.left = left;
    let speed = ((i64::from(left.wrapping_abs()) * 2) / i64::from(half)) as i32;
    list.speed = speed;
    list.slowing = speed / half;
}

fn clamp_tail(x: i32, tail: i32) -> i32 {
    if x < 0 { 0 } else { x.min(tail) }
}

fn list_alpha(w: &MenuWorld<'_>) -> i32 {
    let tail = w.clock.tail;
    if tail == 0 {
        panic!("the tail length is zero");
    }
    let a = (clamp_tail(w.menus.page.ramp.counter.wrapping_sub(w.menus.body.wrapping_add(w.menus.menu_lengths[0])), tail) << 7) / tail;
    ((i64::from(a) * i64::from(clamp_tail(tail.wrapping_sub(w.clock.menu_ramp.counter), tail))) / i64::from(tail)) as i32
}

fn menu_alpha(w: &MenuWorld<'_>) -> i32 {
    let tail = w.clock.tail;
    if tail == 0 {
        panic!("the tail length is zero");
    }
    (clamp_tail(w.clock.menu_ramp.counter.wrapping_sub(w.menus.body), tail) << 7) / tail
}

fn cubes_up(w: &mut MenuWorld<'_>) {
    Menus::show(&mut w.cubes.ramp);
}

fn crossfade_start(fade: &mut ListFade, selected: i32) {
    fade.second_index = selected;
    fade.second = fade.first;
    fade.first = 0;
}

fn list_input(w: &mut MenuWorld<'_>, ext: &mut MenuExternals, notes: &mut Vec<String>) {
    let pressed = ext.pad.pressed;
    let count = w.menus.page.count;
    if pressed & pad::UP != 0 {
        send2(&mut w.sounds, 5);
        list_move(w, 1);
        let selected = w.menus.page.selected;
        crossfade_start(&mut w.menus.list_fade, selected);
        let selected = selected - 1;
        w.menus.page.selected = if selected < 0 { count - 1 } else { selected };
    } else if pressed & pad::DOWN != 0 {
        send2(&mut w.sounds, 5);
        list_move(w, -1);
        let selected = w.menus.page.selected;
        crossfade_start(&mut w.menus.list_fade, selected);
        let selected = selected + 1;
        w.menus.page.selected = if selected < count { selected } else { 0 };
    } else if pressed & pad::CROSS != 0 {
        callback(w, ext, Slot::Enter, notes);
        send2(&mut w.sounds, 4);
        w.menus.page.level = 1;
        let gate = if w.menus.page.selected == 0 { -1 } else { 0 };
        set_gate(w, gate);
    } else if pressed & pad::SQUARE != 0 {
        send(&mut w.sounds, 2, 0x180, false);
        send(&mut w.sounds, 3, PLACEHOLDER_A3, true);
        Menus::show(&mut w.clock.menu_ramp);
    } else if pressed & pad::CIRCLE != 0 {
        send(&mut w.sounds, 0xA, 0x180, false);
        Menus::hide(&mut w.menus.page.ramp);
    } else if pressed & pad::TRIANGLE != 0 {
        notes.push("the Options dialog of System Configuration (triangle) is not modelled".into());
    }
}

fn entry_input(w: &mut MenuWorld<'_>, ext: &mut MenuExternals, notes: &mut Vec<String>) {
    let pressed = ext.pad.pressed;
    if pressed & pad::CROSS != 0 {
        let pulse = w.menus.list_constants.pulse;
        let list = &mut w.cubes.list;
        list.pulse = pulse;
        list.pulsed = list.position.wrapping_add(list.left) / STEP;
        callback(w, ext, Slot::Confirm, notes);
        send(&mut w.sounds, 4, 0x57, false);
        w.menus.page.glow = 0;
        w.menus.page.level = if dirty(ext) { 2 } else { 0 };
        set_gate(w, 1);
    } else if pressed & pad::CIRCLE != 0 {
        callback(w, ext, Slot::Cancel, notes);
        send(&mut w.sounds, 0xA, 9, false);
        w.menus.page.level = 0;
        set_gate(w, 1);
        reload_item0(w, ext);
        w.menus.page.glow = 0;
    }
}

fn config_page(w: &mut MenuWorld<'_>, ext: &mut MenuExternals, notes: &mut Vec<String>) {
    tick_ramp(&mut w.menus.page.ramp);
    let ramp = w.menus.page.ramp;
    let first = w.menus.menu_lengths[0];
    let third = w.menus.menu_lengths[2];
    let tail = w.clock.tail;
    let body = w.menus.body;
    if ramp.state == 1 {
        if ramp.counter == first {
            cubes_up(w);
        }
    } else if ramp.state == 3 {
        if ramp.length.wrapping_sub(ramp.counter) == tail {
            Menus::hide(&mut w.cubes.ramp);
        }
        if w.menus.dialog_ramp.state == 0 {
            if ramp.counter == first.wrapping_add(body) {
                Menus::show(&mut w.clock.vignette_ramp);
            } else if ramp.counter == first {
                Menus::hide(&mut w.head.grey_ramp);
            } else if ramp.counter == third {
                appearance(w, false);
            }
        }
    } else if ramp.state == 2 && w.menus.page.level == 2 && !dirty(ext) {
        w.menus.page.level = 0;
    }
    let n = ((i64::from(if w.menus.video_mode == 2 { 50 } else { 60 }) * 0x7aa8) / 60) as i32;
    let glow = w.menus.page.glow.wrapping_add(0x136);
    w.menus.page.glow = if glow < n { glow } else { glow - 2 * n };

    let fade = &mut w.menus.list_fade;
    fade.first = fade.first.wrapping_add(8).clamp(0, 128);
    fade.second = fade.second.wrapping_sub(8).clamp(0, 128);

    let ramp = w.menus.page.ramp;
    if ramp.state != 0 && w.clock.menu_ramp.state != 2 {
        if w.menus.page.level == 1 {
            adjustment_frame(w, ext);
        } else {
            string_callback(w, ext);
        }
    }

    let ramp = w.menus.page.ramp;
    if ramp.state == 1 {
        if list_alpha(w) != 0 && w.menus.entry_active == 0 {
            w.menus.entry_active = 1;
        }
    } else if ramp.state == 3 {
        if list_alpha(w) == 0 && w.menus.entry_active != 0 {
            w.menus.entry_active = 0;
        }
    } else if ramp.state == 2 {
        let menu = w.clock.menu_ramp;
        if menu.state == 1 {
            if menu_alpha(w) != 0 && w.menus.entry_active != 0 {
                w.menus.entry_active = 0;
            }
        } else if menu.state == 3 && menu_alpha(w) == 0 && w.menus.entry_active == 0 {
            w.menus.entry_active = 1;
        }
        if menu.state == 0 {
            if w.menus.page.level == 0 {
                list_input(w, ext, notes);
            } else if w.menus.page.level == 1 {
                entry_input(w, ext, notes);
            }
        }
    }
}

fn version_fraction(ramp: &Ramp, scale: i32) -> i32 {
    if ramp.length == 0 { 0 } else { (i64::from(ramp.counter) * i64::from(scale) / i64::from(ramp.length)) as i32 }
}

fn version_alpha(m: &MenusState) -> i32 {
    let product = version_fraction(&m.version_ramp, 0x80).wrapping_mul(0x80);
    (if product < 0 { product + 0x7f } else { product }) >> 7
}

fn version_fill(m: &mut MenusState) {
    let v = &mut m.version;
    v.selected = 0;
    v.first = 0;
    let mut count = 0i32;
    while count < VERSION_ROWS as i32 && (count as usize) < m.version_list.len() {
        let entry = &m.version_list[count as usize];
        if entry.id == 6 {
            v.selected = count;
        }
        v.rows[count as usize] = entry.clone();
        count += 1;
    }
    v.count = count;
    let low = v.selected - v.shown + 1;
    v.first = if v.first < low { low } else { v.first.min(v.selected) };
}

fn version_hints(m: &mut MenusState, mode: i32, left: i32, right: i32, third: i32) {
    let video_mode = m.video_mode;
    let out = &mut m.version.hints;
    out[0] = mode;
    if video_mode > 0 {
        out[2] = if left != 0x55 { left } else { 0x56 };
        out[1] = if right != 0x56 { right } else { 0x55 };
    } else {
        out[2] = right;
        out[1] = left;
    }
    out[3] = third;
}

fn nothing_else(w: &MenuWorld<'_>) -> bool {
    let m = &*w.menus;
    if m.page.ramp.state != 0 || m.version_ramp.state != 0 || m.dialog_ramp.state != 0 || m.first_run_ramp.state != 0 {
        return false;
    }
    m.page_pointers[4] == 0
}

fn start_system_configuration(w: &mut MenuWorld<'_>) {
    if w.menus.page.ramp.state != 0 {
        return;
    }
    w.menus.page.ramp.length = w.menus.menu_lengths[0].wrapping_add(w.menus.body).wrapping_add(w.clock.tail);
    Menus::show(&mut w.menus.page.ramp);
    Menus::hide(&mut w.clock.vignette_ramp);
    Menus::show(&mut w.head.grey_ramp);
    appearance(w, true);
}

fn main_menu(w: &mut MenuWorld<'_>, ext: &MenuExternals, browser_enters: bool) {
    tick_ramp(&mut w.menus.main_menu.ramp);
    let weight = w.clock.overlay_level;
    let mode = w.clock.mode;
    if nothing_else(w) {
        let ramp = &mut w.menus.main_menu.ramp;
        if ramp.state == 0 {
            if mode == 2 && weight == 0x80 - w.clock.tail {
                Menus::show(ramp);
            }
        } else if ramp.state == 2 && mode == 3 && weight == 0x80 {
            Menus::hide(ramp);
        }
    }
    if w.menus.main_menu.ramp.state != 2 || !nothing_else(w) || w.clock.scene.leaving != 0 {
        return;
    }
    let pressed = ext.pad.pressed;
    let menu = &mut w.menus.main_menu;
    if pressed & pad::UP != 0 {
        let s = menu.selected - 1;
        if s >= 0 {
            menu.selected = s;
            send(&mut w.sounds, 6, 0xF, false);
        }
    } else if pressed & pad::DOWN != 0 {
        let s = menu.selected + 1;
        if s < menu.count {
            menu.selected = s;
            send(&mut w.sounds, 6, 0xF, false);
        }
    } else if pressed & pad::CROSS != 0 {
        if menu.selected == 0 && mode == 0 {
            send(&mut w.sounds, 4, 0xF, false);
            if browser_enters {
                Menus::hide(&mut w.menus.main_menu.ramp);
            }
        } else if menu.selected == 1 {
            send(&mut w.sounds, 4, 4, false);
            start_system_configuration(w);
        }
    } else if pressed & pad::TRIANGLE != 0 {
        Menus::version_open(w.menus, w.sounds.as_deref_mut());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenusOptions {
    pub browser_enters: bool,
}

impl Default for MenusOptions {
    fn default() -> Self {
        Self { browser_enters: true }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Menus {
    options: MenusOptions,
}

impl Menus {
    pub fn new(options: MenusOptions) -> Self {
        Self { options }
    }

    pub fn options(&self) -> &MenusOptions {
        &self.options
    }

    pub fn version_open(m: &mut MenusState, sounds: Option<&mut Vec<SoundCommand>>) {
        if m.version_ramp.state != 0 {
            return;
        }
        let mut sounds = sounds;
        send(&mut sounds, 4, 0xF, false);
        m.version.job = VERSION_JOB;
        m.version.polls = 0;
        Self::show(&mut m.version_ramp);
    }

    pub fn version_step(m: &mut MenusState, pressed: u32, notes: &mut Vec<String>, sounds: Option<&mut Vec<SoundCommand>>) {
        let mut sounds = sounds;
        tick_ramp(&mut m.version_ramp);
        if m.version_ramp.state == 1 && m.version_ramp.counter == 1 {
            let done = m.version.polls >= 1;
            m.version.polls += 1;
            if done {
                version_fill(m);
            } else {
                m.version_ramp.counter = 0;
            }
        }
        if m.version_ramp.state == 0 && m.version_ramp.changed != 0 {
            m.version.panel_on = 0;
        }
        let alpha = version_alpha(m);
        if m.version_ramp.state != 0 {
            m.version.panel_on = 1;
            m.version.panel_alpha = alpha;
        }
        m.version_drawn = m.version.clone();
        m.version_ramp_drawn = m.version_ramp;
        m.version_drawn_valid = true;
        if m.version_ramp.state == 0 {
            return;
        }
        // func_0022A7A8: a3 of a move is 0x57 when the row selected at entry has sub-rows, else 1 (movz at 0x0022A820).
        let move_a3: u16 = if m.version.rows[m.version.selected.clamp(0, VERSION_ROWS as i32 - 1) as usize].sub_rows != 0 { 0x57 } else { 1 };
        version_hints(m, 1, 0x55, 1, i32::from(move_a3));
        let v = &mut m.version;
        let up = if v.selected == 0 { 0 } else { pad::UP as i32 };
        v.arrows = if v.selected + 1 < v.count { up | pad::DOWN as i32 } else { up };
        if m.version_ramp.state != 2 {
            return;
        }
        if pressed & pad::UP != 0 {
            let s = v.selected - 1;
            if s >= 0 {
                v.selected = s;
                send(&mut sounds, 6, move_a3, false);
            }
            v.first -= if v.selected < v.first { 1 } else { 0 };
        } else if pressed & pad::DOWN != 0 {
            let s = v.selected + 1;
            if s < v.count {
                v.selected = s;
                send(&mut sounds, 6, move_a3, false);
            }
            v.first += if v.selected < v.first + v.shown { 0 } else { 1 };
        } else if pressed & pad::TRIANGLE != 0 {
            if v.rows[v.selected.clamp(0, VERSION_ROWS as i32 - 1) as usize].sub_rows != 0 {
                notes.push("the front page of a Version row (func_00228F68) is not modelled".into());
            }
        } else if pressed & pad::CIRCLE != 0 {
            send2(&mut sounds, 0xA);
            Self::hide(&mut m.version_ramp);
        }
    }

    pub fn show(ramp: &mut Ramp) {
        if ramp.state == 0 {
            ramp.counter = 0;
            ramp.state = 1;
            ramp.changed = 1;
        }
    }

    pub fn hide(ramp: &mut Ramp) {
        if ramp.state == 2 {
            ramp.changed = 1;
            ramp.state = 3;
            ramp.counter = ramp.length;
        }
    }

    pub fn set_mode(w: &mut MenuWorld<'_>, mode: i32) {
        let fade = &mut w.head.fade;
        w.clock.mode = mode;
        fade.y1 = w.height << 4;
        fade.x1 = w.width << 4;
        if mode == 1 {
            w.clock.overlay_level = 0;
            fade.colour[0] = 0xff;
            fade.colour[1] = 0xff;
            fade.colour[2] = 0xff;
        } else if mode == 3 {
            fade.colour[0] = 0;
            fade.colour[1] = 0;
            fade.colour[2] = 0;
            w.clock.overlay_level = 0x80;
            Self::hide(&mut w.clock.vignette_ramp);
        } else if mode == 2 || mode == 4 {
            fade.colour[0] = 0;
            fade.colour[1] = 0;
            fade.colour[2] = 0;
            w.clock.overlay_level = 0;
        }
    }

    pub fn end_of_frame(w: &mut MenuWorld<'_>, ext: &MenuExternals) {
        if ext.config_dirty.is_some() && dirty(ext) {
            return;
        }
        reload_item0(w, ext);
    }

    pub fn menu_step(w: &mut MenuWorld<'_>, ext: &MenuExternals) {
        tick_ramp(&mut w.clock.menu_ramp);
        let ramp = &mut w.clock.menu_ramp;
        if ramp.state == 1 {
            if ramp.counter == w.clock.tail {
                Self::hide(&mut w.cubes.ramp);
            }
        } else if ramp.state == 2 && (ext.pad.pressed & pad::SQUARE) != 0 {
            send(&mut w.sounds, 0, 0x8000, false);
            send(&mut w.sounds, 1, PLACEHOLDER_A3, true);
            Self::hide(&mut w.clock.menu_ramp);
            Self::show(&mut w.cubes.ramp);
        }
    }

    pub fn step(&self, w: &mut MenuWorld<'_>, ext: &mut MenuExternals, notes: &mut Vec<String>) {
        if w.menus.dialog_ramp.state != 0 {
            notes.push("dialogRamp is not hidden: that page is not modelled".into());
        }
        if w.menus.first_run_ramp.state != 0 {
            notes.push("firstRunRamp is not hidden: that page is not modelled".into());
        }
        Self::version_step(w.menus, ext.pad.pressed, notes, w.sounds.as_deref_mut());
        config_page(w, ext, notes);
        main_menu(w, ext, self.options.browser_enters);
        tick_ramp(&mut w.menus.first_run_ramp);
        tick_ramp(&mut w.menus.dialog_ramp);
    }

    pub fn between(w: &mut MenuWorld<'_>, ext: &MenuExternals, notes: &mut Vec<String>) {
        if w.clock.scene.leaving != 0 && w.menus.first_run_ramp.state == 0 {
            if w.clock.mode == 0 {
                Self::set_mode(w, 3);
            }
            return;
        }
        if w.menus.screen_code == ext.disc {
            let ramp = w.menus.main_menu.ramp;
            if ramp.state == 3 && ramp.changed != 0 {
                if w.menus.screen_code != 0x74 {
                    w.menus.screen_code = 9999;
                }
                w.clock.scene.leaving = 1;
            }
        } else {
            notes.push("the disc state changed: the thread branches for a disc are not modelled".into());
        }
    }
}
