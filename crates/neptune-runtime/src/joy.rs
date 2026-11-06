//! Joysticks and gamepads (`JoyX`, `JoyDown`, `JoyHat`, ...) through gilrs.
//!
//! Ports are the connected controllers in the order they were found (port
//! 0 is the first, like Blitz3D's default). Buttons use the numbering that
//! DirectInput gives an Xbox-style controller, which most Blitz3D games were
//! written for: 1 A, 2 B, 3 X, 4 Y, 5 LB, 6 RB, 7 Back, 8 Start, 9 left
//! stick, 10 right stick, 11 LT, 12 RT, 13-16 d-pad up/down/left/right,
//! 17 guide. Named constants (`JOY_A`, `JOY_START`, ...) are extensions.
//!
//! Axes are -1..1 with Blitz's directions: `JoyY` is positive *down*.
//! `JoyX/JoyY` are the left stick, `JoyU/JoyV` the right stick, `JoyZ` the
//! triggers (right minus left), and `JoyPitch/JoyYaw/JoyRoll` the right
//! stick and triggers in degrees (-180..180).

use gilrs::{Axis, Button, Event, EventType, GamepadId, Gilrs};
use neptune_lang::commands::ConstVal;

const BUTTONS: [(Button, &str); 17] = [
    (Button::South, "A"),
    (Button::East, "B"),
    (Button::West, "X"),
    (Button::North, "Y"),
    (Button::LeftTrigger, "LB"),
    (Button::RightTrigger, "RB"),
    (Button::Select, "BACK"),
    (Button::Start, "START"),
    (Button::LeftThumb, "LSTICK"),
    (Button::RightThumb, "RSTICK"),
    (Button::LeftTrigger2, "LT"),
    (Button::RightTrigger2, "RT"),
    (Button::DPadUp, "UP"),
    (Button::DPadDown, "DOWN"),
    (Button::DPadLeft, "LEFT"),
    (Button::DPadRight, "RIGHT"),
    (Button::Mode, "GUIDE"),
];

/// Blitz button number (1-based) of a gilrs button.
pub fn button_number(b: Button) -> Option<usize> {
    BUTTONS.iter().position(|(x, _)| *x == b).map(|i| i + 1)
}

pub fn constants() -> Vec<(String, ConstVal)> {
    BUTTONS.iter().enumerate().map(|(i, (_, n))| (format!("JOY_{n}"), ConstVal::Int(i as i32 + 1))).collect()
}

/// Hat direction in degrees (0 up, clockwise) from the d-pad, or -1.
pub fn hat(up: bool, down: bool, left: bool, right: bool) -> i32 {
    let (x, y) = (right as i32 - left as i32, down as i32 - up as i32);
    match (x, y) {
        (0, -1) => 0,
        (1, -1) => 45,
        (1, 0) => 90,
        (1, 1) => 135,
        (0, 1) => 180,
        (-1, 1) => 225,
        (-1, 0) => 270,
        (-1, -1) => 315,
        _ => -1,
    }
}

/// Direction of an axis value as -1, 0 or 1 (`JoyXDir` and friends).
pub fn dir(v: f32) -> i32 {
    if v > 0.5 {
        1
    } else if v < -0.5 {
        -1
    } else {
        0
    }
}

pub struct Joysticks {
    gilrs: Option<Gilrs>,
    /// Connected controllers in port order.
    ports: Vec<GamepadId>,
    /// Presses not yet reported by `JoyHit`/`GetJoy`, per port and button.
    hits: Vec<[u32; 18]>,
}

impl std::fmt::Debug for Joysticks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Joysticks").field("ports", &self.ports.len()).finish()
    }
}

impl Default for Joysticks {
    fn default() -> Self {
        let gilrs = Gilrs::new().ok();
        let ports = gilrs.as_ref().map(|g| g.gamepads().map(|(id, _)| id).collect()).unwrap_or_default();
        let n = Vec::len(&ports);
        Joysticks { gilrs, ports, hits: vec![[0; 18]; n] }
    }
}

