//! Retained GUI (ext): drop-down lists, list views, trees, tab controls and menus. See `gui.rs` for the shared parts.
//!
//! Every entry of these widgets is registered as a GUI widget of its own:
//! list and drop-down entries ("listitem", internal), tree items, tabs and
//! menu items. The owner keeps the entries' Fyrox handles in
//! `GuiItem.items`, so `GuiState::by_handle` finds an entry's text and the
//! program handle of a tree item, tab or menu item.

use fyrox::{
    core::{pool::Handle, uuid::Uuid},
    gui::{
        border::BorderBuilder,
        decorator::DecoratorBuilder,
        dropdown_list::{DropdownListBuilder, DropdownListMessage},
        list_view::{ListViewBuilder, ListViewMessage},
        menu::{MenuBuilder, MenuItemBuilder, MenuItemContent, MenuItemMessage, MenuItemPlacement},
        message::UiMessage,
        tab_control::{TabControlBuilder, TabControlMessage, TabDefinition},
        text::{Text, TextBuilder, TextMessage},
        tree::{Tree, TreeBuilder, TreeExpansionStrategy, TreeMessage, TreeRootBuilder, TreeRootMessage},
        widget::{WidgetBuilder, WidgetMessage},
        BuildContext, Thickness, UiNode, UserInterface,
    },
    graph::SceneGraph,
};
use neptune_vm::Value;

use crate::commands::Cmd;
use crate::gui::{content_panel, GuiItem, GuiState};
use crate::world::Host3D;

fn int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}
const VOID: Result<Value, String> = Ok(Value::Null);

/// Records what the user did with this module's widgets.
pub fn on_message(gui: &mut GuiState, id: i32, msg: &UiMessage) {
    let Ok(item) = gui.get(id) else { return };
    let index = |i: Option<usize>| i.map_or(-1, |i| i as i32);
    let sel = match item.kind {
        "dropdown" => match msg.data() {
            Some(DropdownListMessage::Selection(s)) => index(*s),
            _ => return,
        },
        "list" => match msg.data() {
            Some(ListViewMessage::Selection(s)) => index(s.first().copied()),
            _ => return,
        },
        "tabs" => match msg.data() {
            Some(TabControlMessage::ActiveTab(u)) => index(u.and_then(|u| item.items.iter().position(|&t| tab_uuid(t) == u))),
            _ => return,
        },
        "tree" => match msg.data() {
            Some(TreeRootMessage::Select(s)) => s.first().and_then(|t| gui.by_handle.get(&t.to_base()).copied()).unwrap_or(0),
            _ => return,
        },
        "menuitem" => {
            if let Some(MenuItemMessage::Click) = msg.data() {
                if let Ok(item) = gui.get_mut(id) {
                    item.clicked += 1;
                }
            }
            return;
        }
        _ => return,
    };
    record(gui, id, sel, true);
}

/// Stores a new selection (an index, or for a tree an item's program handle):
/// `selected`, `value` and `text`; `user` counts it as a change.
fn record(gui: &mut GuiState, id: i32, sel: i32, user: bool) {
    let Ok(item) = gui.get(id) else { return };
    if item.selected == sel {
        return;
    }
    let tree = item.kind == "tree";
    let entry = if tree { (sel > 0).then_some(sel) } else { usize::try_from(sel).ok().and_then(|i| item.items.get(i)).and_then(|h| gui.by_handle.get(h).copied()) };
    let text = entry.and_then(|e| gui.get(e).ok()).map(|e| e.text.clone()).unwrap_or_default();
    if let (true, true, Some(e)) = (user, tree, entry) {
        if let Ok(e) = gui.get_mut(e) {
            e.clicked += 1;
        }
    }
    if let Ok(item) = gui.get_mut(id) {
        item.selected = sel;
        item.value = sel as f32;
        item.text = text;
        if user {
            item.changed += 1;
        }
    }
}

