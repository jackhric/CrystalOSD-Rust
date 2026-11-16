use scene::menu_types::MenusState;
use scene::ramp::Ramp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    OpeningConfiguration,
    Configuration,
    InsideEntry,
    ClosingConfiguration,
    HidingMenu,
    ClockAlone,
    ShowingMenu,
    Version,
}

pub fn screen_of(menus: &MenusState, menu_ramp: &Ramp) -> Screen {
    if menus.version_ramp.state != 0 {
        return Screen::Version;
    }
    match menus.page.ramp.state {
        0 => return Screen::MainMenu,
        1 => return Screen::OpeningConfiguration,
        3 => return Screen::ClosingConfiguration,
        _ => {}
    }
    match menu_ramp.state {
        1 => Screen::HidingMenu,
        2 => Screen::ClockAlone,
        3 => Screen::ShowingMenu,
        _ => {
            if menus.page.level == 1 {
                Screen::InsideEntry
            } else {
                Screen::Configuration
            }
        }
    }
}

pub fn screen_name(screen: Screen) -> &'static str {
    match screen {
        Screen::MainMenu => "main menu",
        Screen::OpeningConfiguration => "opening System Configuration",
        Screen::Configuration => "System Configuration",
        Screen::InsideEntry => "inside an entry",
        Screen::ClosingConfiguration => "closing System Configuration",
        Screen::HidingMenu => "hiding the menu",
        Screen::ClockAlone => "clock alone",
        Screen::ShowingMenu => "showing the menu",
        Screen::Version => "version",
    }
}
