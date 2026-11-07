//! Keyboard and mouse state with Blitz3D (DirectInput) scancodes.

use fyrox::keyboard::KeyCode;
use neptune_lang::commands::ConstVal;

/// (KeyCode, Blitz scancode, NeptuneBASIC constant name)
const KEYS: &[(KeyCode, i32, &str)] = &[
    (KeyCode::Escape, 1, "ESCAPE"),
    (KeyCode::Digit1, 2, "1"), (KeyCode::Digit2, 3, "2"), (KeyCode::Digit3, 4, "3"),
    (KeyCode::Digit4, 5, "4"), (KeyCode::Digit5, 6, "5"), (KeyCode::Digit6, 7, "6"),
    (KeyCode::Digit7, 8, "7"), (KeyCode::Digit8, 9, "8"), (KeyCode::Digit9, 10, "9"),
    (KeyCode::Digit0, 11, "0"),
    (KeyCode::Minus, 12, "MINUS"), (KeyCode::Equal, 13, "EQUALS"),
    (KeyCode::Backspace, 14, "BACKSPACE"), (KeyCode::Tab, 15, "TAB"),
    (KeyCode::KeyQ, 16, "Q"), (KeyCode::KeyW, 17, "W"), (KeyCode::KeyE, 18, "E"),
    (KeyCode::KeyR, 19, "R"), (KeyCode::KeyT, 20, "T"), (KeyCode::KeyY, 21, "Y"),
    (KeyCode::KeyU, 22, "U"), (KeyCode::KeyI, 23, "I"), (KeyCode::KeyO, 24, "O"),
    (KeyCode::KeyP, 25, "P"),
    (KeyCode::BracketLeft, 26, "LBRACKET"), (KeyCode::BracketRight, 27, "RBRACKET"),
    (KeyCode::Enter, 28, "ENTER"), (KeyCode::ControlLeft, 29, "LCONTROL"),
    (KeyCode::KeyA, 30, "A"), (KeyCode::KeyS, 31, "S"), (KeyCode::KeyD, 32, "D"),
    (KeyCode::KeyF, 33, "F"), (KeyCode::KeyG, 34, "G"), (KeyCode::KeyH, 35, "H"),
    (KeyCode::KeyJ, 36, "J"), (KeyCode::KeyK, 37, "K"), (KeyCode::KeyL, 38, "L"),
    (KeyCode::Semicolon, 39, "SEMICOLON"), (KeyCode::Quote, 40, "APOSTROPHE"),
    (KeyCode::Backquote, 41, "GRAVE"), (KeyCode::ShiftLeft, 42, "LSHIFT"),
    (KeyCode::Backslash, 43, "BACKSLASH"),
    (KeyCode::KeyZ, 44, "Z"), (KeyCode::KeyX, 45, "X"), (KeyCode::KeyC, 46, "C"),
    (KeyCode::KeyV, 47, "V"), (KeyCode::KeyB, 48, "B"), (KeyCode::KeyN, 49, "N"),
    (KeyCode::KeyM, 50, "M"),
    (KeyCode::Comma, 51, "COMMA"), (KeyCode::Period, 52, "PERIOD"), (KeyCode::Slash, 53, "SLASH"),
    (KeyCode::ShiftRight, 54, "RSHIFT"), (KeyCode::NumpadMultiply, 55, "MULTIPLY"),
    (KeyCode::AltLeft, 56, "LALT"), (KeyCode::Space, 57, "SPACE"), (KeyCode::CapsLock, 58, "CAPSLOCK"),
    (KeyCode::F1, 59, "F1"), (KeyCode::F2, 60, "F2"), (KeyCode::F3, 61, "F3"), (KeyCode::F4, 62, "F4"),
    (KeyCode::F5, 63, "F5"), (KeyCode::F6, 64, "F6"), (KeyCode::F7, 65, "F7"), (KeyCode::F8, 66, "F8"),
    (KeyCode::F9, 67, "F9"), (KeyCode::F10, 68, "F10"),
    (KeyCode::NumLock, 69, "NUMLOCK"), (KeyCode::ScrollLock, 70, "SCROLL"),
    (KeyCode::Numpad7, 71, "NUMPAD7"), (KeyCode::Numpad8, 72, "NUMPAD8"), (KeyCode::Numpad9, 73, "NUMPAD9"),
    (KeyCode::NumpadSubtract, 74, "SUBTRACT"),
    (KeyCode::Numpad4, 75, "NUMPAD4"), (KeyCode::Numpad5, 76, "NUMPAD5"), (KeyCode::Numpad6, 77, "NUMPAD6"),
    (KeyCode::NumpadAdd, 78, "ADD"),
    (KeyCode::Numpad1, 79, "NUMPAD1"), (KeyCode::Numpad2, 80, "NUMPAD2"), (KeyCode::Numpad3, 81, "NUMPAD3"),
    (KeyCode::Numpad0, 82, "NUMPAD0"), (KeyCode::NumpadDecimal, 83, "DECIMAL"),
    (KeyCode::F11, 87, "F11"), (KeyCode::F12, 88, "F12"),
    (KeyCode::NumpadEnter, 156, "NUMPADENTER"), (KeyCode::ControlRight, 157, "RCONTROL"),
    (KeyCode::NumpadDivide, 181, "DIVIDE"), (KeyCode::AltRight, 184, "RALT"),
    (KeyCode::Pause, 197, "PAUSE"), (KeyCode::Home, 199, "HOME"),
    (KeyCode::ArrowUp, 200, "UP"), (KeyCode::PageUp, 201, "PAGEUP"),
    (KeyCode::ArrowLeft, 203, "LEFT"), (KeyCode::ArrowRight, 205, "RIGHT"),
    (KeyCode::End, 207, "END"), (KeyCode::ArrowDown, 208, "DOWN"), (KeyCode::PageDown, 209, "PAGEDOWN"),
    (KeyCode::Insert, 210, "INSERT"), (KeyCode::Delete, 211, "DELETE"),
    (KeyCode::SuperLeft, 219, "LWIN"), (KeyCode::SuperRight, 220, "RWIN"),
];

