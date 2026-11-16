use serde_json::Value;

use scene::arithmetic::as_float;
use scene::clock::{ClockInputs, FrameInputs, MenusInputs};
use scene::clock_state::{
    ClockColours, ClockState, ClockTime, Colour, CycleCounter, CycleCounters, CycleTables, EasedAngles, LogicConstants, Proportions, RodState, RodsState,
    SceneScale,
};
use scene::frame_head::{HeadState, Rect, RingRecord, TubeConstants};
use scene::matrix::{Mat4, Vec4};
use scene::menu_types::{
    AdjustField, ConfigEntry, ConfigItems, ConfigPage, CubeColours, CubeList, CubeState, ListConstants, ListFade, MainMenu, MenuExternals, MenusState,
    PadWords, VersionPage, VersionRow,
};
use scene::orbs::{ORB_COUNT, OrbEntry, OrbRing, OrbState, RING_LENGTH};
use scene::ramp::Ramp;
use scene::rods::{RodMesh, RodRecord};
use scene::text::{ClockItems, FontCache, FontCacheEntry, FontState, TextInputs, TextRamps, TextSettings};

pub fn at<'v>(v: &'v Value, key: &str) -> &'v Value {
    v.get(key).unwrap_or_else(|| panic!("the fixture has no '{key}'"))
}

pub fn int(v: &Value) -> i32 {
    match v.as_i64() {
        Some(n) => n as i32,
        None => v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) as i32,
    }
}

pub fn word(v: &Value) -> u32 {
    v.as_i64().unwrap_or_else(|| panic!("not an integer: {v}")) as u32
}

pub fn bits(v: &Value) -> u32 {
    let text = v.as_str().unwrap_or_else(|| panic!("not a float pattern: {v}"));
    assert!(text.len() == 10 && text.starts_with("0x"), "not a float pattern: {text}");
    u32::from_str_radix(&text[2..], 16).unwrap()
}

pub fn float(v: &Value) -> f32 {
    as_float(bits(v))
}

pub fn vec4(v: &Value) -> Vec4 {
    [float(&v[0]), float(&v[1]), float(&v[2]), float(&v[3])]
}

pub fn mat4(v: &Value) -> Mat4 {
    [vec4(&v[0]), vec4(&v[1]), vec4(&v[2]), vec4(&v[3])]
}

fn colour(v: &Value) -> Colour {
    [int(&v[0]), int(&v[1]), int(&v[2]), int(&v[3])]
}

pub fn ramp(v: &Value) -> Ramp {
    Ramp { length: int(at(v, "length")), counter: int(at(v, "counter")), changed: int(at(v, "changed")), state: int(at(v, "state")) }
}

fn ints<const N: usize>(v: &Value) -> [i32; N] {
    let list = v.as_array().unwrap_or_else(|| panic!("not a list: {v}"));
    assert_eq!(list.len(), N, "expected {N} words: {v}");
    std::array::from_fn(|i| int(&list[i]))
}

fn time(v: &Value) -> ClockTime {
    ClockTime { milliseconds: float(at(v, "ms")), seconds: int(at(v, "seconds")), minutes: int(at(v, "minutes")), hours: int(at(v, "hours")) }
}