/// `SetGuiText` for the widgets of this module.
pub fn set_text(h: &mut Host3D, id: i32, text: &str) -> Result<(), String> {
    let item = h.world.gui.get(id)?.clone();
    match item.kind {
        // selects the entry with this text
        "dropdown" | "list" => {
            let found = item.items.iter().position(|e| entry_text(&h.world.gui, *e) == text);
            h.world.gui.get_mut(id)?.selected = -2; // force `record` to refresh the text
            set_selected(h, id, found.map_or(-1, |i| i as i32))?;
        }
        "treeitem" => {
            let ui = h.ui()?;
            let content = ui.try_get(item.handle.to_variant::<Tree>()).map(|t| t.content).unwrap_or_default();
            ui.send(content, TextMessage::Text(text.to_string()));
        }
        "tab" => {
            let header = item.items.first().copied().unwrap_or_default();
            h.ui()?.send(header, TextMessage::Text(text.to_string()));
        }
        "menuitem" => {
            let ui = h.ui()?;
            // the item's own text, not its shortcut text (column 2)
            let label = ui.traverse_handle_iter(item.handle).find(|&n| ui.try_get_of_type::<Text>(n).is_ok_and(|t| t.column() != 2));
            if let Some(label) = label {
                ui.send(label, TextMessage::Text(text.to_string()));
            }
        }
        _ => {}
    }
    // a renamed selected tab or tree item renames its owner's selection too
    for owner in h.world.gui.widgets.iter_mut().flatten() {
        let at = owner.items.iter().position(|&e| e == item.handle).map(|i| i as i32);
        if (owner.kind == "tabs" && at == Some(owner.selected)) || (owner.kind == "tree" && owner.selected == id) {
            owner.text = text.to_string();
        }
    }
    Ok(())
}

/// A tab's id in the tab control, made from its content panel's handle.
fn tab_uuid(content: Handle<UiNode>) -> Uuid {
    Uuid::from_u64_pair(0x4e42_7461_6273_0000, ((content.index() as u64) << 32) | content.generation() as u64)
}

/// The text of a registered entry (list item, tab, tree or menu item).
fn entry_text(gui: &GuiState, handle: Handle<UiNode>) -> String {
    gui.by_handle.get(&handle).and_then(|&e| gui.get(e).ok()).map(|e| e.text.clone()).unwrap_or_default()
}

/// Registers a sub-widget (entry) with its text; returns its program handle.
fn register(gui: &mut GuiState, kind: &'static str, handle: Handle<UiNode>, text: &str) -> i32 {
    let mut item = GuiItem::new(kind, handle);
    item.text = text.to_string();
    gui.add(item)
}

fn forget(gui: &mut GuiState, handle: Handle<UiNode>) {
    if let Some(e) = gui.by_handle.remove(&handle) {
        if let Some(slot) = gui.widgets.get_mut(e as usize - 1) {
            *slot = None;
        }
    }
}

/// One entry of a list or drop-down: text on a decorator that shows hover and selection.
fn make_entry(ctx: &mut BuildContext, text: &str) -> Handle<UiNode> {
    let margin = Thickness { left: crate::gui::px(4.0), top: crate::gui::px(2.0), right: crate::gui::px(4.0), bottom: crate::gui::px(2.0) };
    let label = TextBuilder::new(WidgetBuilder::new().with_margin(margin)).with_text(text).build(ctx);
    DecoratorBuilder::new(BorderBuilder::new(WidgetBuilder::new().with_child(label))).build(ctx).to_base()
}

fn split_items(s: &str) -> Vec<String> {
    if s.is_empty() {
        Vec::new()
    } else {
        s.split('|').map(str::to_string).collect()
    }
}

fn list_of(h: &Host3D, id: i32) -> Result<GuiItem, String> {
    let item = h.world.gui.get(id)?;
    match item.kind {
        "dropdown" | "list" => Ok(item.clone()),
        _ => Err(format!("GUI widget {id} is not a list or drop-down")),
    }
}

/// `GuiDropDown` / `GuiList`. A drop-down starts with its first item selected.
fn new_list(h: &mut Host3D, kind: &'static str, items: &str, parent: i32) -> Result<i32, String> {
    let texts = split_items(items);
    let drop = kind == "dropdown";
    let mut entries = Vec::new();
    let id = h.gui_add(kind, parent, |ctx| {
        entries = texts.iter().map(|t| make_entry(ctx, t)).collect();
        let w = if drop {
            let first = (!entries.is_empty()).then_some(0);
            DropdownListBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(120.0, 22.0)))
                .with_items(entries.clone())
                .with_opt_selected(first)
                .with_close_on_selection(true)
                .build(ctx)
                .to_base()
        } else {
            ListViewBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(120.0, 60.0))).with_items(entries.clone()).build(ctx).to_base()
        };
        (w, Handle::NONE)
    })?;
    for (e, t) in entries.iter().zip(&texts) {
        register(&mut h.world.gui, "listitem", *e, t);
    }
    h.world.gui.get_mut(id)?.items = entries;
    if drop && !texts.is_empty() {
        record(&mut h.world.gui, id, 0, false);
    }
    Ok(id)
}

