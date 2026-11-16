use app::input::{Bindings, PadButton, PadReader, bit_of, feed};
use sdl3_sys::events::*;
use sdl3_sys::gamepad::*;
use sdl3_sys::joystick::SDL_JoystickID;
use sdl3_sys::scancode::*;

fn key(kind: SDL_EventType, code: SDL_Scancode, repeat: bool) -> SDL_Event {
    SDL_Event { key: SDL_KeyboardEvent { r#type: kind, scancode: code, repeat, ..Default::default() } }
}

fn button(kind: SDL_EventType, which: u32, b: SDL_GamepadButton) -> SDL_Event {
    SDL_Event { gbutton: SDL_GamepadButtonEvent { r#type: kind, which: SDL_JoystickID(which), button: b.0 as u8, ..Default::default() } }
}

fn device(kind: SDL_EventType, which: u32) -> SDL_Event {
    SDL_Event { gdevice: SDL_GamepadDeviceEvent { r#type: kind, which: SDL_JoystickID(which), ..Default::default() } }
}

fn window(kind: SDL_EventType) -> SDL_Event {
    SDL_Event { window: SDL_WindowEvent { r#type: kind, ..Default::default() } }
}

#[test]
fn bits() {
    assert!(bit_of(PadButton::Up) == 0x1000 && bit_of(PadButton::Right) == 0x2000);
    assert!(bit_of(PadButton::Down) == 0x4000 && bit_of(PadButton::Left) == 0x8000);
    assert!(bit_of(PadButton::Triangle) == 0x10 && bit_of(PadButton::Cross) == 0x20);
    assert!(bit_of(PadButton::Circle) == 0x40 && bit_of(PadButton::Square) == 0x80);
}

#[test]
fn defaults() {
    let b = Bindings::defaults();
    assert!(b.keys[&SDL_SCANCODE_RETURN] == PadButton::Cross && b.keys[&SDL_SCANCODE_Z] == PadButton::Cross);
    assert!(b.keys[&SDL_SCANCODE_ESCAPE] == PadButton::Circle && b.keys[&SDL_SCANCODE_X] == PadButton::Circle);
    assert!(b.keys[&SDL_SCANCODE_BACKSPACE] == PadButton::Square && b.keys[&SDL_SCANCODE_S] == PadButton::Square);
    assert!(b.keys[&SDL_SCANCODE_T] == PadButton::Triangle && b.keys[&SDL_SCANCODE_UP] == PadButton::Up);
    assert!(b.buttons[&SDL_GAMEPAD_BUTTON_SOUTH] == PadButton::Cross && b.buttons[&SDL_GAMEPAD_BUTTON_EAST] == PadButton::Circle);
    assert!(b.buttons[&SDL_GAMEPAD_BUTTON_WEST] == PadButton::Square && b.buttons[&SDL_GAMEPAD_BUTTON_NORTH] == PadButton::Triangle);
    let mut r = b.clone();
    r.bind_key(SDL_SCANCODE_Z, PadButton::Triangle);
    assert!(r.keys[&SDL_SCANCODE_Z] == PadButton::Triangle && r.keys[&SDL_SCANCODE_RETURN] == PadButton::Cross);
}

// Review Focus 1.
#[test]
fn edges() {
    let b = Bindings::defaults();
    let mut reader = PadReader::default();
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_DOWN, SDL_SCANCODE_RETURN, false));
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_UP, SDL_SCANCODE_RETURN, false));
    let w = reader.frame();
    assert!(w.pressed == 0x20 && w.released == 0x20 && w.held == 0 && w.repeating == 0x20);
    let w = reader.frame();
    assert!(w.pressed == 0 && w.released == 0);
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_DOWN, SDL_SCANCODE_DOWN, false));
    assert_eq!(reader.frame().pressed, 0x4000);
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_DOWN, SDL_SCANCODE_DOWN, true));
    for _ in 0..5 {
        let w = reader.frame();
        assert!(w.pressed == 0 && w.held == 0x4000);
    }
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_UP, SDL_SCANCODE_DOWN, false));
    let w = reader.frame();
    assert!(w.released == 0x4000 && w.held == 0);
    // Keyboard and gamepad hold the same button: it stays held until both let go.
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_DOWN, SDL_SCANCODE_Z, false));
    feed(&mut reader, &b, &button(SDL_EVENT_GAMEPAD_BUTTON_DOWN, 7, SDL_GAMEPAD_BUTTON_SOUTH));
    assert_eq!(reader.frame().pressed, 0x20);
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_UP, SDL_SCANCODE_Z, false));
    assert_eq!(reader.frame().held, 0x20);
}

// Review Focus 4.
#[test]
fn releases() {
    let b = Bindings::defaults();
    let mut reader = PadReader::default();
    feed(&mut reader, &b, &button(SDL_EVENT_GAMEPAD_BUTTON_DOWN, 3, SDL_GAMEPAD_BUTTON_EAST));
    assert_eq!(reader.frame().held, 0x40);
    feed(&mut reader, &b, &device(SDL_EVENT_GAMEPAD_REMOVED, 3));
    let w = reader.frame();
    assert!(w.held == 0 && w.released == 0x40);
    feed(&mut reader, &b, &button(SDL_EVENT_GAMEPAD_BUTTON_DOWN, 4, SDL_GAMEPAD_BUTTON_EAST));
    assert_eq!(reader.frame().pressed, 0x40);
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_DOWN, SDL_SCANCODE_LEFT, false));
    reader.frame();
    feed(&mut reader, &b, &window(SDL_EVENT_WINDOW_FOCUS_LOST));
    let w = reader.frame();
    assert!(w.held == 0 && (w.released & 0x8000) != 0 && (w.released & 0x40) != 0);
    feed(&mut reader, &b, &key(SDL_EVENT_KEY_DOWN, SDL_SCANCODE_LEFT, false));
    assert_eq!(reader.frame().pressed, 0x8000);
}
