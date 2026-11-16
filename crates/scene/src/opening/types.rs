// facts/opening.md section 5.2: the play history, 21 entries of 22 bytes (a name, a count, a mask of the entry's cells, the main cell).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryEntry {
    pub name: [u8; 16],
    pub count: u8,
    pub mask: u8,
    pub main_cell: u8,
}

pub type History = [HistoryEntry; 21];

// The disc state at module counter 1 and from counter 2 on (no disc: 0x65, then 0x64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscSchedule {
    pub first: u32,
    pub later: u32,
}

impl Default for DiscSchedule {
    fn default() -> Self {
        Self { first: 0x65, later: 0x64 }
    }
}

// The intro is a closed system fed by these (the design's input table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootOptions {
    pub disc: DiscSchedule,
    pub lights_phase: Option<u32>,
    pub history: Option<History>,
    // Word 0x001F0648: 1 BootOpening, 4 BootIllegal / BootWarning.
    pub module: i32,
    pub clock_forced: bool,
    pub hdd_ready: bool,
    pub hdd_exec: i32,
    pub pal: bool,
    // False for a run that starts from a state saved inside the scene set-up (towers-whole): the first frame is drawn with the placed camera and the matrices
    // never computed (zero, measured in the tower probes); a cold boot's first frame holds one black fill sprite where every other frame holds two (measured, writer not read).
    pub cold_start: bool,
}

impl Default for BootOptions {
    fn default() -> Self {
        Self {
            disc: DiscSchedule::default(),
            lights_phase: None,
            history: None,
            module: 1,
            clock_forced: false,
            hdd_ready: false,
            hdd_exec: 0,
            pal: false,
            cold_start: true,
        }
    }
}

// facts/opening.md section 3: the sound commands the scene reports; the app drops them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SoundEvent {
    pub id: u32,
    pub argument: i32,
    pub counter: i32,
}

// facts/opening.md section 8: the module the hand-off starts, the execute-app type, whether the previous module was the opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandOff {
    pub module: i32,
    pub execute_app_type: i32,
    pub previous_was_opening: bool,
}

impl Default for HandOff {
    fn default() -> Self {
        Self { module: 2, execute_app_type: -1, previous_was_opening: true }
    }
}
