//! Retained GUI (ext): popups, message boxes, file browsers and dialogs, and docking. See `gui.rs` for the shared parts.
//!
//! A message box or file dialog reports the user's answer through the usual
//! `GuiChanged` / `GuiValue` / `GuiText`, so a game polls it like any widget.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::Sender,
};

use fyrox::{
    core::pool::Handle,
    gui::{
        border::BorderBuilder,
        button::{ButtonBuilder, ButtonMessage},
        dock::{DockingManagerBuilder, DockingManagerMessage, SplitDirection, Tile, TileBuilder, TileContent, TileMessage},
        file_browser::{FileBrowserBuilder, FileBrowserMessage},
        grid::{Column, GridBuilder, Row},
        message::UiMessage,
        messagebox::{MessageBoxBuilder, MessageBoxButtons, MessageBoxMessage, MessageBoxResult},
        popup::{Placement, PopupBuilder, PopupMessage},
        stack_panel::StackPanelBuilder,
        style::{resource::StyleResourceExt, Style},
        text::{TextBuilder, TextMessage},
        text_box::{TextBoxBuilder, TextCommitMode},
        widget::{WidgetBuilder, WidgetMessage},
        window::{Window, WindowBuilder, WindowMessage, WindowTitle},
        HorizontalAlignment, Orientation, Thickness, UiNode, UserInterface, VerticalAlignment,
    },
};
use fyrox::core::reflect::Reflect;
use fyrox::graph::SceneGraph;
use neptune_vm::Value;

use crate::commands::Cmd;
use crate::gui::{content_panel, GuiState};
use crate::world::Host3D;

fn int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}
const VOID: Result<Value, String> = Ok(Value::Null);

/// What a file dialog keeps while it is open.
struct FileDialog {
    browser: Handle<UiNode>,
    name_box: Handle<UiNode>,
    ok: Handle<UiNode>,
    cancel: Handle<UiNode>,
    /// Path picked in the browser (a file or a folder).
    selected: PathBuf,
    /// Text of the file-name box.
    name: String,
    /// Sends messages from `on_message`, which has no access to the UI.
    sender: Sender<UiMessage>,
}

thread_local! {
    /// Open file dialogs by window handle.
    static DIALOGS: RefCell<HashMap<Handle<UiNode>, FileDialog>> = RefCell::new(HashMap::new());
}

/// Records what the user did with this module's widgets.
pub fn on_message(gui: &mut GuiState, id: i32, msg: &UiMessage) {
    let Ok(item) = gui.get_mut(id) else { return };
    match item.kind {
        "popup" => match msg.data() {
            Some(PopupMessage::Open) => item.value = 1.0,
            // closed by the user (clicking outside, Escape); GuiShowPopup sets value 0 first
            Some(PopupMessage::Close) if item.value != 0.0 => {
                item.value = 0.0;
                item.changed += 1;
            }
            _ => {}
        },
        "msgbox" => {
            if let Some(MessageBoxMessage::Close(r)) = msg.data() {
                item.value = match r {
                    MessageBoxResult::Ok | MessageBoxResult::Yes => 1.0,
                    MessageBoxResult::No => 2.0,
                    MessageBoxResult::Cancel => 3.0,
                };
                item.changed += 1;
            }
        }
        "filebrowser" => {
            if let Some(FileBrowserMessage::Path(p)) = msg.data() {
                item.text = p.to_string_lossy().into_owned();
                item.changed += 1;
            }
        }
        "filedialog" => {
            let window = item.handle;
            // clicks on the dialog's own buttons are not clicks of the dialog
            if msg.destination() != window && msg.data::<ButtonMessage>().is_some() {
                item.clicked = item.clicked.saturating_sub(1);
            }
            let result = DIALOGS.with(|d| file_dialog_message(d.borrow_mut().get_mut(&window)?, msg));
            if let Some(result) = result {
                if let Some(path) = &result {
                    item.text = path.to_string_lossy().into_owned();
                }
                item.value = result.is_some() as i32 as f32;
                item.changed += 1;
                if let Some(d) = DIALOGS.with(|d| d.borrow_mut().remove(&window)) {
                    let _ = d.sender.send(UiMessage::for_widget(window, WindowMessage::Close));
                }
            }
        }
        _ => {}
    }
}

