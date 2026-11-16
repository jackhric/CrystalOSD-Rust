use std::marker::PhantomData;
use std::sync::Arc;

use crate::arithmetic::{Arithmetic, as_float};
use crate::clock_state::ClockState;
use crate::font::{Font, Glyph};
use crate::frame::GlyphCache;
use crate::frame_head::Rect;
use crate::matrix::{Mat4, Matrix, Vec4};
use crate::menu_types::{AdjustField, ConfigEntry, ConfigItems, ConfigPage, ListConstants, ListFade, MainMenu, MenusState, VERSION_ROWS, VersionPage};
use crate::program_image::ProgramImage;
use crate::ramp::Ramp;
use crate::{Error, Result};

// facts/text.md section 2: one entry of the glyph cache's list (most recently used first). `block` is the
// offset in the font file of the block the code's glyph is in, 0 for none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontCacheEntry {
    pub code: i32,
    pub loaded: i32,
    pub cell: i32,
    pub block: u32,
}

impl Default for FontCacheEntry {
    fn default() -> Self {
        Self { code: 0, loaded: 0, cell: -1, block: 0 }
    }
}

// The library's cache (_scePFontSetupTexCache 0x00290B10): the list, the cells' layout in the texture at GS word
// `texture`, its colour table at `table`, and the block (offset in the font file) it was laid out for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontCache {
    pub list: Vec<FontCacheEntry>,
    pub format: i32,
    pub cell_w: i32,
    pub cell_h: i32,
    pub cells: i32,
    pub width: i32,
    pub height: i32,
    pub log_w: i32,
    pub log_h: i32,
    pub set_up: i32,
    pub block: u32,
    pub memory: u32,
    pub texture: u32,
    pub table: u32,
}

// facts/text.md section 4: the program's font state at 0x003979B0 (Font_Set*, the escapes, updateTransMatrix).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FontState {
    pub line_height: i32,
    pub fixed: i32,
    pub percent: i32,
    pub pitch: i32,
    pub decoration: i32,
    pub clip: i32,
    pub blank: i32,
    pub ascent: i32,
    pub dirty: i32,
    pub tv: f32,
    pub ratio: f32,
    pub locate: [f32; 2],
    pub colour: [f32; 4],
    pub matrix: Mat4,
}

// What the alpha rules of the date, time and button hint read (References/scripts/verify_text2.mjs dateAlpha and
// panelsOf; HDD OSD 1.10U addresses). The menus' code writes them; on the clock screen they stand still.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRamps {
    pub config: Ramp,         // D_002B2E04
    pub main_menu: Ramp,      // D_002B2E78
    pub version: Ramp,        // D_002B3000
    pub dialog_closing: Ramp, // D_002B46B8
    pub first_run: Ramp,      // D_002B46D0
    pub dialog: Ramp,         // the page D_003701C0 points at, + 0x1C
    pub lead: i32,            // D_003702E0
    pub body: i32,            // D_003702CC
    pub panel7: i32,          // D_002B2E00
    pub panel8_on: i32,       // D_00370140
    pub panel8: i32,          // D_0037013C
    pub adjust_row: i32,      // the flag of the row D_002B2FF8 selects in the table D_002B2FEC points at
    pub panel8_ids: [i32; 4], // the string ids of panel 8, D_002B2300 (func_002266E0 writes them)
}

impl Default for TextRamps {
    fn default() -> Self {
        Self {
            config: Ramp::default(),
            main_menu: Ramp::default(),
            version: Ramp::default(),
            dialog_closing: Ramp::default(),
            first_run: Ramp::default(),
            dialog: Ramp::default(),
            lead: 0,
            body: 0,
            panel7: 0,
            panel8_on: 0,
            panel8: 0,
            adjust_row: 0,
            panel8_ids: [1, 1, 1, 1],
        }
    }
}

// The console's settings the text reads: configuration items 0xD (time format) and 0xE (date format), the settings
// word at 0x00371818 (language in bits 4 to 8, summer time in bit 29) and the video mode as cached (D_002AD228).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextSettings {
    pub time_format: i32,
    pub date_format: i32,
    pub settings_word: u32,
    pub video_mode: i32,
}

// Configuration items 6 to 0xB: the date and time func_00226300 formats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClockItems {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

// What the text carries from frame to frame, and its constants.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextInputs {
    pub cache: FontCache,
    pub font: FontState,
    pub ramps: TextRamps,
    pub settings: TextSettings,
}

// What the text reads of the frame: the items, configuration item 0, and the clock's own words the alpha rules read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFrameInputs {
    pub items: ClockItems,
    pub item0: i32,
    pub overlay_level: i32,
    pub tail: i32,
    pub menu: Ramp,
    pub width: i32,
    pub height: i32,
    pub ramps: Option<TextRamps>,
}

impl Default for TextFrameInputs {
    fn default() -> Self {
        Self { items: ClockItems::default(), item0: 0, overlay_level: 0, tail: 0, menu: Ramp::default(), width: 640, height: 224, ramps: None }
    }
}

// A glyph: the twelve-vertex fan (facts/text.md section 3) in native units (pixels without the field's half line,
// S T Q, the RGBAQ bytes) and the region clamp of its cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphVertex {
    pub x: f32,
    pub y: f32,
    pub s: f32,
    pub t: f32,
    pub q: f32,
    pub colour: [u8; 4],
}

impl Default for GlyphVertex {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, s: 0.0, t: 0.0, q: 1.0, colour: [0; 4] }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GlyphDraw {
    pub region: [i32; 4],
    pub fan: [GlyphVertex; 12],
}

// A string as the program hands it to Font_PutsPackets or calcDrawArea, with its font state at the call.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StringRun {
    pub text: Vec<u8>,
    pub measuring: bool,
    pub own: FontState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextDrawKind {
    #[default]
    Glyph,
    Icon,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextDraw {
    pub kind: TextDrawKind,
    pub glyph: GlyphDraw,
    pub icon: Rect,
    pub texture: i32,
}

// `glyphs`: the cache as GS memory holds it when the text is drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFrame {
    pub draws: Vec<TextDraw>,
    pub strings: Vec<StringRun>,
    pub glyphs: GlyphCache,
}

// facts/text.md section 5: what the pages' strings read: the main menu, System Configuration's list and entries, the
// crossfade (R4), the configuration items (the clock's value) and the ramps of the alpha rules.
#[derive(Debug, Clone, PartialEq)]
pub struct PagesInputs {
    pub main_menu: MainMenu,
    pub page: ConfigPage,
    pub entries: [ConfigEntry; 9],
    pub list_fade: ListFade,
    pub list_constants: ListConstants,
    pub adjust_fields: [AdjustField; 6],
    pub items: ConfigItems,
    pub menu_ramp: Ramp,
    pub tail: i32,
    pub overlay_level: i32,
    pub ramps: TextRamps,
    pub width: i32,
    pub height: i32,
    pub pending: i32,         // D_003701D0, what func_0022AAF0 answers
    pub version: VersionPage, // D_002B2FE8 and its table, as the page's drawing sees them
}

