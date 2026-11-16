// facts/browser-devices.md section 8: the Format dialog of a card slot that reports no format (HDD OSD 1.10U), the logic
// 0x00255C18 (state D_00370BC0, answer byte D_00370BBC: 0 No, 1 Yes) and what the render 0x00255868 draws in each state.

use super::devices::CROSS;

const LEFT: u32 = 0x8000;
const RIGHT: u32 = 0x2000;
const CIRCLE: u32 = 0x40;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatStep {
    pub state: i32,
    pub cursor: i32,
    pub dialog: i32,
    pub sounds: Vec<u16>,
    pub job: bool,
    pub rescan: bool,
    pub clear_bit: bool,
}

// State 1 stores answer 0 and state 2 and runs on; in state 2 Left on No and Right on Yes swap the answer (6), Cross sends 4 and
// starts the format job on Yes (state 3) or closes on No, Circle sends 0xA and closes. State 3 waits for the busy word, then the
// rescan flag, sound 9 and state 4 on result 0 (0xB and state 5 otherwise: read only). States 4 and 5 close on Circle, silent.
pub fn format_step(state: i32, cursor: i32, edge: u32, busy: i32, result: i32) -> FormatStep {
    let mut out = FormatStep { state, cursor, dialog: 1, ..FormatStep::default() };
    let mut state = state;
    if state == 1 {
        out.cursor = 0;
        out.state = 2;
        state = 2;
    }
    if state == 2 {
        if edge & LEFT != 0 && out.cursor == 0 {
            out.sounds.push(6);
            out.cursor = 1;
        }
        if edge & RIGHT != 0 && out.cursor == 1 {
            out.sounds.push(6);
            out.cursor = 0;
        }
        if edge & CROSS != 0 {
            out.sounds.push(4);
            if out.cursor == 1 {
                out.job = true;
                out.state = 3;
            } else {
                out.state = 0;
                out.dialog = 0;
            }
        } else if edge & CIRCLE != 0 {
            out.sounds.push(0xa);
            out.state = 0;
            out.dialog = 0;
        }
        return out;
    }
    if state == 3 {
        if busy != 0 {
            return out;
        }
        out.rescan = true;
        if result == 0 {
            out.sounds.push(9);
            out.state = 4;
            out.clear_bit = true;
        } else {
            out.sounds.push(0xb);
            out.state = 5;
        }
        return out;
    }
    if (state == 4 || state == 5) && edge & CIRCLE != 0 {
        out.state = 0;
        out.dialog = 0;
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatDraw {
    pub ids: Vec<i32>,
    pub heights: Vec<i32>,
    pub colours: Option<[i32; 2]>,
}

// 0x00255868: string ids, line heights, and the colours of Yes and No (format 0x00366490, colour 2 on the answer, 4 on the other).
pub fn format_draw(state: i32, cursor: i32) -> Option<FormatDraw> {
    match state {
        1 | 2 => {
            Some(FormatDraw { ids: vec![0x4a, 0x4b, 0x31, 0x32], heights: vec![0x59, 0x6a, 0x80], colours: Some(if cursor == 1 { [2, 4] } else { [4, 2] }) })
        }
        3 => Some(FormatDraw { ids: vec![0x4c, 0x4d], heights: vec![0x5f, 0x70], colours: None }),
        4 => Some(FormatDraw { ids: vec![0x4e], heights: vec![0x68], colours: None }),
        5 => Some(FormatDraw { ids: vec![0x4f], heights: vec![0x68], colours: None }),
        _ => None,
    }
}
