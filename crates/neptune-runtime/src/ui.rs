//! UI widgets (ext): `UIButton`, `UICheckBox`, `UISlider`, `UITextBox`,
//! `UIPanel`, `UILabel`, `UIHover`, `UITheme`.
//!
//! Widgets are *immediate*: call them every frame where they should appear,
//! between `Cls`/`RenderWorld` and `Flip`; each draws itself and returns its
//! result (a click, the new value, the edited text). They always use screen
//! coordinates, even with `Camera2D` on, and do not use up `MouseHit` or
//! `KeyHit` presses, except that a text box with the keyboard focus takes
//! the typed characters.

use fyrox::core::color::Color;

use crate::world::Host3D;

#[derive(Debug, Clone)]
pub struct UiState {
    /// Left button state at the end of the last frame.
    prev_down: bool,
    /// The widget the left button went down on.
    pub(crate) active: Option<u64>,
    /// The text box with the keyboard focus.
    focus: Option<u64>,
    /// The mouse was over a widget this frame / last frame.
    hover: bool,
    pub last_hover: bool,
    pub accent: Color,
}

impl Default for UiState {
    fn default() -> Self {
        UiState { prev_down: false, active: None, focus: None, hover: false, last_hover: false, accent: Color::opaque(70, 130, 240) }
    }
}

const BG: Color = Color::opaque(48, 52, 62);
const BG_HOVER: Color = Color::opaque(64, 70, 84);
const PANEL: Color = Color::opaque(30, 32, 40);
const BORDER: Color = Color::opaque(90, 96, 112);
const TEXT: Color = Color::opaque(235, 235, 240);

fn id(kind: u8, x: f32, y: f32) -> u64 {
    ((kind as u64) << 56) ^ ((x.to_bits() as u64) << 24) ^ (y.to_bits() as u64)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color::opaque(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b))
}

