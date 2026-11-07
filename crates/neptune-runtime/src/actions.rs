//! Input actions (ext): named controls that keys, mouse buttons and gamepad
//! buttons or sticks are bound to, so a game asks `ActionDown("jump")`
//! instead of checking keys, and players can change the controls.
//!
//! An input is written as text, which is what `LastInput`, `ActionInputs`
//! and the bindings file use:
//!
//! * `key:57` – a key by scancode (`KEY_SPACE` is 57)
//! * `mouse:1` – a mouse button (1 left, 2 right, 3 middle)
//! * `joy:1` – a gamepad button on any gamepad (`JOY_A` is 1); `joy0:1` only
//!   on the first gamepad
//! * `axis:x+`, `axis:y-` – a stick pushed one way (x/y left stick, u/v right
//!   stick, z triggers), on any gamepad; `axis0:x+` only on the first
//!
//! Actions are updated once a frame. `ActionHit` counts presses since it was
//! last asked; `ActionValue` is 0-1 and follows a stick smoothly.

use std::collections::{BTreeMap, VecDeque};

use crate::input::Input;

/// A stick must be pushed this far to count as pressed.
const DEADZONE: f32 = 0.25;
/// ... and this far to count as a press for `LastInput`.
const PICK: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Binding {
    Key(i32),
    Mouse(i32),
    /// port (-1 = any), button
    Joy(i32, i32),
    /// port (-1 = any), axis letter, positive direction
    Axis(i32, char, bool),
}

impl Binding {
    pub fn parse(s: &str) -> Option<Binding> {
        let s = s.trim().to_ascii_lowercase();
        let (kind, rest) = s.split_once(':')?;
        let port = |k: &str, base: &str| -> Option<i32> {
            let p = &k[base.len()..];
            if p.is_empty() {
                Some(-1)
            } else {
                p.parse().ok().filter(|n: &i32| *n >= 0)
            }
        };
        if kind == "key" {
            return rest.parse().ok().filter(|k| (1..256).contains(k)).map(Binding::Key);
        }
        if kind == "mouse" {
            return rest.parse().ok().filter(|b| (1..4).contains(b)).map(Binding::Mouse);
        }
        if kind.starts_with("joy") {
            let p = port(kind, "joy")?;
            return rest.parse().ok().filter(|b| (1..=17).contains(b)).map(|b| Binding::Joy(p, b));
        }
        if kind.starts_with("axis") {
            let p = port(kind, "axis")?;
            let mut c = rest.chars();
            let (a, d) = (c.next()?, c.next()?);
            if !"xyzuv".contains(a) || !(d == '+' || d == '-') || c.next().is_some() {
                return None;
            }
            return Some(Binding::Axis(p, a, d == '+'));
        }
        None
    }

    pub fn text(&self) -> String {
        let port = |p: i32| if p < 0 { String::new() } else { p.to_string() };
        match *self {
            Binding::Key(k) => format!("key:{k}"),
            Binding::Mouse(b) => format!("mouse:{b}"),
            Binding::Joy(p, b) => format!("joy{}:{b}", port(p)),
            Binding::Axis(p, a, pos) => format!("axis{}:{a}{}", port(p), if pos { '+' } else { '-' }),
        }
    }

    /// A name to show players: "Space", "Mouse left", "Gamepad A", "Left stick right".
    pub fn name(&self) -> String {
        let pad = |p: i32| if p < 0 { "Gamepad".to_string() } else { format!("Gamepad {}", p + 1) };
        match *self {
            Binding::Key(k) => crate::input::key_name(k).map(title).unwrap_or_else(|| format!("Key {k}")),
            Binding::Mouse(1) => "Mouse left".into(),
            Binding::Mouse(2) => "Mouse right".into(),
            Binding::Mouse(b) => if b == 3 { "Mouse middle".into() } else { format!("Mouse {b}") },
            Binding::Joy(p, b) => format!("{} {}", pad(p), crate::joy::button_name(b).unwrap_or("?")),
            Binding::Axis(p, a, pos) => {
                let (stick, neg, posn) = match a {
                    'x' => ("left stick", "left", "right"),
                    'y' => ("left stick", "up", "down"),
                    'u' => ("right stick", "left", "right"),
                    'v' => ("right stick", "up", "down"),
                    _ => ("triggers", "left", "right"),
                };
                let s = format!("{stick} {}", if pos { posn } else { neg });
                if p < 0 {
                    upper_first(&s)
                } else {
                    format!("{} {s}", pad(p))
                }
            }
        }
    }

