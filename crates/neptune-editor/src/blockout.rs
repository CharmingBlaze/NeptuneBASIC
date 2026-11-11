//! Block-out tools, as level editors from Quake's onwards have them:
//!
//! | Key | What it does |
//! |---|---|
//! | Shift+B | The block tool: drag a box in any view; letting go makes it a solid block |
//! | `[`, `]` | Grid half or twice as fine (1/16 to 64) |
//! | Arrow keys | Move the selection one grid step across the view under the mouse |
//! | Page Up, Page Down | Move the selection one grid step towards or away from that view (up and down in the 3D view) |
//!
//! A block is drawn in the plane the view looks at: in the flat views of the
//! quad view the box's two sides come from the drag and its depth from the
//! last block made (TrenchBroom does the same); in the 3D view the box lies
//! on the ground and takes its height from the last block. Blocks are cube
//! meshes named `col_Block1`, `col_Block2` and so on, so `LoadLevelFile`
//! makes them solid, and the Neptune panel can change that.

use fyroxed_base::{
    command::{Command, CommandGroup},
    fyrox::{
        asset::untyped::ResourceKind,
        core::{
            algebra::{Matrix4, Vector2, Vector3},
            color::Color,
            math::{aabb::AxisAlignedBoundingBox, plane::Plane, ray::Ray},
            pool::Handle,
            type_traits::prelude::*,
            uuid::{uuid, Uuid},
        },
        engine::{ApplicationLoopController, Engine},
        graph::SceneGraph,
        gui::{
            button::Button,
            message::{KeyCode, UiMessage},
            text::{Text, TextBuilder, TextMessage},
            widget::{WidgetBuilder, WidgetMessage},
            BuildContext, HorizontalAlignment, Thickness, VerticalAlignment,
        },
        material::{Material, MaterialResource},
        scene::{
            base::BaseBuilder,
            camera::Camera,
            mesh::{
                surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
                MeshBuilder,
            },
            node::Node,
            transform::TransformBuilder,
        },
    },
    interaction::{make_interaction_mode_button, InteractionMode},
    neptune::OrthoView,
    plugin::EditorPlugin,
    scene::{commands::graph::{AddNodeCommand, MoveNodeCommand}, controller::SceneController, GameScene, Selection},
    settings::Settings,
    Editor, Message,
};

/// Grid steps `[` and `]` go through.
pub const GRID_STEPS: [f32; 11] = [0.0625, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0];

/// The next grid step from `current`, finer (`-1`) or coarser (`1`).
pub fn next_grid_step(current: f32, direction: i32) -> f32 {
    // the nearest step in the list, so steps typed in the settings join it
    let here = GRID_STEPS
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1.log2() - current.max(1e-6).log2()).abs().total_cmp(&(b.1.log2() - current.max(1e-6).log2()).abs()))
        .map_or(4, |(i, _)| i);
    let to = (here as i32 + direction).clamp(0, GRID_STEPS.len() as i32 - 1);
    GRID_STEPS[to as usize]
}