/// Follows a file dialog. Returns `Some(Some(path))` on OK, `Some(None)` on Cancel.
fn file_dialog_message(d: &mut FileDialog, msg: &UiMessage) -> Option<Option<PathBuf>> {
    let dest = msg.destination();
    if let Some(FileBrowserMessage::Path(p)) = msg.data() {
        if dest == d.browser {
            d.selected = p.clone();
            if p.is_file() {
                if let Some(name) = p.file_name() {
                    d.name = name.to_string_lossy().into_owned();
                    let _ = d.sender.send(UiMessage::for_widget(d.name_box, TextMessage::Text(d.name.clone())));
                }
            }
        }
    } else if let Some(TextMessage::Text(t)) = msg.data() {
        if dest == d.name_box {
            d.name = t.clone();
        }
    } else if let Some(ButtonMessage::Click) = msg.data() {
        if dest == d.ok {
            return Some(Some(dialog_path(&d.selected, &d.name)));
        } else if dest == d.cancel {
            return Some(None);
        }
    }
    None
}

/// The folder of the browser's selection joined with the typed file name.
fn dialog_path(selected: &Path, name: &str) -> PathBuf {
    if name.trim().is_empty() {
        return selected.to_path_buf();
    }
    let folder = if selected.is_file() { selected.parent().unwrap_or(Path::new(".")) } else { selected };
    folder.join(name.trim())
}

fn kind_of(h: &Host3D, id: i32, kind: &str) -> Result<Handle<UiNode>, String> {
    let item = h.world.gui.get(id)?;
    if item.kind != kind {
        return Err(format!("GUI widget {id} is not a {kind}"));
    }
    Ok(item.handle)
}

/// Shows a window as a centred modal window.
fn open_modal(h: &mut Host3D, w: Handle<UiNode>) -> Result<(), String> {
    let ui = h.ui()?;
    ui.send(w, WidgetMessage::Visibility(true));
    ui.send(w, WindowMessage::Open { alignment: fyrox::gui::window::WindowAlignment::None, modal: true, focus_content: true });
    center(h, w)
}

/// Centres a widget on its parent. (Fyrox's `WidgetMessage::Center` mixes up
/// sizes under the GUI's DPI scale, so this works in the parent's own units.)
fn center(h: &mut Host3D, w: Handle<UiNode>) -> Result<(), String> {
    // the widget must be linked, visible and measured first
    flush(h)?;
    let ui = h.ui()?;
    ui.update_layout(ui.screen_size());
    let node = ui.try_get_node(w).map_err(|_| "GUI widget missing")?;
    let size = node.actual_local_size();
    let parent = node.parent();
    let psize = ui.try_get_node(parent).map(|p| p.actual_local_size()).unwrap_or_else(|_| ui.screen_size());
    ui.send(w, WidgetMessage::DesiredPosition((psize - size).scale(0.5)));
    Ok(())
}

fn border(h: &mut Host3D, thickness: f32, parent: i32) -> Result<i32, String> {
    h.gui_add("border", parent, |ctx| {
        let content = content_panel(ctx);
        let b = BorderBuilder::new(
            WidgetBuilder::new()
                .with_background(ctx.style.property(Style::BRUSH_DARKER))
                .with_foreground(ctx.style.property(Style::BRUSH_LIGHT))
                .with_child(content),
        )
        .with_stroke_thickness(Thickness::uniform(crate::gui::px(thickness.max(0.0))).into())
        .build(ctx)
        .to_base();
        (b, content)
    })
}

