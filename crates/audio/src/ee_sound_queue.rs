#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SoundCommand {
    pub id: u16,
    pub a1: u16,
    pub a2: u16,
    pub a3: u16,
}

const DEDUP_ID: u16 = 0x6300;
const RAMP_ID: u16 = 0x60D0;

#[derive(Debug, Clone)]
pub struct EeSoundQueue {
    slots: [SoundCommand; EeSoundQueue::ENTRIES],
    head: u32,
    tail: u32,
    ramp_counter: i32,
    sends_ramp: bool,
    carried_a2: u16,
    carried_a3: u16,
}

impl Default for EeSoundQueue {
    fn default() -> Self {
        Self::new(0, true)
    }
}

impl EeSoundQueue {
    pub const ENTRIES: usize = 128;
    pub const RAMP_STEPS: i32 = 15;

    pub fn new(ramp_counter: i32, sends_ramp: bool) -> Self {
        Self { slots: [SoundCommand::default(); Self::ENTRIES], head: 0, tail: 0, ramp_counter, sends_ramp, carried_a2: 0, carried_a3: 0 }
    }

    pub fn with_ramp_counter(ramp_counter: i32) -> Self {
        Self::new(ramp_counter, true)
    }

    pub fn carried_a2(&self) -> u16 {
        self.carried_a2
    }

    pub fn carried_a3(&self) -> u16 {
        self.carried_a3
    }

    fn push(&mut self, command: SoundCommand) -> bool {
        if self.head.wrapping_sub(self.tail) >= Self::ENTRIES as u32 {
            return false;
        }
        self.slots[(self.head & (Self::ENTRIES as u32 - 1)) as usize] = command;
        self.head = self.head.wrapping_add(1);
        true
    }

    // sound_handler_queue_cmd (HDD 0x00200C00): drop at head - tail >= 128; a3 and a2 are left as 0x1F0658 + 8s and 0x1F065C + 8s (0x00200CB4, 0x00200CC0).
    pub fn enqueue(&mut self, command: SoundCommand) -> bool {
        let s = self.head & (Self::ENTRIES as u32 - 1);
        if !self.push(command) {
            return false;
        }
        self.carried_a3 = (0x0658 + 8 * s) as u16;
        self.carried_a2 = (0x065C + 8 * s) as u16;
        true
    }

    // sound_handler_exec_queue_cmd (HDD 0x00200A80): dedup of equal-a2 0x6300 (0x00200B2C..0x00200B5C), tail = head, then sound_handler_2009E0(15) in the pad/sound thread, whose registers are not the sender's.
    pub fn drain(&mut self) -> Vec<SoundCommand> {
        let mut sends = Vec::new();
        let head = self.head;
        let mut seen = false;
        let mut previous = 0u16;
        let mut at = self.tail;
        while at != head {
            let slot = &mut self.slots[(at & (Self::ENTRIES as u32 - 1)) as usize];
            at = at.wrapping_add(1);
            if slot.id == 0 {
                continue;
            }
            let mut send = true;
            if slot.id == DEDUP_ID && seen && previous == slot.a2 {
                send = false;
            } else {
                previous = slot.a2;
            }
            seen = true;
            if send && (self.sends_ramp || slot.id != RAMP_ID) {
                sends.push(*slot);
            }
            *slot = SoundCommand::default();
        }
        self.tail = head;
        if self.ramp_counter < Self::RAMP_STEPS {
            let v = ((0x9FFEC * i64::from(self.ramp_counter) / 0x7F) / i64::from(Self::RAMP_STEPS)) as u32;
            self.push(SoundCommand { id: RAMP_ID, a1: 1, a2: v as u16, a3: v as u16 });
            self.ramp_counter += 1;
        }
        sends
    }
}
