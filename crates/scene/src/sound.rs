#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SoundCommand {
    pub id: u16,
    pub a1: u16,
    pub a2: u16,
    pub a3: u16,
    pub carried_a2: bool,
    pub carried_a3: bool,
}

pub const PLACEHOLDER_A3: u16 = 0;

// Frames from the thread start (6150,1 send) to the set-up pair's send: the EE compute of module_clock_init_resources (jal 0x00225E48) before the loop,
// 31 in hddosd-110U-sound-clock-boot and hddosd-110U-sound-ee-boot.
pub const CLOCK_INIT_FRAMES: u32 = 31;

// Frames from the opening's end (opening_transition_to_clock 0x0021AEE0, counter 248) to the thread start in the console's boot; the opening here ends two frames earlier
// than the console's hand-off call (hddosd-110U-sound-chain: hand-off and 6150,1 enqueue in emulator frame 375, 6140,0 start in 127).
pub const THREAD_START_LAG_FRAMES: u32 = 2;

// module_clock_thread_proc 0x00225D30, jal 0x00225D50.
pub fn clock_thread_start(out: &mut Vec<SoundCommand>) {
    out.push(SoundCommand { id: 0x6150, a1: 1, a2: 0, a3: 0, ..Default::default() });
}

// func_002324C8 (jal 0x00225E48 from module_clock_init_resources): queued when the word 0x002AD22C is nonzero; the second a2 and a3 are the ring temporaries.
pub fn clock_set_up(out: &mut Vec<SoundCommand>) {
    out.push(SoundCommand { id: 0x6150, a1: 6, a2: 0, a3: 0xF, ..Default::default() });
    out.push(SoundCommand { id: 0x6140, a1: 2, a2: 0, a3: 0, carried_a2: true, carried_a3: true });
}