impl Host3D<'_> {
    /// Ends a UI frame (called by `Flip`).
    pub fn ui_end_frame(&mut self) {
        let down = self.input.mouse_down(1);
        let ui = &mut self.world.ui;
        if !down {
            ui.active = None;
        }
        ui.prev_down = down;
        ui.last_hover = ui.hover;
        ui.hover = false;
    }

    fn mouse(&self) -> (f32, f32) {
        (self.input.mouse_x, self.input.mouse_y)
    }

    fn inside(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        let (mx, my) = self.mouse();
        mx >= x && my >= y && mx < x + w && my < y + h
    }

    /// Draws in screen space with the given colour, then restores the
    /// program's colour and camera.
    fn ui_draw(&mut self, f: impl FnOnce(&mut crate::draw2d::Canvas2D)) {
        let c = &mut self.world.canvas;
        let (color, cam, origin) = (c.color, c.camera_on, c.origin);
        c.camera_on = false;
        c.origin = (0.0, 0.0);
        f(c);
        c.color = color;
        c.camera_on = cam;
        c.origin = origin;
    }

    /// Mouse logic shared by widgets: (hovered, pressed now, clicked now).
    fn ui_interact(&mut self, wid: u64, x: f32, y: f32, w: f32, h: f32) -> (bool, bool, bool) {
        let over = self.inside(x, y, w, h);
        let down = self.input.mouse_down(1);
        let ui = &mut self.world.ui;
        ui.hover |= over;
        let pressed = over && down && !ui.prev_down;
        if pressed {
            ui.active = Some(wid);
        }
        let clicked = over && !down && ui.prev_down && ui.active == Some(wid);
        (over, pressed, clicked)
    }

    pub fn ui_button(&mut self, x: f32, y: f32, w: f32, h: f32, label: &str) -> bool {
        let wid = id(1, x, y);
        let (over, _, clicked) = self.ui_interact(wid, x, y, w, h);
        let held = self.world.ui.active == Some(wid) && self.input.mouse_down(1);
        let accent = self.world.ui.accent;
        let fill = if held { accent } else if over { BG_HOVER } else { BG };
        self.ui_draw(|c| {
            c.color = fill;
            c.rect(x, y, w, h, true);
            c.color = if over { accent } else { BORDER };
            c.rect(x, y, w, h, false);
            c.color = TEXT;
            c.text(x + w / 2.0, y + h / 2.0, label, true, true);
        });
        clicked
    }

    pub fn ui_checkbox(&mut self, x: f32, y: f32, label: &str, checked: bool) -> bool {
        let size = 18.0;
        let tw = self.world.canvas.font.width(label);
        let wid = id(2, x, y);
        let (over, _, clicked) = self.ui_interact(wid, x, y, size + 8.0 + tw, size);
        let checked = if clicked { !checked } else { checked };
        let accent = self.world.ui.accent;
        self.ui_draw(|c| {
            c.color = if over { BG_HOVER } else { BG };
            c.rect(x, y, size, size, true);
            c.color = BORDER;
            c.rect(x, y, size, size, false);
            if checked {
                c.color = accent;
                c.rect(x + 4.0, y + 4.0, size - 8.0, size - 8.0, true);
            }
            c.color = TEXT;
            c.text(x + size + 8.0, y + size / 2.0, label, false, true);
        });
        checked
    }

    pub fn ui_slider(&mut self, x: f32, y: f32, w: f32, value: f32, min: f32, max: f32) -> f32 {
        let h = 20.0;
        let wid = id(3, x, y);
        let (over, _, _) = self.ui_interact(wid, x, y, w, h);
        let range = if max != min { max - min } else { 1.0 };
        let mut v = value;
        if self.world.ui.active == Some(wid) && self.input.mouse_down(1) {
            let t = ((self.mouse().0 - x) / w).clamp(0.0, 1.0);
            v = min + t * range;
        }
        let t = ((v - min) / range).clamp(0.0, 1.0);
        let accent = self.world.ui.accent;
        self.ui_draw(|c| {
            c.color = BG;
            c.rect(x, y + h / 2.0 - 3.0, w, 6.0, true);
            c.color = accent;
            c.rect(x, y + h / 2.0 - 3.0, w * t, 6.0, true);
            c.color = if over { TEXT } else { mix(TEXT, BG, 0.2) };
            c.rect(x + w * t - 5.0, y, 10.0, h, true);
        });
        v
    }

    pub fn ui_textbox(&mut self, x: f32, y: f32, w: f32, text: &str) -> String {
        let h = self.world.canvas.font.height() + 10.0;
        let wid = id(4, x, y);
        let (over, pressed, _) = self.ui_interact(wid, x, y, w, h);
        if pressed {
            self.world.ui.focus = Some(wid);
        } else if self.input.mouse_down(1) && !self.world.ui.prev_down && !over && self.world.ui.focus == Some(wid) {
            self.world.ui.focus = None;
        }
        let focused = self.world.ui.focus == Some(wid);
        let mut text = text.to_string();
        if focused {
            // Backspace is a key on every platform (not always a character)
            for _ in 0..self.input.key_hit(14) {
                text.pop();
            }
            for c in self.input.take_chars() {
                match c {
                    13 | 10 => self.world.ui.focus = None,
                    27 => self.world.ui.focus = None,
                    c if c >= 32 && c != 127 => {
                        if let Some(ch) = char::from_u32(c as u32) {
                            text.push(ch);
                        }
                    }
                    _ => {}
                }
            }
        }
        let accent = self.world.ui.accent;
        let blink = focused && (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis()) / 500).is_multiple_of(2);
        let shown = if blink { format!("{text}|") } else { text.clone() };
        self.ui_draw(|c| {
            c.color = PANEL;
            c.rect(x, y, w, h, true);
            c.color = if focused { accent } else if over { BG_HOVER } else { BORDER };
            c.rect(x, y, w, h, false);
            c.color = TEXT;
            c.text(x + 6.0, y + h / 2.0, &shown, false, true);
        });
        text
    }

    pub fn ui_panel(&mut self, x: f32, y: f32, w: f32, h: f32, title: &str) {
        let over = self.inside(x, y, w, h);
        self.world.ui.hover |= over;
        let accent = self.world.ui.accent;
        self.ui_draw(|c| {
            c.color = PANEL;
            c.rect(x, y, w, h, true);
            c.color = BORDER;
            c.rect(x, y, w, h, false);
            if !title.is_empty() {
                let th = c.font.height() + 8.0;
                c.color = mix(PANEL, accent, 0.5);
                c.rect(x, y, w, th, true);
                c.color = TEXT;
                c.text(x + 8.0, y + th / 2.0, title, false, true);
            }
        });
    }

    pub fn ui_label(&mut self, x: f32, y: f32, text: &str) {
        self.ui_draw(|c| {
            c.color = TEXT;
            c.text(x, y, text, false, false);
        });
    }
}
