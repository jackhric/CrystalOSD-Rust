use crate::input::PadButton;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioPress {
    pub after_clock: u64,
    pub button: PadButton,
}

const PRESSES: [ScenarioPress; 16] = [
    ScenarioPress { after_clock: 120, button: PadButton::Down },
    ScenarioPress { after_clock: 200, button: PadButton::Up },
    ScenarioPress { after_clock: 280, button: PadButton::Triangle },
    ScenarioPress { after_clock: 360, button: PadButton::Down },
    ScenarioPress { after_clock: 420, button: PadButton::Up },
    ScenarioPress { after_clock: 480, button: PadButton::Circle },
    ScenarioPress { after_clock: 560, button: PadButton::Down },
    ScenarioPress { after_clock: 640, button: PadButton::Cross },
    ScenarioPress { after_clock: 760, button: PadButton::Down },
    ScenarioPress { after_clock: 820, button: PadButton::Down },
    ScenarioPress { after_clock: 880, button: PadButton::Up },
    ScenarioPress { after_clock: 940, button: PadButton::Cross },
    ScenarioPress { after_clock: 1040, button: PadButton::Circle },
    ScenarioPress { after_clock: 1120, button: PadButton::Square },
    ScenarioPress { after_clock: 1220, button: PadButton::Square },
    ScenarioPress { after_clock: 1320, button: PadButton::Circle },
];

pub fn golden_scenario() -> &'static [ScenarioPress] {
    &PRESSES
}

pub fn golden_frames() -> u64 {
    1440
}