/// `v` on the grid of `step` (unchanged when snapping is off).
pub fn snap(v: f32, step: f32, on: bool) -> f32 {
    if on && step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

/// The axis (0 x, 1 y, 2 z) a direction is closest to, and its sign.
pub fn main_axis(d: Vector3<f32>) -> (usize, f32) {
    let a = d.abs();
    let axis = if a.x >= a.y && a.x >= a.z {
        0
    } else if a.y >= a.z {
        1
    } else {
        2
    };
    (axis, d[axis].signum())
}

/// The axis a view looks along: the block's depth comes from elsewhere.
fn depth_axis(view: Option<OrthoView>) -> usize {
    match view {
        Some(OrthoView::Top) | None => 1,
        Some(OrthoView::Side) => 0,
        Some(OrthoView::Front) | Some(OrthoView::Front2D) => 2,
    }
}

/// The box between two corners in a view's plane, with `depth` along the
/// view's depth axis.
pub fn block_box(a: Vector3<f32>, b: Vector3<f32>, axis: usize, depth: (f32, f32)) -> AxisAlignedBoundingBox {
    let mut min = a.inf(&b);
    let mut max = a.sup(&b);
    min[axis] = depth.0.min(depth.1);
    max[axis] = depth.0.max(depth.1);
    AxisAlignedBoundingBox::from_min_max(min, max)
}

/// Whether a box is big enough to make a block of.
fn is_block(b: &AxisAlignedBoundingBox) -> bool {
    let size = b.max - b.min;
    size.x > 1e-3 && size.y > 1e-3 && size.z > 1e-3
}

/// The first `col_BlockN` name not in use.
pub fn block_name(taken: impl Fn(&str) -> bool) -> String {
    (1..).map(|n| format!("col_Block{n}")).find(|n| !taken(n)).unwrap_or_default()
}

/// A grey block filling `b`.
fn make_block(b: &AxisAlignedBoundingBox, name: &str) -> Node {
    let mut grey = Material::standard();
    grey.set_property("diffuseColor", Color::opaque(150, 150, 158));
    let grey = MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, grey);
    let size = b.max - b.min;
    let cube = SurfaceData::make_cube(Matrix4::new_nonuniform_scaling(&size));
    let place = TransformBuilder::new().with_local_position(b.center()).build();
    MeshBuilder::new(BaseBuilder::new().with_name(name).with_local_transform(place))
        .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(cube)).with_material(grey).build()])
        .build_node()
}

/// The editor camera's ray through a point of the view, and which way the
/// view looks (`None` for the 3D view).
fn ray_at(controller: &dyn SceneController, engine: &Engine, pos: Vector2<f32>, size: Vector2<f32>) -> Option<(Ray, Option<OrthoView>)> {
    let game = controller.downcast_ref::<GameScene>()?;
    let graph = &engine.scenes.try_get(game.scene).ok()?.graph;
    let camera = graph.try_get(game.camera_controller.camera).ok()?;
    let view = (!camera.projection().is_perspective()).then_some(game.camera_controller.neptune_view);
    Some((camera.make_ray(pos, size), view))
}

/// The grid steps of the editor's settings, by axis, and whether to snap.
fn grid(settings: &Settings) -> ([f32; 3], bool) {
    let s = &settings.move_mode_settings;
    ([s.x_snap_step, s.y_snap_step, s.z_snap_step], s.grid_snapping)
}

/// The block tool.
#[derive(TypeUuidProvider)]
#[type_uuid(id = "6c3f8c52-6b8e-4f53-9d36-2f0e1b7a9c11")]
pub struct BlockMode {
    /// Where the drag started, on the drawing plane, and the view's depth axis.
    start: Option<(Vector3<f32>, usize)>,
    /// The box being dragged out.
    preview: Option<AxisAlignedBoundingBox>,
    /// The last block's extent along each axis, given to the next block's depth.
    extent: [(f32, f32); 3],
}

impl Default for BlockMode {
    fn default() -> Self {
        // a first block from the top stands 2 high; from the front or side, 2 deep
        BlockMode { start: None, preview: None, extent: [(-1.0, 1.0), (0.0, 2.0), (-1.0, 1.0)] }
    }
}

impl BlockMode {
    /// The point under the mouse on the drawing plane, on the grid.
    fn point(&self, controller: &dyn SceneController, engine: &Engine, pos: Vector2<f32>, size: Vector2<f32>, settings: &Settings, axis: usize) -> Option<Vector3<f32>> {
        let (ray, _) = ray_at(controller, engine, pos, size)?;
        let mut normal = Vector3::zeros();
        normal[axis] = 1.0;
        // flat views draw through the middle of the last block's depth; the
        // 3D view on the ground the last block stood on
        let mut at = Vector3::zeros();
        let (lo, hi) = self.extent[axis];
        at[axis] = if axis == 1 { lo } else { (lo + hi) / 2.0 };
        let plane = Plane::from_normal_and_point(&normal, &at)?;
        let mut p = ray.plane_intersection_point(&plane)?;
        let (steps, on) = grid(settings);
        for i in 0..3 {
            if i != axis {
                p[i] = snap(p[i], steps[i], on);
            }
        }
        Some(p)
    }
}

