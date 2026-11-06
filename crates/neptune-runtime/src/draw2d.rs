//! 2D drawing on top of the 3D view (Blitz `Text`, `Color`, fonts, ...).
//!
//! Like Blitz3D's back buffer, 2D commands accumulate into a list that is
//! cleared by `Cls`/`RenderWorld` and published by `Flip`. The published
//! list is displayed with a pool of Fyrox UI text widgets, so text uses
//! Fyrox's font rendering.

use std::path::{Path, PathBuf};

use fyrox::{
    asset::{untyped::ResourceKind, Resource},
    core::{algebra::Vector2, color::Color, pool::Handle, uuid::Uuid},
    gui::{
        brush::Brush,
        font::{Font, FontResource, FontStyles, BUILT_IN_FONT},
        text::{Text, TextBuilder, TextMessage},
        widget::{WidgetBuilder, WidgetMessage},
        UserInterface,
    },
};

/// Default text height in pixels (Blitz's built-in font is roughly this size).
pub const DEFAULT_FONT_SIZE: f32 = 16.0;

/// A font loaded with `LoadFont`.
#[derive(Clone, Debug)]
pub struct FontInfo {
    pub resource: FontResource,
    pub size: f32,
}

impl FontInfo {
    pub fn default_font() -> Self {
        FontInfo { resource: BUILT_IN_FONT.resource(), size: DEFAULT_FONT_SIZE }
    }

    /// Width in pixels of `text` rendered with this font.
    pub fn width(&self, text: &str) -> f32 {
        let mut font = self.resource.data_ref();
        let mut w = 0.0;
        let mut prev = None;
        for c in text.chars() {
            if let Some(p) = prev {
                w += font.horizontal_kerning(self.size, p, c).unwrap_or(0.0);
            }
            w += font.glyph_advance(c, self.size);
            prev = Some(c);
        }
        w
    }

    /// Line height in pixels.
    pub fn height(&self) -> f32 {
        let font = self.resource.data_ref();
        (font.ascender(self.size) - font.descender(self.size)).ceil()
    }
}