fn popup(h: &mut Host3D, w: f32, ht: f32, parent: i32) -> Result<i32, String> {
    h.gui_add("popup", parent, |ctx| {
        let content = content_panel(ctx);
        let size = |v: f32| if v > 0.0 { v } else { f32::NAN };
        let p = PopupBuilder::new(WidgetBuilder::new().with_width(crate::gui::px(size(w))).with_height(crate::gui::px(size(ht))))
            .with_content(content)
            // smart placement works in screen pixels, which the DPI-scaled GUI does not use
            .with_smart_placement(false)
            .build(ctx)
            .to_base();
        (p, content)
    })
}

fn show_popup(h: &mut Host3D, id: i32, show: bool) -> Result<(), String> {
    let p = kind_of(h, id, "popup")?;
    h.world.gui.get_mut(id)?.value = show as i32 as f32;
    let (mx, my) = (h.input.mouse_x, h.input.mouse_y);
    let ui = h.ui()?;
    if show {
        // Fyrox works in physical pixels; the mouse is in logical ones
        ui.send(p, PopupMessage::Placement(Placement::Position { position: crate::gui::px2(mx, my), target: Handle::NONE }));
        ui.send(p, PopupMessage::Open);
    } else {
        ui.send(p, PopupMessage::Close);
    }
    Ok(())
}

fn message_box(h: &mut Host3D, title: String, text: String, buttons: i32) -> Result<i32, String> {
    let buttons = match buttons {
        1 => MessageBoxButtons::YesNo,
        2 => MessageBoxButtons::YesNoCancel,
        _ => MessageBoxButtons::Ok,
    };
    let id = h.gui_add("msgbox", 0, |ctx| {
        let win = WindowBuilder::new(WidgetBuilder::new().with_width(crate::gui::px(320.0)).with_height(crate::gui::px(130.0)))
            .with_title(WindowTitle::text(&title))
            .can_minimize(false)
            .can_close(false)
            .can_resize(false)
            .open(true);
        (MessageBoxBuilder::new(win).with_text(&text).with_buttons(buttons).build(ctx).to_base(), Handle::NONE)
    })?;
    h.world.gui.get_mut(id)?.text = text;
    let mb = h.world.gui.get(id)?.handle;
    open_modal(h, mb)?;
    Ok(id)
}

fn file_browser(h: &mut Host3D, path: &str, parent: i32) -> Result<i32, String> {
    let path = if path.is_empty() { "." } else { path };
    let id = h.gui_add("filebrowser", parent, |ctx| {
        (FileBrowserBuilder::new(WidgetBuilder::new().with_width(crate::gui::px(300.0)).with_height(crate::gui::px(250.0))).with_path(path).build(ctx).to_base(), Handle::NONE)
    })?;
    h.world.gui.get_mut(id)?.text = path.to_string();
    Ok(id)
}

