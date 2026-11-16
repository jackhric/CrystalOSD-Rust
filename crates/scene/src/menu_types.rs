use crate::clock_state::{ClockState, Colour};
use crate::console_clock::DateWords;
use crate::frame_head::{HeadState, Rect};
use crate::matrix::Mat4;
use crate::ramp::Ramp;
use crate::rods::RodRecord;
use crate::sound::SoundCommand;

// The pad words the pad reader (HDD func_00235EB0) leaves at 0x00370330: held, pressed, released, repeating.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PadWords {
    pub held: u32,
    pub pressed: u32,
    pub released: u32,
    pub repeating: u32,
}

// The bits the menus' code tests (References/model/clock_menus.mjs).
pub mod pad {
    pub const TRIANGLE: u32 = 0x10;
    pub const CROSS: u32 = 0x20;
    pub const CIRCLE: u32 = 0x40;
    pub const SQUARE: u32 = 0x80;
    pub const UP: u32 = 0x1000;
    pub const RIGHT: u32 = 0x2000;
    pub const DOWN: u32 = 0x4000;
    pub const LEFT: u32 = 0x8000;
}

// clock_memory.mjs configEntries: one entry of System Configuration's list, 0x38 bytes; callbacks are EE addresses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfigEntry {
    pub id: i32,
    pub value_count: i32,
    pub value_index: i32,
    pub item: i32,
    pub value_table: u32,
    pub enter: u32,
    pub string_callback: u32,
    pub frame_callback: u32,
    pub confirm: u32,
    pub cancel: u32,
    pub focus: u32,
    pub rest: [i32; 3],
}

// clock_memory.mjs configPage (HDD 0x002B2DE8): level 0 the list, 1 inside an entry, 2 waiting for the save.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfigPage {
    pub word0: i32,
    pub entries: u32,
    pub count: i32,
    pub title_width: i32,
    pub selected: i32,
    pub word14: i32,
    pub level: i32,
    pub ramp: Ramp,
    pub word2c: i32,
    pub word30: i32,
    pub glow: i32,
}

// clock_memory.mjs mainMenu (HDD 0x002B2E60).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MainMenu {
    pub word0: i32,
    pub items: u32,
    pub count: i32,
    pub word0c: i32,
    pub selected: i32,
    pub word14: i32,
    pub ramp: Ramp,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdjustField {
    pub item: i32,
    pub lowest: i32,
    pub highest: i32,
}

// clock_memory.mjs listConstants: the frame rate, the arrow's divisor, the ring's pulse, the standing cube's pulse.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ListConstants {
    pub rate: f32,
    pub divisor: f32,
    pub pulse: f32,
    pub standing: f32,
}

// D_0037029C, D_003702A0, D_003702A4 (R4): the list entries' crossfade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListFade {
    pub first: i32,
    pub second: i32,
    pub second_index: i32,
}

impl Default for ListFade {
    fn default() -> Self {
        Self { first: 128, second: 0, second_index: 0 }
    }
}

pub type ConfigItems = [i32; 20];

// clock_version.mjs: one row of the Version page's table (HDD 0x00404D48, 0x10 a row): the module's name as a language string id,
// its version string, the module's sub-row count (func_002086A8) and its id in the module table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionRow {
    pub label: i32,
    pub value: Vec<u8>,
    pub sub_rows: i32,
    pub id: i32,
}

pub const VERSION_ROWS: usize = 0x20;

// clock_version.mjs V.record and the words the page writes: the record (title, count, shown rows, selected, first shown), the job
// handle, the button panel words (func_002266C0, func_002266C8, func_002266E0, func_002266D0) and how long the job has been asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionPage {
    pub title: i32,
    pub count: i32,
    pub shown: i32,
    pub selected: i32,
    pub first: i32,
    pub rows: [VersionRow; VERSION_ROWS],
    pub job: i32,
    pub panel_on: i32,
    pub panel_alpha: i32,
    pub arrows: i32,
    pub hints: [i32; 4],
    pub polls: i32,
}

impl Default for VersionPage {
    fn default() -> Self {
        Self {
            title: 0x59,
            count: 0,
            shown: 6,
            selected: 0,
            first: 0,
            rows: std::array::from_fn(|_| VersionRow::default()),
            job: 0,
            panel_on: 0,
            panel_alpha: 0,
            arrows: 0,
            hints: [1, 1, 1, 1],
            polls: 0,
        }
    }
}

// What the menus own (clock_menus.mjs); the clock owns the rest of what they write (MenuWorld).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MenusState {
    pub page: ConfigPage,
    pub entries: [ConfigEntry; 9],
    pub main_menu: MainMenu,
    pub version_ramp: Ramp,
    pub dialog_ramp: Ramp,
    pub first_run_ramp: Ramp,
    pub version: VersionPage,
    pub version_list: Vec<VersionRow>,
    pub version_drawn: VersionPage,
    pub version_ramp_drawn: Ramp,
    pub version_drawn_valid: bool,
    pub page_pointers: [i32; 5],
    pub entry_active: i32,
    pub menu_lengths: [i32; 3],
    pub list_constants: ListConstants,
    pub screen_code: i32,
    pub adjust_fields: [AdjustField; 6],
    pub config_gate: Option<i32>,
    pub list_fade: ListFade,
    pub body: i32,
    pub video_mode: i32,
}

// clock_memory.mjs cubeList: pulse, pulsed place, position, left to go, speed, slowing.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CubeList {
    pub pulse: f32,
    pub pulsed: i32,
    pub position: i32,
    pub left: i32,
    pub speed: i32,
    pub slowing: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CubeColours {
    pub selected: Colour,
    pub plain: Colour,
    pub live: Colour,
}

// The cubes' state (clock_cubes.mjs); the spin is ClockState::spin.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CubeState {
    pub ramp: Ramp,
    pub list: CubeList,
    pub colours: CubeColours,
    pub record: RodRecord,
    pub constants: [f32; 6],
    pub centre_factors: [f32; 2],
    pub view: Mat4,
    pub screen: Mat4,
    pub layer_clear: Colour,
    pub added: Rect,
    pub half: Rect,
    pub chain: Rect,
}

// verify_frame.mjs EXTERNAL, the menus' part: taken from outside every frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuExternals {
    pub pad: PadWords,
    pub disc: i32,
    pub config_dirty: Option<[i32; 3]>,
    pub rtc_mirror: Option<[i32; 6]>,
    pub mechacon_param: Option<[u32; 2]>,
}

// Everything the menus' code reads and writes; configuration item 0 is items[0] (the same word, 0x00409130).
pub struct MenuWorld<'a> {
    pub clock: &'a mut ClockState,
    pub head: &'a mut HeadState,
    pub sprite_fade: &'a mut Ramp,
    pub menus: &'a mut MenusState,
    pub cubes: &'a mut CubeState,
    pub items: &'a mut ConfigItems,
    pub width: i32,
    pub height: i32,
    pub sounds: Option<&'a mut Vec<SoundCommand>>,
    pub rtc_write: Option<&'a mut Option<DateWords>>,
}
