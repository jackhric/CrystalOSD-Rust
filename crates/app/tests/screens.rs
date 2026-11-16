use app::screens::{Screen, screen_name, screen_of};
use scene::menu_types::MenusState;
use scene::ramp::Ramp;

#[test]
fn screens() {
    let mut m = MenusState::default();
    let mut menu = Ramp::default();
    assert_eq!(screen_of(&m, &menu), Screen::MainMenu);
    m.page.ramp.state = 1;
    assert_eq!(screen_of(&m, &menu), Screen::OpeningConfiguration);
    m.page.ramp.state = 2;
    assert_eq!(screen_of(&m, &menu), Screen::Configuration);
    m.page.level = 1;
    assert_eq!(screen_of(&m, &menu), Screen::InsideEntry);
    m.page.level = 0;
    menu.state = 1;
    assert_eq!(screen_of(&m, &menu), Screen::HidingMenu);
    menu.state = 2;
    assert_eq!(screen_of(&m, &menu), Screen::ClockAlone);
    menu.state = 3;
    assert_eq!(screen_of(&m, &menu), Screen::ShowingMenu);
    menu.state = 0;
    m.page.ramp.state = 3;
    assert_eq!(screen_of(&m, &menu), Screen::ClosingConfiguration);
    m.page.ramp.state = 0;
    m.version_ramp.state = 1;
    assert_eq!(screen_of(&m, &menu), Screen::Version);
    assert_eq!(screen_name(Screen::Version), "version");
    m.version_ramp.state = 3;
    assert_eq!(screen_of(&m, &menu), Screen::Version);
    m.version_ramp.state = 0;
    assert_eq!(screen_of(&m, &menu), Screen::MainMenu);
}