fn file_dialog(h: &mut Host3D, title: String, path: &str, save: bool) -> Result<i32, String> {
    let path = Path::new(if path.is_empty() { "." } else { path });
    // a folder opens the browser there; a file name opens its folder with the name filled in
    let (start, name) = if path.is_dir() {
        (path.to_path_buf(), String::new())
    } else {
        let folder = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        (if path.exists() { path.to_path_buf() } else { folder.to_path_buf() }, name)
    };
    // the browser shows the nearest folder that exists; the result keeps the folder asked for
    let shown = start.ancestors().find(|p| !p.as_os_str().is_empty() && p.exists()).unwrap_or(Path::new(".")).to_path_buf();
    let (mut browser, mut name_box, mut ok, mut cancel) = (Handle::NONE, Handle::NONE, Handle::NONE, Handle::NONE);
    let id = h.gui_add("filedialog", 0, |ctx| {
        browser = FileBrowserBuilder::new(WidgetBuilder::new().on_row(0).with_margin(Thickness::uniform(crate::gui::px(1.0)))).with_path(&shown).build(ctx).to_base();
        let label = TextBuilder::new(WidgetBuilder::new().on_column(0).with_vertical_alignment(VerticalAlignment::Center)).with_text("File name:").build(ctx);
        name_box = TextBoxBuilder::new(WidgetBuilder::new().on_column(1).with_height(crate::gui::px(24.0)).with_margin(Thickness::uniform(crate::gui::px(1.0))))
            .with_text(name.clone())
            .with_text_commit_mode(TextCommitMode::Immediate)
            .with_vertical_text_alignment(VerticalAlignment::Center)
            .build(ctx)
            .to_base();
        let name_row = GridBuilder::new(WidgetBuilder::new().on_row(1).with_child(label).with_child(name_box))
            .add_row(Row::auto())
            .add_column(Column::strict(80.0))
            .add_column(Column::stretch())
            .build(ctx);
        let button = |ctx: &mut fyrox::gui::BuildContext, text: &str| {
            ButtonBuilder::new(WidgetBuilder::new().with_width(crate::gui::px(80.0)).with_height(crate::gui::px(24.0)).with_margin(Thickness::uniform(crate::gui::px(1.0)))).with_text(text).build(ctx).to_base()
        };
        ok = button(ctx, if save { "Save" } else { "Open" });
        cancel = button(ctx, "Cancel");
        let buttons = StackPanelBuilder::new(WidgetBuilder::new().on_row(2).with_horizontal_alignment(HorizontalAlignment::Right).with_child(ok).with_child(cancel))
            .with_orientation(Orientation::Horizontal)
            .build(ctx);
        let grid = GridBuilder::new(WidgetBuilder::new().with_margin(Thickness::uniform(crate::gui::px(4.0))).with_child(browser).with_child(name_row).with_child(buttons))
            .add_row(Row::stretch())
            .add_row(Row::auto())
            .add_row(Row::auto())
            .add_column(Column::stretch())
            .build(ctx);
        let win = WindowBuilder::new(WidgetBuilder::new().with_width(crate::gui::px(520.0)).with_height(crate::gui::px(400.0)))
            .with_title(WindowTitle::text(&title))
            .with_content(grid)
            .can_minimize(false)
            .can_close(false)
            .open(true)
            .build(ctx)
            .to_base();
        (win, Handle::NONE)
    })?;
    let gui = &mut h.world.gui;
    let window = gui.get(id)?.handle;
    for sub in [browser, name_box, ok, cancel] {
        gui.by_handle.insert(sub, id);
    }
    let d = FileDialog { browser, name_box, ok, cancel, selected: start, name, sender: h.ui()?.sender() };
    DIALOGS.with(|m| m.borrow_mut().insert(window, d));
    open_modal(h, window)?;
    Ok(id)
}

fn dock(h: &mut Host3D, parent: i32) -> Result<i32, String> {
    h.gui_add("dock", parent, |ctx| {
        let tile = TileBuilder::new(WidgetBuilder::new()).build(ctx);
        (DockingManagerBuilder::new(WidgetBuilder::new().with_width(crate::gui::px(400.0)).with_height(crate::gui::px(300.0)).with_child(tile)).build(ctx).to_base(), Handle::NONE)
    })
}