fn clock_state(i: &Value) -> ClockState {
    let e = at(i, "eased");
    let st = at(i, "state");
    let rods: [RodState; 12] = std::array::from_fn(|k| {
        let r = &at(st, "rods")[k];
        RodState {
            appearance: float(at(r, "appearance")),
            progress: float(at(r, "progress")),
            base: colour(at(r, "base")),
            reflection: colour(at(r, "reflection")),
        }
    });
    let c = at(i, "colours");
    let cc = at(i, "cycleCounters");
    let ct = at(i, "cycleTables");
    let k = at(i, "logicConstants");
    let sc = at(i, "scene");
    ClockState {
        time: time(at(i, "time")),
        eased: EasedAngles {
            second_hand: int(at(e, "secondHand")) as i16,
            hour_hand: int(at(e, "hourHand")) as i16,
            progress: float(at(e, "progress")),
            fraction: float(at(e, "fraction")),
        },
        state: RodsState {
            current_rod: int(at(st, "currentRod")),
            seconds_angle: int(at(st, "secondsAngle")) as i16,
            rod_angle: int(at(st, "rodAngle")) as i16,
            rods,
            accent: colour(at(st, "accent")),
            fourth: colour(at(st, "fourth")),
            progress_target: float(at(st, "progressTarget")),
            accent_frame: colour(at(st, "accentFrame")),
            current_target: colour(at(st, "currentTarget")),
        },
        appearance: ramp(at(i, "appearance")),
        colours: ClockColours {
            base: colour(at(c, "base")),
            reflection: colour(at(c, "reflection")),
            fourth: colour(at(c, "fourth")),
            current_reflection: colour(at(c, "currentReflection")),
        },
        cycle_counters: CycleCounters {
            base: CycleCounter { index: int(at(at(cc, "base"), "index")), wait: int(at(at(cc, "base"), "wait")) },
            accent: CycleCounter { index: int(at(at(cc, "accent"), "index")), wait: int(at(at(cc, "accent"), "wait")) },
            last: int(at(cc, "last")),
        },
        cycle_tables: CycleTables { base: std::array::from_fn(|n| colour(&at(ct, "base")[n])), accent: std::array::from_fn(|n| colour(&at(ct, "accent")[n])) },
        logic_constants: LogicConstants {
            rod_easing: float(at(k, "rodEasing")),
            seconds_easing: float(at(k, "secondsEasing")),
            progress_step: float(at(k, "progressStep")),
        },
        scene: SceneScale { scale: float(at(sc, "scale")), leaving: int(at(sc, "leaving")), field: int(at(sc, "field")) },
        scale_target: float(at(i, "scaleTarget")),
        scale_factor: float(at(i, "scaleFactor")),
        time_filled: int(at(i, "timeFilled")),
        spin: i.get("spin").filter(|s| !s.is_null()).map(|s| int(s) as u16),
        mode: int(at(i, "mode")),
        level: int(at(i, "level")),
        overlay_level: int(at(i, "overlayLevel")),
        vignette_ramp: ramp(at(i, "vignetteRamp")),
        vignette_length: int(at(i, "vignetteLength")),
        menu_ramp: ramp(at(i, "menuRamp")),
        tail: int(at(i, "tail")),
        counter: int(at(i, "counter")),
        proportions: Proportions { ax: float(at(at(i, "proportions"), "ax")), ay: float(at(at(i, "proportions"), "ay")) },
        position: vec4(at(i, "position")),
        direction: vec4(at(i, "direction")),
        up: vec4(at(i, "up")),
        rotation: vec4(at(i, "rotation")),
        camera_offset: float(at(i, "cameraOffset")),
        zmax: float(at(i, "zmax")),
        camera_factor: float(at(i, "cameraFactor")),
    }
}

fn rect(j: &Value) -> Rect {
    Rect {
        colour: colour(at(j, "colour")),
        x0: int(at(j, "x0")),
        y0: int(at(j, "y0")),
        u0: int(at(j, "u0")),
        v0: int(at(j, "v0")),
        x1: int(at(j, "x1")),
        y1: int(at(j, "y1")),
        u1: int(at(j, "u1")),
        v1: int(at(j, "v1")),
        z: int(at(j, "z")),
        blend: int(at(j, "blend")),
        textured: int(at(j, "textured")),
    }
}