    /// How far the input is pressed, 0-1.
    fn value(&self, input: &Input) -> f32 {
        let ports = |p: i32| if p < 0 { 0..input.joys.port_count() as i32 } else { p..p + 1 };
        match *self {
            Binding::Key(k) => input.key_down(k) as i32 as f32,
            Binding::Mouse(b) => input.mouse_down(b) as i32 as f32,
            Binding::Joy(p, b) => ports(p).any(|port| input.joys.down(b, port)) as i32 as f32,
            Binding::Axis(p, a, pos) => ports(p)
                .map(|port| {
                    let v = input.joys.value(port, &a.to_string());
                    let v = if pos { v } else { -v };
                    ((v - DEADZONE) / (1.0 - DEADZONE)).clamp(0.0, 1.0)
                })
                .fold(0.0, f32::max),
        }
    }
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// "LSHIFT" → "Lshift", "NUMPAD7" → "Numpad7", "SPACE" → "Space".
fn title(s: &str) -> String {
    upper_first(&s.to_ascii_lowercase())
}

#[derive(Debug, Default)]
struct Action {
    bindings: Vec<Binding>,
    down: bool,
    hits: u32,
    value: f32,
}

#[derive(Debug, Default)]
pub struct Actions {
    actions: BTreeMap<String, Action>,
    /// Inputs that went down this frame, oldest first (`LastInput`).
    pressed: VecDeque<Binding>,
    /// What was down last frame (for `LastInput`).
    was_down: Vec<Binding>,
}

fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

impl Actions {
    pub fn bind(&mut self, action: &str, b: Binding) {
        let a = self.actions.entry(key(action)).or_default();
        if !a.bindings.contains(&b) {
            a.bindings.push(b);
        }
    }

    /// Removes an action's bindings ("" removes every action).
    pub fn clear(&mut self, action: &str) {
        if action.trim().is_empty() {
            self.actions.clear();
        } else if let Some(a) = self.actions.get_mut(&key(action)) {
            a.bindings.clear();
        }
    }

    pub fn down(&self, action: &str) -> bool {
        self.actions.get(&key(action)).is_some_and(|a| a.down)
    }

    pub fn hit(&mut self, action: &str) -> i32 {
        self.actions.get_mut(&key(action)).map_or(0, |a| std::mem::take(&mut a.hits) as i32)
    }

    pub fn value(&self, action: &str) -> f32 {
        self.actions.get(&key(action)).map_or(0.0, |a| a.value)
    }

    pub fn inputs(&self, action: &str) -> String {
        self.actions.get(&key(action)).map(|a| a.bindings.iter().map(Binding::text).collect::<Vec<_>>().join(",")).unwrap_or_default()
    }

    pub fn last_input(&mut self) -> String {
        self.pressed.pop_front().map(|b| b.text()).unwrap_or_default()
    }

    /// The bindings as text: one `action=input,input` line per action.
    pub fn save_text(&self) -> String {
        self.actions.iter().map(|(n, a)| format!("{n}={}\n", a.bindings.iter().map(Binding::text).collect::<Vec<_>>().join(","))).collect()
    }