/// Resolves a font name the way Blitz does: a path to a font file, or the
/// name of an installed system font ("Arial", "Courier New", ...).
fn find_font(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(windir) = std::env::var("WINDIR") {
        dirs.push(Path::new(&windir).join("Fonts"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(Path::new(&local).join("Microsoft").join("Windows").join("Fonts"));
    }
    dirs.extend(["/usr/share/fonts/truetype", "/usr/share/fonts", "/Library/Fonts", "/System/Library/Fonts"].map(PathBuf::from));
    let base = name.to_lowercase().replace(' ', "");
    let candidates = [base.clone(), base.replace("new", ""), base.chars().take(8).collect()];
    for dir in &dirs {
        for c in &candidates {
            for ext in ["ttf", "otf", "ttc"] {
                let f = dir.join(format!("{c}.{ext}"));
                if f.is_file() {
                    return Some(f);
                }
            }
        }
    }
    None
}

/// Loads a font for `LoadFont`. Bold/italic variants are looked up by the
/// usual Windows file-name suffixes when requested.
pub fn load_font(name: &str, size: f32, bold: bool, italic: bool) -> Result<FontInfo, String> {
    let path = find_font(name).ok_or_else(|| format!("font '{name}' not found"))?;
    let styled = if bold || italic {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let suffix = match (bold, italic) {
            (true, true) => "bi",
            (true, false) => "bd",
            _ => "i",
        };
        let p = path.with_file_name(format!("{stem}{suffix}.ttf"));
        if p.is_file() {
            p
        } else {
            path
        }
    } else {
        path
    };
    let data = std::fs::read(&styled).map_err(|e| format!("cannot read font '{}': {e}", styled.display()))?;
    let font = Font::from_memory(data, 1024, FontStyles::default(), Vec::new()).map_err(|e| format!("invalid font '{name}': {e}"))?;
    Ok(FontInfo { resource: Resource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, font), size: size.max(1.0) })
}

/// One `Text` call.
#[derive(Clone, Debug)]
pub struct TextItem {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub color: Color,
    pub font: FontInfo,
}

impl PartialEq for TextItem {
    fn eq(&self, o: &Self) -> bool {
        self.x == o.x
            && self.y == o.y
            && self.text == o.text
            && self.color == o.color
            && self.font.size == o.font.size
            && self.font.resource == o.font.resource
    }
}

/// 2D drawing state of a program.
#[derive(Debug)]
pub struct Canvas2D {
    /// Back buffer: items drawn since the last `Cls`/`RenderWorld`.
    pub items: Vec<TextItem>,
    /// Set by `Flip`: the frame to display.
    pub published: Option<Vec<TextItem>>,
    pub color: Color,
    pub cls_color: Color,
    pub fonts: Vec<Option<FontInfo>>,
    pub font: FontInfo,
}

impl Default for Canvas2D {
    fn default() -> Self {
        Canvas2D {
            items: Vec::new(),
            published: None,
            color: Color::WHITE,
            cls_color: Color::BLACK,
            fonts: Vec::new(),
            font: FontInfo::default_font(),
        }
    }
}

impl Canvas2D {
    pub fn text(&mut self, x: f32, y: f32, text: &str, center_x: bool, center_y: bool) {
        let font = self.font.clone();
        let x = if center_x { x - (font.width(text) / 2.0).floor() } else { x };
        let y = if center_y { y - (font.height() / 2.0).floor() } else { y };
        self.items.push(TextItem { x, y, text: text.to_string(), color: self.color, font });
    }

    pub fn flip(&mut self) {
        self.published = Some(std::mem::take(&mut self.items));
    }
}

/// Fyrox widgets showing the published 2D frame.
#[derive(Default)]
pub struct Overlay {
    widgets: Vec<(Handle<Text>, Option<TextItem>)>,
}

impl Overlay {
    /// Updates the widgets to show `items`, reusing widgets and only
    /// sending messages for items that changed.
    /// `scale` converts the program's logical pixels to physical pixels.
    pub fn show(&mut self, ui: &mut UserInterface, items: &[TextItem], scale: f32) {
        let scaled: Vec<TextItem> = items
            .iter()
            .map(|i| TextItem {
                x: (i.x * scale).round(),
                y: (i.y * scale).round(),
                font: FontInfo { resource: i.font.resource.clone(), size: (i.font.size * scale).round() },
                ..i.clone()
            })
            .collect();
        let items = &scaled[..];
        while self.widgets.len() < items.len() {
            let h = TextBuilder::new(WidgetBuilder::new().with_visibility(false)).build(&mut ui.build_ctx());
            self.widgets.push((h, None));
        }
        for (i, (h, shown)) in self.widgets.iter_mut().enumerate() {
            let h = *h;
            match items.get(i) {
                Some(item) => {
                    if shown.as_ref() == Some(item) {
                        continue;
                    }
                    let prev = shown.take();
                    if prev.as_ref().map_or(true, |p| p.text != item.text) {
                        ui.send(h, TextMessage::Text(item.text.clone()));
                    }
                    if prev.as_ref().map_or(true, |p| p.x != item.x || p.y != item.y) {
                        ui.send(h, WidgetMessage::DesiredPosition(Vector2::new(item.x, item.y)));
                    }
                    if prev.as_ref().map_or(true, |p| p.color != item.color) {
                        ui.send(h, WidgetMessage::Foreground(Brush::Solid(item.color).into()));
                    }
                    if prev.as_ref().map_or(true, |p| p.font.resource != item.font.resource) {
                        ui.send(h, TextMessage::Font(item.font.resource.clone()));
                    }
                    if prev.as_ref().map_or(true, |p| p.font.size != item.font.size) {
                        ui.send(h, TextMessage::FontSize(item.font.size.into()));
                    }
                    if prev.is_none() {
                        ui.send(h, WidgetMessage::Visibility(true));
                    }
                    *shown = Some(item.clone());
                }
                None => {
                    if shown.take().is_some() {
                        ui.send(h, WidgetMessage::Visibility(false));
                    }
                }
            }
        }
    }
}
