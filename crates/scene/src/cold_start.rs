use std::sync::Arc;

use crate::Result;
use crate::arithmetic::Arithmetic;
use crate::clock::{ClockInputs, FrameInputs, MenusInputs};
use crate::clock_state::SceneScale;
use crate::cold_camera::cold_camera;
use crate::cold_config::{cold_config_words, cold_settings_words};
use crate::cold_font::cold_font;
use crate::cold_ramps::{cold_lengths, ramp_set};
use crate::cold_state::cold_state;
use crate::cold_time::cold_time;
use crate::cold_types::ColdInputs;
use crate::elf_records::{colour_at, ramp_at, rect_at, rod_record_at, vec4_at};
use crate::font::Font;
use crate::frame_head::{RingRecord, TubeConstants};
use crate::matrix::Matrix;
use crate::menu_types::{AdjustField, CubeColours, CubeList, ListConstants, ListFade, VersionRow};
use crate::orbs::ORB_COUNT;
use crate::program_image::ProgramImage;
use crate::ramp::Ramp;
use crate::rods::RodMesh;
use crate::text::{ClockItems, TextSettings};

const SCALE_TARGET: u32 = 0x370294;
const SCALE_FACTOR: u32 = 0x36FC10;
const POSITION: u32 = 0x2B2190;
const DIRECTION: u32 = 0x2B21A0;
const UP: u32 = 0x2B21B0;
const ROTATION: u32 = 0x2B21C0;
const ZMAX: u32 = 0x36FB8C;
const CAMERA_FACTOR: u32 = 0x36FB90;
const GREYS: u32 = 0x370AA8;
const RING_RECORD: u32 = 0x2B5F70;
const TINT: u32 = 0x2B21E0;
const BLUR: u32 = 0x2B6120;
const COPY: u32 = 0x2B6060;
const BARS: u32 = 0x2B2220;
const COLUMN: u32 = 0x2B2500;
const TEMPLATE: u32 = 0x2B5490;
const CLEAR_COLOUR: u32 = 0x2B21D0;
const TUBE: u32 = 0x36FC14;
const ORB_CONSTANTS: u32 = 0x36FBDC;
const ORB_COLOUR: u32 = 0x2B5670;
const ORB_COLOURS: u32 = 0x2B5680;
const DIALOG_CLOSING: u32 = 0x2B46B8;
const FIRST_RUN: u32 = 0x2B46D0;
const PANEL7: u32 = 0x2B2E00;
const PANEL8_ON: u32 = 0x370140;
const VERSION_RECORD: u32 = 0x2B2FE8;
const BROWSER_VERSION: u32 = 0x370680;
const PANEL8: u32 = 0x37013C;
const ENTRY_ACTIVE: u32 = 0x3702C8;
const LIST_CONSTANTS: u32 = 0x36FC00;
const ADJUST_FIELDS: u32 = 0x2B2580;
const CONFIG_GATE: u32 = 0x370300;
const LIST_FADE: u32 = 0x37029C;
const CUBE_LIST: u32 = 0x3702A8;
const CUBE_COLOURS: u32 = 0x2B5750;
const CUBE_RECORD: u32 = 0x2B5AF0;
const CUBE_CONSTANTS: u32 = 0x36FBE8;
const CENTRE_FACTORS: u32 = 0x36FC50;
const LAYER_CLEAR: u32 = 0x2B5CC0;
const ADD_RECORD: u32 = 0x2B5FE0;
const HALF_RECORD: u32 = 0x2B6020;
const CHAIN_RECORD: u32 = 0x2B60E0;
const ZMAX_SCREEN: u32 = 0x36FC0C;
const HANDOFF_CODE: i32 = 0x64;
const SCREEN_DISTANCE: f32 = 512.0;
const SCREEN_CENTRE: f32 = 2048.0;
const SCREEN_MIN_Z: f32 = 1.0;
const SCREEN_NEAR: f32 = 1.0;
const SCREEN_FAR: f32 = 65536.0;

#[derive(Debug, Clone)]
pub struct ColdAssets {
    pub program: Arc<ProgramImage>,
    pub font: Option<Arc<Font>>,
    pub mesh: RodMesh,
    pub cube_mesh: RodMesh,
}

#[derive(Debug, Clone)]
pub struct ColdStartOut {
    pub clock: ClockInputs,
    pub frame: FrameInputs,
}

fn tube_at(p: &ProgramImage, a: u32) -> TubeConstants {
    TubeConstants {
        near: p.single(a),
        turn: p.single(a + 4),
        scroll: p.single(a + 8),
        ripple: p.single(a + 12),
        scroll_end: p.single(a + 16),
        turn_end: p.single(a + 20),
        far: p.single(a + 24),
        radius: p.single(a + 28),
    }
}

pub fn fill_time(frame: &mut FrameInputs) {
    frame.time_filled = Some(1);
}