/// Constant name of a scancode ("SPACE" for 57).
pub fn key_name(k: i32) -> Option<&'static str> {
    KEYS.iter().find(|(_, s, _)| *s == k).map(|(_, _, n)| *n)
}

pub fn scancode(k: KeyCode) -> Option<i32> {
    KEYS.iter().find(|(c, _, _)| *c == k).map(|(_, s, _)| *s)
}

/// `KEY_ESCAPE`, `KEY_W`, `MOUSE_LEFT`, ... constants.
pub fn constants() -> Vec<(String, ConstVal)> {
    let mut v: Vec<(String, ConstVal)> = KEYS.iter().map(|(_, s, n)| (format!("KEY_{n}"), ConstVal::Int(*s))).collect();
    v.push(("MOUSE_LEFT".into(), ConstVal::Int(1)));
    v.push(("MOUSE_RIGHT".into(), ConstVal::Int(2)));
    v.push(("MOUSE_MIDDLE".into(), ConstVal::Int(3)));
    v
}

/// Accumulated input state, fed from window events.
#[derive(Debug)]
pub struct Input {
    down: [bool; 256],
    hits: [u32; 256],
    /// Queue of characters for `GetKey`.
    chars: std::collections::VecDeque<i32>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_z: f32,
    last_mouse: (f32, f32),
    mouse_down: [bool; 4],
    mouse_hits: [u32; 4],
    /// Raw mouse movement since the last `MouseXSpeed`/`MouseYSpeed` (works
    /// while the pointer is hidden or captured), and wheel movement since the
    /// last `MouseZSpeed`.
    raw: (f32, f32),
    last_z: f32,
    pub joys: crate::joy::Joysticks,
    /// (ext) `BindKey`, `ActionDown`, ... (see `actions.rs`)
    pub actions: crate::actions::Actions,
}

