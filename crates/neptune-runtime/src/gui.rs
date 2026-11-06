//! Retained GUI (ext): Fyrox's widget framework for BASIC programs.
//!
//! Unlike the immediate `UI*` widgets (drawn by the program every frame),
//! `Gui*` widgets are made once and then kept, laid out, drawn and driven by
//! Fyrox's UI. A program creates widgets (each returns a handle), places
//! them in windows and panels, and reads what the user did: `GuiClicked`,
//! `GuiChanged`, `GuiValue`, `GuiText`, `GuiSelected`.
//!
//! Every creating command takes an optional `parent`: 0 puts the widget on
//! the screen (placed with `SetGuiPosition`), a window, panel, tab or scroll
//! area puts it inside, laid out by that container. Sizes and positions are
//! in the same logical pixels as 2D drawing, so the GUI keeps its size on
//! high-DPI screens: Fyrox works in physical pixels, and every size, position
//! and font size passes through [`px`] on the way in (mouse positions are
//! divided on the way out). Scaling the whole UI with a layout transform
//! instead would break window dragging, docking and curve editing, which
//! map the mouse without it.
//!
//! The rest of the widgets live in `gui_lists.rs` (drop-downs, lists,
//! trees, tabs, menus), `gui_dialogs.rs` (popups, message boxes, file
//! browsers and dialogs, docking) and `gui_editors.rs` (numbers, vectors,
//! colours, curves); they use the functions here.

use std::collections::HashMap;

use fyrox::{
    core::{
        algebra::Vector2,
        color::Color,
        pool::Handle,
    },
    gui::{
        brush::Brush,
        button::{ButtonBuilder, ButtonContent, ButtonMessage},
        canvas::CanvasBuilder,
        check_box::{CheckBoxBuilder, CheckBoxMessage},
        grid::{Column, GridBuilder, Row},
        image::ImageBuilder,
        message::{MessageDirection, UiMessage},
        progress_bar::{ProgressBarBuilder, ProgressBarMessage},
        scroll_bar::{ScrollBarBuilder, ScrollBarMessage},
        scroll_viewer::ScrollViewerBuilder,
        stack_panel::StackPanelBuilder,
        style::StyledProperty,
        text::{TextBuilder, TextMessage},
        text_box::{TextBoxBuilder, TextCommitMode},
        widget::{WidgetBuilder, WidgetMessage},
        window::{WindowBuilder, WindowMessage, WindowTitle},
        wrap_panel::WrapPanelBuilder,
        BuildContext, HorizontalAlignment, Orientation, RcUiNodeHandle, Thickness, UiNode, UserInterface, VerticalAlignment,
    },
};
use neptune_vm::Value;

use fyrox::graph::SceneGraph;

use crate::commands::Cmd;
use crate::world::Host3D;

/// One widget made by the program.
#[derive(Debug, Clone)]
pub struct GuiItem {
    /// What it is ("button", "window", ...).
    pub kind: &'static str,
    pub handle: Handle<UiNode>,
    /// Where children go (a container's panel); NONE for widgets without children.
    pub content: Handle<UiNode>,
    /// Clicks and changes since the program last asked.
    pub clicked: u32,
    pub changed: u32,
    pub value: f32,
    pub text: String,
    /// Extra values (vectors, colours).
    pub values: [f32; 4],
    /// Items of lists, drop-downs, tabs and menus.
    pub items: Vec<Handle<UiNode>>,
    /// Selected item (-1 = none).
    pub selected: i32,
    /// For grids: the number of columns and rows.
    pub grid: (usize, usize),
}

impl GuiItem {
    pub fn new(kind: &'static str, handle: Handle<UiNode>) -> GuiItem {
        GuiItem {
            kind,
            handle,
            content: Handle::NONE,
            clicked: 0,
            changed: 0,
            value: 0.0,
            text: String::new(),
            values: [0.0; 4],
            items: Vec::new(),
            selected: -1,
            grid: (0, 0),
        }
    }
}