fn add_item(h: &mut Host3D, id: i32, text: &str) -> Result<(), String> {
    let item = list_of(h, id)?;
    let ui = h.ui()?;
    let entry = make_entry(&mut ui.build_ctx(), text);
    if item.kind == "dropdown" {
        ui.send(item.handle, DropdownListMessage::AddItem(entry));
    } else {
        ui.send(item.handle, ListViewMessage::AddItem(entry));
    }
    register(&mut h.world.gui, "listitem", entry, text);
    h.world.gui.get_mut(id)?.items.push(entry);
    Ok(())
}

fn remove_item(h: &mut Host3D, id: i32, index: i32) -> Result<(), String> {
    let item = list_of(h, id)?;
    let i = usize::try_from(index).ok().filter(|&i| i < item.items.len()).ok_or("List item does not exist")?;
    let mut texts: Vec<String> = item.items.iter().map(|&e| entry_text(&h.world.gui, e)).collect();
    texts.remove(i);
    let sel = match item.selected {
        s if s == index => -1,
        s if s > index => s - 1,
        s => s,
    };
    let ui = h.ui()?;
    let entries = if item.kind == "dropdown" {
        // a drop-down cannot remove one item: give it a new set
        let ctx = &mut ui.build_ctx();
        let new: Vec<_> = texts.iter().map(|t| make_entry(ctx, t)).collect();
        ui.send(item.handle, DropdownListMessage::Items(new.clone()));
        for &e in &item.items {
            forget(&mut h.world.gui, e);
        }
        for (e, t) in new.iter().zip(&texts) {
            register(&mut h.world.gui, "listitem", *e, t);
        }
        new
    } else {
        ui.send(item.handle, ListViewMessage::RemoveItem(item.items[i]));
        forget(&mut h.world.gui, item.items[i]);
        let mut rest = item.items.clone();
        rest.remove(i);
        rest
    };
    let gui = &mut h.world.gui;
    gui.get_mut(id)?.items = entries;
    quietly(h, id, |h| {
        flush(h)?;
        set_selected(h, id, sel)
    })
}

fn clear_items(h: &mut Host3D, id: i32) -> Result<(), String> {
    let item = list_of(h, id)?;
    let ui = h.ui()?;
    if item.kind == "dropdown" {
        ui.send(item.handle, DropdownListMessage::Items(Vec::new()));
    } else {
        ui.send(item.handle, ListViewMessage::Items(Vec::new()));
    }
    for &e in &item.items {
        forget(&mut h.world.gui, e);
    }
    h.world.gui.get_mut(id)?.items.clear();
    quietly(h, id, |h| {
        flush(h)?;
        set_selected(h, id, -1)
    })
}

/// `SetGuiSelected`: an index, or for a tree a tree item's handle (0 = none).
/// Not a change for `GuiChanged`.
fn set_selected(h: &mut Host3D, id: i32, index: i32) -> Result<(), String> {
    let item = h.world.gui.get(id)?.clone();
    let pick = usize::try_from(index).ok().filter(|&i| i < item.items.len());
    let sel = if item.kind == "tree" { index } else { pick.map_or(-1, |i| i as i32) };
    let mut tree_sel = Vec::new();
    match item.kind {
        "dropdown" | "list" => {}
        "tabs" if pick.is_none() => return Err("Tab does not exist".into()),
        "tabs" => {}
        "tree" if index != 0 => {
            let t = h.world.gui.get(index)?;
            if t.kind != "treeitem" {
                return Err(format!("GUI widget {index} is not a tree item"));
            }
            tree_sel.push(t.handle.to_variant::<Tree>());
        }
        "tree" => {}
        _ => return Err(format!("GUI widget {id} has no selection")),
    }
    quietly(h, id, |h| {
        // recorded first, so the widget's answer is not counted as the user's
        record(&mut h.world.gui, id, sel, false);
        let ui = h.ui()?;
        match item.kind {
            "dropdown" => ui.send(item.handle, DropdownListMessage::Selection(pick)),
            "list" => ui.send(item.handle, ListViewMessage::Selection(pick.into_iter().collect())),
            "tabs" => ui.send(item.handle, TabControlMessage::ActiveTab(pick.map(|i| tab_uuid(item.items[i])))),
            _ => ui.send(item.handle, TreeRootMessage::Select(tree_sel)),
        }
        flush(h)?;
        let gui = &mut h.world.gui;
        gui.get_mut(id)?.selected = i32::MIN;
        record(gui, id, sel, false);
        Ok(())
    })
}