// HDD OSD 1.10U 0x225DB0 module_clock_init_resources and the thread set-up, in the order of its calls
pub fn cold_start<A: Arithmetic>(assets: &ColdAssets, input: &ColdInputs) -> Result<ColdStartOut> {
    cold_start_guarded::<A::UnderEeRounding>(assets, input)
}

fn cold_start_guarded<A: Arithmetic>(assets: &ColdAssets, input: &ColdInputs) -> Result<ColdStartOut> {
    let p = &*assets.program;
    let camera = cold_camera(p, input.pal, input.screen_width, input.screen_height, input.field);
    let time = cold_time::<A>(p, &input.rtc, input.time_zone, input.summer_time, input.pal)?;
    let lengths = cold_lengths(input.pal);
    let words = cold_settings_words(p, input);
    let video_mode = if input.pal { 2 } else { 1 };
    let config = cold_config_words::<A>(p, &words, video_mode, &lengths, &time, 0);
    let state = cold_state::<A>(p, &lengths, &time.time, input.rand_state);

    let mut c = ClockInputs::default();
    let s = &mut c.state;
    s.time = time.time;
    s.eased = state.eased;
    s.state = state.rods;
    s.appearance = state.appearance;
    s.colours = state.colours;
    s.cycle_counters = state.cycle;
    s.cycle_tables = state.cycle_tables;
    s.logic_constants = state.logic_constants;
    s.scene = SceneScale { scale: camera.scale, leaving: i32::from(camera.leaving), field: camera.field };
    s.scale_target = p.single(SCALE_TARGET);
    s.scale_factor = p.single(SCALE_FACTOR);
    s.time_filled = i32::from(time.time_filled);
    s.spin = Some(0);
    s.mode = camera.mode;
    s.level = 0;
    s.overlay_level = camera.overlay_level;
    s.vignette_ramp = lengths.vignette;
    s.vignette_length = lengths.vignette_length;
    s.menu_ramp = lengths.menu;
    s.tail = lengths.tail;
    s.counter = 0;
    s.proportions = camera.proportions;
    s.position = vec4_at(p, POSITION);
    s.direction = vec4_at(p, DIRECTION);
    s.up = vec4_at(p, UP);
    s.rotation = vec4_at(p, ROTATION);
    s.camera_offset = camera.camera_offset;
    s.zmax = p.single(ZMAX);
    s.camera_factor = p.single(CAMERA_FACTOR);

    let h = &mut c.head;
    h.grey_ramp = lengths.grey;
    for i in 0..3u32 {
        h.greys[i as usize] = p.integer(GREYS + 4 * i);
    }
    h.vignette_ramp = lengths.vignette;
    h.ring = RingRecord {
        alpha: p.integer(RING_RECORD),
        cx: p.integer(RING_RECORD + 4),
        cy: p.integer(RING_RECORD + 8),
        rx: p.integer(RING_RECORD + 12),
        ry: p.integer(RING_RECORD + 16),
        z: p.integer(RING_RECORD + 20),
    };
    h.tint = rect_at(p, TINT);
    h.blur = rect_at(p, BLUR);
    h.copy = rect_at(p, COPY);
    h.fade = camera.fade;
    h.bars = rect_at(p, BARS);
    h.column = rect_at(p, COLUMN);

    c.rod_template = rod_record_at(p, TEMPLATE);
    c.orbs.rings = state.rings;
    c.orbs.sprite_fade = state.sprite_fade;
    c.orbs.fraction = state.eased.fraction;
    c.mesh = assets.mesh.clone();
    c.clear_colour = colour_at(p, CLEAR_COLOUR);
    c.first_display_clear = [0, 0, 0, 0];
    c.tube = tube_at(p, TUBE);
    c.minute_factor = p.single(ORB_CONSTANTS);
    c.fraction_easing = p.single(ORB_CONSTANTS + 4);
    c.orb_colour = colour_at(p, ORB_COLOUR);
    c.has_entry_data = true;
    c.wide = i32::from(input.wide);
    c.orb_random = state.orb_random;
    for k in 0..ORB_COUNT {
        c.orb_colours[k] = colour_at(p, ORB_COLOURS + 16 * k as u32);
    }
    c.width = input.screen_width as i32;
    c.height = input.screen_height as i32;

    let screen_code = if input.wide { HANDOFF_CODE } else { input.screen_code as i32 };
    let ramps = &mut c.text.ramps;
    ramps.config = lengths.config;
    ramps.main_menu = lengths.main_menu;
    ramps.version = lengths.version;
    ramps.dialog_closing = ramp_at(p, DIALOG_CLOSING);
    ramps.first_run = ramp_at(p, FIRST_RUN);
    ramps.dialog = ramp_set(Ramp::default(), lengths.tail);
    ramps.lead = lengths.menu_short;
    ramps.body = lengths.body;
    ramps.panel7 = p.integer(PANEL7);
    ramps.panel8_on = p.integer(PANEL8_ON);
    ramps.panel8 = p.integer(PANEL8);
    ramps.adjust_row = 0;
    c.text.settings = TextSettings { time_format: config.items[13], date_format: config.items[14], settings_word: config.param, video_mode };

    if let Some(font) = &assets.font {
        let font_out = cold_font::<A>(Arc::clone(&assets.program), Arc::clone(font), input.pal, input.gs_allocator)?;
        c.text.cache = font_out.cache;
        c.text.font = font_out.state;
        c.font = Some(Arc::clone(font));
        c.program = Some(Arc::clone(&assets.program));
    }

    let mut menus = MenusInputs::default();
    let m = &mut menus.menus;
    m.page = config.page;
    m.entries = config.entries;
    m.main_menu = config.menu;
    m.version_ramp = lengths.version;
    m.version.title = p.integer(VERSION_RECORD);
    m.version.count = p.integer(VERSION_RECORD + 8);
    m.version.shown = p.integer(VERSION_RECORD + 12);
    m.version_list = vec![
        VersionRow { label: 0x62, value: b"Unknown".to_vec(), sub_rows: 1, id: 0 },
        VersionRow { label: 0x5a, value: p.string(BROWSER_VERSION), sub_rows: 0, id: 3 },
        VersionRow { label: 0x66, value: b"1.30".to_vec(), sub_rows: 0, id: 5 },
    ];
    m.dialog_ramp = ramp_at(p, DIALOG_CLOSING);
    m.first_run_ramp = ramp_at(p, FIRST_RUN);
    m.page_pointers = config.pages;
    m.entry_active = p.integer(ENTRY_ACTIVE);
    m.menu_lengths = [lengths.menu_short, lengths.menu_long, lengths.one];
    m.list_constants = ListConstants {
        rate: p.single(LIST_CONSTANTS),
        divisor: p.single(LIST_CONSTANTS + 4),
        pulse: p.single(LIST_CONSTANTS + 8),
        standing: p.single(LIST_CONSTANTS + 12),
    };
    m.screen_code = screen_code;
    for i in 0..6u32 {
        m.adjust_fields[i as usize] = AdjustField {
            item: p.integer(ADJUST_FIELDS + 12 * i),
            lowest: p.integer(ADJUST_FIELDS + 12 * i + 4),
            highest: p.integer(ADJUST_FIELDS + 12 * i + 8),
        };
    }
    m.config_gate = Some(p.integer(CONFIG_GATE));
    m.list_fade = ListFade { first: p.integer(LIST_FADE), second: p.integer(LIST_FADE + 4), second_index: p.integer(LIST_FADE + 8) };
    m.body = lengths.body;
    m.video_mode = video_mode;

    let cube = &mut menus.cubes;
    cube.ramp = lengths.cube;
    cube.list = CubeList {
        pulse: p.single(CUBE_LIST),
        pulsed: p.integer(CUBE_LIST + 4),
        position: p.integer(CUBE_LIST + 8),
        left: p.integer(CUBE_LIST + 12),
        speed: p.integer(CUBE_LIST + 16),
        slowing: p.integer(CUBE_LIST + 20),
    };
    cube.colours = CubeColours { selected: colour_at(p, CUBE_COLOURS), plain: colour_at(p, CUBE_COLOURS + 16), live: colour_at(p, CUBE_COLOURS + 32) };
    cube.record = rod_record_at(p, CUBE_RECORD);
    for i in 0..6u32 {
        cube.constants[i as usize] = p.single(CUBE_CONSTANTS + 4 * i);
    }
    cube.centre_factors = [p.single(CENTRE_FACTORS), p.single(CENTRE_FACTORS + 4)];
    cube.view = Matrix::<A>::identity();
    cube.screen = Matrix::<A>::view_screen(
        SCREEN_DISTANCE,
        camera.proportions.ax,
        camera.proportions.ay,
        SCREEN_CENTRE,
        SCREEN_CENTRE,
        SCREEN_MIN_Z,
        p.single(ZMAX_SCREEN),
        SCREEN_NEAR,
        SCREEN_FAR,
    );
    cube.layer_clear = colour_at(p, LAYER_CLEAR);
    cube.added = rect_at(p, ADD_RECORD);
    cube.half = rect_at(p, HALF_RECORD);
    cube.chain = rect_at(p, CHAIN_RECORD);
    menus.cube_mesh = assets.cube_mesh.clone();
    menus.items = config.items;
    c.menus = Some(menus);

    let i = &config.items;
    let frame = FrameInputs {
        time: time.time,
        field: camera.field,
        display_index: 0,
        item0: i[0],
        items: ClockItems { year: i[6], month: i[7], day: i[8], hour: i[9], minute: i[10], second: i[11] },
        menu: crate::menu_types::MenuExternals {
            disc: screen_code,
            config_dirty: Some([0, 0, 0]),
            rtc_mirror: Some([input.rtc.year, input.rtc.month, input.rtc.day, input.rtc.hour, input.rtc.minute, input.rtc.second]),
            mechacon_param: Some([config.param, words.date]),
            ..Default::default()
        },
        config_items: Some(config.items),
        ..Default::default()
    };
    Ok(ColdStartOut { clock: c, frame })
}