impl InteractionMode for BlockMode {
    fn on_left_mouse_button_down(
        &mut self,
        _: &Selection,
        controller: &mut dyn SceneController,
        engine: &mut Engine,
        pos: Vector2<f32>,
        size: Vector2<f32>,
        settings: &Settings,
    ) {
        let Some((_, view)) = ray_at(controller, engine, pos, size) else { return };
        let axis = depth_axis(view);
        self.start = self.point(controller, engine, pos, size, settings, axis).map(|p| (p, axis));
        self.preview = None;
    }

    fn on_mouse_move(
        &mut self,
        _: Vector2<f32>,
        pos: Vector2<f32>,
        _: &Selection,
        controller: &mut dyn SceneController,
        engine: &mut Engine,
        size: Vector2<f32>,
        settings: &Settings,
    ) {
        let Some((start, axis)) = self.start else { return };
        if let Some(p) = self.point(controller, engine, pos, size, settings, axis) {
            self.preview = Some(block_box(start, p, axis, self.extent[axis]));
        }
    }

    fn on_left_mouse_button_up(
        &mut self,
        _: &Selection,
        controller: &mut dyn SceneController,
        engine: &mut Engine,
        _: Vector2<f32>,
        _: Vector2<f32>,
        _: &Settings,
    ) {
        self.start = None;
        let Some(b) = self.preview.take().filter(is_block) else { return };
        let Some(game) = controller.downcast_ref::<GameScene>() else { return };
        let Ok(scene) = engine.scenes.try_get(game.scene) else { return };
        let graph = &scene.graph;
        let name = block_name(|n| graph.linear_iter().any(|node| node.name() == n));
        for i in 0..3 {
            self.extent[i] = (b.min[i], b.max[i]);
        }
        let add = AddNodeCommand::new(make_block(&b, &name), Handle::NONE, true).keeping_local_transform();
        game.sender.do_command(add);
    }

    fn update(&mut self, _: &Selection, controller: &mut dyn SceneController, engine: &mut Engine, _: &Settings) {
        let (Some(b), Some(game)) = (&self.preview, controller.downcast_ref::<GameScene>()) else { return };
        if let Ok(scene) = engine.scenes.try_get_mut(game.scene) {
            scene.drawing_context.draw_aabb(b, Color::opaque(255, 200, 40));
        }
    }

    fn deactivate(&mut self, _: &dyn SceneController, _: &mut Engine) {
        self.start = None;
        self.preview = None;
    }

    fn make_button(&mut self, ctx: &mut BuildContext, selected: bool) -> Handle<Button> {
        make_interaction_mode_button(
            ctx,
            include_bytes!("../../../vendor/fyroxed_base/resources/cube.png"),
            "Block - Shortcut: [Shift+B]\n\nDrag in any view to draw a box; letting go makes a solid block \
            (col_Block). In the flat views of the quad view the depth comes from the last block.",
            selected,
        )
    }

    fn uuid(&self) -> Uuid {
        Self::type_uuid()
    }
}

/// Gives each scene the block tool and handles the grid and nudge keys.
#[derive(Default)]
pub struct Blockout {
    /// "Grid 1" in the corner of the 3D view.
    label: Handle<Text>,
    /// The grid step and snapping the label shows.
    shown: Option<(f32, bool)>,
}

impl Blockout {
    /// Grid finer or coarser.
    fn change_grid(editor: &mut Editor, direction: i32) {
        let s = &mut editor.settings.data_mut().move_mode_settings;
        let step = next_grid_step(s.x_snap_step, direction);
        s.x_snap_step = step;
        s.y_snap_step = step;
        s.z_snap_step = step;
    }

