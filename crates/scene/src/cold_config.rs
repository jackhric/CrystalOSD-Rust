use crate::arithmetic::Arithmetic;
use crate::clock_state::ClockLogic;
use crate::cold_ramps::ColdLengths;
use crate::cold_time::ColdTimeOut;
use crate::cold_types::ColdInputs;
use crate::menu_types::{ConfigEntry, ConfigItems, ConfigPage, MainMenu};
use crate::program_image::ProgramImage;

const PARAM: u32 = 0x371818;
const DATE_WORD: u32 = 0x37181C;
const ENTRIES: u32 = 0x2B2BF0;
const PAGE: u32 = 0x2B2DE8;
const MENU: u32 = 0x2B2E60;
const PAGE_POINTERS: u32 = 0x3701C0;
const PAGE_BUFFER: u32 = 0x404370;
const ENTRY_SIZE: u32 = 0x38;
const TEMPLATE_FOUR: u32 = 0x2B2E88;
const TEMPLATE_VIDEO: u32 = 0x2B2EC0;
const TEMPLATE_MIDDLE: u32 = 0x2B2EF8;
const TEMPLATE_LAST: u32 = 0x2B2F30;
const CITY_LIMIT: i32 = 0x80;
const LANGUAGE_DEFAULT: i32 = 1;

struct Table {
    count: i32,
    table: u32,
}