/// place: 0 floating, 1 left, 2 right, 3 top, 4 bottom, 5 a tab in the middle.
fn dock_window(h: &mut Host3D, dock: i32, window: i32, place: i32) -> Result<(), String> {
    let d = kind_of(h, dock, "dock")?;
    let w = kind_of(h, window, "window")?;
    let item = h.world.gui.get_mut(window)?;
    item.value = 1.0;
    let title = item.text.clone();
    let ui = h.ui()?;
    // a window docked as a tab shows its tab label (there is no message to set it)
    if let Ok(win) = ui.try_get_mut_of_type::<Window>(w) {
        win.set_field("tab_label", Box::new(title), &mut |_| {});
    }
    ui.send(w, WidgetMessage::Visibility(true));
    if place <= 0 {
        ui.send(d, DockingManagerMessage::AddFloatingWindow(w.transmute()));
        return Ok(());
    }
    // earlier docking commands must have reached the tiles before they are read
    flush(h)?;
    let ui = h.ui()?;
    let root = ui.try_get_node(d).ok().and_then(|n| n.children().first().copied()).ok_or("Dock has no tile")?;
    let (tile, content) = leaf_tile(ui, root, place);
    let w: Handle<Window> = w.transmute();
    let split = |direction, first| TileMessage::Split { window: w, direction, first };
    let msg = match (&content, place) {
        (TileContent::Empty, _) | (_, 5..) => TileMessage::Content(content.clone().plus_window(w)),
        (_, 1) => split(SplitDirection::Horizontal, true),
        (_, 2) => split(SplitDirection::Horizontal, false),
        (_, 3) => split(SplitDirection::Vertical, true),
        _ => split(SplitDirection::Vertical, false),
    };
    // the tile links the window in itself
    ui.send(tile, msg);
    flush(h)
}

/// The tile a window docked at `place` goes into: down the split tiles towards that side.
fn leaf_tile(ui: &UserInterface, mut tile: Handle<UiNode>, place: i32) -> (Handle<UiNode>, TileContent) {
    loop {
        let content = ui.try_get_of_type::<Tile>(tile).map(|t| t.content.clone()).unwrap_or_default();
        match content {
            TileContent::HorizontalTiles { tiles, .. } | TileContent::VerticalTiles { tiles, .. } => {
                tile = tiles[usize::from(place == 2 || place == 4)].to_base();
            }
            _ => return (tile, content),
        }
    }
}

/// Handles the UI's queued messages now (passing them to the GUI state as the engine would).
fn flush(h: &mut Host3D) -> Result<(), String> {
    let ui = h.ui.as_deref_mut().ok_or("Graphics must be called first")?;
    while let Some(m) = ui.poll_message() {
        h.world.gui.on_message(&m);
    }
    Ok(())
}

pub static COMMANDS: &[(&str, Cmd)] = &[
    // ---- GUI (ext): dialogs, popups and docking
    ("%GuiBorder#thickness=1%parent=0", |h, a, _| int(border(h, a[0].float(), a[1].int())?)),
    ("%GuiPopup#width#height%parent=0", |h, a, _| int(popup(h, a[0].float(), a[1].float(), a[2].int())?)),
    ("GuiShowPopup%popup%show", |h, a, _| {
        show_popup(h, a[0].int(), a[1].int() != 0)?;
        VOID
    }),
    // buttons: 0 OK, 1 Yes/No, 2 Yes/No/Cancel; answer in GuiValue: 1 OK/Yes, 2 No, 3 Cancel
    ("%GuiMessageBox$title$text%buttons=0", |h, a, _| int(message_box(h, a[0].str().to_string(), a[1].str().to_string(), a[2].int())?)),
    ("%GuiFileBrowser$path%parent=0", |h, a, _| int(file_browser(h, &a[0].str(), a[1].int())?)),
    ("%GuiFileDialog$title$path%save=0", |h, a, _| int(file_dialog(h, a[0].str().to_string(), &a[1].str(), a[2].int() != 0)?)),
    ("%GuiDock%parent=0", |h, a, _| int(dock(h, a[0].int())?)),
    // place: 0 floating, 1 left, 2 right, 3 top, 4 bottom, 5 tab
    ("GuiDockWindow%dock%window%place=0", |h, a, _| {
        dock_window(h, a[0].int(), a[1].int(), a[2].int())?;
        VOID
    }),
    ("GuiModal%window", |h, a, _| {
        let w = kind_of(h, a[0].int(), "window")?;
        h.world.gui.get_mut(a[0].int())?.value = 1.0;
        open_modal(h, w)?;
        VOID
    }),
    ("GuiCenter%widget", |h, a, _| {
        let w = h.world.gui.get(a[0].int())?.handle;
        center(h, w)?;
        VOID
    }),
];