fn head_state(i: &Value) -> HeadState {
    let ring = at(i, "ringRecord");
    HeadState {
        grey_ramp: ramp(at(i, "greyRamp")),
        greys: ints::<3>(at(i, "greys")),
        vignette_ramp: ramp(at(i, "vignetteRamp")),
        ring: RingRecord {
            alpha: int(at(ring, "alpha")),
            cx: int(at(ring, "cx")),
            cy: int(at(ring, "cy")),
            rx: int(at(ring, "rx")),
            ry: int(at(ring, "ry")),
            z: int(at(ring, "z")),
        },
        tint: rect(at(i, "tint")),
        blur: rect(at(i, "blurRecord")),
        copy: rect(at(i, "copyRecord")),
        fade: rect(at(i, "fadeRecord")),
        bars: rect(at(i, "bars")),
        column: rect(at(i, "column")),
    }
}

fn rod_record(j: &Value) -> RodRecord {
    RodRecord {
        number: int(at(j, "number")),
        faces: int(at(j, "faces")),
        local: mat4(at(j, "local")),
        sx: float(at(j, "sx")),
        sy: float(at(j, "sy")),
        sz: float(at(j, "sz")),
        base: colour(at(j, "base")),
        strength: float(at(j, "strength")),
        textured: colour(at(j, "textured")),
        pair: [float(&at(j, "pair")[0]), float(&at(j, "pair")[1])],
        refraction: float(at(j, "refraction")),
        reflection: colour(at(j, "reflection")),
        extra: colour(at(j, "extra")),
    }
}

fn text_ramps(r: &Value) -> TextRamps {
    TextRamps {
        config: ramp(at(r, "config")),
        main_menu: ramp(at(r, "mainMenu")),
        version: ramp(at(r, "version")),
        dialog_closing: ramp(at(r, "dialogClosing")),
        first_run: ramp(at(r, "firstRun")),
        dialog: ramp(at(r, "dialog")),
        lead: int(at(r, "lead")),
        body: int(at(r, "body")),
        panel7: int(at(r, "panel7")),
        panel8_on: int(at(r, "panel8On")),
        panel8: int(at(r, "panel8")),
        adjust_row: int(at(r, "adjustRow")),
        ..TextRamps::default()
    }
}

pub fn font_state(j: &Value) -> FontState {
    FontState {
        line_height: int(at(j, "lineHeight")),
        fixed: int(at(j, "fixed")),
        percent: int(at(j, "percent")),
        pitch: int(at(j, "pitch")),
        decoration: int(at(j, "decoration")),
        clip: int(at(j, "clip")),
        blank: int(at(j, "blank")),
        ascent: int(at(j, "ascent")),
        dirty: int(at(j, "dirty")),
        tv: float(at(j, "tv")),
        ratio: float(at(j, "ratio")),
        locate: [float(&at(j, "locate")[0]), float(&at(j, "locate")[1])],
        colour: std::array::from_fn(|n| float(&at(j, "colour")[n])),
        matrix: mat4(at(j, "matrix")),
    }
}

fn text_inputs(i: &Value) -> TextInputs {
    let font = at(i, "font");
    let list = at(font, "list")
        .as_array()
        .unwrap()
        .iter()
        .map(|e| FontCacheEntry { code: int(at(e, "code")), loaded: int(at(e, "loaded")), cell: int(at(e, "cell")), block: word(at(e, "block")) })
        .collect();
    let items = at(i, "configItems");
    TextInputs {
        cache: FontCache {
            list,
            format: int(at(font, "format")),
            cell_w: int(at(font, "cellW")),
            cell_h: int(at(font, "cellH")),
            cells: int(at(font, "cells")),
            width: int(at(font, "width")),
            height: int(at(font, "height")),
            log_w: int(at(font, "logW")),
            log_h: int(at(font, "logH")),
            set_up: int(at(font, "setUp")),
            block: word(at(font, "block")),
            memory: word(at(font, "memory")),
            texture: word(at(font, "texture")),
            table: word(at(font, "table")),
        },
        font: font_state(at(font, "state")),
        ramps: text_ramps(at(i, "textRamps")),
        settings: TextSettings {
            time_format: int(&items[0xd]),
            date_format: int(&items[0xe]),
            settings_word: word(&at(i, "mechaconParam")[0]),
            video_mode: int(at(i, "videoMode")),
        },
    }
}

