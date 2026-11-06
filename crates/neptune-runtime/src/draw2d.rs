//! 2D drawing on top of the 3D view (Blitz `Text`, `Color`, fonts, ...).
//!
//! Like Blitz3D's back buffer, 2D commands accumulate into a list that is
//! cleared by `Cls`/`RenderWorld` and published by `Flip`. The published
//! list is replayed in order by one full-screen Fyrox UI widget, so text,
//! shapes and images use Fyrox's UI renderer and layer as drawn.

use std::path::{Path, PathBuf};

use fyrox::{
    asset::{untyped::ResourceKind, Resource},
    core::{
        algebra::{Matrix3, Vector2}, color::Color, math::Rect, pool::Handle, reflect::prelude::*, type_traits::prelude::*,
        uuid::Uuid, uuid_provider, visitor::prelude::*,
    },
    graph::SceneGraph,
    gui::{
        brush::Brush,
        define_widget_deref,
        draw::{CommandTexture, Draw, DrawingContext},
        font::{Font, FontResource, FontStyles, BUILT_IN_FONT},
        formatted_text::FormattedTextBuilder,
        message::UiMessage,
        widget::{Widget, WidgetBuilder, WidgetMessage},
        BuildContext, Control, UiNode, UserInterface,
    },
    resource::texture::TextureResource,
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

/// One 2D drawing command, in program (logical) pixels.
#[derive(Clone, Debug)]
pub enum DrawOp {
    Text { x: f32, y: f32, text: String, color: Color, font: FontInfo },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Color, solid: bool },
    Oval { x: f32, y: f32, w: f32, h: f32, color: Color, solid: bool },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, color: Color },
    /// A textured rectangle; `uv` selects the part of the texture (an animation frame).
    Image { x: f32, y: f32, w: f32, h: f32, uv: Rect<f32>, texture: TextureResource },
    /// `Viewport`: later operations are clipped to this rectangle (`None` = whole screen).
    Clip { rect: Option<Rect<f32>> },
    /// (ext) `Camera2D`: later operations are transformed (`None` = screen space).
    Transform { m: Option<Matrix3<f32>> },
    /// (ext) A rotated or scaled image (`DrawImageEx`): corners clockwise from
    /// the top-left, tinted by `color`.
    Quad { pts: [Vector2<f32>; 4], uv: Rect<f32>, texture: TextureResource, color: Color },
}

impl DrawOp {
    /// Converts logical pixels to physical pixels.
    fn scaled(&self, k: f32) -> DrawOp {
        let mut op = self.clone();
        match &mut op {
            DrawOp::Text { x, y, font, .. } => {
                *x = (*x * k).round();
                *y = (*y * k).round();
                font.size = (font.size * k).round();
            }
            DrawOp::Rect { x, y, w, h, .. } | DrawOp::Oval { x, y, w, h, .. } | DrawOp::Image { x, y, w, h, .. } => {
                *x *= k;
                *y *= k;
                *w *= k;
                *h *= k;
            }
            DrawOp::Line { x1, y1, x2, y2, .. } => {
                *x1 *= k;
                *y1 *= k;
                *x2 *= k;
                *y2 *= k;
            }
            DrawOp::Clip { rect } => {
                if let Some(r) = rect {
                    *r = Rect::new(r.x() * k, r.y() * k, r.w() * k, r.h() * k);
                }
            }
            DrawOp::Transform { m } => {
                // the same transform in physical pixels: S * m * S^-1
                if let Some(m) = m {
                    let s = Matrix3::new_nonuniform_scaling(&Vector2::new(k, k));
                    let si = Matrix3::new_nonuniform_scaling(&Vector2::new(1.0 / k, 1.0 / k));
                    *m = s * *m * si;
                }
            }
            DrawOp::Quad { pts, .. } => {
                for p in pts.iter_mut() {
                    *p *= k;
                }
            }
        }
        op
    }
}

/// 2D drawing state of a program.
#[derive(Debug)]
pub struct Canvas2D {
    /// Back buffer: commands since the last `Cls`/`RenderWorld`.
    pub items: Vec<DrawOp>,
    /// Set by `Flip`: the frame to display.
    pub published: Option<Vec<DrawOp>>,
    pub color: Color,
    pub cls_color: Color,
    pub fonts: Vec<Option<FontInfo>>,
    pub font: FontInfo,
    /// `Origin` offset added to all 2D coordinates.
    pub origin: (f32, f32),
    /// (ext) `Camera2D` and whether it is in use (`Camera2DEnable`).
    pub camera: crate::camera2d::Camera2D,
    pub camera_on: bool,
    /// The transform the draw list is currently under.
    applied: Option<Matrix3<f32>>,
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
            origin: (0.0, 0.0),
            camera: Default::default(),
            camera_on: false,
            applied: None,
        }
    }
}