impl Joysticks {
    /// Processes controller events (once per frame).
    pub fn poll(&mut self) {
        let Some(g) = self.gilrs.as_mut() else { return };
        while let Some(Event { id, event, .. }) = g.next_event() {
            match event {
                EventType::Connected => {
                    if !self.ports.contains(&id) {
                        self.ports.push(id);
                        self.hits.push([0; 18]);
                    }
                }
                EventType::ButtonPressed(b, _) => {
                    if let (Some(port), Some(n)) = (self.ports.iter().position(|p| *p == id), button_number(b)) {
                        self.hits[port][n] += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn pad(&self, port: i32) -> Option<gilrs::Gamepad<'_>> {
        let id = *self.ports.get(usize::try_from(port).ok()?)?;
        let g = self.gilrs.as_ref()?.connected_gamepad(id)?;
        Some(g)
    }

    /// 0 none, 1 digital, 2 analog (every gamepad gilrs finds has sticks).
    pub fn kind(&self, port: i32) -> i32 {
        if self.pad(port).is_some() {
            2
        } else {
            0
        }
    }

    pub fn down(&self, button: i32, port: i32) -> bool {
        let Some(p) = self.pad(port) else { return false };
        usize::try_from(button - 1).ok().and_then(|i| BUTTONS.get(i)).is_some_and(|(b, _)| p.is_pressed(*b))
    }

    pub fn hit(&mut self, button: i32, port: i32) -> i32 {
        let (Ok(p), Ok(b)) = (usize::try_from(port), usize::try_from(button)) else { return 0 };
        match self.hits.get_mut(p).and_then(|h| h.get_mut(b)) {
            Some(n) => std::mem::take(n) as i32,
            None => 0,
        }
    }

    /// First button pressed since the last call (`GetJoy`), or 0.
    pub fn get(&mut self, port: i32) -> i32 {
        let Some(h) = usize::try_from(port).ok().and_then(|p| self.hits.get_mut(p)) else { return 0 };
        match (1..h.len()).find(|&b| h[b] > 0) {
            Some(b) => {
                h[b] -= 1;
                b as i32
            }
            None => 0,
        }
    }

    pub fn flush(&mut self) {
        self.hits.fill([0; 18]);
    }

    fn axis(&self, port: i32, a: Axis) -> f32 {
        self.pad(port).map_or(0.0, |p| p.value(a))
    }

    fn trigger(&self, port: i32, b: Button) -> f32 {
        self.pad(port).and_then(|p| p.button_data(b).map(|d| d.value())).unwrap_or(0.0)
    }

    /// Blitz axis by name: x, y, z, u, v, pitch, yaw, roll.
    pub fn value(&self, port: i32, which: &str) -> f32 {
        let triggers = self.trigger(port, Button::RightTrigger2) - self.trigger(port, Button::LeftTrigger2);
        match which {
            "x" => self.axis(port, Axis::LeftStickX),
            "y" => -self.axis(port, Axis::LeftStickY),
            "z" => triggers,
            "u" => self.axis(port, Axis::RightStickX),
            "v" => -self.axis(port, Axis::RightStickY),
            "pitch" => -self.axis(port, Axis::RightStickY) * 180.0,
            "yaw" => self.axis(port, Axis::RightStickX) * 180.0,
            "roll" => triggers * 180.0,
            _ => 0.0,
        }
    }

    pub fn hat(&self, port: i32) -> i32 {
        let Some(p) = self.pad(port) else { return -1 };
        hat(p.is_pressed(Button::DPadUp), p.is_pressed(Button::DPadDown), p.is_pressed(Button::DPadLeft), p.is_pressed(Button::DPadRight))
    }

    pub fn any_hit(&self) -> bool {
        self.hits.iter().any(|h| h.iter().any(|&n| n > 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_hat_and_dirs() {
        assert_eq!(button_number(Button::South), Some(1));
        assert_eq!(button_number(Button::Start), Some(8));
        assert_eq!(button_number(Button::Mode), Some(17));
        assert_eq!(hat(true, false, false, false), 0);
        assert_eq!(hat(false, true, true, false), 225);
        assert_eq!(hat(false, false, false, false), -1);
        assert_eq!((dir(0.9), dir(-0.7), dir(0.2)), (1, -1, 0));
        assert!(constants().iter().any(|(n, v)| n == "JOY_START" && *v == ConstVal::Int(8)));
    }
}
