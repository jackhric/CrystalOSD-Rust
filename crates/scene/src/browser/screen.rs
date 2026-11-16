// The Browser as one screen of the app: the shell (shell.rs), the root page's device row (devices.rs), the card page and its
// Format dialog (format.rs), the guide and the chrome (chrome.rs), one pass per frame, HDD OSD 1.10U NTSC unless `pal`.
// Every rule here is a facts page's (browser-shell.md, browser-chrome.md, browser-devices.md); where a page stops, the choice
// made is named APP POLICY and listed in the report of this port.

use std::sync::Arc;

use super::chrome::{BrowserFrame, ChromeState, DialogDraw, GUIDE_X_ENGLISH, Guide, GuideSlot, TITLE_RIGHT_COLOUR, Titles, chrome_frame};
use super::devices::{CursorState, LEFT, RIGHT, SlotInfo, SlotWords, cursor_step, header, scan_store, status_line, title_ids};
use super::format::{format_draw, format_step};
use super::shell::{ExitWords, Gate, PassInput, Shell, clamp, limits};
use super::sounds::{self, Sent};
use crate::menu_types::PadWords;
use crate::program_image::ProgramImage;
use crate::sound::SoundCommand;

const LANGUAGE_TABLES: u32 = 0x002a_d200;
const GUIDE_X_TABLE: u32 = 0x002b_d2e0;
const CROSS: u32 = 0x20;
const CIRCLE: u32 = 0x40;
const HDD_DEVICE: i32 = 1;

// facts/browser-chrome.md "Button guide": a slot's counter is clamp(counter + step, 0, 0x18) per frame.
const GUIDE_MAX: i32 = 0x18;
// facts/browser-chrome.md: the fourth slot's picture x for "Options" (width 83): 640 - 83 - 0x34 = 505 (measured on the leave captures).
const OPTIONS_WIDTH: i32 = 83;
// facts/browser-devices.md section 8: the state 3 of hddosd-110U-browser-devices-formatted lasts 94 frames (measured).
const FORMAT_JOB_FRAMES: i32 = 94;
// facts/browser-devices.md section 1: the library's free count for a card the OSD formatted, empty: 8000 (measured).
pub const FORMATTED_EMPTY_FREE: i32 = 8000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CardSlot {
    pub formatted: bool,
    pub free: i32,
}

