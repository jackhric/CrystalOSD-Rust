// facts/browser-shell.md section 7: the Browser's sound commands (HDD OSD 1.10U). Every one is (0x6300, 1, a2) through
// sound_handle_queue_cmd_auto 0x0024E090 into the enqueue 0x00200C00; a3 is never loaded at these sites (the native port sends
// 0), except the second call of the 7 / 8 pair, which keeps the ring temporary the enqueue left (carried).

use super::devices::{CROSS, DOWN, LEFT, RIGHT, TRIANGLE, UP};
use crate::sound::SoundCommand;

pub const BROWSER_SOUND: u16 = 0x6300;
const CIRCLE: u32 = 0x40;
// 0x00254F84: a held direction or Circle (D_003707A0 & 0xF040) stops Cross and Triangle.
const HELD_STOPS: u32 = UP | DOWN | LEFT | RIGHT | CIRCLE;
// 0x002550A0: an item whose +0x90 has bit 5 or bit 16 takes the Triangle path.
const TRIANGLE_ITEM_BITS: u32 = 0x10020;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sent {
    pub a2: u16,
    pub ra: u32,
    pub ring: bool,
}

pub fn command(sent: &Sent) -> SoundCommand {
    SoundCommand { id: BROWSER_SOUND, a1: 1, a2: sent.a2, a3: 0, carried_a2: false, carried_a3: sent.ring }
}

// sound_handle_queue_cmd_auto 0x0024E090: queued when D_00370740 != 0xA, or the view mode D_003706F8 is 3, or it is 2 with
// D_00370460 == 2.
pub fn gate(device: i32, view: i32, word460: i32) -> bool {
    device != 0xa || view == 3 || (view == 2 && word460 == 2)
}

// The move flags func_00254840 leaves on the stack at 0x00254C90 (left sp+8, right sp+0xC, up sp+0, down sp+4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MoveFlags {
    pub left: i32,
    pub right: i32,
    pub up: i32,
    pub down: i32,
}

