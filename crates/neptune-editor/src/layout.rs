//! Where the editor's panels are when it first opens.
//!
//! Fyrox's own layout gives the 3D view half the window's width and puts
//! three panels of little use to a level designer along the bottom. This
//! one is arranged like Blender: the view fills the left of the window, the
//! object list is at the top right with the Neptune panel and the Inspector
//! under it, and the
//! asset browser, the log and the undo list share one strip under the view
//! as tabs. The audio panel starts closed (the View menu opens it).

use fyroxed_base::fyrox::{
    core::{algebra::Vector2, ImmutableString},
    gui::{
        dock::config::{
            DockingManagerLayoutDescriptor, FloatingWindowDescriptor, MultiWindowDescriptor, SplitTilesDescriptor, TileContentDescriptor,
            TileDescriptor,
        },
        Orientation,
    },
};

/// The panels by the names the editor gives their windows.
pub const OBJECTS: &str = "WorldOutliner";
pub const VIEW: &str = "SceneViewer";
pub const INSPECTOR: &str = "Inspector";
pub const NEPTUNE: &str = crate::panel::NAME;
pub const BOTTOM: [&str; 3] = ["AssetBrowser", "LogPanel", "CommandStackPanel"];
pub const CLOSED: [&str; 1] = ["AudioPanel"];

/// Share of the window's width the 3D view gets (Fyrox's layout gives it half).
const VIEW_WIDTH: f32 = 0.78;
/// Share of the left column's height the 3D view gets.
const VIEW_HEIGHT: f32 = 0.78;
/// Share of the right column's height the object list gets.
const OBJECTS_HEIGHT: f32 = 0.30;
/// Share of the rest the Neptune panel gets (the Inspector has the rest).
const NEPTUNE_HEIGHT: f32 = 0.36;

fn tile(content: TileContentDescriptor) -> Box<TileDescriptor> {
    Box::new(TileDescriptor { content })
}

fn window(name: &str) -> Box<TileDescriptor> {
    tile(TileContentDescriptor::Window(ImmutableString::new(name)))
}

fn split(orientation: Orientation, splitter: f32, first: Box<TileDescriptor>, second: Box<TileDescriptor>) -> Box<TileDescriptor> {
    tile(TileContentDescriptor::SplitTiles(SplitTilesDescriptor { splitter, orientation, children: [first, second] }))
}

fn tabs() -> Box<TileDescriptor> {
    let tabs = MultiWindowDescriptor { index: 0, names: BOTTOM.iter().map(ImmutableString::new).collect() };
    tile(TileContentDescriptor::MultiWindow(tabs))
}

fn with_closed(root: TileDescriptor) -> DockingManagerLayoutDescriptor {
    let closed = |name: &&str| FloatingWindowDescriptor {
        name: ImmutableString::new(name),
        position: Vector2::new(200.0, 200.0),
        size: Vector2::new(0.0, 0.0),
        is_open: false,
    };
    DockingManagerLayoutDescriptor { floating_windows: CLOSED.iter().map(closed).collect(), root_tile_descriptor: Some(root) }
}

/// Neptune's layout.
pub fn neptune() -> DockingManagerLayoutDescriptor {
    let left = split(Orientation::Vertical, VIEW_HEIGHT, window(VIEW), tabs());
    let under = split(Orientation::Vertical, NEPTUNE_HEIGHT, window(NEPTUNE), window(INSPECTOR));
    let right = split(Orientation::Vertical, OBJECTS_HEIGHT, window(OBJECTS), under);
    with_closed(*split(Orientation::Horizontal, VIEW_WIDTH, left, right))
}

/// Neptune's second layout, from before the Neptune panel.
pub fn before_panel() -> DockingManagerLayoutDescriptor {
    let left = split(Orientation::Vertical, VIEW_HEIGHT, window(VIEW), tabs());
    let right = split(Orientation::Vertical, 0.34, window(OBJECTS), window(INSPECTOR));
    with_closed(*split(Orientation::Horizontal, VIEW_WIDTH, left, right))
}

/// Neptune's first layout (object list left, Inspector right), which
/// settings saved by an earlier version of the editor may hold untouched.
pub fn earlier() -> DockingManagerLayoutDescriptor {
    let middle = split(Orientation::Vertical, 0.78, window(VIEW), tabs());
    let right = split(Orientation::Horizontal, 0.80, middle, window(INSPECTOR));
    with_closed(*split(Orientation::Horizontal, 0.15, window(OBJECTS), right))
}

/// Whether saved settings should get Neptune's layout: they have none, or
/// one of Neptune's that nobody rearranged.
pub fn replaces(saved: Option<&DockingManagerLayoutDescriptor>) -> bool {
    saved.is_none_or(|layout| *layout == earlier() || *layout == before_panel())
}

/// Whether a layout made by the editor has every panel Neptune's places, so
/// that replacing it loses none.
pub fn covers(theirs: &DockingManagerLayoutDescriptor) -> bool {
    let ours = neptune();
    [OBJECTS, VIEW, INSPECTOR].iter().chain(&BOTTOM).all(|name| theirs.has_window(name) && ours.has_window(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_panel_has_a_place() {
        let layout = neptune();
        for name in [OBJECTS, VIEW, INSPECTOR, NEPTUNE].iter().chain(&BOTTOM) {
            assert!(layout.has_window(name), "{name}");
        }
        assert!(!layout.has_window("AudioPanel"));
        assert!(layout.floating_windows.iter().all(|w| !w.is_open));
    }

    #[test]
    fn a_layout_someone_arranged_is_kept() {
        assert!(replaces(None));
        assert!(replaces(Some(&earlier())));
        assert!(replaces(Some(&before_panel())));
        assert!(!replaces(Some(&neptune())));
        let mut own = neptune();
        own.floating_windows.clear();
        assert!(!replaces(Some(&own)));
    }
}
