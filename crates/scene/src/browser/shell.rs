// facts/browser-shell.md: the Browser module's shell, one pass of the thread loop (HDD OSD 1.10U 0x0024F070, ROM 2.30
// 0x00248498) at a time: the fade overlay, the HDD error dialog, the leave request, the leave counter and the exit.

pub const CIRCLE: u32 = 0x40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoLimits {
    pub fade: i32,
    pub dialog: i32,
    pub leave: i32,
    pub page_in: i32,
}

// facts/browser-shell.md sections 3, 4, 5: fade length 0x5A / 0x4B, dialog length 0x1E / 0x19, the leave counter's last read
// 0x5B / 0x4C, the page fade-in 0x1A / 0x15 (NTSC / PAL).
pub const NTSC: VideoLimits = VideoLimits { fade: 0x5a, dialog: 0x1e, leave: 0x5b, page_in: 0x1a };
pub const PAL: VideoLimits = VideoLimits { fade: 0x4b, dialog: 0x19, leave: 0x4c, page_in: 0x15 };

pub fn limits(pal: bool) -> VideoLimits {
    if pal { PAL } else { NTSC }
}

// The fade record, HDD OSD 0x00370660 (state), +0xC (count), +0x10 (colour), +0x14 (length); ROM 2.30 0x002C8BB4 (a byte).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fade {
    pub state: i32,
    pub count: i32,
    pub colour: i32,
    pub length: i32,
}

// browser_exit_previous_module (HDD OSD 0x0024ED20): 0x1F0648, 0x1F064C, 0x1F0010, 0x1F0644, 0x1F0014.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExitWords {
    pub destination: i32,
    pub previous: i32,
    pub execute: i32,
    pub flag: i32,
    pub word014: i32,
}

// The shell's words (HDD OSD): leave counter D_003706A8, PAL flag D_003706B0, leave destination D_003706B4, execute type
// D_003706B8, HDD status D_003708E8 / D_003708EC, dialog counter D_003708F0, direction D_00370BE4, kind D_00370BE8, the mode
// word D_003706D0, D_0037068C and the byte D_00370625.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shell {
    pub leave: i32,
    pub pal: i32,
    pub destination: i32,
    pub execute: i32,
    pub e8: i32,
    pub ec: i32,
    pub f0: i32,
    pub be4: i32,
    pub be8: i32,
    pub fade: Fade,
    pub exit: ExitWords,
    pub mode: i32,
    pub word68c: i32,
    pub byte625: i32,
}