/// Physical pixels per logical pixel (set every frame from the window).
static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// Logical pixels to Fyrox's physical pixels.
pub fn px(v: f32) -> f32 {
    v * f32::from_bits(SCALE.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn px2(x: f32, y: f32) -> Vector2<f32> {
    Vector2::new(px(x), px(y))
}

pub fn set_scale(scale: f32) {
    SCALE.store(scale.max(0.01).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Fyrox's default text size, before scaling.
const BASE_FONT_SIZE: f32 = 14.0;

#[derive(Debug, Default)]
pub struct GuiState {
    pub widgets: Vec<Option<GuiItem>>,
    /// The panel every top-level widget is placed on (scaled for DPI).
    pub root: Handle<UiNode>,
    /// Widget (or sub-widget, like a menu item) → program handle.
    pub by_handle: HashMap<Handle<UiNode>, i32>,
    /// The mouse is over a GUI widget (updated each frame).
    pub mouse_over: bool,
    /// The program's style (`GuiTheme`, `GuiThemeColor`, `GuiThemeFontSize`).
    pub style: Option<fyrox::gui::style::Style>,
    /// The program's text size (logical) and the scale the style was made for.
    pub font_size: f32,
    pub styled_scale: f32,
    /// Windows that were open last frame (to notice when the user closes one).
    pub open_windows: std::collections::HashSet<i32>,
}

impl GuiState {
    pub fn get(&self, id: i32) -> Result<&GuiItem, String> {
        usize::try_from(id - 1).ok().and_then(|i| self.widgets.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("GUI widget {id} does not exist"))
    }

    pub fn get_mut(&mut self, id: i32) -> Result<&mut GuiItem, String> {
        usize::try_from(id - 1).ok().and_then(|i| self.widgets.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("GUI widget {id} does not exist"))
    }

    pub fn add(&mut self, item: GuiItem) -> i32 {
        let h = item.handle;
        self.widgets.push(Some(item));
        let id = self.widgets.len() as i32;
        self.by_handle.insert(h, id);
        id
    }

    /// Records what the user did with the program's widgets.
    pub fn on_message(&mut self, msg: &UiMessage) {
        if msg.direction() != MessageDirection::FromWidget {
            return;
        }
        let Some(&id) = self.by_handle.get(&msg.destination()) else { return };
        let Ok(item) = self.get_mut(id) else { return };
        if let Some(ButtonMessage::Click) = msg.data() {
            item.clicked += 1;
        } else if let Some(CheckBoxMessage::Check(v)) = msg.data() {
            item.value = if v.unwrap_or(false) { 1.0 } else { 0.0 };
            item.changed += 1;
        } else if let Some(TextMessage::Text(t)) = msg.data() {
            if item.kind == "textbox" && *t != item.text {
                item.text = t.clone();
                item.changed += 1;
            }
        } else if let Some(ScrollBarMessage::Value(v)) = msg.data() {
            if (item.value - v).abs() > f32::EPSILON {
                item.value = *v;
                item.changed += 1;
            }
        } else if let Some(WidgetMessage::MouseDown { .. }) = msg.data() {
            if item.kind == "image" {
                item.clicked += 1;
            }
        }
        crate::gui_lists::on_message(self, id, msg);
        crate::gui_dialogs::on_message(self, id, msg);
        crate::gui_editors::on_message(self, id, msg);
    }

    /// Keeps the root panel the size of the window (physical pixels), text
    /// sizes in step with the DPI scale, and notices windows the user closed.
    pub fn sync_root(&mut self, ui: &mut UserInterface, size: Vector2<f32>, scale: f32) {
        set_scale(scale);
        if self.root.is_none() {
            return;
        }
        ui.send(self.root, WidgetMessage::Width(size.x * scale));
        ui.send(self.root, WidgetMessage::Height(size.y * scale));
        if self.styled_scale != scale {
            // fonts follow the DPI scale through the style
            self.styled_scale = scale;
            let mut style = self.style.clone().unwrap_or_else(fyrox::gui::style::Style::dark_style);
            let logical = if self.font_size > 0.0 { self.font_size } else { BASE_FONT_SIZE };
            style.set(fyrox::gui::style::Style::FONT_SIZE, logical * scale);
            self.style = Some(style.clone());
            ui.set_style(fyrox::gui::style::resource::StyleResource::new_embedded(style));
        }
        // a window closed with its X button: Fyrox only hides it
        let windows: Vec<(i32, Handle<UiNode>)> =
            self.widgets.iter().enumerate().filter_map(|(i, w)| w.as_ref().filter(|w| w.kind == "window").map(|w| (i as i32 + 1, w.handle))).collect();
        for (id, h) in windows {
            let visible = ui.try_get_node(h).is_ok_and(|n| n.visibility());
            if visible {
                self.open_windows.insert(id);
            } else if self.open_windows.remove(&id) {
                if let Ok(item) = self.get_mut(id) {
                    if item.value != 0.0 {
                        item.value = 0.0;
                        item.changed += 1;
                    }
                }
            }
        }
        let picked = ui.hit_test(ui.cursor_position());
        self.mouse_over = picked.is_some() && picked != self.root && ui.is_node_child_of(picked, self.root);
    }
}

fn int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}
fn float(v: f32) -> Result<Value, String> {
    Ok(Value::Float(v))
}
const VOID: Result<Value, String> = Ok(Value::Null);

/// Colour from 0-255 values.
pub fn rgba(r: i32, g: i32, b: i32, a: i32) -> Color {
    let c = |v: i32| v.clamp(0, 255) as u8;
    Color::from_rgba(c(r), c(g), c(b), c(a))
}

impl Host3D<'_> {
    pub fn ui(&mut self) -> Result<&mut UserInterface, String> {
        self.ui.as_deref_mut().ok_or_else(|| "Graphics or Graphics3D must be called before using GUI commands".to_string())
    }

    /// The panel top-level widgets go on, made on first use.
    fn gui_root(&mut self) -> Result<Handle<UiNode>, String> {
        if self.world.gui.root.is_none() {
            let (w, h, scale) = (self.world.width as f32, self.world.height as f32, self.ui_scale);
            let ui = self.ui()?;
            let ctx = &mut ui.build_ctx();
            let root = CanvasBuilder::new(WidgetBuilder::new().with_width(w * scale).with_height(h * scale)).build(ctx).to_base();
            self.world.gui.root = root;
        }
        Ok(self.world.gui.root)
    }

    /// Builds a widget with `f`, puts it in `parent` (0 = the screen) and
    /// registers it. Returns the program's handle.
    pub fn gui_add(&mut self, kind: &'static str, parent: i32, f: impl FnOnce(&mut BuildContext) -> (Handle<UiNode>, Handle<UiNode>)) -> Result<i32, String> {
        let target = if parent == 0 {
            self.gui_root()?
        } else {
            let p = self.world.gui.get(parent)?;
            if p.content.is_none() {
                return Err(format!("GUI widget {parent} ({}) cannot hold other widgets", p.kind));
            }
            p.content
        };
        self.gui_root()?;
        let ui = self.ui()?;
        let (handle, content) = f(&mut ui.build_ctx());
        ui.send(handle, WidgetMessage::LinkWith(target));
        let mut item = GuiItem::new(kind, handle);
        item.content = content;
        Ok(self.world.gui.add(item))
    }

    pub fn gui_send(&mut self, id: i32, data: impl fyrox::gui::message::MessageData) -> Result<(), String> {
        let h = self.world.gui.get(id)?.handle;
        self.ui()?.send(h, data);
        Ok(())
    }

    /// Mouse position relative to a widget's top-left corner (logical pixels).
    pub fn gui_mouse(&mut self, id: i32) -> Result<(f32, f32), String> {
        let h = self.world.gui.get(id)?.handle;
        let scale = self.ui_scale.max(1e-3);
        let (mx, my) = (self.input.mouse_x, self.input.mouse_y);
        let ui = self.ui()?;
        let b = ui.try_get_node(h).map(|n| n.screen_bounds()).map_err(|_| "GUI widget missing".to_string())?;
        Ok((mx - b.x() / scale, my - b.y() / scale))
    }

    /// `SetGuiText`.
    pub fn set_gui_text(&mut self, id: i32, text: &str) -> Result<(), String> {
        let item = self.world.gui.get_mut(id)?;
        item.text = text.to_string();
        let (kind, h) = (item.kind, item.handle);
        let ui = self.ui()?;
        match kind {
            "button" => ui.send(h, ButtonMessage::Content(ButtonContent::text(text))),
            "window" => ui.send(h, WindowMessage::Title(WindowTitle::text(text))),
            "label" | "textbox" => ui.send(h, TextMessage::Text(text.to_string())),
            "checkbox" => {
                // a check box shows its text in a text widget next to the box
                let label = ui.traverse_handle_iter(h).find(|&n| ui.try_get_of_type::<fyrox::gui::text::Text>(n).is_ok());
                if let Some(label) = label {
                    ui.send(label, TextMessage::Text(text.to_string()));
                }
            }
            _ => crate::gui_lists::set_text(self, id, text)?,
        }
        Ok(())
    }

    /// `SetGuiValue`.
    pub fn set_gui_value(&mut self, id: i32, v: f32) -> Result<(), String> {
        let item = self.world.gui.get_mut(id)?;
        item.value = v;
        let (kind, h) = (item.kind, item.handle);
        let ui = self.ui()?;
        match kind {
            "checkbox" => ui.send(h, CheckBoxMessage::Check(Some(v != 0.0))),
            "slider" => ui.send(h, ScrollBarMessage::Value(v)),
            "progress" => ui.send(h, ProgressBarMessage::Progress(v.clamp(0.0, 1.0))),
            "window" => {
                if v != 0.0 {
                    // a window inside the GUI root also needs showing again
                    ui.send(h, WidgetMessage::Visibility(true));
                    ui.send(h, WindowMessage::Open { alignment: fyrox::gui::window::WindowAlignment::None, modal: false, focus_content: true });
                } else {
                    ui.send(h, WindowMessage::Close);
                }
            }
            _ => crate::gui_editors::set_value(self, id, v)?,
        }
        Ok(())
    }
}

fn halign(v: i32) -> HorizontalAlignment {
    match v {
        1 => HorizontalAlignment::Left,
        2 => HorizontalAlignment::Center,
        3 => HorizontalAlignment::Right,
        _ => HorizontalAlignment::Stretch,
    }
}

fn valign(v: i32) -> VerticalAlignment {
    match v {
        1 => VerticalAlignment::Top,
        2 => VerticalAlignment::Center,
        3 => VerticalAlignment::Bottom,
        _ => VerticalAlignment::Stretch,
    }
}

impl Host3D<'_> {
    /// Changes the GUI style and applies it to every widget.
    fn gui_style(&mut self, f: impl FnOnce(&mut fyrox::gui::style::Style)) -> Result<(), String> {
        let mut style = self.world.gui.style.take().unwrap_or_else(fyrox::gui::style::Style::dark_style);
        f(&mut style);
        self.world.gui.style = Some(style.clone());
        self.ui()?.set_style(fyrox::gui::style::resource::StyleResource::new_embedded(style));
        Ok(())
    }
}

/// A stack panel for the inside of windows, tabs and scroll areas.
pub fn content_panel(ctx: &mut BuildContext) -> Handle<UiNode> {
    StackPanelBuilder::new(WidgetBuilder::new().with_margin(Thickness::uniform(crate::gui::px(4.0)))).build(ctx).to_base()
}

fn text_of(a: &Value) -> String {
    a.str().to_string()
}

/// Commands; see the module comment.
pub static COMMANDS: &[(&str, Cmd)] = &[
    // ---- GUI (ext): containers
    ("%GuiWindow$title#x#y#width#height%parent=0", |h, a, _| {
        let (title, x, y, w, ht) = (text_of(&a[0]), a[1].float(), a[2].float(), a[3].float(), a[4].float());
        let id = h.gui_add("window", a[5].int(), |ctx| {
            let content = content_panel(ctx);
            let win = WindowBuilder::new(WidgetBuilder::new().with_width(crate::gui::px(w)).with_height(crate::gui::px(ht)).with_desired_position(crate::gui::px2(x, y)))
                .with_title(WindowTitle::text(&title))
                .with_content(content)
                .can_minimize(false)
                .open(true)
                .build(ctx)
                .to_base();
            (win, content)
        })?;
        let item = h.world.gui.get_mut(id)?;
        item.value = 1.0;
        item.text = title;
        int(id)
    }),
    // layout: 0 column, 1 row, 2 wrap, 3 free (children placed with SetGuiPosition)
    ("%GuiPanel%layout=0%parent=0", |h, a, _| {
        let layout = a[0].int();
        int(h.gui_add("panel", a[1].int(), |ctx| {
            let wb = WidgetBuilder::new();
            let p = match layout {
                1 => StackPanelBuilder::new(wb).with_orientation(Orientation::Horizontal).build(ctx).to_base(),
                2 => WrapPanelBuilder::new(wb).with_orientation(Orientation::Horizontal).build(ctx).to_base(),
                3 => CanvasBuilder::new(wb).build(ctx).to_base(),
                _ => StackPanelBuilder::new(wb).build(ctx).to_base(),
            };
            (p, p)
        })?)
    }),
    // a grid of equal columns and rows; place children with GuiCell
    ("%GuiGrid%columns%rows%parent=0", |h, a, _| {
        let (cols, rows) = (a[0].int().max(1) as usize, a[1].int().max(1) as usize);
        let id = h.gui_add("grid", a[2].int(), |ctx| {
            let g = GridBuilder::new(WidgetBuilder::new())
                .add_columns((0..cols).map(|_| Column::stretch()).collect())
                .add_rows((0..rows).map(|_| Row::stretch()).collect())
                .build(ctx)
                .to_base();
            (g, g)
        })?;
        h.world.gui.get_mut(id)?.grid = (cols, rows);
        int(id)
    }),
    ("GuiCell%widget%column%row", |h, a, _| {
        h.gui_send(a[0].int(), WidgetMessage::Column(a[1].int().max(0) as usize))?;
        h.gui_send(a[0].int(), WidgetMessage::Row(a[2].int().max(0) as usize))?;
        VOID
    }),
    // a scrolling area; widgets put in it are stacked in a column
    ("%GuiScroll%parent=0", |h, a, _| {
        int(h.gui_add("scroll", a[0].int(), |ctx| {
            let content = content_panel(ctx);
            let sv = ScrollViewerBuilder::new(WidgetBuilder::new()).with_content(content).build(ctx).to_base();
            (sv, content)
        })?)
    }),
    // ---- GUI (ext): basic widgets
    ("%GuiButton$text%parent=0", |h, a, _| {
        let text = text_of(&a[0]);
        let id = h.gui_add("button", a[1].int(), |ctx| (ButtonBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(60.0, 24.0))).with_text(&text).build(ctx).to_base(), Handle::NONE))?;
        h.world.gui.get_mut(id)?.text = a[0].str().to_string();
        int(id)
    }),
    ("%GuiLabel$text%parent=0", |h, a, _| {
        let text = text_of(&a[0]);
        let id = h.gui_add("label", a[1].int(), |ctx| (TextBuilder::new(WidgetBuilder::new()).with_text(text).build(ctx).to_base(), Handle::NONE))?;
        h.world.gui.get_mut(id)?.text = a[0].str().to_string();
        int(id)
    }),
    ("%GuiTextBox$text=\"\"%parent=0%multiline=0", |h, a, _| {
        let (text, multi) = (text_of(&a[0]), a[2].int() != 0);
        let id = h.gui_add("textbox", a[1].int(), |ctx| {
            let tb = TextBoxBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(80.0, 22.0)))
                .with_text(text)
                .with_multiline(multi)
                .with_text_commit_mode(TextCommitMode::Immediate)
                .build(ctx)
                .to_base();
            (tb, Handle::NONE)
        })?;
        h.world.gui.get_mut(id)?.text = a[0].str().to_string();
        int(id)
    }),
    ("%GuiCheckBox$text%checked=0%parent=0", |h, a, _| {
        let (text, checked) = (text_of(&a[0]), a[1].int() != 0);
        let id = h.gui_add("checkbox", a[2].int(), |ctx| {
            let label = TextBuilder::new(WidgetBuilder::new().with_margin(Thickness::left(crate::gui::px(4.0)))).with_text(text).build(ctx);
            (CheckBoxBuilder::new(WidgetBuilder::new()).with_content(label).checked(Some(checked)).build(ctx).to_base(), Handle::NONE)
        })?;
        let item = h.world.gui.get_mut(id)?;
        item.value = checked as i32 as f32;
        item.text = a[0].str().to_string();
        int(id)
    }),
    ("%GuiSlider#min#max#value%parent=0", |h, a, _| {
        let (min, max, v) = (a[0].float(), a[1].float(), a[2].float());
        let id = h.gui_add("slider", a[3].int(), |ctx| {
            let s = ScrollBarBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(100.0, 20.0)))
                .with_orientation(Orientation::Horizontal)
                .with_min(min)
                .with_max(max)
                .with_value(v)
                .build(ctx)
                .to_base();
            (s, Handle::NONE)
        })?;
        h.world.gui.get_mut(id)?.value = v;
        int(id)
    }),
    ("%GuiProgress#value=0%parent=0", |h, a, _| {
        let v = a[0].float().clamp(0.0, 1.0);
        let id = h.gui_add("progress", a[1].int(), |ctx| {
            (ProgressBarBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(100.0, 16.0))).with_progress(v).build(ctx).to_base(), Handle::NONE)
        })?;
        h.world.gui.get_mut(id)?.value = v;
        int(id)
    }),
    // an image (from LoadImage/CreateImage); with GuiClicked and GuiMouseX/Y it makes a custom widget
    ("%GuiImage%image%parent=0", |h, a, _| {
        let img = h.image(a[0].int())?;
        let (tex, (w, ht)) = (img.masked.clone(), img.size());
        int(h.gui_add("image", a[1].int(), |ctx| {
            (ImageBuilder::new(WidgetBuilder::new().with_width(crate::gui::px(w)).with_height(crate::gui::px(ht))).with_texture(tex).build(ctx).to_base(), Handle::NONE)
        })?)
    }),
    // ---- GUI (ext): reading and changing widgets
    ("SetGuiText%widget$text", |h, a, _| {
        h.set_gui_text(a[0].int(), &a[1].str())?;
        VOID
    }),
    ("$GuiText%widget", |h, a, _| Ok(Value::Str(h.world.gui.get(a[0].int())?.text.as_str().into()))),
    ("SetGuiValue%widget#value", |h, a, _| {
        h.set_gui_value(a[0].int(), a[1].float())?;
        VOID
    }),
    ("#GuiValue%widget", |h, a, _| float(h.world.gui.get(a[0].int())?.value)),
    ("%GuiClicked%widget", |h, a, _| int(std::mem::take(&mut h.world.gui.get_mut(a[0].int())?.clicked) as i32)),
    ("%GuiChanged%widget", |h, a, _| int(std::mem::take(&mut h.world.gui.get_mut(a[0].int())?.changed) as i32)),
    ("SetGuiPosition%widget#x#y", |h, a, _| {
        h.gui_send(a[0].int(), WidgetMessage::DesiredPosition(px2(a[1].float(), a[2].float())))?;
        VOID
    }),
    // 0 for a size means "as big as the content needs"
    ("SetGuiSize%widget#width#height", |h, a, _| {
        let (w, ht) = (a[1].float(), a[2].float());
        h.gui_send(a[0].int(), WidgetMessage::Width(if w > 0.0 { px(w) } else { f32::NAN }))?;
        h.gui_send(a[0].int(), WidgetMessage::Height(if ht > 0.0 { px(ht) } else { f32::NAN }))?;
        VOID
    }),
    ("GuiMargin%widget#left#top#right#bottom", |h, a, _| {
        let t = Thickness { left: crate::gui::px(a[1].float()), top: crate::gui::px(a[2].float()), right: crate::gui::px(a[3].float()), bottom: crate::gui::px(a[4].float()) };
        h.gui_send(a[0].int(), WidgetMessage::Margin(t))?;
        VOID
    }),
    // 0 stretch, 1 left/top, 2 centre, 3 right/bottom
    ("GuiAlign%widget%horizontal%vertical", |h, a, _| {
        h.gui_send(a[0].int(), WidgetMessage::HorizontalAlignment(halign(a[1].int())))?;
        h.gui_send(a[0].int(), WidgetMessage::VerticalAlignment(valign(a[2].int())))?;
        VOID
    }),
    ("GuiVisible%widget%visible", |h, a, _| {
        h.gui_send(a[0].int(), WidgetMessage::Visibility(a[1].int() != 0))?;
        VOID
    }),
    ("GuiEnable%widget%enabled", |h, a, _| {
        h.gui_send(a[0].int(), WidgetMessage::Enabled(a[1].int() != 0))?;
        VOID
    }),
    ("GuiFocus%widget", |h, a, _| {
        h.gui_send(a[0].int(), WidgetMessage::Focus)?;
        VOID
    }),
    ("GuiTooltip%widget$text", |h, a, _| {
        let text = text_of(&a[1]);
        let ui = h.ui()?;
        let tip = {
            let ctx = &mut ui.build_ctx();
            let t = fyrox::gui::border::BorderBuilder::new(
                WidgetBuilder::new()
                    .with_visibility(false)
                    .with_child(TextBuilder::new(WidgetBuilder::new().with_margin(Thickness::uniform(crate::gui::px(4.0)))).with_text(text).build(ctx)),
            )
            .build(ctx);
            RcUiNodeHandle::new(t.to_base::<UiNode>(), ctx.sender())
        };
        h.gui_send(a[0].int(), WidgetMessage::Tooltip(Some(tip)))?;
        VOID
    }),
    ("GuiColor%widget%red%green%blue%alpha=255", |h, a, _| {
        let c = rgba(a[1].int(), a[2].int(), a[3].int(), a[4].int());
        let item = h.world.gui.get(a[0].int())?.clone();
        let ui = h.ui()?;
        // a button paints itself with its decorator: normal, hover and pressed shades
        let decorator = ui.try_get_of_type::<fyrox::gui::button::Button>(item.handle).map(|b| *b.decorator).unwrap_or_default();
        if decorator.is_some() {
            let shade = |k: f32| {
                let f = |v: u8| ((v as f32 * k).min(255.0)) as u8;
                StyledProperty::from(Brush::Solid(Color::from_rgba(f(c.r), f(c.g), f(c.b), c.a)))
            };
            ui.send(decorator, fyrox::gui::decorator::DecoratorMessage::NormalBrush(shade(1.0)));
            ui.send(decorator, fyrox::gui::decorator::DecoratorMessage::HoverBrush(shade(1.2)));
            ui.send(decorator, fyrox::gui::decorator::DecoratorMessage::PressedBrush(shade(0.8)));
        } else {
            ui.send(item.handle, WidgetMessage::Background(StyledProperty::from(Brush::Solid(c))));
        }
        VOID
    }),
    ("GuiTextColor%widget%red%green%blue", |h, a, _| {
        let c = rgba(a[1].int(), a[2].int(), a[3].int(), 255);
        h.gui_send(a[0].int(), WidgetMessage::Foreground(StyledProperty::from(Brush::Solid(c))))?;
        VOID
    }),
    ("GuiFontSize%widget#size", |h, a, _| {
        let size = px(a[1].float().max(1.0));
        let item = h.world.gui.get(a[0].int())?.clone();
        let ui = h.ui()?;
        // text inside the widget (a label, or the text of a button or check box)
        let targets: Vec<Handle<UiNode>> = std::iter::once(item.handle).chain(ui.traverse_handle_iter(item.handle)).collect();
        for t in targets {
            if ui.try_get_of_type::<fyrox::gui::text::Text>(t).is_ok() {
                ui.send(t, TextMessage::FontSize(StyledProperty::from(size)));
            }
        }
        VOID
    }),
    ("FreeGui%widget", |h, a, _| {
        let item = h.world.gui.get(a[0].int())?.clone();
        h.ui()?.send(item.handle, WidgetMessage::Remove);
        let g = &mut h.world.gui;
        g.by_handle.retain(|_, v| *v != a[0].int());
        g.widgets[a[0].int() as usize - 1] = None;
        VOID
    }),
    // ---- GUI (ext): styling for every widget at once
    // 0 dark, 1 light
    ("GuiTheme%theme", |h, a, _| {
        let base = if a[0].int() == 1 { fyrox::gui::style::Style::light_style() } else { fyrox::gui::style::Style::dark_style() };
        h.world.gui.style = Some(base);
        let size = if h.world.gui.font_size > 0.0 { h.world.gui.font_size } else { BASE_FONT_SIZE };
        h.gui_style(|s| {
            s.set(fyrox::gui::style::Style::FONT_SIZE, px(size));
        })?;
        VOID
    }),
    // name is a style brush: Darkest, Darker, Dark, Primary, LighterPrimary, Light, Lighter,
    // Lightest, Bright, Brightest, BrightBlue, DimBlue, Text, Foreground, Highlight, Ok, ...
    ("GuiThemeColor$name%red%green%blue%alpha=255", |h, a, _| {
        let key = format!("Global.Brush.{}", a[0].str());
        let c = rgba(a[1].int(), a[2].int(), a[3].int(), a[4].int());
        h.gui_style(|s| {
            s.set(key.as_str(), Brush::Solid(c));
        })?;
        VOID
    }),
    ("GuiThemeFontSize#size", |h, a, _| {
        let size = a[0].float().max(1.0);
        h.world.gui.font_size = size;
        h.gui_style(|s| {
            s.set(fyrox::gui::style::Style::FONT_SIZE, px(size));
        })?;
        VOID
    }),
    ("#GuiMouseX%widget", |h, a, _| float(h.gui_mouse(a[0].int())?.0)),
    ("#GuiMouseY%widget", |h, a, _| float(h.gui_mouse(a[0].int())?.1)),
    // the mouse is over the GUI: games can ignore clicks then
    ("%GuiUnderMouse", |h, _, _| int(h.world.gui.mouse_over as i32)),
];

/// Runs a closure with access to a widget's handle and the UI (helpers for other GUI modules).
pub fn with_ui<R>(h: &mut Host3D, id: i32, f: impl FnOnce(&mut UserInterface, &GuiItem) -> R) -> Result<R, String> {
    let item = h.world.gui.get(id)?.clone();
    Ok(f(h.ui()?, &item))
}