impl Default for Input {
    fn default() -> Self {
        Input {
            down: [false; 256],
            hits: [0; 256],
            chars: Default::default(),
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_z: 0.0,
            last_mouse: (0.0, 0.0),
            mouse_down: [false; 4],
            mouse_hits: [0; 4],
            raw: (0.0, 0.0),
            last_z: 0.0,
            joys: Default::default(),
            actions: Default::default(),
        }
    }
}

impl Input {
    pub fn key_event(&mut self, k: KeyCode, pressed: bool) {
        if let Some(s) = scancode(k) {
            let s = s as usize;
            if pressed && !self.down[s] {
                self.hits[s] += 1;
            }
            self.down[s] = pressed;
        }
    }
    pub fn char_event(&mut self, c: char) {
        self.chars.push_back(c as i32);
    }
    pub fn mouse_button(&mut self, button: usize, pressed: bool) {
        if button < 4 {
            if pressed && !self.mouse_down[button] {
                self.mouse_hits[button] += 1;
            }
            self.mouse_down[button] = pressed;
        }
    }
    pub fn key_down(&self, k: i32) -> bool {
        (0..256).contains(&k) && self.down[k as usize]
    }
    /// Number of presses since the last call for this key.
    pub fn key_hit(&mut self, k: i32) -> i32 {
        if !(0..256).contains(&k) {
            return 0;
        }
        std::mem::take(&mut self.hits[k as usize]) as i32
    }
    pub fn get_key(&mut self) -> i32 {
        self.chars.pop_front().unwrap_or(0)
    }
    /// All typed characters waiting for `GetKey` (a focused UI text box).
    pub fn take_chars(&mut self) -> Vec<i32> {
        self.chars.drain(..).collect()
    }
    pub fn mouse_down(&self, b: i32) -> bool {
        (1..4).contains(&b) && self.mouse_down[b as usize]
    }
    pub fn mouse_hit(&mut self, b: i32) -> i32 {
        if !(1..4).contains(&b) {
            return 0;
        }
        std::mem::take(&mut self.mouse_hits[b as usize]) as i32
    }
    /// Raw (device) mouse movement in logical pixels.
    pub fn mouse_motion(&mut self, dx: f32, dy: f32) {
        self.raw.0 += dx;
        self.raw.1 += dy;
    }
    /// Horizontal mouse movement since the previous `MouseXSpeed` call.
    pub fn mouse_speed_x(&mut self) -> f32 {
        self.last_mouse.0 = self.mouse_x;
        std::mem::take(&mut self.raw.0)
    }
    /// Vertical mouse movement since the previous `MouseYSpeed` call.
    pub fn mouse_speed_y(&mut self) -> f32 {
        self.last_mouse.1 = self.mouse_y;
        std::mem::take(&mut self.raw.1)
    }
    /// Wheel movement since the previous `MouseZSpeed` call.
    pub fn mouse_speed_z(&mut self) -> f32 {
        let d = self.mouse_z - self.last_z;
        self.last_z = self.mouse_z;
        d
    }
    /// `MoveMouse`: the pointer jumps without counting as movement.
    pub fn moved_to(&mut self, x: f32, y: f32) {
        self.mouse_x = x;
        self.mouse_y = y;
        self.last_mouse = (x, y);
    }
    /// First mouse button pressed since the last call (`GetMouse`), or 0.
    pub fn get_mouse(&mut self) -> i32 {
        match (1..4).find(|&b| self.mouse_hits[b] > 0) {
            Some(b) => {
                self.mouse_hits[b] -= 1;
                b as i32
            }
            None => 0,
        }
    }
    /// Scancode of a key pressed since the last call (`WaitKey`), or 0.
    pub fn any_key_hit(&mut self) -> i32 {
        match (0..256).find(|&k| self.hits[k] > 0) {
            Some(k) => {
                self.hits[k] -= 1;
                k as i32
            }
            None => 0,
        }
    }
    pub fn flush_keys(&mut self) {
        self.down = [false; 256];
        self.hits = [0; 256];
        self.chars.clear();
    }
    pub fn flush_mouse(&mut self) {
        self.mouse_down = [false; 4];
        self.mouse_hits = [0; 4];
        self.raw = (0.0, 0.0);
    }
}