#[derive(Debug, Clone)]
pub struct BrowserSetup {
    pub pal: bool,
    pub width: i32,
    pub height: i32,
    pub mechacon_param: u32,
    pub cards: [Option<CardSlot>; 2],
    pub program: Option<Arc<ProgramImage>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Root,
    Card(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Item {
    device: i32,
    slot: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormatDialog {
    state: i32,
    cursor: i32,
    busy: i32,
}

pub struct BrowserOutput {
    pub frame: BrowserFrame,
    pub sounds: Vec<SoundCommand>,
    pub exit: Option<ExitWords>,
}

pub struct BrowserScreen {
    setup: BrowserSetup,
    pub shell: Shell,
    slots: [SlotWords; 2],
    page: Page,
    cursor: i32,
    items: Vec<Item>,
    page_in: i32,
    empty: i32,
    format: Option<FormatDialog>,
    guide: [i32; 4],
    guide_step: [i32; 4],
    title_counter: i32,
    transition: Option<Page>,
    pub field: i32,
    pub spdif: Option<u16>,
    done: bool,
}

fn scan(cards: &[Option<CardSlot>; 2]) -> [SlotWords; 2] {
    std::array::from_fn(|n| {
        let info = match cards[n] {
            Some(c) => SlotInfo { kind: 2, free: if c.formatted { c.free } else { 0 }, format: i32::from(c.formatted), multitap: 0 },
            None => SlotInfo::default(),
        };
        scan_store(&info).expect("a slot state the facts cover")
    })
}

impl BrowserScreen {
    // The thread start (browser-shell.md section 1): the shell's start with the scan words the emulator's scan leaves, (1, 0), which
    // the native port may set (section 2: kind 0 opens whichever of (1,0) or (1,1)); D_001F0114 for the 0x8070 send (section 7).
    pub fn start(setup: BrowserSetup, before: &Shell) -> Self {
        let shell = Shell::start(setup.pal, Some((1, 0)), before);
        let slots = scan(&setup.cards);
        let spdif = sounds::remote_a3(sounds::spdif_store(setup.mechacon_param));
        let mut items = vec![Item { device: HDD_DEVICE, slot: None }];
        for (n, s) in slots.iter().enumerate() {
            if s.kind != 0 {
                items.push(Item { device: 2 + 4 * n as i32, slot: Some(n) });
            }
        }
        let page_in = limits(setup.pal).page_in;
        BrowserScreen {
            setup,
            shell,
            slots,
            page: Page::Root,
            cursor: 0,
            items,
            page_in,
            empty: 0,
            format: None,
            guide: [0; 4],
            guide_step: [0; 4],
            title_counter: 0,
            transition: None,
            field: 0,
            spdif,
            done: false,
        }
    }

    fn string(&self, id: i32) -> Vec<u8> {
        let Some(p) = &self.setup.program else { return Vec::new() };
        let mut id = id;
        if self.guide_swap() && (id == 0x55 || id == 0x56) {
            id = 0x55 + 0x56 - id;
        }
        let strings = p.word(LANGUAGE_TABLES.wrapping_add(4 * self.language() as u32));
        p.string(p.word(strings.wrapping_add(4u32.wrapping_mul(id as u32))))
    }

    fn language(&self) -> i32 {
        let field = ((self.setup.mechacon_param >> 4) & 0x1f) as i32;
        if field != 0 { field } else { 1 }
    }

    // APP POLICY: the video mode word get_vidmode_with_fallback reads is not measured on a PC; the captures have it nonzero
    // (the guide's slots 1 and 2 swap: facts/browser-chrome.md), so the port uses the swap.
    fn guide_swap(&self) -> bool {
        true
    }

    fn concat(&self, ids: &[i32]) -> Vec<u8> {
        ids.iter().flat_map(|&id| self.string(id)).collect()
    }

    fn device_name(&self, item: &Item) -> Vec<u8> {
        let kind = item.slot.map_or(0, |n| self.slots[n].kind);
        title_ids(item.device, kind, false).map(|ids| self.concat(&ids)).unwrap_or_default()
    }

    // printf of the English free-count strings 0x2E / 0x2F with their %d / %03d.
    fn printed(&self, id: i32, args: &[i32]) -> Vec<u8> {
        let format = self.string(id);
        let mut out = Vec::new();
        let mut next = args.iter();
        let mut i = 0;
        while i < format.len() {
            if format[i] == b'%' && i + 1 < format.len() {
                let mut j = i + 1;
                let zero = format[j] == b'0';
                let mut width = 0usize;
                while j < format.len() && format[j].is_ascii_digit() {
                    width = width * 10 + (format[j] - b'0') as usize;
                    j += 1;
                }
                if j < format.len() && format[j] == b'd' {
                    let v = next.next().copied().unwrap_or(0);
                    let text = if zero { format!("{v:0width$}") } else { format!("{v:width$}") };
                    out.extend_from_slice(text.as_bytes());
                    i = j + 1;
                    continue;
                }
            }
            out.push(format[i]);
            i += 1;
        }
        out
    }

    fn enable(&mut self, slot: usize, on: bool) {
        self.guide_step[slot] = if on { 1 } else { -1 };
    }

    // The guide's slots: browser-chrome.md says the error dialog, at n == 0x1D with the direction 1, disables slots 2 and 3 and
    // enables slot 1, and that its close calls 0x00251CF8 (both read). APP POLICY (0x00251DA0's selection is read in part), matched
    // to hddosd-110U-browser-chrome-leave and -session: the close disables slot 1; a page change disables every slot; on the root
    // page, once its fade-in is full, Back, plus Enter on a card and Options on the HDD item; on a card page Back, plus Enter while
    // the Format dialog is open. The dialog's words, the page, the cursor and the page fade-in are those at the pass's start.
    fn guide_policy(&mut self, f0_start: i32, be4_start: i32, page: Page, cursor: i32, page_in: i32) {
        if f0_start != 0 {
            if self.shell.f0 == limits(self.setup.pal).dialog - 1 && self.shell.be4 == 1 {
                self.enable(2, false);
                self.enable(3, false);
                self.enable(1, true);
            }
            if be4_start == 1 && self.shell.be4 == -1 {
                self.enable(1, false);
            }
            return;
        }
        if self.shell.leave != 0 {
            return;
        }
        if self.transition.is_some() {
            for slot in 0..4 {
                self.enable(slot, false);
            }
            return;
        }
        match page {
            Page::Root => {
                if page_in != limits(self.setup.pal).page_in {
                    return;
                }
                let on_card = self.items.get(cursor as usize).is_some_and(|i| i.slot.is_some());
                let on_hdd = self.items.get(cursor as usize).is_some_and(|i| i.slot.is_none());
                self.enable(1, true);
                self.enable(2, on_card);
                self.enable(3, on_hdd);
            }
            Page::Card(_) => {
                self.enable(1, true);
                self.enable(2, self.format.is_some());
                self.enable(3, false);
            }
        }
    }

    fn step_guide(&mut self) {
        for slot in 0..4 {
            self.guide[slot] = clamp(self.guide[slot] + self.guide_step[slot], 0, GUIDE_MAX);
        }
    }

    fn enabled(&self, slot: usize) -> i32 {
        i32::from(self.guide_step[slot] > 0)
    }

    fn send(&self, sent: &Sent, out: &mut Vec<SoundCommand>) {
        if sounds::gate(0, 1, 0) {
            out.push(sounds::command(sent));
        }
    }

    // The root page's input (func_00254840, view mode 1): Left / Right move the cursor (verified, section 7 of browser-devices.md),
    // Cross on a card sends 7 and 8 and opens its page (browser-shell.md section 7; the page builder 0x002499E8 opens the Format
    // dialog on a card with no format). APP POLICY: Up, Down, Triangle and Cross on the HDD item do nothing (their effects are read
    // in part only: the move flags, func_002547C0, the Options page).
    fn root_input(&mut self, pad: &PadWords, out: &mut Vec<SoundCommand>) {
        let edge = pad.pressed;
        let state = CursorState {
            cursor: self.cursor,
            first: 0,
            dialog: i32::from(self.format.is_some()),
            page_in: self.page_in,
            pal: i32::from(self.setup.pal),
            device: 0,
            list_count: self.items.len() as i32,
            record_count: self.items.len() as i32,
            edge,
            repeat: pad.repeating & (LEFT | RIGHT),
            ..CursorState::default()
        };
        let step = cursor_step(&state);
        if step.modelled && step.sound {
            self.cursor = step.cursor;
            self.send(&Sent { a2: 6, ra: 0x0025_4f54, ring: false }, out);
            return;
        }
        if self.page_in != limits(self.setup.pal).page_in || pad.held & (LEFT | RIGHT | 0x1000 | 0x4000 | CIRCLE) != 0 {
            return;
        }
        if self.enabled(2) != 0
            && edge & CROSS != 0
            && let Some(item) = self.items.get(self.cursor as usize).copied()
            && let Some(slot) = item.slot
        {
            self.send(&Sent { a2: 7, ra: 0x0025_4fec, ring: false }, out);
            self.send(&Sent { a2: 8, ra: 0x0025_4ffc, ring: true }, out);
            self.open_page(Page::Card(slot));
        }
    }

    // facts/browser-chrome.md "Titles": the top right title's alpha is (record + 0xAC) * 8, 0x80 from 16 on (read). APP POLICY: that
    // counter is 0 at the entry and at a page switch and rises one per pass, falls one per pass while a page change runs; a cursor move
    // keeps it (as in hddosd-110U-browser-chrome-root and -session).
    // APP POLICY, measured on hddosd-110U-browser-session (the facts state only that the page fade-in recovers one per pass,
    // browser-shell.md section 8): a page change takes D_0037072C down by 1 in the request's input (before the titles draw) and by 1
    // at the end of every pass from then on (the master's tail, 0x00253488); at 1 the
    // card page's builder opens the Format dialog of an unformatted card (0x002499E8, read); at 0 the page switches (the mode word
    // D_003706D0 becomes 2 on a card page, 0 on the root; the status line's fade word 0x00370764 restarts from 0 and rises one per
    // pass to 60); then D_0037072C rises one per pass.
    fn open_page(&mut self, page: Page) {
        self.transition = Some(page);
        self.page_in -= 1;
    }

    fn step_page(&mut self) {
        let max = limits(self.setup.pal).page_in;
        let Some(target) = self.transition else {
            self.page_in = (self.page_in + 1).min(max);
            self.empty = (self.empty + 1).min(60);
            return;
        };
        self.page_in -= 1;
        if self.page_in == 1
            && let Page::Card(slot) = target
            && self.slots[slot].formatted == 0
        {
            self.format = Some(FormatDialog { state: 1, cursor: 0, busy: 0 });
        }
        if self.page_in <= 0 {
            self.page_in = 0;
            self.transition = None;
            if let Page::Root = target
                && let Page::Card(slot) = self.page
            {
                self.cursor = self.items.iter().position(|i| i.slot == Some(slot)).unwrap_or(0) as i32;
            }
            self.page = target;
            self.shell.mode = if target == Page::Root { 0 } else { 2 };
            self.title_counter = 0;
            self.empty = 0;
        }
        self.empty = (self.empty + 1).min(60);
    }

    // A card page: Circle pops it (sound 0xA, browser-shell.md section 7); the Format dialog's logic runs while it is open.
    fn card_input(&mut self, pad: &PadWords, out: &mut Vec<SoundCommand>) {
        let edge = pad.pressed;
        if let Some(dialog) = self.format {
            let step = format_step(dialog.state, dialog.cursor, edge, dialog.busy, 0);
            for &a2 in &step.sounds {
                self.send(&Sent { a2, ra: sounds::format_ra(a2, edge), ring: false }, out);
            }
            let mut next = FormatDialog { state: step.state, cursor: step.cursor, busy: dialog.busy };
            if step.job {
                next.busy = FORMAT_JOB_FRAMES;
            }
            if step.clear_bit
                && let Page::Card(slot) = self.page
            {
                self.slots[slot] = scan_store(&SlotInfo { kind: 2, free: FORMATTED_EMPTY_FREE, format: 1, multitap: 0 }).expect("a formatted card");
            }
            self.format = if step.dialog == 0 { None } else { Some(next) };
            return;
        }
        if self.page_in == limits(self.setup.pal).page_in && self.enabled(1) != 0 && edge & CIRCLE != 0 {
            self.send(&Sent { a2: 0xa, ra: 0x0025_4950, ring: false }, out);
            self.open_page(Page::Root);
        }
    }

    fn titles(&self) -> Titles {
        match self.page {
            Page::Root => {
                let selected = self.items.get(self.cursor as usize).copied();
                Titles {
                    left: self.concat(&[6]),
                    right: selected.map(|i| self.device_name(&i)).unwrap_or_default(),
                    right_alpha: Some((self.title_counter * 8).min(0x80)),
                    right_second: Vec::new(),
                }
            }
            Page::Card(_) => Titles { left: Vec::new(), right: Vec::new(), right_alpha: Some(0x80), right_second: Vec::new() },
        }
    }

    fn card_texts(&self, frame: &mut BrowserFrame) {
        let Page::Card(slot) = self.page else { return };
        let item = Item { device: 2 + 4 * slot as i32, slot: Some(slot) };
        let at = frame.frame.passes.len();
        let name = self.device_name(&item);
        frame.texts.push(super::chrome::TextRun { label: "card header", text: name, colour: TITLE_RIGHT_COLOUR, alpha: Some(0x80), before_pass: at });
        let words = &self.slots[slot];
        let line = match header(words) {
            Ok(id) => self.string(id),
            Err(Some(t)) => self.printed(t.id, &t.args.iter().flatten().copied().collect::<Vec<_>>()),
            Err(None) => Vec::new(),
        };
        frame.texts.push(super::chrome::TextRun { label: "card free", text: line, colour: TITLE_RIGHT_COLOUR, alpha: Some(0x80), before_pass: at });
        let flags = if self.slots[slot].formatted == 0 { 8 } else { 0 };
        if let Some(status) = status_line(flags, item.device, self.empty, self.page_in, i32::from(self.setup.pal), 1, 0) {
            if let Some(dialog) = self.format
                && let Some(draw) = format_draw(dialog.state, dialog.cursor)
            {
                for id in draw.ids {
                    frame.texts.push(super::chrome::TextRun {
                        label: "format dialog",
                        text: self.string(id),
                        colour: 0x606060,
                        alpha: Some(status.alpha),
                        before_pass: at,
                    });
                }
            } else {
                frame.texts.push(super::chrome::TextRun {
                    label: "status line",
                    text: self.string(status.id),
                    colour: 0x606060,
                    alpha: Some(status.alpha),
                    before_pass: at,
                });
            }
        }
    }

    pub fn exited(&self) -> bool {
        self.done
    }

    pub fn frame(&mut self, pad: &PadWords) -> BrowserOutput {
        let mut out = Vec::new();
        let edge = pad.pressed;
        let pal = self.setup.pal;
        let (page_start, cursor_start, page_in_start) = (self.page, self.cursor, self.page_in);
        let idle = self.shell.leave == 0 && self.shell.f0 == 0;
        let gate = Gate {
            view: 1,
            page_in: self.page_in,
            hold: 0,
            dialog_word: i32::from(self.format.is_some()),
            slide: 0,
            slide_b: 0,
            back_enabled: self.enabled(1),
            page_kind: Some(if self.page == Page::Root { 0 } else { 1 }),
        };
        let asked = self.shell.asks_to_leave(&gate, &PassInput { edge, ..PassInput::default() }, pal);
        if idle && !asked && self.transition.is_none() {
            match self.page {
                Page::Root => self.root_input(pad, &mut out),
                Page::Card(_) => self.card_input(pad, &mut out),
            }
        }
        if let Some(dialog) = self.format.as_mut()
            && dialog.busy > 0
        {
            dialog.busy -= 1;
        }
        let dialog_before = (self.shell.f0, self.shell.be4, self.shell.be8);
        let leave_before = self.shell.leave;
        let trail = self.shell.step(&gate, &PassInput { edge, ..PassInput::default() }, Some(asked));
        self.step_guide();
        self.guide_policy(dialog_before.0, dialog_before.1, page_start, cursor_start, page_in_start);
        self.title_counter = if self.transition.is_some() { (self.title_counter - 1).max(0) } else { (self.title_counter + 1).min(16) };

        let state = ChromeState {
            width: self.setup.width,
            height: self.setup.height,
            field: self.field,
            display_index: self.field,
            fade: trail.fade_drawn,
            tile_counter: trail.fade_drawn.count,
            page_in: self.page_in,
            titles: self.titles(),
            dialog: if dialog_before.0 != 0 {
                Some(DialogDraw {
                    n: dialog_before.0,
                    step: dialog_before.1,
                    kind: dialog_before.2,
                    abort: leave_before != 0 || trail.requested,
                    lines: [0x15c, 0x15d, 0x15e, 0x15f].iter().map(|&id| self.string(id)).collect(),
                })
            } else {
                None
            },
            guide: Guide {
                slots: (0..4)
                    .filter(|&s| self.guide[s] != 0)
                    .map(|s| GuideSlot { s4: s as i32, counter: self.guide[s], label: self.guide_label(s as i32) })
                    .collect(),
                swap: self.guide_swap(),
                xs: self.guide_xs(),
                slot3_x: self.setup.width - OPTIONS_WIDTH - 0x34,
                dy: 2,
            },
            rom: false,
        };
        let state = if state.dialog.as_ref().is_some_and(|d| d.n + d.step < 0) { ChromeState { dialog: None, ..state } } else { state };
        let mut frame = chrome_frame(&state, trail.alpha);
        self.card_texts(&mut frame);
        self.step_page();
        self.field ^= 1;
        let exit = if trail.exited { Some(self.shell.exit) } else { None };
        if exit.is_some() {
            self.done = true;
        }
        BrowserOutput { frame, sounds: out, exit }
    }

    // 0x002524D0: slots 0..2 show GetOSDString(0x54 + s2), s2 the picture index (1 and 2 swapped with the video mode); slot 3 on the
    // root page with the device word 0 shows id 0x1B5 ("Options") for an item of type 0 or 0xD (read).
    fn guide_label(&self, s4: i32) -> Vec<u8> {
        if s4 == 3 {
            return self.string(0x1b5);
        }
        let s2 = match (self.guide_swap(), s4) {
            (true, 1) => 2,
            (true, 2) => 1,
            _ => s4,
        };
        self.string(0x54 + s2)
    }

    fn guide_xs(&self) -> [i32; 3] {
        match &self.setup.program {
            Some(p) => std::array::from_fn(|k| p.integer(GUIDE_X_TABLE + 16 * self.language() as u32 + 4 * k as u32)),
            None => GUIDE_X_ENGLISH,
        }
    }
}