fn orb_state(i: &Value) -> OrbState {
    let rings: [OrbRing; ORB_COUNT] = std::array::from_fn(|k| {
        let r = &at(i, "rings")[k];
        OrbRing {
            head: int(at(r, "head")),
            count: int(at(r, "count")),
            full: int(at(r, "full")),
            entries: std::array::from_fn(|n| {
                let e = &at(r, "entries")[n];
                OrbEntry { x: float(at(e, "x")), y: float(at(e, "y")), z: float(at(e, "z")), colour: colour(at(e, "colour")) }
            }),
        }
    });
    let _ = RING_LENGTH;
    OrbState { rings, sprite_fade: ramp(at(i, "spriteFade")), fraction: float(at(at(i, "eased"), "fraction")) }
}

fn config_entry(j: &Value) -> ConfigEntry {
    ConfigEntry {
        id: int(at(j, "id")),
        value_count: int(at(j, "valueCount")),
        value_index: int(at(j, "valueIndex")),
        item: int(at(j, "item")),
        value_table: word(at(j, "valueTable")),
        enter: word(at(j, "enter")),
        string_callback: word(at(j, "stringCallback")),
        frame_callback: word(at(j, "frameCallback")),
        confirm: word(at(j, "confirm")),
        cancel: word(at(j, "cancel")),
        focus: word(at(j, "focus")),
        rest: ints::<3>(at(j, "rest")),
    }
}

fn config_page(j: &Value) -> ConfigPage {
    ConfigPage {
        word0: int(at(j, "word0")),
        entries: word(at(j, "entries")),
        count: int(at(j, "count")),
        title_width: int(at(j, "titleWidth")),
        selected: int(at(j, "selected")),
        word14: int(at(j, "word14")),
        level: int(at(j, "level")),
        ramp: ramp(at(j, "ramp")),
        word2c: int(at(j, "word2c")),
        word30: int(at(j, "word30")),
        glow: int(at(j, "glow")),
    }
}

fn main_menu(j: &Value) -> MainMenu {
    MainMenu {
        word0: int(at(j, "word0")),
        items: word(at(j, "items")),
        count: int(at(j, "count")),
        word0c: int(at(j, "word0c")),
        selected: int(at(j, "selected")),
        word14: int(at(j, "word14")),
        ramp: ramp(at(j, "ramp")),
    }
}

fn version_row(row: &Value) -> VersionRow {
    let value = match at(row, "value") {
        Value::String(s) => s.as_bytes().to_vec(),
        Value::Array(list) => list.iter().map(|b| int(b) as u8).collect(),
        other => panic!("a version row value: {other}"),
    };
    VersionRow { label: int(at(row, "label")), value, sub_rows: int(at(row, "subRows")), id: int(at(row, "id")) }
}

fn version_page(page: &Value) -> VersionPage {
    let mut v = VersionPage {
        title: int(at(page, "title")),
        count: int(at(page, "count")),
        shown: int(at(page, "shown")),
        selected: int(at(page, "selected")),
        first: int(at(page, "first")),
        ..VersionPage::default()
    };
    for (n, row) in at(page, "rows").as_array().unwrap().iter().enumerate().take(v.rows.len()) {
        v.rows[n] = version_row(row);
    }
    if let Some(x) = page.get("job") {
        v.job = int(x);
    }
    if let Some(x) = page.get("panelOn") {
        v.panel_on = int(x);
    }
    if let Some(x) = page.get("panelAlpha") {
        v.panel_alpha = int(x);
    }
    if let Some(x) = page.get("arrows") {
        v.arrows = int(x);
    }
    if let Some(x) = page.get("hints") {
        v.hints = ints::<4>(x);
    }
    if let Some(x) = page.get("polls") {
        v.polls = int(x);
    }
    v
}

pub fn has_menus(i: &Value) -> bool {
    i.get("configPage").is_some()
}