// The words the leave request at 0x00254840 reads and the shell does not write: view mode D_003706F8, page fade-in
// D_0037072C, the hold byte D_00370678, the Format dialog word D_00370820, the slide words D_00370858 / D_0037085C, the
// guide byte D_00370771 and the kind of the page record (0x002BD420 + 0x1C0 * D_00370728, +0x124; 0 = the root page).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gate {
    pub view: i32,
    pub page_in: i32,
    pub hold: i32,
    pub dialog_word: i32,
    pub slide: i32,
    pub slide_b: i32,
    pub back_enabled: i32,
    pub page_kind: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassInput {
    pub edge: u32,
    pub dvd_error: i32,
    pub disc_error: i32,
    pub io: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassTrail {
    pub requested: bool,
    pub alpha: Option<i32>,
    pub fade_drawn: Fade,
    pub checked: i32,
    pub exited: bool,
}

// func_0024DFA8: the value held in [low, high].
pub fn clamp(value: i32, low: i32, high: i32) -> i32 {
    if value < low {
        low
    } else if high < value {
        high
    } else {
        value
    }
}

// func_0024CEB0 (ROM 0x00246860): state 1 alpha 0x80; state 2 (count << 7) / length, state 3 0x80 minus it (signed MIPS divide);
// a nonzero count falls by one; at count 0 state 2 ends (0, nothing drawn) and state 3 holds (1, count -1).
pub fn fade_step(fade: Fade) -> (Option<i32>, Fade) {
    let mut next = fade;
    if fade.state == 1 {
        return (Some(0x80), next);
    }
    assert!(fade.length != 0, "fade length 0 (the console breaks)");
    let part = fade.count.wrapping_shl(7).wrapping_div(fade.length);
    let alpha = if fade.state == 2 { part } else { 0x80 - part };
    if fade.count == 0 {
        if fade.state == 2 {
            next.state = 0;
            return (None, next);
        }
        if fade.state == 3 {
            next.state = 1;
        }
    }
    next.count = fade.count.wrapping_sub(1);
    (Some(alpha), next)
}

// module_browser_24CE60 / func_0024CE88 (ROM 0x00246810 / 0x00246838): start state 2 (in) or 3 (out) unless it is running.
pub fn fade_start(fade: Fade, want: i32, length: i32) -> Fade {
    if fade.state == want { fade } else { Fade { state: want, count: length, length, ..fade } }
}

// The entry scan's request (HDD OSD 0x0024B96C): D_003708E8 != 2 -> kind 0; D_003708EC == 0 -> kind 0; ec != e8 -> kind 1;
// else no dialog. Kind 0 only is verified (the emulator's scan leaves (1, 0)).
pub fn scan_kind(e8: i32, ec: i32) -> Option<i32> {
    if e8 != 2 || ec == 0 {
        Some(0)
    } else if ec != e8 {
        Some(1)
    } else {
        None
    }
}

impl Shell {
    // The thread start (HDD OSD 0x0024F038): init zeroes the leave counter, stores the PAL flag, the fade-in starts with the
    // video mode's length, the scan opens the dialog its words ask for (0x0025BC08: D_00370BE8 = kind, D_00370BE4 = D_003708F0 = 1).
    // `scan`: the two status words (e8, ec); None on ROM 2.30, which has no HDD dialog.
    pub fn start(pal: bool, scan: Option<(i32, i32)>, before: &Shell) -> Shell {
        let mut s = Shell { leave: 0, pal: i32::from(pal), fade: fade_start(before.fade, 2, limits(pal).fade), ..*before };
        if let Some((e8, ec)) = scan {
            let kind = scan_kind(e8, ec);
            s.e8 = e8;
            s.ec = ec;
            s.f0 = i32::from(kind.is_some());
            s.be4 = i32::from(kind.is_some());
            s.be8 = kind.unwrap_or(before.be8);
        }
        s
    }

    // browser_transition_to_other_module (HDD OSD 0x0024EBF0): the mode word 3 (0x00250768(3)), D_0037068C = 0, D_00370625 = 0,
    // the fade-out, the destination, execute type -1, leave counter 1.
    pub fn leave_request(&mut self, destination: i32, pal: bool) {
        self.mode = 3;
        self.word68c = 0;
        self.byte625 = 0;
        self.fade = fade_start(self.fade, 3, limits(pal).fade);
        self.destination = destination;
        self.execute = -1;
        self.leave = 1;
    }

    // The leave request's predicate at 0x00253860 and 0x00254840 (HDD OSD): nothing open, nothing leaving, the root page at rest,
    // the Back slot of the guide on, Circle in the pad's edge.
    pub fn asks_to_leave(&self, gate: &Gate, input: &PassInput, pal: bool) -> bool {
        let idle = self.leave == 0 && input.dvd_error == 0 && input.disc_error == 0 && self.f0 == 0;
        let open =
            gate.dialog_word == 0 && gate.page_in == limits(pal).page_in && gate.hold != 1 && gate.slide == 0 && gate.slide_b == 0 && gate.back_enabled != 0;
        idle && gate.view == 1 && open && input.edge & CIRCLE != 0 && gate.page_kind == Some(0)
    }

    // One pass: the leave request (`request` replaces the predicate where it is only read in part, ROM 2.30), the dialog
    // 0x0025BD18, the fade overlay 0x0024D0A0 -> 0x0024CEB0, the leave counter at 0x0024F0A0 (the thread exits when the old
    // counter is at least 0x5B / 0x4C and D_003705FC is 0).
    pub fn step(&mut self, gate: &Gate, input: &PassInput, request: Option<bool>) -> PassTrail {
        let pal = self.pal != 0;
        let mut trail = PassTrail::default();
        if request.unwrap_or_else(|| self.asks_to_leave(gate, input, pal)) {
            self.leave_request(2, pal);
            trail.requested = true;
        }
        if self.f0 != 0 {
            if self.leave != 0 || input.dvd_error != 0 || input.disc_error != 0 {
                self.f0 = 0;
                self.be4 = -1;
            } else {
                let length = limits(pal).dialog;
                self.f0 = clamp(self.f0.wrapping_add(self.be4), 0, length);
                if self.be8 == 0 {
                    self.e8 = 1;
                } else if self.be8 == 1 {
                    self.ec = 1;
                }
                if self.f0 == length && input.edge & CIRCLE != 0 {
                    self.be4 = -1;
                }
            }
        }
        trail.fade_drawn = self.fade;
        if self.fade.state != 0 {
            let (alpha, next) = fade_step(self.fade);
            trail.alpha = alpha;
            self.fade = next;
        }
        trail.checked = self.leave;
        if self.leave != 0 {
            let old = self.leave;
            self.leave = old.wrapping_add(1);
            if old >= limits(pal).leave && input.io == 0 {
                self.exit = ExitWords { destination: self.destination, previous: 3, execute: self.execute, flag: 1, word014: 0 };
                trail.exited = true;
            }
        }
        trail
    }
}
