// Neptune: this file is not part of Fyrox. It holds what Neptune Level
// Editor adds to the editor's own code, so that the changes to Fyrox's files
// stay one line each (search for `Neptune:`).

//! Neptune Level Editor's changes to the editor.

use crate::fyrox::{
    core::pool::Handle,
    graph::SceneGraph,
    gui::{stack_panel::StackPanel, text::Text, widget::WidgetMessage, UiNode, UserInterface},
};
use std::sync::atomic::{AtomicBool, Ordering};

static FULL_INSPECTOR: AtomicBool = AtomicBool::new(false);

/// Whether the Inspector shows every setting Fyrox has, not only those a
/// level designer uses.
pub fn full_inspector() -> bool {
    FULL_INSPECTOR.load(Ordering::Relaxed)
}

/// Chooses between the short Inspector (the default) and the full one.
pub fn set_full_inspector(full: bool) {
    FULL_INSPECTOR.store(full, Ordering::Relaxed);
}

/// Settings of every scene object that a level never needs: the engine's
/// own bookkeeping, and transform parts that only imported animations use.
/// They are matched by the name the Inspector shows.
pub const ENGINE_SETTINGS: &[&str] = &[
    "Pre Rotation",
    "Post Rotation",
    "Rotation Offset",
    "Rotation Pivot",
    "Scaling Offset",
    "Scaling Pivot",
    "Render Mask",
    "Lifetime",
    "Lod Group",
    "Tag",
    "Frustum Culling",
    "Is Resource Instance Root",
    "Global Visibility",
    "Global Transform",
    "Global Enabled",
    "Instance Id",
    "Scripts",
];

/// Whether the short Inspector leaves a setting out.
pub fn is_engine_setting(name: &str) -> bool {
    ENGINE_SETTINGS.contains(&name)
}

/// `fyrox_impl::scene::mesh::Mesh` as `Mesh`.
pub fn short_type_name(full: &str) -> &str {
    let plain = full.split('<').next().unwrap_or(full);
    plain.rsplit("::").next().unwrap_or(plain)
}

/// Hides the rows of the engine's own settings in a newly built Inspector.
/// A row is a child of one of the Inspector's lists (stack panels), and its
/// label is a text somewhere inside it.
pub fn simplify_inspector(ui: &UserInterface, root: Handle<UiNode>) {
    if full_inspector() {
        return;
    }
    let mut rows = Vec::new();
    for (handle, node) in ui.traverse_iter(root) {
        let Some(label) = node.query_component::<Text>() else { continue };
        if !is_engine_setting(&label.text()) {
            continue;
        }
        let mut row = handle;
        while let Ok(parent) = ui.try_get_node(ui.node(row).parent()) {
            if parent.has_component::<StackPanel>() {
                rows.push(row);
                break;
            }
            row = ui.node(row).parent();
        }
    }
    for row in rows {
        ui.send(row, WidgetMessage::Visibility(false));
    }
}

// Tested in crates/neptune-editor (this crate is not a workspace member).