fn menus_state(i: &Value) -> MenusState {
    let entries = at(i, "configEntries").as_array().unwrap();
    assert_eq!(entries.len(), 9, "expected 9 configuration entries");
    let k = at(i, "listConstants");
    let mut s = MenusState {
        page: config_page(at(i, "configPage")),
        entries: std::array::from_fn(|n| config_entry(&entries[n])),
        main_menu: main_menu(at(i, "mainMenu")),
        version_ramp: ramp(at(i, "versionRamp")),
        dialog_ramp: ramp(at(i, "dialogRamp")),
        first_run_ramp: ramp(at(i, "firstRunRamp")),
        page_pointers: ints::<5>(at(i, "pagePointers")),
        entry_active: int(at(i, "entryActive")),
        menu_lengths: ints::<3>(at(i, "menuLengths")),
        list_constants: ListConstants {
            rate: float(at(k, "rate")),
            divisor: float(at(k, "divisor")),
            pulse: float(at(k, "pulse")),
            standing: float(at(k, "standing")),
        },
        screen_code: int(at(i, "screenCode")),
        body: int(at(i, "body")),
        video_mode: int(at(i, "videoMode")),
        ..MenusState::default()
    };
    if let Some(fields) = i.get("adjustFields") {
        let fields = fields.as_array().unwrap();
        assert_eq!(fields.len(), 6, "expected 6 adjustment fields");
        for (n, f) in fields.iter().enumerate() {
            s.adjust_fields[n] = AdjustField { item: int(at(f, "item")), lowest: int(at(f, "lowest")), highest: int(at(f, "highest")) };
        }
    }
    if let Some(gate) = i.get("configGate").filter(|g| !g.is_null()) {
        s.config_gate = Some(int(gate));
    }
    if let Some(fade) = i.get("listFade") {
        s.list_fade = ListFade { first: int(at(fade, "first")), second: int(at(fade, "second")), second_index: int(at(fade, "secondIndex")) };
    }
    if let Some(page) = i.get("versionPage") {
        s.version = version_page(page);
    }
    if let Some(list) = i.get("versionList") {
        s.version_list = list.as_array().unwrap().iter().map(version_row).collect();
    }
    if let Some(drawn) = i.get("versionDrawn") {
        s.version_drawn = version_page(drawn);
        s.version_ramp_drawn = ramp(at(drawn, "ramp"));
        s.version_drawn_valid = true;
    }
    s
}

fn cube_state(i: &Value) -> CubeState {
    let l = at(i, "cubeList");
    let c = at(i, "cubeColours");
    let k = at(i, "cubeConstants");
    CubeState {
        ramp: ramp(at(i, "cubeRamp")),
        list: CubeList {
            pulse: float(at(l, "pulse")),
            pulsed: int(at(l, "pulsed")),
            position: int(at(l, "position")),
            left: int(at(l, "left")),
            speed: int(at(l, "speed")),
            slowing: int(at(l, "slowing")),
        },
        colours: CubeColours { selected: colour(at(c, "selected")), plain: colour(at(c, "plain")), live: colour(at(c, "live")) },
        record: rod_record(at(i, "cubeRecord")),
        constants: [
            float(at(k, "standingFade")),
            float(at(k, "ringFade")),
            float(at(k, "twoPi")),
            float(at(k, "turn")),
            float(at(k, "quarter")),
            float(at(k, "minusPi")),
        ],
        centre_factors: [float(at(at(i, "centreFactors"), "cube")), float(at(at(i, "centreFactors"), "layer"))],
        view: mat4(at(i, "cubeView")),
        screen: mat4(at(i, "cubeScreen")),
        layer_clear: colour(at(i, "layerClear")),
        added: rect(at(i, "addRecord")),
        half: rect(at(i, "halfRecord")),
        chain: rect(at(i, "chainRecord")),
    }
}

fn config_items(i: &Value) -> ConfigItems {
    ints::<20>(at(i, "configItems"))
}

pub fn pad_words(p: &Value) -> PadWords {
    PadWords { held: word(at(p, "held")), pressed: word(at(p, "pressed")), released: word(at(p, "released")), repeating: word(at(p, "repeating")) }
}