    /// Moves the selection one grid step: `across` along the view's right and
    /// up (screen directions), `deeper` towards or away from the viewer.
    fn nudge(editor: &mut Editor, across: Vector2<f32>, deeper: f32) {
        let (steps, _) = grid(&editor.settings);
        let entry = editor.scenes.current_scene_entry_ref();
        let Some(selected) = entry.selection.as_graph() else { return };
        let Some(game) = entry.controller.downcast_ref::<GameScene>() else { return };
        let Ok(scene) = editor.engine.scenes.try_get(game.scene) else { return };
        let graph = &scene.graph;
        let Ok(camera) = graph.try_get(game.camera_controller.camera) else { return };
        let (right, up) = screen_axes(camera);
        let (depth_axis, depth_sign) = main_axis(right.cross(&up));
        let mut world = Vector3::zeros();
        for (axis, sign, amount) in [(main_axis(right).0, main_axis(right).1, across.x), (main_axis(up).0, main_axis(up).1, across.y), (depth_axis, depth_sign, deeper)] {
            world[axis] += sign * amount * steps[axis];
        }
        let mut moves = Vec::new();
        for &node in &selected.nodes {
            if node == graph.get_root() {
                continue;
            }
            let Ok(n) = graph.try_get_node(node) else { continue };
            let old = **n.local_transform().position();
            // a move under a turned or scaled parent is in the parent's axes
            let local = graph
                .try_get_node(n.parent())
                .ok()
                .and_then(|p| p.global_transform().try_inverse())
                .map_or(world, |inv| inv.transform_vector(&world));
            moves.push(Command::new(MoveNodeCommand::new(node, old, old + local)));
        }
        match moves.len() {
            0 => {}
            1 => editor.message_sender.send_command(moves.pop().unwrap()),
            _ => editor.message_sender.do_command(CommandGroup::from(moves).with_custom_name("Nudge")),
        }
    }
}

/// The world directions of the screen's right and up in a camera's view, for
/// the 3D view kept level (up is straight up, right along the ground).
fn screen_axes(camera: &Camera) -> (Vector3<f32>, Vector3<f32>) {
    let m = camera.global_transform();
    // a camera looks along its +Z, and Fyrox draws its +X on the left
    let right = -Vector3::new(m[(0, 0)], m[(1, 0)], m[(2, 0)]);
    let up = Vector3::new(m[(0, 1)], m[(1, 1)], m[(2, 1)]);
    if camera.projection().is_perspective() {
        let flat = Vector3::new(right.x, 0.0, right.z);
        return (flat.try_normalize(1e-6).unwrap_or(Vector3::x()), Vector3::y());
    }
    (right, up)
}

impl EditorPlugin for Blockout {
    fn on_start(&mut self, editor: &mut Editor) {
        let frame = editor.scene_viewer.frame();
        let ui = editor.engine.user_interfaces.first_mut();
        let ctx = &mut ui.build_ctx();
        self.label = TextBuilder::new(
            WidgetBuilder::new()
                .with_hit_test_visibility(false)
                .with_horizontal_alignment(HorizontalAlignment::Left)
                .with_vertical_alignment(VerticalAlignment::Bottom)
                .with_margin(Thickness::uniform(6.0)),
        )
        .build(ctx);
        ui.send(self.label, WidgetMessage::LinkWith(frame.to_base()));
    }

    fn on_post_update(&mut self, editor: &mut Editor, _: ApplicationLoopController) {
        // every scene gets the block tool
        let entry = editor.scenes.current_scene_entry_mut();
        if entry.controller.downcast_ref::<GameScene>().is_some() && entry.interaction_modes.of_type::<BlockMode>().is_none() {
            entry.interaction_modes.add(BlockMode::default());
        }
        let snap = &editor.settings.move_mode_settings;
        let shown = (snap.x_snap_step, snap.grid_snapping);
        if Some(shown) != self.shown {
            self.shown = Some(shown);
            let text = if shown.1 { format!("Grid {}  ([ and ] to change)", shown.0) } else { format!("Grid {}, snapping off", shown.0) };
            editor.engine.user_interfaces.first().send(self.label, TextMessage::Text(text));
        }
    }

