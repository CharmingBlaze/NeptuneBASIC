// Neptune: this file is not part of Fyrox. It holds what Neptune Level
// Editor adds to the editor's own code, so that the changes to Fyrox's files
// stay one line each (search for `Neptune:`).

//! Neptune Level Editor's changes to the editor.

use crate::fyrox::{
    core::{
        algebra::{UnitQuaternion, Vector2, Vector3},
        math::Rect,
        pool::Handle,
    },
    graph::SceneGraph,
    gui::{stack_panel::StackPanel, text::Text, widget::WidgetMessage, UiNode, UserInterface},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

static FULL_INSPECTOR: AtomicBool = AtomicBool::new(false);

/// Space above and below each row of the object list.
pub const ROW_GAP: f32 = 2.0;

/// Whether the editor shows everything Fyrox has: every setting in the
/// Inspector (not only those a level designer uses) and the engine's
/// built-in assets in the asset browser.
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

// ---------------------------------------------------------------- quad view
//
// Neptune Level Editor can split the 3D view into four, as Hammer and
// Radiant do: the 3D view and flat views from the top, the front and the
// side. The view under the mouse is drawn by the editor's own camera, so
// every tool works in it unchanged; Neptune's `quad.rs` (in
// crates/neptune-editor) moves the other three to cameras of their own.

/// Which way the editor's camera looks when it is orthographic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OrthoView {
    /// Fyrox's own 2D view: along +Z, kept close to the XY plane.
    #[default]
    Front2D,
    /// Straight down, the ground plane flat on the screen.
    Top,
    /// Along +Z, like `Front2D` but seeing the whole level.
    Front,
    /// Along -X.
    Side,
}

impl OrthoView {
    /// The camera's turn: it looks along its +Z.
    pub fn rotation(self) -> UnitQuaternion<f32> {
        match self {
            OrthoView::Front2D | OrthoView::Front => UnitQuaternion::identity(),
            OrthoView::Top => UnitQuaternion::from_axis_angle(&Vector3::x_axis(), 90f32.to_radians()),
            OrthoView::Side => UnitQuaternion::from_axis_angle(&Vector3::y_axis(), -90f32.to_radians()),
        }
    }

    /// The grid shader's plane (0 XZ, 1 XY, 2 ZY) and the indices of the
    /// snap steps (x 0, y 1, z 2) along its two directions.
    pub fn grid(self) -> (i32, [usize; 2]) {
        match self {
            OrthoView::Front2D | OrthoView::Front => (1, [0, 1]),
            OrthoView::Top => (0, [0, 2]),
            OrthoView::Side => (2, [2, 1]),
        }
    }

    /// Where an orthographic camera goes: moved in its view plane by
    /// `step`, and pulled back half the far distance so that it sees the
    /// whole level in front of and behind the plane through the origin.
    pub fn place(self, position: Vector3<f32>, step: Vector2<f32>, z_far: f32) -> Vector3<f32> {
        if self == OrthoView::Front2D {
            // what Fyrox's editor did before Neptune
            return Vector3::new(position.x + step.x, position.y + step.y, crate::camera::DEFAULT_Z_OFFSET);
        }
        let r = self.rotation();
        let (side, up, look) = (r * Vector3::x(), r * Vector3::y(), r * Vector3::z());
        let moved = position + side * step.x + up * step.y;
        moved - look * moved.dot(&look) - look * (z_far * 0.5)
    }
}

/// The grid's plane and scale for the editor's camera.
pub fn grid_for(perspective: bool, view: OrthoView, steps: [f32; 3], snapping: bool) -> (i32, Vector2<f32>) {
    let (plane, axes) = if perspective { (0, [0, 2]) } else { view.grid() };
    let per = |s: f32| if s == 0.0 { 1.0 } else { 1.0 / s };
    let scale = if snapping { Vector2::new(per(steps[axes[0]]), per(steps[axes[1]])) } else { Vector2::repeat(1.0) };
    (plane, scale)
}

/// Names start with this on the quad view's cameras and grids, which the
/// editor leaves switched on and with their own render masks.
pub const QUAD_NODE: &str = "NeptuneQuad";

pub fn is_quad_node(node: &crate::fyrox::scene::node::Node) -> bool {
    node.name().starts_with(QUAD_NODE)
}

/// The part of the 3D view the mouse is in, as fractions of it from its top
/// left (x, y, width, height); `None` when the view is not split.
static QUAD_PART: Mutex<Option<[f32; 4]>> = Mutex::new(None);

pub fn set_quad_part(part: Option<[f32; 4]>) {
    *QUAD_PART.lock().unwrap_or_else(|e| e.into_inner()) = part;
}

/// The 3D view's bounds as the editor's tools should see them: when the
/// view is split, the bounds of the part the mouse is in, which its own
/// camera draws whole (Neptune's `quad.rs` gives each part a picture).
pub fn quad_bounds(frame: Rect<f32>) -> Rect<f32> {
    match *QUAD_PART.lock().unwrap_or_else(|e| e.into_inner()) {
        Some([x, y, w, h]) => Rect::new(
            frame.position.x + x * frame.size.x,
            frame.position.y + y * frame.size.y,
            w * frame.size.x,
            h * frame.size.y,
        ),
        None => frame,
    }
}
