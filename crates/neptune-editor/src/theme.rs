//! Neptune IDE's colours for the editor's widgets.
//!
//! The editor draws with named brushes (`Style::BRUSH_*`), from darkest
//! background to brightest detail. Each palette here gives those names the
//! colours of an IDE theme (`crates/neptune-ide/src/theme.rs`): keep the two
//! files in step.

use fyroxed_base::{
    fyrox::{
        core::color::Color,
        gui::{
            brush::Brush,
            style::{resource::StyleResource, Style},
        },
    },
    settings::general::EditorStyle,
    Editor,
};

/// The brushes of one theme, as `0xRRGGBB`.
pub struct Palette {
    /// Backgrounds, from `BRUSH_DARKEST` to `BRUSH_LIGHTEST`.
    pub surfaces: [u32; 8],
    /// Details drawn over the backgrounds: `BRUSH_BRIGHT`, `BRUSH_BRIGHTEST`.
    pub details: [u32; 2],
    pub selection: u32,
    pub selection_dim: u32,
    pub accent: u32,
    pub text: u32,
    pub foreground: u32,
    pub information: u32,
    pub warning: u32,
    pub error: u32,
    pub ok: u32,
    /// OK buttons: normal, hover, pressed.
    pub ok_button: [u32; 3],
    /// Cancel buttons: normal, hover, pressed.
    pub cancel_button: [u32; 3],
}

/// "Neptune Deep", the IDE's default theme.
pub const NEPTUNE_DEEP: Palette = Palette {
    surfaces: [0x0A0F18, 0x0E1420, 0x131B2A, 0x182235, 0x1C2739, 0x243047, 0x2E3D58, 0x3A4B69],
    details: [0x4A5870, 0x8796AE],
    selection: 0x2B7FB8,
    selection_dim: 0x1F5F8C,
    accent: 0x2BC4E0,
    text: 0xD6E0EE,
    foreground: 0xEAF1FA,
    information: 0xD6E0EE,
    warning: 0xF2B84B,
    error: 0xFF5D6C,
    ok: 0x56D39B,
    ok_button: [0x1F7A55, 0x27946A, 0x2FAE7E],
    cancel_button: [0x8F2F3A, 0xA83A46, 0xC14552],
};

/// "Daylight", the IDE's light theme.
pub const DAYLIGHT: Palette = Palette {
    surfaces: [0xCFCFC7, 0xD9D9D2, 0xE2E2DC, 0xE9E9E4, 0xEFEFEA, 0xF4F4F0, 0xF8F8F5, 0xFBFBF9],
    details: [0x5B6773, 0x1F2933],
    selection: 0x4A9BE0,
    selection_dim: 0x7FB5E6,
    accent: 0x0B6FB8,
    text: 0x1F2933,
    foreground: 0xFFFFFF,
    information: 0x0B6FB8,
    warning: 0xB87400,
    error: 0xD0263A,
    ok: 0x1E8E5A,
    ok_button: [0x1E8E5A, 0x27A069, 0x30B278],
    cancel_button: [0xD0263A, 0xDA3F51, 0xE45868],
};

fn brush(rgb: u32) -> Brush {
    Brush::Solid(Color::opaque((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
}

/// Gives every global brush of `style` the palette's colour. Brushes the
/// editor adds for its own panels are left as they are.
pub fn paint(style: &mut Style, p: &Palette) {
    const SURFACES: [&str; 8] = [
        Style::BRUSH_DARKEST,
        Style::BRUSH_DARKER,
        Style::BRUSH_DARK,
        Style::BRUSH_PRIMARY,
        Style::BRUSH_LIGHTER_PRIMARY,
        Style::BRUSH_LIGHT,
        Style::BRUSH_LIGHTER,
        Style::BRUSH_LIGHTEST,
    ];
    for (name, rgb) in SURFACES.into_iter().zip(p.surfaces) {
        style.set(name, brush(rgb));
    }
    let named = [
        (Style::BRUSH_BRIGHT, p.details[0]),
        (Style::BRUSH_BRIGHTEST, p.details[1]),
        (Style::BRUSH_BRIGHT_BLUE, p.selection),
        (Style::BRUSH_DIM_BLUE, p.selection_dim),
        (Style::BRUSH_HIGHLIGHT, p.accent),
        (Style::BRUSH_TEXT, p.text),
        (Style::BRUSH_FOREGROUND, p.foreground),
        (Style::BRUSH_INFORMATION, p.information),
        (Style::BRUSH_WARNING, p.warning),
        (Style::BRUSH_ERROR, p.error),
        (Style::BRUSH_OK, p.ok),
        (Style::BRUSH_OK_NORMAL, p.ok_button[0]),
        (Style::BRUSH_OK_HOVER, p.ok_button[1]),
        (Style::BRUSH_OK_PRESSED, p.ok_button[2]),
        (Style::BRUSH_CANCEL_NORMAL, p.cancel_button[0]),
        (Style::BRUSH_CANCEL_HOVER, p.cancel_button[1]),
        (Style::BRUSH_CANCEL_PRESSED, p.cancel_button[2]),
    ];
    for (name, rgb) in named {
        style.set(name, brush(rgb));
    }
}

/// Replaces the editor's dark and light styles with Neptune's and shows the
/// one its settings choose.
pub fn apply(editor: &mut Editor) {
    for (which, palette) in [(EditorStyle::Dark, &NEPTUNE_DEEP), (EditorStyle::Light, &DAYLIGHT)] {
        let Some(resource) = editor.styles.get(&which) else { continue };
        let mut style = resource.data_ref().clone();
        paint(&mut style, palette);
        editor.styles.insert(which, StyleResource::new_embedded(style));
    }
    if let Some(style) = editor.styles.get(&editor.settings.general.style) {
        editor.engine.user_interfaces.first_mut().set_style(style.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(base: Style, p: &Palette) -> Style {
        let mut style = base;
        paint(&mut style, p);
        style
    }

    #[test]
    fn every_global_brush_is_repainted() {
        let before = Style::dark_style();
        let after = painted(Style::dark_style(), &NEPTUNE_DEEP);
        for container in before.inner() {
            let name = container.name.as_str();
            if name.starts_with("Global.Brush.") {
                assert_ne!(before.get::<Brush>(name), after.get::<Brush>(name), "{name} kept Fyrox's colour");
            }
        }
        assert_eq!(after.get::<Brush>(Style::BRUSH_DARKER), Some(brush(0x0E1420)));
    }

    #[test]
    fn the_light_style_gets_daylight() {
        let before = Style::light_style();
        let after = painted(Style::light_style(), &DAYLIGHT);
        // Fyrox's light style leaves some brushes out; painting adds them
        assert!(after.inner().len() >= before.inner().len());
        assert_eq!(after.get::<Brush>(Style::BRUSH_TEXT), Some(brush(0x1F2933)));
        assert_eq!(after.get::<Brush>(Style::BRUSH_OK_NORMAL), Some(brush(0x1E8E5A)));
    }

    #[test]
    fn surfaces_run_one_way() {
        let light = |rgb: u32| ((rgb >> 16) & 255) + ((rgb >> 8) & 255) + (rgb & 255);
        for p in [&NEPTUNE_DEEP, &DAYLIGHT] {
            assert!(p.surfaces.windows(2).all(|w| light(w[0]) < light(w[1])));
        }
    }
}