impl Canvas2D {
    /// Puts later drawing under the 2D camera's transform (or screen space).
    fn ensure_transform(&mut self) {
        if self.items.is_empty() {
            self.applied = None;
        }
        let want = self.camera_on.then(|| self.camera.matrix());
        if want != self.applied {
            self.items.push(DrawOp::Transform { m: want });
            self.applied = want;
        }
    }

    /// `DrawImageEx`: an image rotated (degrees clockwise) and scaled about
    /// its handle at `(x, y)`.
    #[allow(clippy::too_many_arguments)]
    pub fn image_ex(&mut self, x: f32, y: f32, w: f32, h: f32, handle: (f32, f32), angle: f32, uv: Rect<f32>, texture: TextureResource, color: Color) {
        self.ensure_transform();
        let (x, y) = (x + self.origin.0, y + self.origin.1);
        let (s, c) = angle.to_radians().sin_cos();
        let corner = |cx: f32, cy: f32| {
            let (dx, dy) = (cx - handle.0, cy - handle.1);
            Vector2::new(x + dx * c - dy * s, y + dx * s + dy * c)
        };
        let pts = [corner(0.0, 0.0), corner(w, 0.0), corner(w, h), corner(0.0, h)];
        self.items.push(DrawOp::Quad { pts, uv, texture, color });
    }

    pub fn text(&mut self, x: f32, y: f32, text: &str, center_x: bool, center_y: bool) {
        self.ensure_transform();
        let font = self.font.clone();
        let x = if center_x { x - (font.width(text) / 2.0).floor() } else { x } + self.origin.0;
        let y = if center_y { y - (font.height() / 2.0).floor() } else { y } + self.origin.1;
        self.items.push(DrawOp::Text { x, y, text: text.to_string(), color: self.color, font });
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, solid: bool) {
        self.ensure_transform();
        let (x, y) = (x + self.origin.0, y + self.origin.1);
        self.items.push(DrawOp::Rect { x, y, w, h, color: self.color, solid });
    }

    pub fn oval(&mut self, x: f32, y: f32, w: f32, h: f32, solid: bool) {
        self.ensure_transform();
        let (x, y) = (x + self.origin.0, y + self.origin.1);
        self.items.push(DrawOp::Oval { x, y, w, h, color: self.color, solid });
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.ensure_transform();
        let (ox, oy) = self.origin;
        self.items.push(DrawOp::Line { x1: x1 + ox, y1: y1 + oy, x2: x2 + ox, y2: y2 + oy, color: self.color });
    }

    pub fn image(&mut self, x: f32, y: f32, w: f32, h: f32, uv: Rect<f32>, texture: TextureResource) {
        self.ensure_transform();
        let (x, y) = (x + self.origin.0, y + self.origin.1);
        self.items.push(DrawOp::Image { x, y, w, h, uv, texture });
    }

    pub fn flip(&mut self) {
        self.published = Some(std::mem::take(&mut self.items));
    }
}

/// A full-screen UI widget that replays a 2D draw list in order, so text,
/// shapes and images layer exactly as the program drew them.
#[derive(Default, Clone, Visit, Reflect, Debug, ComponentProvider)]
#[reflect(derived_type = "UiNode")]
pub struct CanvasWidget {
    pub widget: Widget,
    #[visit(skip)]
    #[reflect(hidden)]
    pub ops: Vec<DrawOp>,
}

define_widget_deref!(CanvasWidget);
uuid_provider!(CanvasWidget = "9b7a3c55-2f1e-4d6b-8a8e-5c1d2b7f4e10");

impl Control for CanvasWidget {
    fn measure_override(&self, _ui: &UserInterface, _available: Vector2<f32>) -> Vector2<f32> {
        Vector2::new(*self.widget.width, *self.widget.height)
    }