// The words func_00254840 reads for its senders. `None` is a word the path did not read: reaching it is an error.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RowWords {
    pub dialog_word: i32,
    pub page_in: i32,
    pub pal: i32,
    pub hold: i32,
    pub slide: i32,
    pub slide_b: i32,
    pub back: i32,
    pub enter: i32,
    pub options: i32,
    pub held: u32,
    pub edge: u32,
    pub repeat: u32,
    pub mode: i32,
    pub page_kind: Option<i32>,
    pub page_flags: Option<u32>,
    pub popped_kind: Option<i32>,
    pub count: Option<i32>,
    pub word600: Option<i32>,
    pub word604: Option<i32>,
    pub flags: Option<MoveFlags>,
    pub item90: Option<u32>,
    pub item_b0: Option<i32>,
    pub item690: Option<i32>,
    pub predicate: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowError {
    NotProbed(&'static str),
    NotReached(&'static str),
}

fn need<T>(value: Option<T>, name: &'static str) -> Result<T, RowError> {
    value.ok_or(RowError::NotProbed(name))
}

fn send(a2: u16, ra: u32) -> Sent {
    Sent { a2, ra, ring: false }
}

// func_00254840 (view mode 1), once per pass. Test order: the gates, Circle (a pushed page pops: 0xA), the Triangle case of
// D_003706D0 == 2, the count gate, Left / Right / Up / Down on the repeat word (6), the held mask, Cross (7 then 8), Triangle (4).
pub fn row(s: &RowWords, ntsc_page_in: i32, pal_page_in: i32) -> Result<Vec<Sent>, RowError> {
    if s.dialog_word != 0 {
        return Ok(vec![]);
    }
    if s.page_in != if s.pal != 0 { pal_page_in } else { ntsc_page_in } {
        return Ok(vec![]);
    }
    if s.hold == 1 || s.slide != 0 || s.slide_b != 0 {
        return Ok(vec![]);
    }
    if s.back != 0 && s.edge & CIRCLE != 0 {
        if need(s.page_kind, "page kind")? == 0 {
            return Ok(vec![]);
        }
        if need(s.page_flags, "page flags")? & 4 == 4 {
            return Ok(vec![]);
        }
        return Ok(if need(s.popped_kind, "popped kind")? == 0xa { vec![] } else { vec![send(0xa, 0x0025_4950)] });
    }
    let triangle = s.options != 0 && s.edge & TRIANGLE != 0;
    if triangle && s.mode == 2 {
        return Err(RowError::NotReached("Triangle with D_003706D0 == 2"));
    }
    if need(s.count, "count")? == 0 || need(s.word600, "D_00370600")? == 0 || need(s.word604, "D_00370604")? != 0 {
        return Ok(vec![]);
    }
    let f = need(s.flags, "move flags")?;
    if s.repeat & LEFT != 0 {
        return Ok(if f.left != 0 { vec![send(6, 0x0025_4f54)] } else { vec![] });
    }
    if s.repeat & RIGHT != 0 {
        return Ok(if f.right != 0 { vec![send(6, 0x0025_4f54)] } else { vec![] });
    }
    if s.repeat & UP != 0 {
        return Ok(if f.up != 0 { vec![send(6, 0x0025_4f54)] } else { vec![] });
    }
    if s.repeat & DOWN != 0 {
        if f.down == 0 {
            return Ok(vec![]);
        }
        return Ok(vec![if f.down != 2 && s.mode != 2 { send(6, 0x0025_4f7c) } else { send(6, 0x0025_4f54) }]);
    }
    if s.held & HELD_STOPS != 0 {
        return Ok(vec![]);
    }
    if s.enter != 0 && s.edge & CROSS != 0 {
        if need(s.item90, "item +0x90")? & 2 == 0 {
            return Err(RowError::NotReached("Cross on an item without +0x90 bit 1"));
        }
        if need(s.predicate, "func_002547C0 return")? == 0 || need(s.item_b0, "item +0xB0")? == 0 {
            return Ok(vec![]);
        }
        return Ok(vec![send(7, 0x0025_4fec), Sent { a2: 8, ra: 0x0025_4ffc, ring: true }]);
    }
    if triangle {
        let b0 = need(s.item_b0, "item +0xB0")?;
        let sound = b0 == 0 || need(s.item690, "item +0x690")? == 1 || need(s.item90, "item +0x90")? & TRIANGLE_ITEM_BITS != 0 || b0 == 0xe;
        return Ok(if sound { vec![send(4, 0x0025_50dc)] } else { vec![] });
    }
    Ok(vec![])
}

// The Format dialog's sound sites (func_00255C18): Left / Right 6 (ra 0x00255CC0 / 0x00255CF4), Cross 4 (0x00255D14), Circle 0xA
// (0x00255D6C); 9 and 0xB from the state-3 handler (0x00255DB4, 0x00255DF8; read, not reached by a capture).
pub fn format_ra(a2: u16, edge: u32) -> u32 {
    match a2 {
        6 if edge & LEFT != 0 => 0x0025_5cc0,
        6 => 0x0025_5cf4,
        4 => 0x0025_5d14,
        0xa => 0x0025_5d6c,
        9 => 0x0025_5db4,
        _ => 0x0025_5df8,
    }
}

// module_browser_24EB38 0x0024EB7C: D_001F0114 = 0x14 when the S/PDIF mode (var_mechacon_config_param_1 & 1) is 0, else 0x15;
// the first drain pass after it sends sceSdRemote(1, 0x8070, 0xA, a3) with a3 0 for 0x14 and 2 for 0x15.
pub fn spdif_store(mechacon_param: u32) -> u32 {
    if mechacon_param & 1 == 0 { 0x14 } else { 0x15 }
}

pub fn remote_a3(word: u32) -> Option<u16> {
    match word {
        0x14 => Some(0),
        0x15 => Some(2),
        _ => None,
    }
}
