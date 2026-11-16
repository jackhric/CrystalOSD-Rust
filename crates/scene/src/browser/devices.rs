// facts/browser-devices.md: the memory card scan, the root page's items, the card page header and status line, the cursor.
// HDD OSD 1.10U addresses; ROM 2.30 where the page maps them.

use std::marker::PhantomData;

use crate::arithmetic::Arithmetic;

pub const LEFT: u32 = 0x8000;
pub const RIGHT: u32 = 0x2000;
pub const UP: u32 = 0x1000;
pub const DOWN: u32 = 0x4000;
pub const CROSS: u32 = 0x20;
pub const TRIANGLE: u32 = 0x10;
const EDGE_KEYS: u32 = 0x70;

// sceMcGetInfo's view of a slot: type 2 a PS2 card, 0 none; `free` the library's free count; `format` nonzero when formatted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlotInfo {
    pub kind: i32,
    pub free: i32,
    pub format: i32,
    pub multitap: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlotWords {
    pub kind: i32,
    pub kilobytes: i32,
    pub formatted: i32,
}

// isfirstslot 0x002493A8 (ROM 0x00243368): type 2 gives max(free - 2, 0); type 0 with free 0 gives 0. Other types: read only.
pub fn is_first_slot(kind: i32, free: i32) -> Option<i32> {
    match (kind, free) {
        (2, _) => Some(free.wrapping_sub(2).max(0)),
        (0, 0) => Some(0),
        _ => None,
    }
}

// The scan loop 0x0024B940 (ROM 0x00245250), stores 0x0024BAE0 / 0x0024BB00: type word, kilobytes and (format != 0) of a slot.
pub fn scan_store(slot: &SlotInfo) -> Option<SlotWords> {
    let kilobytes = is_first_slot(slot.kind, slot.free)?;
    if slot.kind != 0 && slot.multitap == 1 {
        return None;
    }
    Some(SlotWords { kind: slot.kind, kilobytes, formatted: i32::from(slot.format != 0) })
}