    fn draw(&self, ctx: &mut DrawingContext) {
        let full = self.clip_bounds();
        let mut clip = full;
        let material = &self.material;
        // the widget's own transform; the 2D camera is applied on top of it
        let base = *ctx.transform_stack.transform();
        let mut pushed = false;
        for op in &self.ops {
            match op {
                DrawOp::Transform { m } => {
                    if pushed {
                        ctx.transform_stack.pop();
                        pushed = false;
                    }
                    if let Some(m) = m {
                        ctx.transform_stack.push(base * m);
                        pushed = true;
                    }
                }
                DrawOp::Quad { pts, uv, texture, color } => {
                    let tc = [
                        Vector2::new(uv.x(), uv.y()),
                        Vector2::new(uv.x() + uv.w(), uv.y()),
                        Vector2::new(uv.x() + uv.w(), uv.y() + uv.h()),
                        Vector2::new(uv.x(), uv.y() + uv.h()),
                    ];
                    let i = ctx.last_vertex_index();
                    for (p, t) in pts.iter().zip(tc) {
                        ctx.push_vertex(*p, t);
                    }
                    ctx.push_triangle(i, i + 1, i + 2);
                    ctx.push_triangle(i, i + 2, i + 3);
                    ctx.commit(clip, Brush::Solid(*color), CommandTexture::Texture(texture.clone()), material, None);
                }
                DrawOp::Clip { rect } => {
                    clip = match rect {
                        // viewport is in screen pixels; the widget's clip bounds are too
                        Some(r) => {
                            let (x0, y0) = (r.x().max(full.x()), r.y().max(full.y()));
                            let x1 = (r.x() + r.w()).min(full.x() + full.w());
                            let y1 = (r.y() + r.h()).min(full.y() + full.h());
                            Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
                        }
                        None => full,
                    };
                }
                DrawOp::Text { x, y, text, color, font } => {
                    let mut ft = FormattedTextBuilder::new(font.resource.clone())
                        .with_text(text.clone())
                        .with_font_size(font.size.into())
                        .with_brush(Brush::Solid(*color))
                        .with_constraint(Vector2::new(f32::INFINITY, f32::INFINITY))
                        .build();
                    ft.measure_and_arrange();
                    ctx.draw_text(clip, Vector2::new(*x, *y), material, &ft);
                }
                DrawOp::Rect { x, y, w, h, color, solid } => {
                    let r = Rect::new(*x, *y, *w, *h);
                    if *solid {
                        ctx.push_rect_filled(&r, None);
                    } else {
                        ctx.push_rect(&r, 1.0);
                    }
                    ctx.commit(clip, Brush::Solid(*color), CommandTexture::None, material, None);
                }
                DrawOp::Oval { x, y, w, h, color, solid } => {
                    let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
                    let n = ((rx.max(ry) * 0.75) as usize).clamp(12, 128);
                    let pt = |i: usize| {
                        let a = i as f32 / n as f32 * std::f32::consts::TAU;
                        Vector2::new(cx + rx * a.cos(), cy + ry * a.sin())
                    };
                    for i in 0..n {
                        if *solid {
                            ctx.push_triangle_filled([Vector2::new(cx, cy), pt(i), pt(i + 1)]);
                        } else {
                            ctx.push_line(pt(i), pt(i + 1), 1.0);
                        }
                    }
                    ctx.commit(clip, Brush::Solid(*color), CommandTexture::None, material, None);
                }
                DrawOp::Line { x1, y1, x2, y2, color } => {
                    ctx.push_line(Vector2::new(*x1, *y1), Vector2::new(*x2, *y2), 1.0);
                    ctx.commit(clip, Brush::Solid(*color), CommandTexture::None, material, None);
                }
                DrawOp::Image { x, y, w, h, uv, texture } => {
                    let tc = [
                        Vector2::new(uv.x(), uv.y()),
                        Vector2::new(uv.x() + uv.w(), uv.y()),
                        Vector2::new(uv.x() + uv.w(), uv.y() + uv.h()),
                        Vector2::new(uv.x(), uv.y() + uv.h()),
                    ];
                    ctx.push_rect_filled(&Rect::new(*x, *y, *w, *h), Some(&tc));
                    ctx.commit(clip, Brush::Solid(Color::WHITE), CommandTexture::Texture(texture.clone()), material, None);
                }
            }
        }
        if pushed {
            ctx.transform_stack.pop();
        }
    }

    fn handle_routed_message(&mut self, ui: &mut UserInterface, message: &mut UiMessage) {
        self.widget.handle_routed_message(ui, message);
    }
}

/// Owns the canvas widget that shows the published 2D frame.
#[derive(Default)]
pub struct Overlay {
    widget: Handle<UiNode>,
    size: Vector2<f32>,
}

impl Overlay {
    /// Shows `ops`; `scale` converts logical to physical pixels and
    /// `screen` is the window size in physical pixels.
    pub fn show(&mut self, ui: &mut UserInterface, ops: &[DrawOp], scale: f32, screen: Vector2<f32>) {
        if self.widget.is_none() {
            let ctx: &mut BuildContext = &mut ui.build_ctx();
            let w = CanvasWidget { widget: WidgetBuilder::new().with_hit_test_visibility(false).build(ctx), ops: Vec::new() };
            self.widget = ctx.add_node(UiNode::new(w));
        }
        if self.size != screen {
            self.size = screen;
            ui.send(self.widget, WidgetMessage::Width(screen.x));
            ui.send(self.widget, WidgetMessage::Height(screen.y));
        }
        if let Ok(c) = ui.try_get_mut_of_type::<CanvasWidget>(self.widget) {
            c.ops = ops.iter().map(|o| o.scaled(scale)).collect();
            c.invalidate_visual();
        }
    }
}
