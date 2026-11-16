use scene::frame::TargetName;
use scene::menu_types::PadWords;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Native,
    Window,
    Double,
    Quadruple,
}

// What the panel edits; the frame loop reads it and clears the one-shot requests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelState {
    pub resolution: Resolution,
    pub samples: u32,
    pub paused: bool,
    pub step: bool,
    pub tv_aspect: bool,
    pub shown: TargetName,
    pub time: [i32; 3],
    pub set_time: bool,
    pub local_time: bool,
    pub screenshot: bool,
    pub restart_opening: bool,
    pub skip_opening: bool,
    pub mute: bool,
    pub volume: f32,
}

impl Default for PanelState {
    fn default() -> Self {
        PanelState {
            resolution: Resolution::Native,
            samples: 1,
            paused: false,
            step: false,
            tv_aspect: true,
            shown: TargetName::Display,
            time: [12, 0, 0],
            set_time: false,
            local_time: false,
            screenshot: false,
            restart_opening: false,
            skip_opening: false,
            mute: false,
            volume: 1.0,
        }
    }
}

// What the panel shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelInfo {
    pub output_width: u32,
    pub output_height: u32,
    pub sample_counts: u32,
    pub logic_frames: u64,
    pub frames_per_second: f32,
    pub validation_errors: u32,
    pub clock: String,
    pub last_screenshot: String,
    pub screen: String,
    pub hand_off: String,
    pub phase: String,
    pub last_error: String,
    pub counter: i32,
    pub stage: i32,
    pub camera_z: f32,
    pub sounds: usize,
    pub audio_available: bool,
    pub audio: String,
    pub pad: PadWords,
}

#[cfg(feature = "debug-panel")]
pub fn draw_panel(state: &mut PanelState, info: &PanelInfo) {
    use std::ffi::{CString, c_char};

    use dear_imgui_sys::*;

    let text = |s: &str| CString::new(s.replace('\0', "")).expect("no NUL");
    let vec2 = |x: f32, y: f32| ImVec2_c { x, y };
    // SAFETY: called between NewFrame and Render on the thread that owns the ImGui context; every string outlives its call.
    unsafe {
        let line = |s: String| {
            let s = text(&s);
            igText(c"%s".as_ptr(), s.as_ptr());
        };
        igSetNextWindowPos(vec2(12.0, 12.0), ImGuiCond_FirstUseEver, vec2(0.0, 0.0));
        igSetNextWindowBgAlpha(0.85);
        igBegin(c"CrystalOSD".as_ptr(), std::ptr::null_mut(), ImGuiWindowFlags_AlwaysAutoResize);
        line(info.clock.clone());
        line(format!("output {}x{}, {:.1} fps, logic frame {}", info.output_width, info.output_height, info.frames_per_second, info.logic_frames));
        line(format!("validation errors: {}", info.validation_errors));
        if !info.phase.is_empty() {
            line(format!("chain phase: {}", info.phase));
        }
        if !info.last_error.is_empty() {
            let s = text(&format!("last error: {}", info.last_error));
            igTextColored(ImVec4_c { x: 1.0, y: 0.3, z: 0.3, w: 1.0 }, c"%s".as_ptr(), s.as_ptr());
        }
        if !info.screen.is_empty() {
            line(format!("pad held {:04x} pressed {:04x}", info.pad.held, info.pad.pressed));
            line(format!("screen: {}", info.screen));
            line(format!("module counter {}, stage {}, camera z {:.2}", info.counter, info.stage, info.camera_z));
            line(format!("hand-off: {}", if info.hand_off.is_empty() { "pending" } else { &info.hand_off }));
            line(format!("sound events of the last frame: {}", info.sounds));
            if igButton(c"Restart opening".as_ptr(), vec2(0.0, 0.0)) {
                state.restart_opening = true;
            }
            igSameLine(0.0, -1.0);
            if igButton(c"Skip opening".as_ptr(), vec2(0.0, 0.0)) {
                state.skip_opening = true;
            }
        }
        igSeparator();

        let resolutions = [c"640x448 (native)".as_ptr(), c"window".as_ptr(), c"x2".as_ptr(), c"x4".as_ptr()];
        let mut resolution = state.resolution as i32;
        if igCombo_Str_arr(c"resolution".as_ptr(), &mut resolution, resolutions.as_ptr(), 4, -1) {
            state.resolution = [Resolution::Native, Resolution::Window, Resolution::Double, Resolution::Quadruple][resolution as usize];
        }
        let counts = [1u32, 2, 4, 8];
        let msaa: [*const c_char; 4] = [c"off".as_ptr(), c"2x".as_ptr(), c"4x".as_ptr(), c"8x".as_ptr()];
        let mut chosen = 0;
        for (i, &count) in counts.iter().enumerate() {
            if count == state.samples {
                chosen = i;
            }
        }
        if igBeginCombo(c"MSAA".as_ptr(), msaa[chosen], 0) {
            for i in 0..4 {
                if info.sample_counts & counts[i] == 0 {
                    continue;
                }
                if igSelectable_Bool(msaa[i], i == chosen, 0, vec2(0.0, 0.0)) {
                    state.samples = counts[i];
                }
            }
            igEndCombo();
        }
        let targets = [c"display".as_ptr(), c"refraction source".as_ptr(), c"work".as_ptr()];
        let mut shown = state.shown as i32;
        if igCombo_Str_arr(c"show".as_ptr(), &mut shown, targets.as_ptr(), 3, -1) {
            state.shown = [TargetName::Display, TargetName::RefractionSource, TargetName::Work][shown as usize];
        }
        igCheckbox(c"4:3 picture".as_ptr(), &mut state.tv_aspect);
        igSeparator();

        igCheckbox(c"pause".as_ptr(), &mut state.paused);
        igSameLine(0.0, -1.0);
        igBeginDisabled(!state.paused);
        if igButton(c"step".as_ptr(), vec2(0.0, 0.0)) {
            state.step = true;
        }
        igEndDisabled();
        igSetNextItemWidth(120.0);
        igInputInt3(c"h m s".as_ptr(), state.time.as_mut_ptr(), 0);
        if igButton(c"set time".as_ptr(), vec2(0.0, 0.0)) {
            state.set_time = true;
        }
        igSameLine(0.0, -1.0);
        if igButton(c"local time".as_ptr(), vec2(0.0, 0.0)) {
            state.local_time = true;
        }
        igSeparator();

        if info.audio_available {
            igCheckbox(c"mute".as_ptr(), &mut state.mute);
            igSetNextItemWidth(160.0);
            igSliderFloat(c"volume".as_ptr(), &mut state.volume, 0.0, 1.0, c"%.2f".as_ptr(), 0);
            line(format!("audio: {}", info.audio));
        } else {
            line("audio: off".to_owned());
        }
        igSeparator();

        if igButton(c"screenshot".as_ptr(), vec2(0.0, 0.0)) {
            state.screenshot = true;
        }
        if !info.last_screenshot.is_empty() {
            let s = text(&info.last_screenshot);
            igTextWrapped(c"%s".as_ptr(), s.as_ptr());
        }
        igEnd();
    }
}
