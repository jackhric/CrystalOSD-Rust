use std::collections::BTreeMap;

use scene::menu_types::{PadWords, pad};
use sdl3_sys::events::{
    SDL_EVENT_GAMEPAD_BUTTON_DOWN, SDL_EVENT_GAMEPAD_BUTTON_UP, SDL_EVENT_GAMEPAD_REMOVED, SDL_EVENT_KEY_DOWN, SDL_EVENT_KEY_UP, SDL_EVENT_WINDOW_FOCUS_LOST,
    SDL_Event, SDL_EventType,
};
use sdl3_sys::gamepad::*;
use sdl3_sys::scancode::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadButton {
    Up,
    Down,
    Left,
    Right,
    Cross,
    Circle,
    Square,
    Triangle,
}

pub fn bit_of(button: PadButton) -> u32 {
    match button {
        PadButton::Up => pad::UP,
        PadButton::Down => pad::DOWN,
        PadButton::Left => pad::LEFT,
        PadButton::Right => pad::RIGHT,
        PadButton::Cross => pad::CROSS,
        PadButton::Circle => pad::CIRCLE,
        PadButton::Square => pad::SQUARE,
        PadButton::Triangle => pad::TRIANGLE,
    }
}

#[derive(Debug, Clone, Default)]
pub struct PadReader {
    held: BTreeMap<u32, u32>,
    previous: u32,
    pressed: u32,
    released: u32,
}

impl PadReader {
    fn held(&self) -> u32 {
        self.held.values().fold(0, |out, bits| out | bits)
    }

    pub fn down(&mut self, source: u32, bit: u32) {
        if self.held() & bit == 0 {
            self.pressed |= bit;
        }
        *self.held.entry(source).or_default() |= bit;
    }

    pub fn up(&mut self, source: u32, bit: u32) {
        *self.held.entry(source).or_default() &= !bit;
        if self.held() & bit == 0 {
            self.released |= bit;
        }
    }

    pub fn drop_source(&mut self, source: u32) {
        let Some(bits) = self.held.remove(&source) else { return };
        self.released |= bits & !self.held();
    }

    pub fn clear(&mut self) {
        self.released |= self.held();
        self.held.clear();
    }

    pub fn frame(&mut self) -> PadWords {
        let now = self.held();
        let mut w = PadWords { held: now, pressed: (now & !self.previous) | self.pressed, released: (self.previous & !now) | self.released, repeating: 0 };
        w.repeating = w.pressed;
        self.previous = now;
        self.pressed = 0;
        self.released = 0;
        w
    }
}

#[derive(Clone, Default)]
pub struct Bindings {
    pub keys: BTreeMap<SDL_Scancode, PadButton>,
    pub buttons: BTreeMap<SDL_GamepadButton, PadButton>,
}

impl Bindings {
    pub fn defaults() -> Self {
        Bindings {
            keys: BTreeMap::from([
                (SDL_SCANCODE_UP, PadButton::Up),
                (SDL_SCANCODE_DOWN, PadButton::Down),
                (SDL_SCANCODE_LEFT, PadButton::Left),
                (SDL_SCANCODE_RIGHT, PadButton::Right),
                (SDL_SCANCODE_RETURN, PadButton::Cross),
                (SDL_SCANCODE_Z, PadButton::Cross),
                (SDL_SCANCODE_ESCAPE, PadButton::Circle),
                (SDL_SCANCODE_X, PadButton::Circle),
                (SDL_SCANCODE_BACKSPACE, PadButton::Square),
                (SDL_SCANCODE_S, PadButton::Square),
                (SDL_SCANCODE_T, PadButton::Triangle),
            ]),
            buttons: BTreeMap::from([
                (SDL_GAMEPAD_BUTTON_DPAD_UP, PadButton::Up),
                (SDL_GAMEPAD_BUTTON_DPAD_DOWN, PadButton::Down),
                (SDL_GAMEPAD_BUTTON_DPAD_LEFT, PadButton::Left),
                (SDL_GAMEPAD_BUTTON_DPAD_RIGHT, PadButton::Right),
                (SDL_GAMEPAD_BUTTON_SOUTH, PadButton::Cross),
                (SDL_GAMEPAD_BUTTON_EAST, PadButton::Circle),
                (SDL_GAMEPAD_BUTTON_WEST, PadButton::Square),
                (SDL_GAMEPAD_BUTTON_NORTH, PadButton::Triangle),
            ]),
        }
    }

    pub fn bind_key(&mut self, key: SDL_Scancode, button: PadButton) {
        self.keys.insert(key, button);
    }

    pub fn bind_button(&mut self, pad: SDL_GamepadButton, button: PadButton) {
        self.buttons.insert(pad, button);
    }
}

pub fn feed(reader: &mut PadReader, bindings: &Bindings, event: &SDL_Event) {
    // SAFETY: every SDL_Event variant starts with the type word, and each arm reads the variant its type names.
    unsafe {
        let kind = SDL_EventType(event.r#type);
        if kind == SDL_EVENT_KEY_DOWN || kind == SDL_EVENT_KEY_UP {
            if event.key.repeat {
                return;
            }
            let Some(&button) = bindings.keys.get(&event.key.scancode) else { return };
            if kind == SDL_EVENT_KEY_DOWN {
                reader.down(0, bit_of(button));
            } else {
                reader.up(0, bit_of(button));
            }
        } else if kind == SDL_EVENT_GAMEPAD_BUTTON_DOWN || kind == SDL_EVENT_GAMEPAD_BUTTON_UP {
            let Some(&button) = bindings.buttons.get(&SDL_GamepadButton(i32::from(event.gbutton.button))) else { return };
            let source = event.gbutton.which.0;
            if kind == SDL_EVENT_GAMEPAD_BUTTON_DOWN {
                reader.down(source, bit_of(button));
            } else {
                reader.up(source, bit_of(button));
            }
        } else if kind == SDL_EVENT_GAMEPAD_REMOVED {
            reader.drop_source(event.gdevice.which.0);
        } else if kind == SDL_EVENT_WINDOW_FOCUS_LOST {
            reader.clear();
        }
    }
}