    /// Reads `action=input,input` lines; the actions named replace theirs.
    pub fn load_text(&mut self, text: &str) -> Result<(), String> {
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            let (name, list) = line.split_once('=').ok_or_else(|| format!("line {}: expected action=inputs", i + 1))?;
            self.clear(name);
            self.actions.entry(key(name)).or_default();
            for s in list.split(',').filter(|s| !s.trim().is_empty()) {
                let b = Binding::parse(s).ok_or_else(|| format!("line {}: '{}' is not an input", i + 1, s.trim()))?;
                self.bind(name, b);
            }
        }
        Ok(())
    }
}

impl Input {
    /// Works out every action's state for this frame (after events and
    /// gamepads were read).
    pub fn update_actions(&mut self) {
        let mut acts = std::mem::take(&mut self.actions);
        for a in acts.actions.values_mut() {
            let v = a.bindings.iter().map(|b| b.value(self)).fold(0.0, f32::max);
            let down = v > 0.0;
            if down && !a.down {
                a.hits += 1;
            }
            a.down = down;
            a.value = v;
        }
        // newly pressed inputs, for "press a key for Jump" menus
        let mut now = Vec::new();
        for k in 1..256 {
            if self.key_down(k) {
                now.push(Binding::Key(k));
            }
        }
        for b in 1..4 {
            if self.mouse_down(b) {
                now.push(Binding::Mouse(b));
            }
        }
        for port in 0..self.joys.port_count() as i32 {
            for b in 1..=17 {
                if self.joys.down(b, port) {
                    now.push(Binding::Joy(-1, b));
                }
            }
            for a in ['x', 'y', 'u', 'v', 'z'] {
                let v = self.joys.value(port, &a.to_string());
                if v.abs() > PICK {
                    now.push(Binding::Axis(-1, a, v > 0.0));
                }
            }
        }
        for b in &now {
            if !acts.was_down.contains(b) && acts.pressed.len() < 16 {
                acts.pressed.push_back(*b);
            }
        }
        acts.was_down = now;
        self.actions = acts;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_parse_and_print() {
        for s in ["key:57", "mouse:1", "joy:1", "joy0:8", "axis:x+", "axis1:v-"] {
            assert_eq!(Binding::parse(s).unwrap().text(), s);
        }
        for bad in ["key:0", "mouse:9", "joy:40", "axis:q+", "axis:x", "pad:1", "key57"] {
            assert!(Binding::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(Binding::parse("KEY:57").unwrap().name(), "Space");
        assert_eq!(Binding::parse("joy:1").unwrap().name(), "Gamepad A");
        assert_eq!(Binding::parse("axis0:y-").unwrap().name(), "Gamepad 1 left stick up");
        assert_eq!(Binding::parse("axis:x+").unwrap().name(), "Left stick right");
    }

    #[test]
    fn keys_drive_actions() {
        let mut input = Input::default();
        input.actions.bind("Jump", Binding::Key(57));
        input.actions.bind("jump", Binding::Joy(-1, 1));
        input.key_event(fyrox::keyboard::KeyCode::Space, true);
        input.update_actions();
        assert!(input.actions.down("JUMP"));
        assert_eq!(input.actions.value("jump"), 1.0);
        input.update_actions();
        // still held: one press, counted once
        assert_eq!(input.actions.hit("jump"), 1);
        assert_eq!(input.actions.hit("jump"), 0);
        assert_eq!(input.actions.last_input(), "key:57");
        assert_eq!(input.actions.last_input(), "");
        input.key_event(fyrox::keyboard::KeyCode::Space, false);
        input.update_actions();
        assert!(!input.actions.down("jump"));
        assert_eq!(input.actions.inputs("jump"), "key:57,joy:1");
    }

    #[test]
    fn bindings_round_trip() {
        let mut a = Actions::default();
        a.bind("fire", Binding::Mouse(1));
        a.bind("fire", Binding::Key(29));
        a.bind("left", Binding::Axis(-1, 'x', false));
        let text = a.save_text();
        let mut b = Actions::default();
        b.load_text(&text).unwrap();
        assert_eq!(b.save_text(), text);
        assert!(b.load_text("fire=key:999").is_err());
    }
}