impl Default for PagesInputs {
    fn default() -> Self {
        Self {
            main_menu: MainMenu::default(),
            page: ConfigPage::default(),
            entries: [ConfigEntry::default(); 9],
            list_fade: ListFade::default(),
            list_constants: ListConstants::default(),
            adjust_fields: [AdjustField::default(); 6],
            items: [0; 20],
            menu_ramp: Ramp::default(),
            tail: 0,
            overlay_level: 0,
            ramps: TextRamps::default(),
            width: 640,
            height: 224,
            pending: 0,
            version: VersionPage::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PagesFrame {
    pub text: TextFrame,
    pub title_width: Option<i32>, // configPage + 0xC as the list's drawing measures it (browser_str_related2)
    // Entry callbacks the model does not cover (their value is left undrawn); such an entry is not enterable until ported.
    pub unmodelled: Vec<u32>,
}

// HDD OSD 1.10U data the text reads, by EE address (facts/text.md sections 4 and 5; disassembly in
// HDD OSD 1.10U disassembly: func_00226300, do_format_date, do_format_time, draw_button_panel_hkdosd_p4_tgt, DrawIcon).
const LANGUAGE_TABLES: u32 = 0x002ad200; // langtblptrs: one string table per language
const ESCAPE_COLOURS: u32 = 0x00348b20; // the escape c's colours, three words each
const DATE_NONE: [u32; 3] = [0x00360d00, 0x00360d38, 0x00360d38]; // aP0P00P0P00P0P0, _0: a negative year
const DATE_YEAR_FIRST: u32 = 0x00360d28; // "%04d/%02d/%02d"
const DATE_YEAR_LAST: u32 = 0x00360d60; // "%02d/%02d/%04d"
const TIME_NONE: [u32; 2] = [0x00360d70, 0x00360db0]; // aP0P00P0P00P0P0_1, _2: a negative hour
const TIME_24: u32 = 0x00360d98; // "\ap@0%2d\ap00:%02d:%02d"
const TIME_12: u32 = 0x00360df0; // the same and " %s"
const MORNING: u32 = 0x00360e10; // "\ar0.80\ap@AA\ap00M\ar0.00", the same with P
const AFTERNOON: u32 = 0x00360e28;
const SUMMER_MARK: u32 = 0x00365558; // "\ar0.88\ao020\ar0.00", ""
const NO_MARK: u32 = 0x00370138;
const TIME_LINE: u32 = 0x00370130; // "%s %s"
const DATE_RATIO: u32 = 0x0036fb94;
const HINT_RATIO: u32 = 0x0036fb98;
const VERSION_RATIO: u32 = 0x0036fbb0;
const VERSION_PAL: u32 = 0x003656c8;
// A Version row not filled yet holds null pointers (the record's count is 3 from the start, its table zero): the page draws the string at
// address 0 of the console's RAM, the first eight bytes of the kernel's exception vector, before the job's list is in.
const NULL_POINTER_TEXT: &[u8] = b"\x01\x80\x1a\x3c\x38\x59\x59\xff";
const PAL_MULTIPLY: u32 = 0x00365570; // func_00226300
const PAL_DIVIDE: u32 = 0x00365578;
const HINT_PAL_MULTIPLY: u32 = 0x00365590; // func_00226958
const HINT_PAL_DIVIDE: u32 = 0x00365598;
const ICON_PAL_MULTIPLY: u32 = 0x00365580; // DrawIcon
const ICON_PAL_DIVIDE: u32 = 0x00365588;
const PANELS: u32 = 0x002b2318; // four string ids per panel, 0x14 apart; + 0xA0 by video mode
const SLOTS: u32 = 0x002b2470; // the four slots' x, 16 bytes per language
const SLOT_PICTURES: u32 = 0x002b24f0; // the picture of each slot
const HINT_COLOUR: u32 = 0x002b2460;
const ICON_RECORD: u32 = 0x002b2260; // DrawIcon's rectangle record
const ICON_PLACES: u32 = 0x002b22a0; // u0, v0, u1, v1 per picture

// The menus' strings and colours (draw_clock_menu_items, browser_str_related, func_002311E8, clock_str_related).
const CHOSEN_COLOUR: u32 = 0x002b2540;
const PLAIN_COLOUR: u32 = 0x002b2550;
const VALUE_COLOUR: u32 = 0x002b2560;
const TITLE_COLOUR: u32 = 0x002b2570;
const ARROW: u32 = 0x003702c0; // "o018"
const ADJUST_TEMPLATE: u32 = 0x003655b0; // year, month, day: item, lowest, highest (func_00226E68)
const FORMAT_YEAR: u32 = 0x00370160; // "%04d", "%02d", "%2d"
const FORMAT_02: u32 = 0x00370168;
const FORMAT_2: u32 = 0x00370170;
const FIXED_OPEN: u32 = 0x00370178; // "p@0", "p00"
const FIXED_CLOSE: u32 = 0x00370180;
const SLASH: u32 = 0x00370188;
const SPACE: u32 = 0x00370190;
const COLON: u32 = 0x00370198;
const VALUE_TEMPLATE_12: u32 = 0x003655d8;
const VALUE_TEMPLATE_24: u32 = 0x00365600;
const FIELD_MORNING: u32 = 0x00365618;
const FIELD_AFTERNOON: u32 = 0x00365630;
const LIST_PAL_CHOSEN: u32 = 0x00365950;
const LIST_PAL_VALUE: u32 = 0x00365958;
const CLOCK_STRING: u32 = 0x00227420;
const ITEM_STRING: u32 = 0x00228470;
const ITEM_STRING_JUMP: u32 = 0x00227c60;
const CLOCK_EDIT: u32 = 0x00227ad0;
const ROW_EDIT: u32 = 0x00228660;

fn clamp_to(v: i32, top: i32) -> i32 {
    if v < 0 {
        0
    } else if v > top {
        top
    } else {
        v
    }
}

fn by128(x: i64) -> i32 {
    ((if x < 0 { x + 127 } else { x }) >> 7) as i32
}

fn divide(a: i64, b: i32) -> i32 {
    if b == 0 {
        panic!("text: a ramp or tail of length 0 divides");
    }
    (a / i64::from(b)) as i32
}

fn scale_of(r: &Ramp, n: i32) -> i32 {
    divide(i64::from(r.counter) * i64::from(n), r.length)
}

fn half(n: i32) -> i32 {
    n.wrapping_add(((n as u32) >> 31) as i32) >> 1
}

fn trunc_int(x: f64) -> i32 {
    x.trunc() as i32
}

// func_00231E78: the main menu's items and their button panel.
fn menu_alpha_of(r: &TextRamps, tail: i32, overlay: i32) -> i32 {
    let mut a = scale_of(&r.main_menu, 0x80);
    a = divide(i64::from(a) * i64::from(clamp_to(tail.wrapping_sub(r.config.counter), tail)), tail);
    a = by128(i64::from(a) * i64::from(0x80 - scale_of(&r.version, 0x80)));
    a = by128(i64::from(a) * i64::from(overlay));
    if r.dialog_closing.state != 0 || r.first_run.state != 0 {
        a = 0;
    }
    a
}

// func_00230E10: System Configuration's list and its button panel.
fn config_alpha_of(r: &TextRamps, tail: i32, menu: &Ramp) -> i32 {
    let a = divide(i64::from(clamp_to(r.config.counter.wrapping_sub(r.body.wrapping_add(r.lead)), tail)) << 7, tail);
    divide(i64::from(a) * i64::from(clamp_to(tail.wrapping_sub(menu.counter), tail)), tail)
}

// sprintf as the program uses it: %s, %d with a zero flag and a width.
fn format(pattern: &[u8], texts: &[Vec<u8>], numbers: &[i32]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut text = 0usize;
    let mut number = 0usize;
    let mut i = 0usize;
    while i < pattern.len() {
        if pattern[i] != b'%' {
            out.push(pattern[i]);
            i += 1;
            continue;
        }
        i += 1;
        let mut zero = false;
        if i < pattern.len() && pattern[i] == b'0' {
            zero = true;
            i += 1;
        }
        let mut width = 0usize;
        while i < pattern.len() && pattern[i].is_ascii_digit() {
            width = width * 10 + usize::from(pattern[i] - b'0');
            i += 1;
        }
        if i >= pattern.len() {
            panic!("text: a format ends in a conversion");
        }
        if pattern[i] == b's' {
            out.extend_from_slice(&texts[text]);
            text += 1;
        } else if pattern[i] == b'd' {
            let v = i64::from(numbers[number]);
            number += 1;
            let mut digits: Vec<u8> = (if v < 0 { -v } else { v }).to_string().into_bytes();
            let sign = usize::from(v < 0);
            while digits.len() + sign < width && zero {
                digits.insert(0, b'0');
            }
            if v < 0 {
                digits.insert(0, b'-');
            }
            while digits.len() < width {
                digits.insert(0, b' ');
            }
            out.extend_from_slice(&digits);
        } else if pattern[i] == b'%' {
            out.push(b'%');
        } else {
            panic!("text: the conversion %{} is not modelled", pattern[i] as char);
        }
        i += 1;
    }
    out
}

// What the program's filter hands the library for one character (fontFilterPutc 0x00212A20).
struct Character {
    code: i32,
    locate: Vec4,
    colour: [f32; 4],
    matrix: Mat4,
    fresh: bool,
}

// _scePFont_Getc (0x002928F8): UTF-8, 0 at the end, -1 on a broken sequence.
struct Reader<'a> {
    text: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn take(&mut self) -> i32 {
        if self.i >= self.text.len() {
            return 0;
        }
        let mut c = i32::from(self.text[self.i]);
        self.i += 1;
        if c < 0x80 {
            return c;
        }
        let mut more;
        if c < 0xc0 {
            return -1;
        }
        if c < 0xe0 {
            c &= 0x1f;
            more = 1;
        } else if c < 0xf0 {
            c &= 0x0f;
            more = 2;
        } else if c < 0xf8 {
            c &= 7;
            more = 3;
        } else {
            return -1;
        }
        while more > 0 {
            let next = if self.i < self.text.len() {
                let b = i32::from(self.text[self.i]);
                self.i += 1;
                b
            } else {
                0
            };
            if (next & 0xc0) != 0x80 {
                return -1;
            }
            c = (c << 6) | (next & 0x3f);
            more -= 1;
        }
        c
    }

    fn digit(&mut self) -> i32 {
        self.take() - 0x30
    }
}

struct Pen {
    pen: [f32; 2],
    reach: [f32; 2],
    fresh: bool,
    drew: bool,
    last_pen: f32,
    colour: [f32; 4],
}

impl Pen {
    fn widest(&mut self) {
        if self.reach[0] < self.pen[0] {
            self.reach[0] = self.pen[0];
        }
        if self.reach[1] < self.pen[1] {
            self.reach[1] = self.pen[1];
        }
    }
}

// facts/text.md: the date and time (func_00226300) and the button hint (func_002269E0) of the clock screen, through
// the program's font code (Font_PutsPackets 0x00213BA8, fontFilter 0x00212D78) and the library's
// (_scePFont_Putc 0x00291858), the cache carried from string to string and frame to frame.
#[derive(Debug, Clone)]
pub struct Text<A> {
    font_file: Arc<Font>,
    program: Arc<ProgramImage>,
    cache: FontCache,
    font: FontState,
    ramps: TextRamps,
    settings: TextSettings,
    library_colour: [f32; 4],
    cells: Vec<i32>,
    drawn: Vec<bool>,
    width: i32,
    height: i32,
    // D_00370158 and D_0037015C: the width of the clock value's template and the time format it was measured for.
    template_width: i32,
    template_format: i32,
    after_pages: bool,
    panel8: [i32; 4],
    arithmetic: PhantomData<A>,
}

impl<A: Arithmetic> Text<A> {
    pub fn new(font: Arc<Font>, program: Arc<ProgramImage>, inputs: &TextInputs) -> Result<Self> {
        let mut text = Self {
            font_file: font,
            program,
            cache: inputs.cache.clone(),
            font: inputs.font,
            ramps: inputs.ramps,
            settings: inputs.settings,
            library_colour: [0.0; 4],
            cells: Vec::new(),
            drawn: Vec::new(),
            width: 640,
            height: 224,
            template_width: 0,
            template_format: 0,
            after_pages: false,
            panel8: [1, 1, 1, 1],
            arithmetic: PhantomData,
        };
        // The library's context is read from the capture (scene.json's input.font); without it there is no cache to draw from.
        if text.cache.list.is_empty() {
            return Err(Error("text: the glyph cache is empty (the input has no font context); build the clock without text".into()));
        }
        text.library_colour = text.font.colour;
        text.cells = vec![0; text.cache.cells as usize];
        for e in &text.cache.list {
            if e.loaded != 0 && e.cell >= 0 && e.cell < text.cache.cells {
                text.cells[e.cell as usize] = e.code;
            }
        }
        // D_00370158 is written the first time the list draws the clock's value, which needs the configuration ramp running: an
        // input that starts with it running has the width already, measured at the size the list sets (1.0); the cache is left as it was.
        if inputs.ramps.config.state != 0 {
            let cache = text.cache.clone();
            let font = text.font;
            let colour = text.library_colour;
            text.font.ratio = 1.0;
            text.font.dirty = 1;
            let mut scratch = TextFrame::default();
            text.template_format = inputs.settings.time_format;
            let template = text.program.string(if text.template_format == 1 { VALUE_TEMPLATE_12 } else { VALUE_TEMPLATE_24 });
            text.template_width = text.width_of(&template, &mut scratch);
            text.cache = cache;
            text.font = font;
            text.library_colour = colour;
        }
        Ok(text)
    }

    pub fn cache(&self) -> &FontCache {
        &self.cache
    }

    pub fn font(&self) -> &FontState {
        &self.font
    }

    fn pal(&self) -> bool {
        self.settings.video_mode == 2
    }

    // func_002132B8: the size, times the width in percent when one is set.
    fn scale_x(s: &FontState) -> f32 {
        if s.percent != 0 { A::div(A::mul(s.ratio, A::from_int(s.percent)), 100.0) } else { s.ratio }
    }

    // updateTransMatrix (0x00213380): diag(x, x tv, 1, 1) moved by (pitch x, ascent 0.7 tv size), when the size changed.
    fn update_matrix(s: &mut FontState) {
        if s.dirty == 0 {
            return;
        }
        s.dirty = 0;
        let x = Self::scale_x(s);
        s.matrix = [
            [x, 0.0, 0.0, 0.0],
            [0.0, A::mul(x, s.tv), 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [A::mul(A::from_int(s.pitch), x), A::mul(A::mul(A::mul(A::from_int(s.ascent), as_float(0x3f333333)), s.tv), s.ratio), 0.0, 1.0],
        ];
    }

    // Font_SetRatio (0x002127B8): the size, and the matrix marked to be made again.
    fn set_ratio(&mut self, ratio: f32) {
        self.font.ratio = ratio;
        self.font.dirty = 1;
    }

    // Font_SetColor: each int times 1/128.
    fn set_colour(&mut self, r: i32, g: i32, b: i32, a: i32) {
        let step = as_float(0x3c000000);
        self.font.colour = [A::mul(A::from_int(r), step), A::mul(A::from_int(g), step), A::mul(A::from_int(b), step), A::mul(A::from_int(a), step)];
    }

    fn set_locate(&mut self, x: i32, y: i32) {
        self.font.locate = [A::from_int(x), A::from_int(y)];
    }

    // config_get_osd_language (0x00203DD8) with get_vidmode_with_fallback: the layout row of the hints.
    fn language(&self) -> i32 {
        let field = ((self.settings.settings_word >> 4) & 0x1f) as i32;
        if self.settings.video_mode <= 0 {
            return if field == 1 { 1 } else { 0 };
        }
        if field == 0 || field >= 8 {
            return 1;
        }
        field
    }

    // fontFilterPutc (0x00212A20): a fixed width centres the character in it; the clip skips what is off the screen.
    fn character(&mut self, code: i32, measuring: bool, p: &mut Pen, out: &mut TextFrame) {
        let font = Arc::clone(&self.font_file);
        let s = self.font;
        if s.decoration != 0 {
            panic!("text: a string with a background or an underline is not modelled");
        }
        let mut fixed_after = false;
        let mut after = 0.0f32;
        if s.fixed != 0 || s.clip != 0 {
            let Some(glyph) = font.glyph(code).or_else(|| font.glyph(0xd818)) else { return };
            let before = p.pen[0];
            let x = s.matrix[0][0];
            let pitch = s.matrix[3][0];
            if s.fixed != 0 {
                p.pen[0] = A::add(before, A::mul(A::from_int(s.fixed.wrapping_sub(glyph.metrics[6]) / 2), x));
                after = A::add(A::add(before, pitch), A::mul(A::from_int(s.fixed), x));
                fixed_after = true;
            } else {
                after = A::add(A::add(before, pitch), A::mul(A::from_int(glyph.metrics[6]), x));
            }
            if s.clip != 0 && (640.0 < before || after < 0.0) {
                p.pen[0] = after;
                p.widest();
                return;
            }
        }
        let c = Character { code, locate: [p.pen[0], p.pen[1], 0.0, 0.0], colour: p.colour, matrix: s.matrix, fresh: p.fresh };
        let moved = self.put_character(&c, measuring, out);
        // A character the library gives up leaves its pen where it was (the model has no pen for it; not captured).
        let pen_x = moved.unwrap_or(p.pen[0]);
        if moved.is_some() {
            p.drew = true;
            p.last_pen = pen_x;
            self.library_colour = p.colour;
        }
        p.pen[0] = if fixed_after { after } else { pen_x };
        p.fresh = false;
    }

    // References/model/clock_text.mjs putString with verify_text2.mjs stringOf: one string through Font_PutsPackets
    // (0x00213BA8) or calcDrawArea (0x00213D38), the library asked for each character in turn. Returns the farthest the pen
    // went (x), which is what a measurement gives.
    fn put_string(&mut self, text: &[u8], measuring: bool, out: &mut TextFrame) -> f32 {
        out.strings.push(StringRun { text: text.to_vec(), measuring, own: self.font });
        if !measuring {
            self.cache.set_up = 1;
        }
        Self::update_matrix(&mut self.font);
        let colour = if measuring { self.library_colour } else { self.font.colour };
        let start_x = if measuring { 0.0 } else { self.font.locate[0] };
        let start_y = if measuring { 0.0 } else { self.font.locate[1] };
        let mut p = Pen { pen: [start_x, start_y], reach: [start_x, start_y], fresh: true, drew: false, last_pen: 0.0, colour };
        let font = Arc::clone(&self.font_file);
        let program = Arc::clone(&self.program);
        let mut r = Reader { text, i: 0 };

        let mut c = r.take();
        while c > 0 {
            if c >= 0x20 {
                if c == 0xfeff || c == 0xfffe || c == 0xffff {
                    c = r.take();
                    continue;
                }
                self.character(c, measuring, &mut p, out);
            } else if c == 9 {
                let along = A::sub(A::add(p.pen[0], 63.0), 1.0);
                p.pen[0] = A::sub(along, A::cut(f64::from(along) % 63.0));
            } else if c == 10 {
                p.fresh = true;
                p.pen[0] = start_x;
                p.pen[1] = A::add(p.pen[1], A::from_int(self.font.line_height));
            } else if c == 7 {
                let letter = r.take();
                if letter == i32::from(b'c') {
                    let n = r.digit();
                    let at = ESCAPE_COLOURS.wrapping_add(12u32.wrapping_mul(n as u32));
                    let step = as_float(0x3c000000);
                    p.colour = [
                        A::mul(A::from_int(program.integer(at)), step),
                        A::mul(A::from_int(program.integer(at.wrapping_add(4))), step),
                        A::mul(A::from_int(program.integer(at.wrapping_add(8))), step),
                        p.colour[3],
                    ];
                } else if letter == i32::from(b'a') {
                    let hundreds = r.digit() * 100;
                    let tens = r.digit() * 10;
                    let n = hundreds + tens + r.digit();
                    p.colour[3] = A::div(A::from_int(n), 255.0);
                } else if letter == i32::from(b'p') {
                    let first = r.take();
                    if first == 0x40 {
                        if let Some(glyph) = font.glyph(r.take()) {
                            self.font.fixed = glyph.metrics[6];
                        }
                    } else {
                        self.font.fixed = (first - 0x30) * 10 + r.digit();
                    }
                } else if letter == i32::from(b'r') {
                    let mut n = r.digit() * 100;
                    r.take();
                    n += r.digit() * 10;
                    n += r.digit();
                    self.font.percent = n;
                    self.font.dirty = 1;
                    Self::update_matrix(&mut self.font);
                } else if letter == i32::from(b's') {
                    p.pen[0] = A::add(p.pen[0], A::from_int(self.font.blank));
                } else if letter == i32::from(b'y') {
                    let sign = r.take();
                    let hex = |v: i32| {
                        if (0x30..=0x39).contains(&v) {
                            v - 0x30
                        } else if (0x61..=0x66).contains(&(v | 0x20)) {
                            (v | 0x20) - 0x57
                        } else {
                            0
                        }
                    };
                    let mut n = hex(r.take()) * 16;
                    n += hex(r.take());
                    if n != 0 && sign == 0x2d {
                        n = -n;
                    }
                    p.pen[1] = A::add(start_y, A::mul(A::from_int(n), 0.0625));
                } else if letter == i32::from(b'o') {
                    let hundreds = r.digit() * 100;
                    let tens = r.digit() * 10;
                    let code = 0xd800 + hundreds + tens + r.digit();
                    self.character(code, measuring, &mut p, out);
                }
            }
            p.widest();
            c = r.take();
        }
        // Font_PutsPackets reads the pen and the colour back from the library.
        if !measuring {
            self.font.colour = p.colour;
            if p.drew {
                self.font.locate = [p.last_pen, p.pen[1]];
            }
        }
        p.reach[0]
    }

    // References/model/clock_text.mjs carryList with verify_text2.mjs putCharacter: the cache entry for the code moves to
    // the head of the list (taking the cell of the least recently used entry passed when it has none), its picture is
    // uploaded when it is not loaded, and the twelve-vertex fan is placed (facts/text.md sections 2 and 3).
    fn put_character(&mut self, c: &Character, measuring: bool, out: &mut TextFrame) -> Option<f32> {
        let font = Arc::clone(&self.font_file);
        let list = &mut self.cache.list;
        let mut at = 0usize;
        let mut spare: Option<usize> = None;
        if list[0].cell != -1 {
            spare = Some(0);
        }
        if list[0].code != c.code && list[0].code != 0 && list.len() > 1 {
            at = 1;
            loop {
                if list[at].code == c.code {
                    break;
                }
                if list[at].cell != -1 {
                    spare = Some(at);
                }
                if at == list.len() - 1 || list[at].code == 0 {
                    break;
                }
                at += 1;
            }
        }
        let mut entry = list[at];
        if let Some(spare) = spare
            && entry.cell == -1
            && spare != at
        {
            entry.loaded = 0;
            entry.cell = list[spare].cell;
            list[spare].cell = -1;
            list[spare].loaded = 0;
        }
        let found: Option<Glyph<'_>> = font.glyph(c.code);
        if entry.code != c.code {
            entry.loaded = 0;
            entry.code = c.code;
            entry.block = found.map_or(0, |g| g.block.at);
        }
        list.remove(at);
        list.insert(0, entry);
        // _scePFont_Putc (0x00291958..0x0029197C, 0x00291980): the entry keeps the code with no block, and the character is
        // given up (-2) with nothing drawn.
        let glyph = found?;
        if entry.block == 0 {
            return None;
        }
        let block = glyph.block;

        if !measuring {
            if entry.loaded == 0 {
                // _scePFontSetupTexCache: a block of another cell size lays the cache out again.
                if self.cache.block != block.at {
                    if (block.flags & 7) != 0 {
                        panic!("text: a block that is not 4 bits a pixel: its cache is not modelled");
                    }
                    let cell_w = (block.width + 2 + 7) & !7;
                    let cell_h = (block.height + 2 + 3) & !3;
                    if !(self.cache.format == 0x14 && self.cache.cell_w == cell_w && self.cache.cell_h == cell_h) {
                        if self.drawn.iter().any(|&drawn| drawn) {
                            panic!("text: the cache is laid out again after a glyph of this frame was drawn from it");
                        }
                        let pages = (self.cache.memory.wrapping_add(0x7ff) >> 11) as i32;
                        let mut across = A::to_int(A::sqrt(A::from_int(pages)));
                        while pages % across != 0 {
                            across -= 1;
                        }
                        let cache = &mut self.cache;
                        cache.format = 0x14;
                        cache.cell_w = cell_w;
                        cache.cell_h = cell_h;
                        cache.width = across * 0x80;
                        cache.height = (pages / across) * 0x80;
                        cache.cells = (cache.width / cell_w) * (cache.height / cell_h);
                        cache.log_w = 32 - ((cache.width - 1) as u32).leading_zeros() as i32;
                        cache.log_h = 32 - ((cache.height - 1) as u32).leading_zeros() as i32;
                        let cells = cache.cells;
                        for (i, e) in cache.list.iter_mut().enumerate() {
                            e.loaded = 0;
                            e.cell = if (i as i32) < cells { i as i32 } else { -1 };
                        }
                        self.cells = vec![0; cells as usize];
                        self.drawn = vec![false; cells as usize];
                    }
                    self.cache.set_up = 1;
                    self.cache.block = block.at;
                }
                // _scePFontUpdateTex: the picture into the entry's cell, the one the lay-out dealt the head of the list.
                let head = self.cache.list[0].cell;
                let cell = head as usize;
                if head < 0 || cell >= self.drawn.len() {
                    panic!("text: the entry for code {} has cell {} of {}", c.code, head, self.drawn.len());
                }
                if self.drawn[cell] {
                    panic!("text: a cell drawn in this frame is given another glyph in the same frame");
                }
                self.cells[cell] = c.code;
                self.cache.list[0].loaded = 1;
            }
            self.cache.set_up = 0;
        }

        let [origin_x, baseline, left, right, top, bottom, advance] = glyph.metrics;
        let x0 = A::from_int(left - 1);
        let x1 = A::from_int(right + 1);
        let y_top = A::from_int(-(top + 1));
        let y_bottom = A::from_int(-(bottom - 1));
        let pen = 0.0f32;
        let step = A::from_int(advance);
        let shape: [[f32; 2]; 11] = [
            [pen, 0.0],
            [pen, y_top],
            [x0, y_top],
            [x0, 0.0],
            [x0, y_bottom],
            [pen, y_bottom],
            [step, y_bottom],
            [x1, y_bottom],
            [x1, 0.0],
            [x1, y_top],
            [step, y_top],
        ];
        let scale: Vec4 = [block.scale_x, block.scale_y, 1.0, 1.0];
        let mut m: Mat4 = [[0.0; 4]; 4];
        for r in 0..4 {
            for i in 0..4 {
                m[r][i] = A::mul(c.matrix[r][i], scale[i]);
            }
        }
        for i in 0..4 {
            m[3][i] = A::add(m[3][i], c.locate[i]);
        }
        let mut placed: [Vec4; 11] = [[0.0; 4]; 11];
        for k in 0..shape.len() {
            placed[k] = Matrix::<A>::apply(&m, &[shape[k][0], shape[k][1], 0.0, 1.0]);
        }
        // The first character after the pen was set starts at the pen: the column through the pen is brought back to it.
        if c.fresh {
            let rise = A::sub(placed[5][1], placed[1][1]);
            let shift = if rise == 0.0 {
                A::sub(placed[1][0], c.locate[0])
            } else {
                A::sub(A::add(A::div(A::mul(A::sub(c.locate[1], placed[1][1]), A::sub(placed[5][0], placed[1][0])), rise), placed[1][0]), c.locate[0])
            };
            for v in placed.iter_mut() {
                v[0] = A::sub(v[0], shift);
            }
        }
        // The pen moves to where the advance's column crosses the pen's height.
        let rise = A::sub(placed[10][1], placed[6][1]);
        let pen_x = if rise == 0.0 {
            placed[6][0]
        } else {
            A::add(A::div(A::mul(A::sub(c.locate[1], placed[6][1]), A::sub(placed[10][0], placed[6][0])), rise), placed[6][0])
        };
        if measuring {
            return Some(pen_x);
        }

        let columns = self.cache.width / self.cache.cell_w;
        let cell = self.cache.list[0].cell;
        let cell_x = (cell % columns) * self.cache.cell_w;
        let cell_y = (cell / columns) * self.cache.cell_h;
        self.drawn[cell as usize] = true;
        let s0 = cell_x + 1;
        let t0 = cell_y + 1;
        let s_pen = origin_x + s0;
        let t_base = baseline + t0;
        let s_left = left - 1 + s_pen;
        let s_right = right + 1 + s_pen;
        let s_advance = advance + s_pen;
        let t_top = -(top + 1) + t_base;
        let t_bottom = -(bottom - 1) + t_base;
        let texel: [[i32; 2]; 11] = [
            [s_pen, t_base],
            [s_pen, t_top],
            [s_left, t_top],
            [s_left, t_base],
            [s_left, t_bottom],
            [s_pen, t_bottom],
            [s_advance, t_bottom],
            [s_right, t_bottom],
            [s_right, t_base],
            [s_right, t_top],
            [s_advance, t_top],
        ];
        let per_s = A::div(1.0, A::from_int(1 << self.cache.log_w));
        let per_t = A::div(1.0, A::from_int(1 << self.cache.log_h));
        let mut colour = [0u8; 4];
        for i in 0..4 {
            colour[i] = A::to_int(A::mul(c.colour[i], 128.0)) as u8;
        }
        // The screen matrix: the unit matrix moved to the GS's centre (2048 - W/2, 2048 - H/2).
        let origin_xs = A::from_int(0x800 - (self.width >> 1));
        let origin_ys = A::from_int(0x800 - (self.height >> 1));
        let screen: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [origin_xs, origin_ys, 0.0, 1.0]];
        let mut draw = TextDraw { kind: TextDrawKind::Glyph, ..Default::default() };
        draw.glyph.region = [s0 - 1, s0 + block.width, t0 - 1, t0 + block.height];
        for k in 0..placed.len() {
            let on = Matrix::<A>::apply(&screen, &placed[k]);
            let q = A::div(1.0, on[3]);
            let v = &mut draw.glyph.fan[k];
            v.s = A::mul(A::mul(A::from_int(texel[k][0]), per_s), q);
            v.t = A::mul(A::mul(A::from_int(texel[k][1]), per_t), q);
            v.q = A::mul(A::mul(1.0, 1.0), q);
            v.colour = colour;
            v.x = A::to_int(A::mul(A::mul(on[0], q), 16.0)) as f32 / 16.0 - origin_xs;
            v.y = A::to_int(A::mul(A::mul(on[1], q), 16.0)) as f32 / 16.0 - origin_ys;
        }
        draw.glyph.fan[11] = draw.glyph.fan[1];
        out.draws.push(draw);
        Some(pen_x)
    }

    // func_00213EE8: the measured width, (int)((int)reach + pitch x scale).
    fn width_of(&mut self, text: &[u8], out: &mut TextFrame) -> i32 {
        let x = Self::scale_x(&self.font);
        let reach = self.put_string(text, true, out);
        A::to_int(A::add(A::from_int(A::to_int(reach)), A::mul(A::from_int(self.font.pitch), x)))
    }

    // func_00213EE8: the width of a string through calcDrawArea at the font state as it stands.
    pub fn string_width(&mut self, text: &[u8]) -> i32 {
        let mut scratch = TextFrame::default();
        self.width_of(text, &mut scratch)
    }

    // DrawIcon (0x00226508): a button's picture, 28 wide from texture 8 (pictures 0 and 1) or 25 wide from texture 9, half
    // as high, its corners from D_002B22A0; in PAL its bottom is scaled by 0.5405 / 0.47 (facts/text.md section 5).
    fn icon(&mut self, picture: i32, x: i32, y: i32, alpha: i32, out: &mut TextFrame) {
        let wide = (0..2).contains(&picture);
        let size = if wide { 0x1c } else { 0x19 };
        let p = &*self.program;
        let mut r = Rect::default();
        for i in 0..4 {
            r.colour[i] = p.integer(ICON_RECORD + 4 * i as u32);
        }
        r.z = p.integer(ICON_RECORD + 0x30);
        r.blend = p.integer(ICON_RECORD + 0x34);
        r.textured = p.integer(ICON_RECORD + 0x38);
        r.colour[3] = alpha;
        r.x0 = x << 4;
        r.y0 = y << 4;
        r.x1 = (x + size) << 4;
        r.y1 = (y + ((size as u32) >> 1) as i32) << 4;
        let place = ICON_PLACES.wrapping_add(16u32.wrapping_mul(picture as u32));
        r.u0 = (p.integer(place) << 4) + 8;
        r.v0 = (p.integer(place + 4) << 4) + 8;
        r.u1 = (p.integer(place + 8) << 4) + 8;
        r.v1 = (p.integer(place + 12) << 4) + 8;
        if self.pal() {
            r.y1 = trunc_int(f64::from(r.y0) + f64::from((r.y1 - r.y0) as f32) * p.double_at(ICON_PAL_MULTIPLY) / p.double_at(ICON_PAL_DIVIDE));
        }
        out.draws.push(TextDraw { kind: TextDrawKind::Icon, icon: r, texture: if wide { 8 } else { 9 }, ..Default::default() });
    }

    // draw_button_panel (0x00226770): the panel's four slots, each a string id (1 for none); a slot's picture at the
    // slot's x for the language and its text 28 to the right, one line lower; the fourth slot ends 24 from the right.
    fn button_panel(&mut self, panel: i32, alpha: i32, y: i32, input: &TextFrameInputs, out: &mut TextFrame) {
        let p = Arc::clone(&self.program);
        let row = self.language();
        self.set_ratio(p.single(HINT_RATIO));
        let video = if self.settings.video_mode > -1 { self.settings.video_mode } else { 0 };
        let table = PANELS.wrapping_add(0x14u32.wrapping_mul(panel as u32)).wrapping_add(if video != 0 { 0xa0 } else { 0 });
        let slots = SLOTS.wrapping_add(16u32.wrapping_mul(row as u32));
        for slot in 0..4u32 {
            let id = if panel == 8 { self.panel8[slot as usize] } else { p.integer(table + 4 * slot) };
            if id == 1 {
                continue;
            }
            let text = self.language_string(id);
            let x = if slot == 3 {
                let reach = self.width_of(&text, out) + 0x18;
                self.icon(p.integer(SLOT_PICTURES + 0xc), input.width - reach - 0x1c, y, alpha, out);
                input.width - reach
            } else {
                self.icon(p.integer(SLOT_PICTURES + 4 * slot), p.integer(slots + 4 * slot), y, alpha, out);
                p.integer(slots + 4 * slot) + 0x1c
            };
            // DrawNonSelectableItem: the colour D_002B2460 with the panel's alpha, one line under the picture's top.
            self.set_colour(p.integer(HINT_COLOUR), p.integer(HINT_COLOUR + 4), p.integer(HINT_COLOUR + 8), alpha);
            self.set_locate(x, y + 1);
            self.put_string(&text, false, out);
        }
        self.set_ratio(1.0);
    }

    pub fn frame(&mut self, input: &TextFrameInputs) -> TextFrame {
        let p = Arc::clone(&self.program);
        self.width = input.width;
        self.height = input.height;
        if !self.after_pages {
            self.drawn = vec![false; self.cells.len()];
        }
        self.after_pages = false;
        let ramps = input.ramps.unwrap_or(self.ramps);
        self.panel8 = ramps.panel8_ids;
        let mut out = TextFrame::default();

        // func_00226300: the date at the left and the time ending 22 from the right, 14 from the top (32 when item 0 is 2).
        let mut y = if input.item0 == 2 { 0x20 } else { 0xe };
        if self.pal() {
            y = trunc_int(f64::from(y as f32) * p.double_at(PAL_MULTIPLY) / p.double_at(PAL_DIVIDE));
        }
        self.set_ratio(p.single(DATE_RATIO));
        // func_00230008: 128 unless a dialog closes (or the first run's ramp holds it at 0), by the ramps of System
        // Configuration otherwise; times the overlay level.
        let date_alpha = if ramps.dialog_closing.state != 0 {
            0x80
        } else if ramps.first_run.state != 0 {
            0
        } else {
            divide(i64::from(clamp_to(ramps.config.counter.wrapping_sub(ramps.lead.wrapping_add(ramps.body)), input.tail)) << 7, input.tail)
        };
        self.set_colour(0x60, 0x60, 0x60, by128(i64::from(date_alpha) * i64::from(input.overlay_level)));

        // do_format_date (0x00214550) by item 0xE; do_format_time (0x00214640) by item 0xD.
        let t = &input.items;
        let date = match self.settings.date_format {
            0 => {
                if t.year < 0 {
                    p.string(DATE_NONE[0])
                } else {
                    format(&p.string(DATE_YEAR_FIRST), &[], &[t.year, t.month, t.day])
                }
            }
            1 => {
                if t.year < 0 {
                    p.string(DATE_NONE[1])
                } else {
                    format(&p.string(DATE_YEAR_LAST), &[], &[t.month, t.day, t.year])
                }
            }
            2 => {
                if t.year < 0 {
                    p.string(DATE_NONE[2])
                } else {
                    format(&p.string(DATE_YEAR_LAST), &[], &[t.day, t.month, t.year])
                }
            }
            other => panic!("text: date format {other} gives no string"),
        };
        self.set_locate(0x16, y);
        self.put_string(&date, false, &mut out);
        // config_get_daylight_saving: bit 29 of the settings word.
        let mark = p.string(if ((self.settings.settings_word >> 29) & 1) != 0 { SUMMER_MARK } else { NO_MARK });
        let time = match self.settings.time_format {
            0 => {
                if t.hour < 0 {
                    p.string(TIME_NONE[0])
                } else {
                    format(&p.string(TIME_24), &[], &[t.hour, t.minute, t.second])
                }
            }
            1 => {
                if t.hour < 0 {
                    p.string(TIME_NONE[1])
                } else {
                    let twelve = if t.hour % 12 != 0 { t.hour % 12 } else { 12 };
                    format(&p.string(TIME_12), &[p.string(if t.hour < 12 { MORNING } else { AFTERNOON })], &[twelve, t.minute, t.second])
                }
            }
            other => panic!("text: time format {other} gives no string"),
        };
        let line = format(&p.string(TIME_LINE), &[mark, time], &[]);
        let width = self.width_of(&line, &mut out);
        self.set_locate(input.width - width - 0x16, y);
        self.put_string(&line, false, &mut out);

        // func_002269E0: the button panels and their alpha; func_00226958: their line, 200 (182 when item 0 is 2).
        let mut hint_y = if input.item0 == 2 { 0xb6 } else { 0xc8 };
        if self.pal() {
            hint_y = trunc_int(f64::from(hint_y as f32) * p.double_at(HINT_PAL_MULTIPLY) / p.double_at(HINT_PAL_DIVIDE));
        }
        if ramps.panel7 == 1 {
            self.button_panel(7, 0x80, hint_y, input, &mut out);
        } else {
            if ramps.panel8_on != 0 {
                self.button_panel(8, ramps.panel8, hint_y, input, &mut out);
            }
            // func_002326F0: the first of panels 1 to 6 whose alpha is above 0 (0x00226A6C returns after it), by func_00231E78, func_00230E10, func_0022A238 (1 and 0), func_00228F40,
            // func_00230C28, capped at 128 (verify_text2.mjs panelsOf).
            let menu = input.menu;
            let tail = input.tail;
            let adjust = |side: i32| {
                let a = by128(i64::from(scale_of(&ramps.version, 0x80)) * i64::from(0x80 - scale_of(&ramps.dialog, 0x80)));
                if a <= 0 {
                    return 0;
                }
                if ((if ramps.adjust_row != 0 { 1 } else { 0 }) ^ side) == 0 { a } else { 0 }
            };
            for panel in 1..7 {
                let mut a = match panel {
                    1 => menu_alpha_of(&ramps, tail, input.overlay_level),
                    2 => config_alpha_of(&ramps, tail, &menu),
                    3 => adjust(1),
                    4 => adjust(0),
                    5 => scale_of(&ramps.dialog, 0x80),
                    _ => divide(i64::from(clamp_to(menu.counter.wrapping_sub(ramps.body), tail)) << 7, tail),
                };
                if a > 0x80 {
                    a = 0x80;
                }
                if a > 0 {
                    self.button_panel(panel, a, hint_y, input, &mut out);
                    break;
                }
            }
        }

        self.finish(&mut out);
        out
    }

    // get_lang_string (0x002081B8): the language's table, as config_set_langtbl chose it (clock_text.mjs languageOf).
    fn language_string(&self, mut id: i32) -> Vec<u8> {
        let p = &*self.program;
        if self.settings.video_mode > 0 && (id == 0x55 || id == 0x56) {
            id = 0x55 + 0x56 - id;
        }
        let field = (self.settings.settings_word >> 4) & 0x1f;
        let language = if field != 0 { field } else { 1 };
        let strings = p.word(LANGUAGE_TABLES.wrapping_add(4u32.wrapping_mul(language)));
        p.string(p.word(strings.wrapping_add(4u32.wrapping_mul(id as u32))))
    }

    fn colour_from(&mut self, at: u32, alpha: i32) {
        let p = Arc::clone(&self.program);
        self.set_colour(p.integer(at), p.integer(at + 4), p.integer(at + 8), alpha);
    }

    // DrawNonSelectableItem (0x002269E6..): the colour at `colour` with the alpha, the pen, the string; returns what
    // func_00213E90 gives the caller after it: (int)((int)(reach - start) + pitch x scale).
    fn draw_item(&mut self, x: i32, y: i32, colour: u32, alpha: i32, text: &[u8], out: &mut TextFrame) -> i32 {
        self.colour_from(colour, alpha);
        self.set_locate(x, y);
        let start = self.font.locate[0];
        let reach = self.put_string(text, false, out);
        A::to_int(A::add(A::from_int(A::to_int(A::sub(reach, start))), A::mul(A::from_int(self.font.pitch), Self::scale_x(&self.font))))
    }

    // draw_menu_item (0x00226C00): nothing under alpha 16; otherwise the string centred on x by its measured width.
    fn menu_item(&mut self, x: i32, y: i32, colour: u32, alpha: i32, text: &[u8], out: &mut TextFrame) {
        if alpha < 16 {
            return;
        }
        let width = self.width_of(text, out);
        self.draw_item(x - half(width), y, colour, alpha, text, out);
    }

    // clock_str_related (0x002270A8): the width of the value's template, measured when the time format changes.
    fn template_width(&mut self, time_format: i32, out: &mut TextFrame) -> i32 {
        if self.template_width == 0 || self.template_format != time_format {
            self.template_format = time_format;
            let template = self.program.string(if time_format == 1 { VALUE_TEMPLATE_12 } else { VALUE_TEMPLATE_24 });
            self.template_width = self.width_of(&template, out);
        }
        self.template_width
    }

    // draw_clock_menu_items (0x00232170): item n centred on 430, 16 apart from 14 above the middle; the chosen one in
    // D_002B2540, the others in D_002B2550. func_00232020 and the main menu's ramp decide whether it draws at all.
    fn main_menu_items(&mut self, input: &PagesInputs, out: &mut TextFrame) {
        let r = &input.ramps;
        if r.main_menu.state != 2 {
            return;
        }
        if r.config.state != 0 || r.version.state != 0 || r.dialog_closing.state != 0 || r.first_run.state != 0 || input.pending != 0 {
            return;
        }
        let alpha = menu_alpha_of(r, input.tail, input.overlay_level);
        let p = Arc::clone(&self.program);
        self.set_ratio(1.0);
        for n in 0..input.main_menu.count {
            let id = p.integer(input.main_menu.items.wrapping_add(16u32.wrapping_mul(n as u32)));
            let name = self.language_string(id);
            self.menu_item(430, half(input.height) - 14 + 16 * n, if n == input.main_menu.selected { CHOSEN_COLOUR } else { PLAIN_COLOUR }, alpha, &name, out);
        }
    }

    // func_0022A410: the Version page. The title centred on 404 (27 above the table's top, 31.05 on PAL), the rows from two above the
    // first shown one, a line (11, 13 on PAL) a row whether it shows or not; the label right-aligned on 391, the value at 417; the
    // chosen row's label in the chosen colour when its module has sub-rows. The page's alpha is func_0022A1D8's.
    fn version_page(&mut self, input: &PagesInputs, out: &mut TextFrame) {
        let r = &input.ramps;
        if r.version.state == 0 || r.dialog.state == 2 {
            return;
        }
        let v = &input.version;
        let p = Arc::clone(&self.program);
        let alpha = by128(i64::from(scale_of(&r.version, 0x80)) * i64::from(0x80 - scale_of(&r.dialog, 0x80)));
        let centre = f64::from(half(input.height));
        let top = trunc_int(centre - (if self.pal() { p.double_at(VERSION_PAL) } else { 6.0 }));
        let title_y = trunc_int(f64::from(top) - (if self.pal() { p.double_at(VERSION_PAL + 8) } else { 27.0 }));
        let first = trunc_int(0.0 - (if self.pal() { p.double_at(VERSION_PAL + 16) } else { 17.0 }));
        let step = if self.pal() { 13 } else { 11 };
        self.set_ratio(p.single(VERSION_RATIO));
        let title = self.language_string(v.title);
        self.menu_item(0x194, title_y, TITLE_COLOUR, alpha, &title, out);
        let mut y = first + top;
        let mut row = v.first - 2;
        while row < v.first + v.shown + 4 {
            if !(row < 0 || row >= v.count || row < v.first || row >= v.first + v.shown) {
                let entry = &v.rows[row as usize];
                let label = if entry.label != 0 { self.language_string(entry.label) } else { NULL_POINTER_TEXT.to_vec() };
                let value = if entry.value.is_empty() { NULL_POINTER_TEXT.to_vec() } else { entry.value.clone() };
                let mut label_colour = PLAIN_COLOUR;
                let mut value_colour = PLAIN_COLOUR;
                if row == v.selected {
                    label_colour = if entry.sub_rows != 0 { CHOSEN_COLOUR } else { VALUE_COLOUR };
                    value_colour = VALUE_COLOUR;
                }
                if alpha >= 16 {
                    let width = self.width_of(&label, out);
                    self.draw_item(0x187 - width, y, label_colour, alpha, &label, out);
                }
                self.draw_item(0x1a1, y, value_colour, alpha, &value, out);
            }
            row += 1;
            y += step;
        }
    }

    // browser_str_related (0x00231388): the title, the widest entry's width (browser_str_related2), the arrow and the one or two
    // entries of the crossfade (R4); the places and colours are verify_text2.mjs LIST_Y and PLACES.
    fn list(&mut self, input: &PagesInputs, out: &mut TextFrame, unmodelled: &mut Vec<u32>) -> Option<i32> {
        let r = &input.ramps;
        let p = Arc::clone(&self.program);
        let top = if self.pal() { 0x65 } else { 0x58 };
        let chosen_y = trunc_int(f64::from(top) + (if self.pal() { p.double_at(LIST_PAL_CHOSEN) } else { 24.0 }));
        if r.config.state == 0 || input.menu_ramp.state == 2 {
            return None;
        }
        let alpha = config_alpha_of(r, input.tail, &input.menu_ramp);
        self.set_ratio(1.0);
        let title = self.language_string(input.page.word0);
        self.menu_item(430, top, TITLE_COLOUR, alpha, &title, out);
        let mut widest = 0;
        for i in 0..input.page.count {
            let name = self.language_string(input.entries[i as usize].id);
            self.width_of(&name, out);
            widest = widest.max(self.width_of(&name, out));
        }
        let arrow = p.string(ARROW);
        let reach = half(widest) + self.width_of(&arrow, out) + 0x1be;
        let centre = if reach < input.width - 0x18 { 430 } else { 430 - (reach + 0x18 - input.width) };
        if input.page.level != 1 && r.config.state == 2 && input.menu_ramp.state == 0 {
            // The pulse after the chosen entry: |(int)(sinf(counter x 0x7AA8 / (fps x 0x7AA8 / 60) / divisor) x 128)|.
            let period = (if self.pal() { 50 } else { 60 }) * 0x7aa8 / 60;
            let counter = ((input.page.glow as u32).wrapping_mul(0x7aa8) as i32) / period;
            let pulse = A::to_int(A::mul(A::sinf(A::div(A::from_int(counter), input.list_constants.divisor)), 128.0));
            self.draw_item(centre + half(widest) + 0x10, chosen_y, VALUE_COLOUR, if pulse < 0 { pulse.wrapping_neg() } else { pulse }, &arrow, out);
        }
        self.list_entry(input, input.page.selected, by128(i64::from(input.list_fade.first) * i64::from(alpha)), out, unmodelled);
        if input.list_fade.second != 0 {
            self.list_entry(input, input.list_fade.second_index, by128(i64::from(input.list_fade.second) * i64::from(alpha)), out, unmodelled);
        }
        Some(widest)
    }

    // func_002311E8: one entry of the list, its name under the title and its value under that.
    fn list_entry(&mut self, input: &PagesInputs, index: i32, alpha: i32, out: &mut TextFrame, unmodelled: &mut Vec<u32>) {
        let p = Arc::clone(&self.program);
        let top = if self.pal() { 0x65 } else { 0x58 };
        let chosen_y = trunc_int(f64::from(top) + (if self.pal() { p.double_at(LIST_PAL_CHOSEN) } else { 24.0 }));
        let value_y = trunc_int(f64::from(top) + (if self.pal() { p.double_at(LIST_PAL_VALUE) } else { 42.0 }));
        let entry = input.entries[index as usize];
        let name = self.language_string(entry.id);
        let arrows = self.width_of(&p.string(ARROW), out) + 0x10;
        let width = self.width_of(&name, out);
        let end = 430 + half(width) + arrows;
        let centre = if end < input.width - 0x18 { 430 } else { 430 - (end + 0x18 - input.width) };
        let inside = input.page.level == 1;
        self.menu_item(centre, chosen_y, if inside { VALUE_COLOUR } else { CHOSEN_COLOUR }, alpha, &name, out);
        let editing = inside && input.page.selected == index;
        if !self.entry_value(input, &entry, 430, value_y, alpha, editing, out) {
            unmodelled.push(if editing { entry.frame_callback } else { entry.string_callback });
        }
    }

    // The entry's callback at +0x18 (its value), or at +0x1C while it is being changed.
    #[allow(clippy::too_many_arguments)]
    fn entry_value(&mut self, input: &PagesInputs, entry: &ConfigEntry, x: i32, y: i32, alpha: i32, editing: bool, out: &mut TextFrame) -> bool {
        let p = Arc::clone(&self.program);
        let callback = if editing { entry.frame_callback } else { entry.string_callback };
        if callback == CLOCK_STRING {
            // func_00226E68: the first three fields in the order of the date format, from the template at D_003655B0.
            let mut fields = input.adjust_fields;
            let field = |n: u32| AdjustField {
                item: p.integer(ADJUST_TEMPLATE + 12 * n),
                lowest: p.integer(ADJUST_TEMPLATE + 12 * n + 4),
                highest: p.integer(ADJUST_TEMPLATE + 12 * n + 8),
            };
            match input.items[0xe] {
                0 => {
                    fields[0] = field(0);
                    fields[1] = field(1);
                    fields[2] = field(2);
                }
                1 => {
                    fields[0] = field(1);
                    fields[1] = field(2);
                    fields[2] = field(0);
                }
                2 => {
                    fields[0] = field(2);
                    fields[1] = field(1);
                    fields[2] = field(0);
                }
                _ => {}
            }
            return self.clock_value(input, entry, &fields, x, y, alpha, false, out);
        } else if callback == CLOCK_EDIT {
            return self.clock_value(input, entry, &input.adjust_fields, x, y, alpha, true, out);
        } else if callback == ITEM_STRING || callback == ITEM_STRING_JUMP {
            // func_002283C0: the row of the value table the item holds, then clock_config_get_item_str's string for it.
            if entry.item < 0 || entry.item as usize >= input.items.len() {
                return false;
            }
            let value = input.items[entry.item as usize];
            let table = entry.value_table;
            let mut index = entry.value_index;
            if value != p.integer(table.wrapping_add(32u32.wrapping_mul(index as u32))) {
                index = 0;
                if entry.value_count > 0 && value != p.integer(table) {
                    for row in 1..entry.value_count {
                        if value == p.integer(table.wrapping_add(32u32.wrapping_mul(row as u32))) {
                            index = row;
                            break;
                        }
                    }
                }
            }
            let name = self.language_string(p.integer(table.wrapping_add(32u32.wrapping_mul(index as u32)).wrapping_add(4)));
            self.menu_item(x, y, VALUE_COLOUR, alpha, &name, out);
        } else if callback == ROW_EDIT {
            self.value_row(entry, x, y, alpha, out);
        } else {
            return false;
        }
        true
    }

    // func_002284F8: every value of the entry side by side, 16 apart, centred on x; the chosen one in D_002B2540.
    fn value_row(&mut self, entry: &ConfigEntry, x: i32, y: i32, alpha: i32, out: &mut TextFrame) {
        let p = Arc::clone(&self.program);
        let name = |text: &Self, row: i32| text.language_string(p.integer(entry.value_table.wrapping_add(32u32.wrapping_mul(row as u32)).wrapping_add(4)));
        let mut total = 0;
        for row in 0..entry.value_count {
            let n = name(self, row);
            total += self.width_of(&n, out);
        }
        total += (entry.value_count - 1) * 16;
        let mut at = x - half(total);
        for row in 0..entry.value_count {
            let n = name(self, row);
            self.draw_item(at, y, if row == entry.value_index { CHOSEN_COLOUR } else { PLAIN_COLOUR }, alpha, &n, out);
            let n = name(self, row);
            at += self.width_of(&n, out) + 16;
        }
    }

    // clock_str_related (0x002270A8): the date and time field by field (the program draws each as \ap@0, the digits, \ap00,
    // then the separator), the pen carried by func_00213E90; `editing` draws the chosen field in D_002B2540, the others in D_002B2550.
    #[allow(clippy::too_many_arguments)]
    fn clock_value(
        &mut self,
        input: &PagesInputs,
        entry: &ConfigEntry,
        fields: &[AdjustField; 6],
        x: i32,
        y: i32,
        alpha: i32,
        editing: bool,
        out: &mut TextFrame,
    ) -> bool {
        let p = Arc::clone(&self.program);
        let chosen = if editing { CHOSEN_COLOUR } else { VALUE_COLOUR };
        let plain = if editing { PLAIN_COLOUR } else { VALUE_COLOUR };
        let time_format = input.items[0xd];
        let mut at = x - half(self.template_width(time_format, out));
        for n in 0..entry.value_count {
            let kind = fields[n as usize].item - 6;
            if !(0..=5).contains(&kind) {
                return false;
            }
            let value = input.items[(6 + kind) as usize];
            let text = match kind {
                0 => format(&p.string(FORMAT_YEAR), &[], &[value]),
                3 => format(&p.string(FORMAT_2), &[], &[if time_format == 1 { if value % 12 != 0 { value % 12 } else { 12 } } else { value }]),
                _ => format(&p.string(FORMAT_02), &[], &[value]),
            };
            self.put_string(&p.string(FIXED_OPEN), false, out);
            at += self.draw_item(at, y, if n == entry.value_index { chosen } else { plain }, alpha, &text, out);
            self.put_string(&p.string(FIXED_CLOSE), false, out);
            match n {
                0 | 1 => at += self.draw_item(at, y, plain, alpha, &p.string(SLASH), out),
                2 => at += 2 * self.width_of(&p.string(SPACE), out),
                3 | 4 => at += self.draw_item(at, y, plain, alpha, &p.string(COLON), out),
                5 if time_format == 1 => {
                    self.draw_item(at, y, plain, alpha, &p.string(if input.items[9] < 12 { FIELD_MORNING } else { FIELD_AFTERNOON }), out);
                }
                _ => {}
            }
        }
        true
    }

    // The pages function's text (the main menu's items, System Configuration's list), before the bars, the cache carried.
    pub fn pages(&mut self, input: &PagesInputs) -> PagesFrame {
        self.width = input.width;
        self.height = input.height;
        self.drawn = vec![false; self.cells.len()];
        self.after_pages = true;
        let mut out = PagesFrame::default();
        self.main_menu_items(input, &mut out.text);
        out.title_width = self.list(input, &mut out.text, &mut out.unmodelled);
        self.version_page(input, &mut out.text);
        self.finish(&mut out.text);
        out
    }

    // One string through Font_PutsPackets at the font state as it stands, as a frame of its own (for tests).
    pub fn draw_string(&mut self, text: &[u8], width: i32, height: i32) -> TextFrame {
        self.width = width;
        self.height = height;
        self.drawn = vec![false; self.cells.len()];
        let mut out = TextFrame::default();
        self.put_string(text, false, &mut out);
        self.finish(&mut out);
        out
    }

    // The cache as GS memory holds it once the frame's text is drawn.
    fn finish(&self, out: &mut TextFrame) {
        out.glyphs.address = self.cache.texture;
        out.glyphs.cell_width = self.cache.cell_w;
        out.glyphs.cell_height = self.cache.cell_h;
        out.glyphs.width = self.cache.width;
        out.glyphs.log_width = self.cache.log_w;
        out.glyphs.log_height = self.cache.log_h;
        out.glyphs.cells = self.cells.clone();
    }
}

// TextRamps from the menus' live ramps (config, mainMenu, version, dialogClosing, firstRun, lead, body); the rest from `constants`.
pub fn text_ramps_of(menus: &MenusState, constants: &TextRamps) -> TextRamps {
    let mut r = *constants;
    r.config = menus.page.ramp;
    r.main_menu = menus.main_menu.ramp;
    r.version = menus.version_ramp;
    r.dialog_closing = menus.dialog_ramp;
    r.first_run = menus.first_run_ramp;
    r.lead = menus.menu_lengths[0];
    r.body = menus.body;
    // func_002266E0(0x5E, 0x55, 0x56, 0x57), which the list's entry callbacks call when the page opens (D_00227BE8, D_00227CA8,
    // clock_config_change_cb_*); inside an entry the page calls it with others.
    if menus.page.level == 0 {
        r.panel8_ids = [0x5e, 0x55, 0x56, 0x57];
    }
    if menus.version_ramp.state != 0 || menus.version.panel_on != 0 {
        let v = &menus.version;
        r.panel8_on = v.panel_on;
        r.panel8 = v.panel_alpha;
        if menus.version_ramp.state != 0 {
            r.panel8_ids = v.hints;
        }
        r.adjust_row = v.rows[v.selected.clamp(0, VERSION_ROWS as i32 - 1) as usize].sub_rows;
    }
    r
}

// The menus' state at the pages call: the state before the frame's input with the ramps as ticked (a ramp whose counter moved in the
// step is taken whole from `after`) and the glow as stepped; selection, level and entries are not yet changed by the frame's input.
pub fn menus_at_pages(before: &MenusState, after: &MenusState) -> MenusState {
    let mut at = before.clone();
    at.page.glow = after.page.glow;
    let ticked = |b: &mut Ramp, a: &Ramp| {
        if b.counter != a.counter {
            *b = *a;
        }
    };
    ticked(&mut at.page.ramp, &after.page.ramp);
    ticked(&mut at.main_menu.ramp, &after.main_menu.ramp);
    ticked(&mut at.version_ramp, &after.version_ramp);
    ticked(&mut at.dialog_ramp, &after.dialog_ramp);
    ticked(&mut at.first_run_ramp, &after.first_run_ramp);
    if after.version_drawn_valid {
        at.version = after.version_drawn.clone();
        at.version_ramp = after.version_ramp_drawn;
    }
    at
}

pub fn pages_of(menus: &MenusState, clock: &ClockState, items: &ConfigItems, ramps: &TextRamps, width: i32, height: i32) -> PagesInputs {
    PagesInputs {
        main_menu: menus.main_menu,
        page: menus.page,
        entries: menus.entries,
        list_fade: menus.list_fade,
        list_constants: menus.list_constants,
        adjust_fields: menus.adjust_fields,
        items: *items,
        menu_ramp: clock.menu_ramp,
        tail: clock.tail,
        overlay_level: clock.overlay_level,
        ramps: *ramps,
        width,
        height,
        pending: menus.page_pointers[4],
        version: menus.version.clone(),
    }
}

// The cache's texture as the GS samples it (facts/text.md sections 1 and 2): each cell's picture one texel in from its
// corner, through its block's colour table (the GS looks the colour up before it filters); the rest clear.
// 2^logWidth x 2^logHeight texels, R G B A.
pub fn glyph_cache_image(font: &Font, cache: &GlyphCache) -> crate::Result<Vec<u8>> {
    let size = 1i32 << cache.log_width;
    let rows = 1i32 << cache.log_height;
    let mut out = vec![0u8; size as usize * rows as usize * 4];
    if cache.cell_width <= 0 {
        return Ok(out);
    }
    let columns = cache.width / cache.cell_width;
    for (n, &code) in cache.cells.iter().enumerate() {
        if code == 0 {
            continue;
        }
        let Some(glyph) = font.glyph(code) else { return Err(crate::Error("glyph cache: no glyph for a cell's code".to_owned())) };
        let table = font.table(glyph.block);
        let cell_x = (n as i32 % columns) * cache.cell_width;
        let cell_y = (n as i32 / columns) * cache.cell_height;
        for y in 0..glyph.block.height {
            for x in 0..glyph.block.width {
                let c = &table[font.pixel(&glyph, x, y) as usize];
                let at = ((cell_y + 1 + y) as usize * size as usize + (cell_x + 1 + x) as usize) * 4;
                out[at..at + 4].copy_from_slice(c);
            }
        }
    }
    Ok(out)
}