/// Runs a change made by the program: whatever the widget reports back
/// while it settles is not counted by `GuiChanged`.
fn quietly(h: &mut Host3D, id: i32, f: impl FnOnce(&mut Host3D) -> Result<(), String>) -> Result<(), String> {
    let changed = h.world.gui.get(id)?.changed;
    let r = f(h);
    if let Ok(item) = h.world.gui.get_mut(id) {
        item.changed = changed;
    }
    r
}

/// Handles the UI's queued messages now, as the engine would, so that a
/// series of changes in one frame is applied in order. (Fyrox's drop-down
/// and list echo selections to each other; queued changes would overtake.)
fn flush(h: &mut Host3D) -> Result<(), String> {
    let ui = h.ui.as_deref_mut().ok_or("Graphics must be called first")?;
    while let Some(m) = ui.poll_message() {
        h.world.gui.on_message(&m);
    }
    Ok(())
}

fn item_text(h: &Host3D, id: i32, index: i32) -> Result<Value, String> {
    let item = h.world.gui.get(id)?;
    let e = usize::try_from(index).ok().and_then(|i| item.items.get(i)).ok_or("List item does not exist")?;
    Ok(Value::Str(entry_text(&h.world.gui, *e).as_str().into()))
}

/// `GuiTreeItem`: an item in a tree or under another tree item.
fn tree_item(h: &mut Host3D, parent: i32, text: &str) -> Result<i32, String> {
    let p = h.world.gui.get(parent)?.clone();
    if p.kind != "tree" && p.kind != "treeitem" {
        return Err(format!("GUI widget {parent} is not a tree or tree item"));
    }
    let ui = h.ui()?;
    let t = {
        let ctx = &mut ui.build_ctx();
        let label = TextBuilder::new(WidgetBuilder::new().with_margin(Thickness::left(crate::gui::px(2.0)))).with_text(text).build(ctx);
        TreeBuilder::new(WidgetBuilder::new()).with_content(label).build(ctx)
    };
    if p.kind == "tree" {
        ui.send(p.handle, TreeRootMessage::AddItem(t));
    } else {
        ui.send(p.handle, TreeMessage::AddItem(t));
    }
    let gui = &mut h.world.gui;
    gui.get_mut(parent)?.items.push(t.to_base());
    Ok(register(gui, "treeitem", t.to_base(), text))
}

fn expand(h: &mut Host3D, id: i32, open: bool) -> Result<(), String> {
    let item = h.world.gui.get(id)?;
    if item.kind != "treeitem" {
        return Err(format!("GUI widget {id} is not a tree item"));
    }
    h.gui_send(id, TreeMessage::Expand { expand: open, expansion_strategy: TreeExpansionStrategy::Direct })
}

/// `GuiTab`: a page of a tab control; widgets put in it are stacked in a column.
fn new_tab(h: &mut Host3D, tabs: i32, title: &str) -> Result<i32, String> {
    let p = h.world.gui.get(tabs)?.clone();
    if p.kind != "tabs" {
        return Err(format!("GUI widget {tabs} is not a tab control"));
    }
    let first = p.items.is_empty();
    let ui = h.ui()?;
    let (header, content) = {
        let ctx = &mut ui.build_ctx();
        let margin = Thickness { left: crate::gui::px(6.0), top: crate::gui::px(3.0), right: crate::gui::px(6.0), bottom: crate::gui::px(3.0) };
        (TextBuilder::new(WidgetBuilder::new().with_margin(margin)).with_text(title).build(ctx).to_base(), content_panel(ctx))
    };
    let uuid = tab_uuid(content);
    ui.send(p.handle, TabControlMessage::AddTab(TabDefinition { uuid, header, content, can_be_closed: false, user_data: None }));
    if first {
        ui.send(p.handle, TabControlMessage::ActiveTab(Some(uuid)));
    } else {
        ui.send(content, WidgetMessage::Visibility(false));
    }
    let gui = &mut h.world.gui;
    let id = register(gui, "tab", content, title);
    let tab = gui.get_mut(id)?;
    tab.content = content;
    tab.items.push(header);
    gui.get_mut(tabs)?.items.push(content);
    if first {
        record(gui, tabs, 0, false);
    }
    Ok(id)
}

