//! Colour schemes and the square-cornered egui style built from them.

use eframe::egui::{self, Color32, CornerRadius, Margin, Shadow, Stroke, Style, Vec2};

use crate::lang::Tk;

#[derive(Clone)]
pub struct Theme {
    pub name: &'static str,
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub rail: Color32,
    pub line: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub caret: Color32,
    pub selection: Color32,
    pub current_line: Color32,
    pub match_bg: Color32,
    pub word_bg: Color32,
    pub guide: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub ok: Color32,
    pub debug_line: Color32,
    pub keyword: Color32,
    pub command: Color32,
    pub constant: Color32,
    pub number: Color32,
    pub string: Color32,
    pub comment: Color32,
    pub op: Color32,
    pub label: Color32,
    pub function: Color32,
    pub ty: Color32,
}

const fn c(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

const fn a(rgb: u32, alpha: u8) -> Color32 {
    let (r, g, b) = ((rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255);
    let af = alpha as u32;
    Color32::from_rgba_premultiplied((r * af / 255) as u8, (g * af / 255) as u8, (b * af / 255) as u8, alpha)
}

pub const THEMES: &[Theme] = &[
    Theme {
        name: "Neptune Deep",
        dark: true,
        bg: c(0x0E1420),
        panel: c(0x131B2A),
        rail: c(0x0A0F18),
        line: c(0x243047),
        hover: c(0x1C2739),
        active: c(0x25344C),
        accent: c(0x2BC4E0),
        on_accent: c(0x06121A),
        text: c(0xD6E0EE),
        dim: c(0x8796AE),
        faint: c(0x4A5870),
        caret: c(0x6FE3F7),
        selection: a(0x2B7FB8, 110),
        current_line: a(0x7FA8D8, 18),
        match_bg: a(0xE8B23A, 95),
        word_bg: a(0x7FA8D8, 45),
        guide: c(0x1E2A3D),
        error: c(0xFF5D6C),
        warning: c(0xF2B84B),
        ok: c(0x56D39B),
        debug_line: a(0xE8B23A, 60),
        keyword: c(0x5CC8FF),
        command: c(0x7EE8C7),
        constant: c(0xC9A7FF),
        number: c(0xFFB27A),
        string: c(0xA6E36E),
        comment: c(0x5F7391),
        op: c(0x9FB2CC),
        label: c(0xFFD76A),
        function: c(0xFFD76A),
        ty: c(0xFF9EC4),
    },
    Theme {
        name: "Blitz Classic",
        dark: true,
        bg: c(0x225588),
        panel: c(0x1C4670),
        rail: c(0x163A5E),
        line: c(0x2F6AA3),
        hover: c(0x2A6398),
        active: c(0x3274AE),
        accent: c(0xFFEE00),
        on_accent: c(0x10243A),
        text: c(0xEEEEEE),
        dim: c(0xB4CCE4),
        faint: c(0x7FA3C8),
        caret: c(0xFFFFFF),
        selection: a(0x9CD0FF, 120),
        current_line: a(0xFFFFFF, 14),
        match_bg: a(0xFFEE00, 90),
        word_bg: a(0xFFFFFF, 40),
        guide: c(0x2C6399),
        error: c(0xFF8A8A),
        warning: c(0xFFD070),
        ok: c(0x66FFAA),
        debug_line: a(0xFFEE00, 70),
        keyword: c(0xAAFFFF),
        command: c(0xAAFFFF),
        constant: c(0xFFFFFF),
        number: c(0x33FFDD),
        string: c(0x00FF66),
        comment: c(0xFFEE00),
        op: c(0xEEEEEE),
        label: c(0xFFFFFF),
        function: c(0xFFFFFF),
        ty: c(0xFFFFFF),
    },
    Theme {
        name: "Graphite",
        dark: true,
        bg: c(0x1A1B1E),
        panel: c(0x212226),
        rail: c(0x141518),
        line: c(0x33353B),
        hover: c(0x2B2D32),
        active: c(0x383B42),
        accent: c(0xF08A3C),
        on_accent: c(0x1A1006),
        text: c(0xDCDDE0),
        dim: c(0x94979E),
        faint: c(0x5A5D64),
        caret: c(0xF5A968),
        selection: a(0xB07A4A, 100),
        current_line: a(0xFFFFFF, 10),
        match_bg: a(0xD8B24A, 90),
        word_bg: a(0xFFFFFF, 34),
        guide: c(0x2A2C31),
        error: c(0xF2616B),
        warning: c(0xE5B84F),
        ok: c(0x7CC97C),
        debug_line: a(0xF0A23C, 60),
        keyword: c(0xF29A5C),
        command: c(0x8FC7E8),
        constant: c(0xC8A8E8),
        number: c(0xE5C07B),
        string: c(0xA8CC8C),
        comment: c(0x6B6F78),
        op: c(0xB4B7BD),
        label: c(0xE5C07B),
        function: c(0xE8D08A),
        ty: c(0xE88FA8),
    },
    Theme {
        name: "Daylight",
        dark: false,
        bg: c(0xFBFBF9),
        panel: c(0xEFEFEA),
        rail: c(0xE2E2DC),
        line: c(0xCFCFC7),
        hover: c(0xE2E4E6),
        active: c(0xD3DAE2),
        accent: c(0x0B6FB8),
        on_accent: c(0xFFFFFF),
        text: c(0x1F2933),
        dim: c(0x5B6773),
        faint: c(0x9AA4AE),
        caret: c(0x0B6FB8),
        selection: a(0x4A9BE0, 90),
        current_line: a(0x20405F, 14),
        match_bg: a(0xF0B429, 110),
        word_bg: a(0x20405F, 32),
        guide: c(0xE3E3DD),
        error: c(0xD0263A),
        warning: c(0xB87400),
        ok: c(0x1E8E5A),
        debug_line: a(0xF0B429, 100),
        keyword: c(0x0550AE),
        command: c(0x0A7A6A),
        constant: c(0x7A3FB8),
        number: c(0xB5500B),
        string: c(0x3B7A12),
        comment: c(0x7D8A96),
        op: c(0x45525E),
        label: c(0x8A5A00),
        function: c(0x8A5A00),
        ty: c(0xB02A6B),
    },
];

impl Theme {
    pub fn by_name(name: &str) -> &'static Theme {
        THEMES.iter().find(|t| t.name == name).unwrap_or(&THEMES[0])
    }

    pub fn token(&self, k: Tk) -> Color32 {
        match k {
            Tk::Plain => self.text,
            Tk::Keyword => self.keyword,
            Tk::Command => self.command,
            Tk::Constant => self.constant,
            Tk::Number => self.number,
            Tk::Str => self.string,
            Tk::Comment => self.comment,
            Tk::Op => self.op,
            Tk::Label => self.label,
        }
    }

    /// Square corners and no soft shadows, everywhere.
    pub fn apply(&self, style: &mut Style) {
        let v = &mut style.visuals;
        *v = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        let square = CornerRadius::ZERO;
        v.window_corner_radius = square;
        v.menu_corner_radius = square;
        v.window_shadow = Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(90) };
        v.popup_shadow = Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(80) };
        v.window_fill = self.panel;
        v.panel_fill = self.panel;
        v.faint_bg_color = self.hover;
        v.extreme_bg_color = self.bg;
        v.window_stroke = Stroke::new(1.0, self.line);
        v.override_text_color = Some(self.text);
        v.hyperlink_color = self.accent;
        v.selection.bg_fill = self.active;
        v.selection.stroke = Stroke::new(1.0, self.accent);
        v.text_cursor.stroke = Stroke::new(2.0, self.caret);
        v.slider_trailing_fill = true;
        v.striped = false;
        let ws = &mut v.widgets;
        for (w, fill, line, fg) in [
            (&mut ws.noninteractive, self.panel, self.line, self.dim),
            (&mut ws.inactive, self.hover, self.line, self.text),
            (&mut ws.hovered, self.active, self.accent, self.text),
            (&mut ws.active, self.accent, self.accent, self.on_accent),
            (&mut ws.open, self.active, self.line, self.text),
        ] {
            w.corner_radius = square;
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = Stroke::new(1.0, line);
            w.fg_stroke = Stroke::new(1.0, fg);
            w.expansion = 0.0;
        }
        ws.noninteractive.bg_stroke = Stroke::new(1.0, self.line);
        ws.inactive.bg_stroke = Stroke::NONE;
        let s = &mut style.spacing;
        s.item_spacing = Vec2::new(6.0, 4.0);
        s.button_padding = Vec2::new(8.0, 3.0);
        s.interact_size.y = 22.0;
        s.window_margin = Margin::same(10);
        s.menu_margin = Margin::same(3);
        s.scroll.floating = true;
        s.scroll.bar_width = 9.0;
        s.scroll.floating_width = 3.0;
        s.scroll.floating_allocated_width = 0.0;
        s.indent = 14.0;
        style.animation_time = 0.08;
        style.scroll_animation = egui::style::ScrollAnimation::duration(0.09);
        style.interaction.tooltip_delay = 0.45;
    }
}