// 0x00253D38 (ROM 0x0024D8B0): the string ids concatenated for device `dev` (0 the root, 1 the HDD, 2..9 the card slots).
pub fn title_ids(dev: i32, kind: i32, rom: bool) -> Option<Vec<i32>> {
    match dev {
        2..=9 if kind == 2 => Some(vec![if rom { 0x95 } else { 0x8a }, 7 + ((dev - 2) >> 2)]),
        0 => Some(vec![6]),
        1 if !rom => Some(vec![0x1a1]),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageLayout {
    pub cols: i32,
    pub rows: i32,
    pub f0: f32,
    pub f4: f32,
    pub d8: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemPosition {
    pub x: f32,
    pub y: f32,
}

pub struct Layout<A: Arithmetic> {
    _a: PhantomData<A>,
}

impl<A: Arithmetic> Layout<A> {
    // 0x002551B0 (ROM 0x0024F2E8), verified for n = count - first below 5 and no scroll: cols 2 when n >= 2 else 1,
    // rows max(ceil(n / cols), 1), f0 = (cols * 24 + 6) / 2, f4 = (rows * 24 + 4) / 2, d8 = (rows - 1) * 40 + 160.
    pub fn page(count: i32, first: i32, scroll_top: i32) -> Option<PageLayout> {
        let n = count - first;
        if scroll_top != 0 || n >= 5 {
            return None;
        }
        let cols = if n >= 2 { 2 } else { 1 };
        let rows = ((n + cols - 1) / cols).max(1);
        Some(PageLayout {
            cols,
            rows,
            f0: A::mul(A::add(A::mul(A::from_int(cols), 24.0), 6.0), 0.5),
            f4: A::mul(A::add(A::mul(A::from_int(rows), 24.0), 4.0), 0.5),
            d8: A::add(A::mul(A::from_int(rows - 1), 40.0), 160.0),
        })
    }

    // 0x00257860 (ROM 0x00252558): x = ((-f0 + 3) + 12) + (i % cols) * 24, y = ((-f4 + 2) + 12) + (i / cols) * 24.
    pub fn item(index: i32, cols: i32, f0: f32, f4: f32) -> ItemPosition {
        let col = index % cols;
        let row = index / cols;
        ItemPosition {
            x: A::add(A::mul(A::from_int(col), 24.0), A::add(A::add(-f0, 3.0), 12.0)),
            y: A::add(A::mul(A::from_int(row), 24.0), A::add(A::add(-f4, 2.0), 12.0)),
        }
    }

    // 0x00259AB8..0x00259B34: the selection phase plus the step word 0x0036FF14 (ROM 0x002C8518); the wrap at 2^20 is not verified.
    pub fn next_phase(phase: f32, step: f32) -> Option<f32> {
        let sum = A::add(phase, step);
        if sum >= 1_048_576.0 { None } else { Some(sum) }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorState {
    pub cursor: i32,
    pub first: i32,
    pub dialog: i32,
    pub page_in: i32,
    pub hold: i32,
    pub slide: i32,
    pub slide_b: i32,
    pub pal: i32,
    pub mode: i32,
    pub device: i32,
    pub list_count: i32,
    pub busy: i32,
    pub edge: u32,
    pub repeat: u32,
    pub record_count: i32,
    pub scroll_top: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorStep {
    pub cursor: i32,
    pub modelled: bool,
    pub sound: bool,
}

// 0x00254840 (ROM 0x0024E440), Left and Right on an NTSC page without scrolling: Left moves to cursor - 1 unless the cursor is the
// first item (the root page stays), Right to cursor + 1 unless it is at min(recCount, listCount) - 1; a move sends (0x6300, 1, 6).
pub fn cursor_step(s: &CursorState) -> CursorStep {
    let same = CursorStep { cursor: s.cursor, modelled: true, sound: false };
    let unmodelled = CursorStep { modelled: false, ..same };
    if s.pal != 0 {
        return unmodelled;
    }
    if s.dialog != 0 || s.page_in != 0x1a || s.hold == 1 || s.slide != 0 || s.slide_b != 0 {
        return same;
    }
    if s.edge & EDGE_KEYS != 0 {
        return unmodelled;
    }
    if s.record_count == 0 || s.list_count == 0 || s.busy != 0 {
        return same;
    }
    if s.mode == 2 || s.scroll_top != 0 || s.repeat & (UP | DOWN) != 0 {
        return unmodelled;
    }
    let last = s.record_count.min(s.list_count) - 1;
    if s.repeat & LEFT != 0 {
        if s.cursor == s.first {
            return if s.device == 0 { same } else { unmodelled };
        }
        return CursorStep { cursor: s.cursor - 1, modelled: true, sound: true };
    }
    if s.repeat & RIGHT != 0 {
        if s.cursor >= last {
            return same;
        }
        return CursorStep { cursor: s.cursor + 1, modelled: true, sound: true };
    }
    same
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusLine {
    pub id: i32,
    pub alpha: i32,
}

// 0x002516F0 (call 0x00251890), NTSC: id 0x51 "Unformatted" when the device is not 0 and the page flags have bit 3, else 0x50
// "No data."; alpha trunc(trunc(100 * empty / 60) * (pageIn - 10) / 16), `empty` the fade word 0x00370764.
pub fn status_line(flags: i32, device: i32, empty: i32, page_in: i32, pal: i32, mode: i32, slide: i32) -> Option<StatusLine> {
    if pal != 0 || (mode == 2 && slide > 0) {
        return None;
    }
    let id = if device != 0 && flags & 8 != 0 { 0x51 } else { 0x50 };
    let ramp = (100 * empty) / 60;
    Some(StatusLine { id, alpha: (ramp * (page_in - 10)) / (0x1a - 10) })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreeText {
    pub id: i32,
    pub args: [Option<i32>; 2],
    pub exit: u32,
}

// 0x00253FA0: under 1000 KB id 0x2E (kb), exit 0x002540B8; 1000..9999 id 0x2F (kb / 1000, kb % 1000), exit 0x00254090
// ("7,998 KB Free"). 10000 and above (MB, ids 0x1C0 / 0x1C1) is read only.
pub fn free_text(kb: i32) -> Option<FreeText> {
    if kb >= 10000 {
        None
    } else if kb >= 1000 {
        Some(FreeText { id: 0x2f, args: [Some(kb / 1000), Some(kb % 1000)], exit: 0x0025_4090 })
    } else {
        Some(FreeText { id: 0x2e, args: [Some(kb), None], exit: 0x0025_40b8 })
    }
}

// The header code 0x00251330..0x00251360: a card slot whose format word is 0 draws id 0x2D "Unformatted", else its free text.
pub fn header(slot: &SlotWords) -> Result<i32, Option<FreeText>> {
    if slot.formatted == 0 { Ok(0x2d) } else { Err(free_text(slot.kilobytes)) }
}