fn menu_externals(i: &Value) -> MenuExternals {
    MenuExternals {
        pad: pad_words(at(i, "pad")),
        disc: int(at(i, "disc")),
        config_dirty: i.get("configDirty").map(ints::<3>),
        rtc_mirror: i.get("rtcMirror").map(ints::<6>),
        mechacon_param: i.get("mechaconParam").map(|m| [word(&m[0]), word(&m[1])]),
    }
}

// Source/Scene/SceneInputs.cpp clockInputs: the clock's inputs from a fixture frame's "input".
pub fn clock_inputs(i: &Value, mesh: &RodMesh, cube_mesh: Option<&RodMesh>) -> ClockInputs {
    let t = at(i, "tubeConstants");
    let k = at(i, "orbConstants");
    let mut c = ClockInputs {
        state: clock_state(i),
        head: head_state(i),
        rod_template: rod_record(at(i, "template")),
        orbs: orb_state(i),
        mesh: mesh.clone(),
        clear_colour: colour(at(i, "clearColour")),
        first_display_clear: colour(at(at(i, "display"), "firstClear")),
        tube: TubeConstants {
            near: float(at(t, "near")),
            turn: float(at(t, "turn")),
            scroll: float(at(t, "scroll")),
            ripple: float(at(t, "ripple")),
            scroll_end: float(at(t, "scrollEnd")),
            turn_end: float(at(t, "turnEnd")),
            far: float(at(t, "far")),
            radius: float(at(t, "radius")),
        },
        minute_factor: float(at(k, "minuteFactor")),
        fraction_easing: float(at(k, "fractionEasing")),
        orb_colour: colour(at(i, "orbColour")),
        has_entry_data: i.get("wide").is_some() && i.get("orbRandom").is_some() && i.get("orbColours").is_some(),
        width: int(at(at(i, "screen"), "width")),
        height: int(at(at(i, "screen"), "height")),
        ..ClockInputs::default()
    };
    if let Some(w) = i.get("wide") {
        c.wide = int(w);
    }
    if let Some(r) = i.get("orbRandom") {
        c.orb_random = std::array::from_fn(|n| int(&r[n]));
    }
    if let Some(r) = i.get("orbColours") {
        c.orb_colours = std::array::from_fn(|n| colour(&r[n]));
    }
    if i.get("font").is_some() {
        c.text = text_inputs(i);
    }
    if let Some(cubes) = cube_mesh
        && has_menus(i)
    {
        c.menus =
            Some(MenusInputs { menus: menus_state(i), cubes: cube_state(i), cube_mesh: cubes.clone(), items: config_items(i), options: Default::default() });
    }
    c
}

// Source/Scene/SceneInputs.cpp frameInputs.
pub fn frame_inputs(i: &Value) -> FrameInputs {
    let mut out = FrameInputs {
        time: time(at(i, "time")),
        field: int(at(at(i, "scene"), "field")),
        display_index: int(at(i, "index")),
        item0: int(at(i, "item0")),
        ..FrameInputs::default()
    };
    if let Some(t) = i.get("timeFilled") {
        out.time_filled = Some(int(t));
    }
    if let Some(items) = i.get("configItems") {
        out.items = ClockItems {
            year: int(&items[6]),
            month: int(&items[7]),
            day: int(&items[8]),
            hour: int(&items[9]),
            minute: int(&items[10]),
            second: int(&items[11]),
        };
    }
    if has_menus(i) {
        out.menu = menu_externals(i);
        out.config_items = Some(config_items(i));
        out.text_ramps = Some(match i.get("textRamps") {
            Some(r) => text_ramps(r),
            None => {
                let rest = Ramp { length: 10, counter: 0, changed: 0, state: 0 };
                TextRamps { config: rest, main_menu: rest, version: rest, dialog_closing: rest, first_run: rest, dialog: rest, ..TextRamps::default() }
            }
        });
    }
    out
}