    fn on_ui_message(&mut self, message: &mut UiMessage, editor: &mut Editor) {
        if message.destination() != editor.scene_viewer.frame() {
            return;
        }
        let Some(WidgetMessage::KeyDown(key)) = message.data::<WidgetMessage>() else { return };
        let m = editor.engine.user_interfaces.first().keyboard_modifiers();
        let entry = editor.scenes.current_scene_entry_ref();
        let terrain = entry.current_interaction_mode == Some(fyroxed_base::interaction::terrain::TerrainInteractionMode::type_uuid());
        let plain = !m.control && !m.alt && !m.shift;
        let handled = match key {
            KeyCode::KeyB if m.shift && !m.control && !m.alt => {
                editor.message_sender.send(Message::SetInteractionMode(BlockMode::type_uuid()));
                true
            }
            // the terrain brush has its own use for [ and ]
            KeyCode::BracketLeft if plain && !terrain => {
                Self::change_grid(editor, -1);
                true
            }
            KeyCode::BracketRight if plain && !terrain => {
                Self::change_grid(editor, 1);
                true
            }
            KeyCode::ArrowLeft if plain => nudge(editor, Vector2::new(-1.0, 0.0), 0.0),
            KeyCode::ArrowRight if plain => nudge(editor, Vector2::new(1.0, 0.0), 0.0),
            KeyCode::ArrowUp if plain => nudge(editor, Vector2::new(0.0, 1.0), 0.0),
            KeyCode::ArrowDown if plain => nudge(editor, Vector2::new(0.0, -1.0), 0.0),
            KeyCode::PageUp if plain => nudge(editor, Vector2::zeros(), 1.0),
            KeyCode::PageDown if plain => nudge(editor, Vector2::zeros(), -1.0),
            _ => false,
        };
        if handled {
            message.set_handled(true);
        }
    }
}

/// Nudges when something is selected and the camera is not being flown with
/// the right mouse button (then the arrow keys move the camera).
fn nudge(editor: &mut Editor, across: Vector2<f32>, deeper: f32) -> bool {
    let entry = editor.scenes.current_scene_entry_ref();
    let flying = entry.controller.downcast_ref::<GameScene>().is_some_and(|g| g.camera_controller.is_interacting());
    let selected = entry.selection.as_graph().is_some_and(|s| !s.nodes.is_empty());
    if flying || !selected {
        return false;
    }
    Blockout::nudge(editor, across, deeper);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_steps_double_and_halve() {
        assert_eq!(next_grid_step(1.0, 1), 2.0);
        assert_eq!(next_grid_step(1.0, -1), 0.5);
        assert_eq!(next_grid_step(64.0, 1), 64.0);
        assert_eq!(next_grid_step(0.0625, -1), 0.0625);
        // a step typed in the settings joins the list at the nearest one
        assert_eq!(next_grid_step(0.3, 1), 0.5);
    }

    #[test]
    fn snapping() {
        assert_eq!(snap(1.3, 0.5, true), 1.5);
        assert_eq!(snap(1.3, 0.5, false), 1.3);
        assert_eq!(snap(-0.74, 0.25, true), -0.75);
    }

    #[test]
    fn a_box_takes_its_depth_from_the_last_block() {
        let b = block_box(Vector3::new(2.0, 9.0, 0.0), Vector3::new(-1.0, 9.0, 3.0), 1, (0.0, 2.0));
        assert_eq!(b.min, Vector3::new(-1.0, 0.0, 0.0));
        assert_eq!(b.max, Vector3::new(2.0, 2.0, 3.0));
        assert!(is_block(&b));
        let flat = block_box(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 3.0), 1, (0.0, 2.0));
        assert!(!is_block(&flat));
    }

    #[test]
    fn blocks_get_the_next_free_name() {
        assert_eq!(block_name(|n| n == "col_Block1" || n == "col_Block2"), "col_Block3");
        assert_eq!(block_name(|_| false), "col_Block1");
    }

    #[test]
    fn axes() {
        assert_eq!(main_axis(Vector3::new(0.1, -0.9, 0.2)), (1, -1.0));
        assert_eq!(main_axis(Vector3::new(-1.0, 0.0, 0.0)), (0, -1.0));
        assert_eq!(depth_axis(Some(OrthoView::Top)), 1);
        assert_eq!(depth_axis(Some(OrthoView::Side)), 0);
        assert_eq!(depth_axis(None), 1);
    }
}
