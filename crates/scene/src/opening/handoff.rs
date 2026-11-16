use crate::opening::types::HandOff;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HandOffInputs {
    pub snapshot: u32,
    pub clock_forced: bool,
    pub hdd_ready: bool,
    pub hdd_exec: i32,
    pub cdda: i32,
}

const OPENING_MODULE: i32 = 1;

pub const FRAMES_TO_CLOCK: i32 = 34;

// facts/opening.md section 8, verify_opening3_handoff.mjs verify() and byDisc()
pub fn decide(input: &HandOffInputs) -> HandOff {
    if input.clock_forced && !input.hdd_ready && input.hdd_exec != 1 {
        return HandOff { module: 2, execute_app_type: -1, previous_was_opening: true };
    }

    let mut out = HandOff { module: OPENING_MODULE, execute_app_type: -1, previous_was_opening: false };
    match input.snapshot {
        0x6A | 0x6B => out.execute_app_type = 2,
        0x6C | 0x6D => out.execute_app_type = 1,
        0x6E => out.execute_app_type = 0,
        0x6F => out.execute_app_type = 5,
        0x70 => out.execute_app_type = 4,
        0x72 => out.module = if input.cdda > 0 { 5 } else { 2 },
        0x73 => out.execute_app_type = 3,
        0x74 => out.module = 4,
        _ => out.module = 2,
    }
    if input.hdd_ready && input.hdd_exec == 1 {
        out = HandOff { module: 0, execute_app_type: 6, previous_was_opening: true };
    }
    if out.execute_app_type == -1 {
        out.previous_was_opening = true;
    }
    out
}