const VIDEO_ENTRY: [Table; 3] = [Table { count: 2, table: 0x2B25D0 }, Table { count: 7, table: 0x2B2610 }, Table { count: 7, table: 0x2B26F0 }];
const LAST_ENTRY: [Table; 3] = [Table { count: 2, table: 0x2B2930 }, Table { count: 9, table: 0x2B2970 }, Table { count: 9, table: 0x2B2A90 }];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ColdSettingsWords {
    pub param: u32,
    pub date: u32,
    pub keyboard: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ColdConfigOut {
    pub items: ConfigItems,
    pub entries: [ConfigEntry; 9],
    pub page: ConfigPage,
    pub menu: MainMenu,
    pub pages: [i32; 5],
    pub param: u32,
}

fn entry_at(program: &ProgramImage, at: u32) -> ConfigEntry {
    ConfigEntry {
        id: program.integer(at),
        value_count: program.integer(at + 4),
        value_index: program.integer(at + 8),
        item: program.integer(at + 12),
        value_table: program.word(at + 0x10),
        enter: program.word(at + 0x14),
        string_callback: program.word(at + 0x18),
        frame_callback: program.word(at + 0x1C),
        confirm: program.word(at + 0x20),
        cancel: program.word(at + 0x24),
        focus: program.word(at + 0x28),
        rest: [program.integer(at + 0x2C), program.integer(at + 0x30), program.integer(at + 0x34)],
    }
}

fn language_of(param: &mut u32, video_mode: i32) -> i32 {
    let field = (*param >> 4) & 0x1F;
    if video_mode == 0 {
        return if field == 1 { 1 } else { 0 };
    }
    if field == 0 {
        *param = (*param & !0x1F0) | ((LANGUAGE_DEFAULT as u32) << 4);
        return LANGUAGE_DEFAULT;
    }
    if field < 8 { field as i32 } else { LANGUAGE_DEFAULT }
}

// HDD OSD 1.10U 0x203A20 (config_set_default_main), the getters 0x203CF8..0x204E18 verify_cold_config.mjs
pub fn cold_settings_words(program: &ProgramImage, inputs: &ColdInputs) -> ColdSettingsWords {
    let kept = program.word(PARAM) & 0x80000000;
    let param = kept
        | ((inputs.aspect as u32) & 3) << 1
        | ((inputs.language as u32) & 0x1F) << 4
        | ((inputs.time_zone as u32) & 0x7FF) << 9
        | ((inputs.city as u32) & 0x1FF) << 20
        | u32::from(inputs.summer_time) << 29
        | ((inputs.time_format as u32) & 1) << 30;
    let date = (program.word(DATE_WORD) & !3) | ((inputs.date_format as u32) & 3);
    ColdSettingsWords { param, date, keyboard: 0 }
}

// HDD OSD 1.10U 0x234F78 config_load_clock_osd 0x234F88, 0x227F08 verify_cold_config.mjs
pub fn cold_config_words<A: Arithmetic>(
    program: &ProgramImage,
    words: &ColdSettingsWords,
    video_mode: i32,
    lengths: &ColdLengths,
    time: &ColdTimeOut,
    selected: i32,
) -> ColdConfigOut {
    let mut out = ColdConfigOut::default();
    let mut param = words.param;
    let items = &mut out.items;
    for (i, item) in items.iter_mut().enumerate() {
        *item = program.integer(0x409130 + 4 * i as u32);
    }
    let aspect = (param >> 1) & 3;
    items[0] = if aspect < 3 { aspect as i32 } else { 0 };
    items[1] = (param & 1) as i32;
    items[2] = ((param >> 3) & 1) as i32;
    items[3] = language_of(&mut param, video_mode);
    items[4] = (words.keyboard & 1) as i32;
    items[13] = ((param >> 30) & 1) as i32;
    let date_format = words.date & 3;
    items[14] = if date_format < 3 { date_format as i32 } else { 0 };
    let city = ((param >> 20) & 0x1FF) as i32;
    items[15] = city.min(CITY_LIMIT);
    items[16] = ((param >> 29) & 1) as i32;
    items[17] = ((words.date >> 2) & 1) as i32;
    items[18] = ((words.date >> 3) & 1) as i32;
    items[19] = ((words.date >> 4) & 1) as i32;
    let seconds = ClockLogic::<A>::seconds(&time.time);
    let minutes = A::add(A::from_int(time.time.minutes), A::div(seconds, 60.0));
    items[6] = time.year;
    items[7] = time.month;
    items[8] = time.day;
    items[9] = A::to_int(ClockLogic::<A>::hours(&time.time));
    items[10] = A::to_int(minutes);
    items[11] = A::to_int(seconds);
    out.param = param;

    for (i, entry) in out.entries.iter_mut().enumerate() {
        *entry = entry_at(program, ENTRIES + ENTRY_SIZE * i as u32);
    }
    out.page.word0 = program.integer(PAGE);
    out.page.entries = program.word(PAGE + 4);
    out.page.count = program.integer(PAGE + 8);
    out.page.title_width = program.integer(PAGE + 12);
    out.page.selected = selected;
    out.page.word14 = program.integer(PAGE + 0x14);
    out.page.level = program.integer(PAGE + 0x18);
    out.page.ramp = lengths.config;
    out.page.word2c = program.integer(PAGE + 0x2C);
    out.page.word30 = program.integer(PAGE + 0x30);
    out.page.glow = program.integer(PAGE + 0x34);
    out.menu.word0 = program.integer(MENU);
    out.menu.items = program.word(MENU + 4);
    out.menu.count = program.integer(MENU + 8);
    out.menu.word0c = program.integer(MENU + 12);
    out.menu.selected = program.integer(MENU + 0x10);
    out.menu.word14 = program.integer(MENU + 0x14);
    out.menu.ramp = lengths.main_menu;
    for (i, page) in out.pages.iter_mut().enumerate() {
        *page = program.integer(PAGE_POINTERS + 4 * i as u32);
    }
    out.pages[0] = PAGE_BUFFER as i32;

    let kept_id = out.entries[out.page.selected as usize].id;
    let mut k = 4usize;
    if out.items[19] != 0 {
        out.entries[4] = entry_at(program, TEMPLATE_FOUR);
        k = 5;
    }
    out.entries[k] = entry_at(program, TEMPLATE_VIDEO);
    out.entries[k].value_count = VIDEO_ENTRY[video_mode as usize].count;
    out.entries[k].value_table = VIDEO_ENTRY[video_mode as usize].table;
    out.entries[k + 1] = entry_at(program, TEMPLATE_MIDDLE);
    out.entries[k + 2] = entry_at(program, TEMPLATE_LAST);
    out.entries[k + 2].value_count = LAST_ENTRY[video_mode as usize].count;
    out.entries[k + 2].value_table = LAST_ENTRY[video_mode as usize].table;
    out.page.count = (k + 3) as i32;
    if out.entries[out.page.selected as usize].id != kept_id {
        let mut found = 0;
        for i in 0..k + 3 {
            if out.entries[i].id == kept_id {
                found = i as i32;
                break;
            }
        }
        out.page.selected = found;
    }
    out
}

pub fn cold_config<A: Arithmetic>(program: &ProgramImage, inputs: &ColdInputs, lengths: &ColdLengths, time: &ColdTimeOut) -> ColdConfigOut {
    cold_config_words::<A>(program, &cold_settings_words(program, inputs), if inputs.pal { 2 } else { 1 }, lengths, time, 0)
}