/// `GuiMenuItem`: an item on a menu bar or in another item's sub-menu.
fn menu_item(h: &mut Host3D, parent: i32, text: &str) -> Result<i32, String> {
    let p = h.world.gui.get(parent)?.clone();
    if p.kind != "menu" && p.kind != "menuitem" {
        return Err(format!("GUI widget {parent} is not a menu or menu item"));
    }
    let top = p.kind == "menu";
    let ui = h.ui()?;
    let item = {
        let ctx = &mut ui.build_ctx();
        let (content, size) = if top { (MenuItemContent::text_centered(text), crate::gui::px2(40.0, 22.0)) } else { (MenuItemContent::text(text), crate::gui::px2(120.0, 22.0)) };
        let item = MenuItemBuilder::new(WidgetBuilder::new().with_min_size(size)).with_content(content).build(ctx);
        if top {
            ctx[item].placement.set_value_and_mark_modified(MenuItemPlacement::Bottom);
        }
        item
    };
    if top {
        // a menu bar has no message to add items: put it in the bar's row
        let panel = menu_panel(ui, p.handle).ok_or("Menu bar missing")?;
        ui.send(item, WidgetMessage::LinkWith(panel));
    } else {
        ui.send(p.handle, MenuItemMessage::AddItem(item));
    }
    let gui = &mut h.world.gui;
    gui.get_mut(parent)?.items.push(item.to_base());
    Ok(register(gui, "menuitem", item.to_base(), text))
}

/// The row a menu bar keeps its items in (menu → border → stack panel).
fn menu_panel(ui: &UserInterface, menu: Handle<UiNode>) -> Option<Handle<UiNode>> {
    let back = *ui.try_get_node(menu).ok()?.children().first()?;
    ui.try_get_node(back).ok()?.children().first().copied()
}

pub static COMMANDS: &[(&str, Cmd)] = &[
    // ---- GUI (ext): lists ("a|b|c" items)
    ("%GuiDropDown$items%parent=0", |h, a, _| int(new_list(h, "dropdown", &a[0].str(), a[1].int())?)),
    ("%GuiList$items%parent=0", |h, a, _| int(new_list(h, "list", &a[0].str(), a[1].int())?)),
    ("GuiAddItem%widget$text", |h, a, _| {
        add_item(h, a[0].int(), &a[1].str())?;
        VOID
    }),
    ("GuiRemoveItem%widget%index", |h, a, _| {
        remove_item(h, a[0].int(), a[1].int())?;
        VOID
    }),
    ("GuiClearItems%widget", |h, a, _| {
        clear_items(h, a[0].int())?;
        VOID
    }),
    ("%GuiCountItems%widget", |h, a, _| int(h.world.gui.get(a[0].int())?.items.len() as i32)),
    ("$GuiItemText%widget%index", |h, a, _| item_text(h, a[0].int(), a[1].int())),
    ("%GuiSelected%widget", |h, a, _| int(h.world.gui.get(a[0].int())?.selected)),
    ("SetGuiSelected%widget%index", |h, a, _| {
        set_selected(h, a[0].int(), a[1].int())?;
        VOID
    }),
    // ---- GUI (ext): trees
    ("%GuiTree%parent=0", |h, a, _| {
        let id = h.gui_add("tree", a[0].int(), |ctx| (TreeRootBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(120.0, 60.0))).build(ctx).to_base(), Handle::NONE))?;
        h.world.gui.get_mut(id)?.selected = 0;
        int(id)
    }),
    ("%GuiTreeItem%parent$text", |h, a, _| int(tree_item(h, a[0].int(), &a[1].str())?)),
    ("GuiExpand%item%expand", |h, a, _| {
        expand(h, a[0].int(), a[1].int() != 0)?;
        VOID
    }),
    // ---- GUI (ext): tabs
    ("%GuiTabs%parent=0", |h, a, _| {
        int(h.gui_add("tabs", a[0].int(), |ctx| (TabControlBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(160.0, 80.0))).build(ctx).to_base(), Handle::NONE))?)
    }),
    ("%GuiTab%tabs$title", |h, a, _| int(new_tab(h, a[0].int(), &a[1].str())?)),
    // ---- GUI (ext): menus
    ("%GuiMenu%parent=0", |h, a, _| int(h.gui_add("menu", a[0].int(), |ctx| (MenuBuilder::new(WidgetBuilder::new()).build(ctx).to_base(), Handle::NONE))?)),
    ("%GuiMenuItem%parent$text", |h, a, _| int(menu_item(h, a[0].int(), &a[1].str())?)),
];
