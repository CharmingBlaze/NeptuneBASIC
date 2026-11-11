//! Quad view: the 3D view split into four, as in Hammer and UnrealEd, with
//! the 3D view at the top left and flat (orthographic) views from the top,
//! the front and the side.
//!
//! | Key | What it does |
//! |---|---|
//! | Ctrl+Alt+Q | Quad view on and off (Blender's key); off keeps the view under the mouse |
//! | Ctrl+Space | In quad view, the view under the mouse fills the window, and back (Blender's and TrenchBroom's key) |
//! | Numpad 7, 1, 3 | Top, front and side view (Blender's keys); Numpad 5 the 3D view |
//!
//! The view under the mouse is always drawn by the editor's own camera, so
//! selecting, the move, turn and scale tools, the mouse wheel and the
//! camera keys work in it as they do in the single view. When the mouse
//! moves to another view, the editor's camera takes that view over and a
//! camera of Neptune's draws the one it left. The three extra cameras (and
//! a grid for each) live under the editor's own objects, which are never
//! saved with the level. `fyroxed_base::neptune` has the parts this needs
//! inside the editor (`OrthoView`, `quad_bounds`).
//!
//! Each view is drawn whole into a picture of its own, and four images over
//! the 3D view show them. (A Fyrox camera that draws into only part of a
//! picture comes out shrunk: its HDR, FXAA and copy passes each squeeze the
//! whole picture into the camera's part.)

use fyroxed_base::{
    fyrox::{
        core::{
            algebra::{Matrix4, UnitQuaternion, Vector2, Vector3},
            pool::Handle,
        },
        engine::ApplicationLoopController,
        graph::SceneGraph,
        gui::{
            grid::{Column, Grid, GridBuilder, Row},
            image::{Image, ImageBuilder, ImageMessage},
            menu::{MenuItem, MenuItemMessage},
            message::{KeyCode, UiMessage},
            widget::{WidgetBuilder, WidgetMessage},
            Thickness,
        },
        material::{Material, MaterialResource},
        resource::texture::{TextureResource, TextureResourceExtension},
        scene::{
            base::BaseBuilder,
            camera::{Camera, CameraBuilder, OrthographicProjection, Projection},
            collider::BitMask,
            graph::Graph,
            mesh::{
                surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
                MeshBuilder, RenderPath,
            },
            pivot::Pivot,
            Scene,
        },
    },
    menu::create_menu_item,
    neptune::{self, OrthoView},
    plugin::EditorPlugin,
    scene::GameScene,
    Editor, Message,
};

/// The four views, in the order of their places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Perspective,
    Top,
    Front,
    Side,
}

impl View {
    pub const ALL: [View; 4] = [View::Perspective, View::Top, View::Front, View::Side];

    fn index(self) -> usize {
        self as usize
    }

    /// The view's part of the 3D view, from its top left: x, y, width, height.
    pub fn part(self) -> [f32; 4] {
        match self {
            View::Perspective => [0.0, 0.0, 0.5, 0.5],
            View::Top => [0.5, 0.0, 0.5, 0.5],
            View::Front => [0.0, 0.5, 0.5, 0.5],
            View::Side => [0.5, 0.5, 0.5, 0.5],
        }
    }

    fn ortho(self) -> Option<OrthoView> {
        match self {
            View::Perspective => None,
            View::Top => Some(OrthoView::Top),
            View::Front => Some(OrthoView::Front),
            View::Side => Some(OrthoView::Side),
        }
    }
}

/// The view whose part holds a point given as fractions of the 3D view.
pub fn view_at(x: f32, y: f32) -> View {
    match (x >= 0.5, y >= 0.5) {
        (false, false) => View::Perspective,
        (true, false) => View::Top,
        (false, true) => View::Front,
        (true, true) => View::Side,
    }
}

/// Which camera draws `view`: 0 the editor's (the active view), then
/// Neptune's three in the order of the other views.
pub fn camera_of(active: View, view: View) -> usize {
    if view == active {
        return 0;
    }
    1 + View::ALL.iter().filter(|v| **v != active).position(|v| *v == view).unwrap_or(0)
}

/// Where a view's camera stands and how it looks, kept while another
/// camera draws it.
#[derive(Clone, Debug)]
struct Slot {
    /// The editor camera's pivot.
    pivot: Vector3<f32>,
    yaw: f32,
    pitch: f32,
    z_offset: f32,
    projection: Projection,
}

impl Slot {
    /// A flat view of the level around the origin.
    fn ortho(view: OrthoView, z_near: f32, z_far: f32) -> Self {
        Slot {
            pivot: view.place(Vector3::zeros(), Vector2::zeros(), z_far),
            yaw: 0.0,
            pitch: 0.0,
            z_offset: 0.0,
            projection: Projection::Orthographic(OrthographicProjection { z_near, z_far, vertical_size: 12.0 }),
        }
    }

    /// Where a camera showing this slot stands and how it is turned, as the
    /// editor's camera controller places its pivot, hinge and camera.
    fn pose(&self, view: View) -> (Vector3<f32>, UnitQuaternion<f32>) {
        match view.ortho() {
            Some(o) => (self.pivot, o.rotation()),
            None => {
                let turn = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.yaw)
                    * UnitQuaternion::from_axis_angle(&Vector3::x_axis(), self.pitch);
                (self.pivot + turn * Vector3::new(0.0, 0.0, self.z_offset), turn)
            }
        }
    }
}

/// Render-mask bits: every object draws in every view, but each view sees
/// only its own grid. Bit 1 is the editor's grid; bits 2 to 4 are the grids
/// of Neptune's three cameras. The editor gives everything of its own the
/// mask `GameScene::EDITOR_OBJECTS_MASK` before each frame, except nodes
/// whose names start with `neptune::QUAD_NODE`.
const GRIDS: u32 = 0b11110;
fn extra_bit(i: usize) -> u32 {
    1 << (2 + i)
}

/// Neptune's cameras in one scene.
struct Extras {
    scene: Handle<Scene>,
    cameras: [Handle<Camera>; 3],
    materials: [MaterialResource; 3],
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Toggle,
    Maximize,
    Show(View),
}

#[derive(Default)]
pub struct QuadView {
    /// The view is split (Ctrl+Alt+Q).
    quad: bool,
    /// Quad view, with one view filling the window for now (Ctrl+Space).
    maximized: bool,
    /// The view the editor's camera draws.
    active: Option<View>,
    slots: Vec<Slot>,
    /// The scene the slots belong to.
    scene: Option<Handle<Scene>>,
    extras: Option<Extras>,
    /// Mouse buttons held down in the 3D view: while one is, the view does
    /// not change under the mouse.
    held: u32,
    menu: Vec<(Handle<MenuItem>, Action)>,
    /// The four images over the 3D view, in the order of `View::ALL`.
    overlay: Handle<Grid>,
    images: [Handle<Image>; 4],
    /// Each camera's picture (the editor's first, then Neptune's three), and
    /// the size they were made for. A picture stays with its camera: Fyrox
    /// ties a picture to the first camera that draws into it.
    pictures: Vec<TextureResource>,
    picture_size: (u32, u32),
    /// `--quad`: split the view once the first scene is open.
    start_split: bool,
}

impl QuadView {
    /// The quad view plugin; `split` starts with the view split in four.
    pub fn new(split: bool) -> Self {
        QuadView { start_split: split, ..Default::default() }
    }

    fn split(&self) -> bool {
        self.quad && !self.maximized
    }

    fn active(&self) -> View {
        self.active.unwrap_or(View::Perspective)
    }

    /// Makes sure there is a slot for each view, starting from the
    /// editor's camera as it is now.
    fn ensure_slots(&mut self, editor: &mut Editor) {
        if !self.slots.is_empty() {
            return;
        }
        let (z_near, z_far) = (editor.settings.graphics.z_near, editor.settings.graphics.z_far);
        let Some(game) = editor.scenes.current_scene_controller_ref().downcast_ref::<GameScene>() else { return };
        let Ok(scene) = editor.engine.scenes.try_get(game.scene) else { return };
        let current = save(game, &scene.graph);
        self.slots = View::ALL.iter().map(|v| v.ortho().map_or_else(|| current.clone(), |o| Slot::ortho(o, z_near, z_far))).collect();
        self.active = Some(View::Perspective);
        self.scene = Some(game.scene);
    }

    /// Gives the editor's camera `view`, keeping the one it had.
    fn activate(&mut self, editor: &mut Editor, view: View) {
        self.ensure_slots(editor);
        if self.slots.is_empty() || self.active == Some(view) {
            return;
        }
        let old = self.active();
        let Some(game) = editor.scenes.current_scene_controller_mut().downcast_mut::<GameScene>() else { return };
        let Ok(scene) = editor.engine.scenes.try_get_mut(game.scene) else { return };
        self.slots[old.index()] = save(game, &scene.graph);
        load(game, &mut scene.graph, &self.slots[view.index()], view);
        self.active = Some(view);
    }

    /// Lays the cameras and pictures out for the current mode.
    fn arrange(&mut self, editor: &mut Editor) {
        let split = self.split();
        neptune::set_quad_part(None);
        if !split {
            self.remove_extras(editor);
        }
        let active = self.active();
        let picture = if split { self.pictures.first().cloned() } else { None };
        if let Some(game) = editor.scenes.current_scene_controller_ref().downcast_ref::<GameScene>() {
            let (camera, grid) = (game.camera_controller.camera, game.camera_controller.grid);
            if let Ok(scene) = editor.engine.scenes.try_get_mut(game.scene) {
                let graph = &mut scene.graph;
                if let Ok(c) = graph.try_get_mut(camera) {
                    c.set_render_target(picture);
                    // named so the editor leaves its mask alone while the view is split
                    c.set_name(if split { "NeptuneQuadEditorCamera" } else { "EditorCamera" });
                    let mask = if split { BitMask(!GRIDS | 0b10) } else { GameScene::EDITOR_OBJECTS_MASK };
                    c.render_mask.set_value_and_mark_modified(mask);
                }
                if let Ok(g) = graph.try_get_mut(grid) {
                    // named so the editor leaves its mask alone (`neptune::is_quad_node`)
                    g.set_name(if split { "NeptuneQuadGrid" } else { "" });
                    g.render_mask.set_value_and_mark_modified(if split { BitMask(0b10) } else { GameScene::EDITOR_OBJECTS_MASK });
                }
            }
        }
        if split {
            neptune::set_quad_part(Some(active.part()));
        }
        let ui = editor.engine.user_interfaces.first();
        ui.send(self.overlay, WidgetMessage::Visibility(split));
        // each image shows the picture of the camera now drawing its view
        // (all four every time: an image can miss a change sent while the
        // window is being laid out)
        if self.pictures.len() == 4 {
            for view in View::ALL {
                let camera = camera_of(active, view);
                ui.send(self.images[view.index()], ImageMessage::Texture(Some(self.pictures[camera].clone())));
            }
        }
    }

    /// Makes each view's picture a quarter of the 3D view's size and shows
    /// them; true when they were made again.
    fn ensure_pictures(&mut self, editor: &mut Editor) -> bool {
        let ui = editor.engine.user_interfaces.first();
        let size = ui[editor.scene_viewer.frame()].actual_local_size();
        let want = (((size.x / 2.0) as u32).max(1), ((size.y / 2.0) as u32).max(1));
        if want == self.picture_size && self.pictures.len() == 4 {
            return false;
        }
        self.picture_size = want;
        self.pictures = (0..4).map(|_| TextureResource::new_render_target(want.0, want.1)).collect();
        true
    }

    /// Gives the open scene's camera its 3D view and the whole window back,
    /// and forgets the views, before the editor goes to another scene.
    fn release(&mut self, editor: &mut Editor) {
        if self.slots.is_empty() {
            return;
        }
        self.activate(editor, View::Perspective);
        let quad = std::mem::replace(&mut self.quad, false);
        self.arrange(editor);
        self.quad = quad;
        self.slots.clear();
        self.active = None;
        self.scene = None;
    }

    fn remove_extras(&mut self, editor: &mut Editor) {
        let Some(extras) = self.extras.take() else { return };
        if let Ok(scene) = editor.engine.scenes.try_get_mut(extras.scene) {
            for c in extras.cameras {
                if scene.graph.is_valid_handle(c) {
                    scene.graph.remove_node(c);
                }
            }
        }
    }

    /// Neptune's three cameras, made when first needed.
    fn ensure_extras(&mut self, editor: &mut Editor) {
        let Some(game) = editor.scenes.current_scene_controller_ref().downcast_ref::<GameScene>() else { return };
        if self.extras.as_ref().is_some_and(|e| e.scene == game.scene) {
            return;
        }
        let (scene_handle, root, material) = (game.scene, game.editor_objects_root, game.grid_material.clone());
        self.remove_extras(editor);
        let exposure = editor.settings.camera.exposure;
        let Ok(scene) = editor.engine.scenes.try_get_mut(scene_handle) else { return };
        let graph = &mut scene.graph;
        let mut cameras = [Handle::NONE; 3];
        let materials: [MaterialResource; 3] = std::array::from_fn(|_| {
            let copy: Material = material.data_ref().clone();
            MaterialResource::new_embedded(copy)
        });
        for i in 0..3 {
            let grid = MeshBuilder::new(BaseBuilder::new().with_frustum_culling(false).with_name("NeptuneQuadGrid"))
                .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(SurfaceData::make_quad(&Matrix4::new_scaling(2.0))))
                    .with_material(materials[i].clone())
                    .build()])
                .with_render_path(RenderPath::Forward)
                .build(graph);
            cameras[i] = CameraBuilder::new(BaseBuilder::new().with_name("NeptuneQuadCamera")).with_exposure(exposure).build(graph);
            graph.link_nodes(cameras[i], root);
            graph.link_nodes(grid, cameras[i]);
            graph[grid].render_mask.set_value_and_mark_modified(BitMask(extra_bit(i)));
            graph[cameras[i]].render_mask.set_value_and_mark_modified(BitMask(!GRIDS | extra_bit(i)));
        }
        self.extras = Some(Extras { scene: scene_handle, cameras, materials });
    }

    /// Puts Neptune's cameras where the views the editor's camera is not
    /// drawing are.
    fn update_extras(&mut self, editor: &mut Editor) {
        if !self.split() || self.slots.is_empty() {
            return;
        }
        if self.ensure_pictures(editor) {
            // the editor's camera and the images take the new pictures
            self.arrange(editor);
        }
        self.ensure_extras(editor);
        let Some(extras) = &self.extras else { return };
        let settings = &editor.settings;
        let (z_near, z_far) = (settings.graphics.z_near, settings.graphics.z_far);
        let snap = &settings.move_mode_settings;
        let steps = [snap.x_snap_step, snap.y_snap_step, snap.z_snap_step];
        let snapping = snap.grid_snapping;
        let Ok(scene) = editor.engine.scenes.try_get_mut(extras.scene) else { return };
        let others = View::ALL.into_iter().filter(|v| *v != self.active());
        for (i, view) in others.enumerate() {
            let slot = &self.slots[view.index()];
            let (position, rotation) = slot.pose(view);
            let picture = self.pictures[1 + i].clone();
            update_camera(&mut scene.graph, extras.cameras[i], slot, picture, position, rotation, z_near, z_far);
            let (plane, scale) = neptune::grid_for(view.ortho().is_none(), view.ortho().unwrap_or_default(), steps, snapping);
            let mut m = extras.materials[i].data_ref();
            m.set_property("orientation", plane);
            m.set_property("isPerspective", view.ortho().is_none());
            m.set_property("scale", scale);
        }
    }

    fn act(&mut self, editor: &mut Editor, action: Action) {
        match action {
            Action::Toggle => {
                self.quad = !self.quad;
                self.maximized = false;
                self.ensure_slots(editor);
            }
            Action::Maximize if self.quad => self.maximized = !self.maximized,
            Action::Maximize => return,
            Action::Show(view) => {
                self.quad = false;
                self.maximized = false;
                self.activate(editor, view);
            }
        }
        self.arrange(editor);
        self.update_extras(editor);
    }

    fn on_frame_mouse(&mut self, editor: &mut Editor, pos: Vector2<f32>) {
        if !self.split() || self.held > 0 {
            return;
        }
        let ui = editor.engine.user_interfaces.first();
        let bounds = ui[editor.scene_viewer.frame()].screen_bounds();
        if bounds.size.x <= 0.0 || bounds.size.y <= 0.0 {
            return;
        }
        let rel = pos - bounds.position;
        let view = view_at(rel.x / bounds.size.x, rel.y / bounds.size.y);
        if view != self.active() {
            self.activate(editor, view);
            self.arrange(editor);
            // the other cameras move now: the editor may not update again
            // until something else happens, and the views would be swapped
            self.update_extras(editor);
        }
    }
}

fn save(game: &GameScene, graph: &Graph) -> Slot {
    let cc = &game.camera_controller;
    Slot {
        pivot: graph.try_get(cc.pivot).map(|p| **p.local_transform().position()).unwrap_or_default(),
        yaw: cc.yaw(),
        pitch: cc.pitch(),
        z_offset: cc.z_offset,
        projection: graph.try_get(cc.camera).map(|c| c.projection().clone()).unwrap_or_default(),
    }
}

fn load(game: &mut GameScene, graph: &mut Graph, slot: &Slot, view: View) {
    let cc = &mut game.camera_controller;
    cc.set_yaw(slot.yaw);
    cc.set_pitch(slot.pitch);
    cc.z_offset = slot.z_offset;
    cc.neptune_view = view.ortho().unwrap_or_default();
    cc.set_projection(graph, slot.projection.clone());
    let (pivot, hinge, camera) = (cc.pivot, cc.camera_hinge, cc.camera);
    // placed now, not on the next update, so the view does not jump
    let (rotation, offset) = match view.ortho() {
        Some(o) => (o.rotation(), 0.0),
        None => (UnitQuaternion::from_axis_angle(&Vector3::y_axis(), slot.yaw), slot.z_offset),
    };
    if let Ok(p) = graph.try_get_mut(pivot) {
        p.local_transform_mut().set_position(slot.pivot).set_rotation(rotation);
    }
    if let Ok(h) = graph.try_get_mut::<Pivot>(hinge) {
        let tilt = if view.ortho().is_some() { 0.0 } else { slot.pitch };
        h.local_transform_mut().set_rotation(UnitQuaternion::from_axis_angle(&Vector3::x_axis(), tilt));
    }
    if let Ok(c) = graph.try_get_mut(camera) {
        c.local_transform_mut().set_position(Vector3::new(0.0, 0.0, offset)).set_rotation(UnitQuaternion::identity());
    }
}

#[allow(clippy::too_many_arguments)]
fn update_camera(
    graph: &mut Graph,
    camera: Handle<Camera>,
    slot: &Slot,
    picture: TextureResource,
    position: Vector3<f32>,
    rotation: UnitQuaternion<f32>,
    z_near: f32,
    z_far: f32,
) {
    let Ok(c) = graph.try_get_mut(camera) else { return };
    let mut projection = slot.projection.clone();
    projection.set_z_near(z_near);
    projection.set_z_far(z_far);
    c.set_projection(projection);
    c.set_render_target(Some(picture));
    c.local_transform_mut().set_position(position).set_rotation(rotation);
}

impl EditorPlugin for QuadView {
    fn on_start(&mut self, editor: &mut Editor) {
        let view_menu = editor.menu.view_menu.menu;
        let frame = editor.scene_viewer.frame();
        let ui = editor.engine.user_interfaces.first_mut();
        let ctx = &mut ui.build_ctx();
        let items = [
            ("Quad View (Ctrl+Alt+Q)", Action::Toggle),
            ("Maximize View (Ctrl+Space)", Action::Maximize),
            ("3D View (Numpad 5)", Action::Show(View::Perspective)),
            ("Top View (Numpad 7)", Action::Show(View::Top)),
            ("Front View (Numpad 1)", Action::Show(View::Front)),
            ("Side View (Numpad 3)", Action::Show(View::Side)),
        ];
        for (title, action) in items {
            let id = fyroxed_base::fyrox::core::uuid::Uuid::new_v4();
            let item = create_menu_item(title, id, vec![], ctx);
            self.menu.push((item, action));
        }
        // the images let the mouse through to the 3D view under them
        let mut cells = WidgetBuilder::new().with_hit_test_visibility(false).with_visibility(false);
        for view in View::ALL {
            let [x, y, _, _] = view.part();
            let image = ImageBuilder::new(
                WidgetBuilder::new()
                    .with_hit_test_visibility(false)
                    .with_margin(Thickness::uniform(1.0))
                    .on_column(usize::from(x > 0.0))
                    .on_row(usize::from(y > 0.0)),
            )
            .with_flip(true)
            .build(ctx);
            self.images[view.index()] = image;
            cells = cells.with_child(image);
        }
        self.overlay = GridBuilder::new(cells)
            .add_columns(vec![Column::stretch(), Column::stretch()])
            .add_rows(vec![Row::stretch(), Row::stretch()])
            .build(ctx);
        for (item, _) in &self.menu {
            ui.send(view_menu, MenuItemMessage::AddItem(*item));
        }
        ui.send(self.overlay, WidgetMessage::LinkWith(frame.to_base()));
    }

    fn on_message(&mut self, message: &Message, editor: &mut Editor) {
        // before the editor goes to another scene, this one's camera gets its
        // 3D view and the whole window back (the editor also "changes" to
        // the scene already open, which is no change)
        let leaving = match message {
            Message::SetCurrentScene(id) => *id != editor.scenes.current_scene_entry_ref().id,
            Message::AddScene { .. } | Message::AddUiScene { .. } | Message::LoadScene(_) | Message::CloseScene(_) => true,
            Message::NewScene | Message::NewUiScene => true,
            _ => false,
        };
        if leaving {
            self.release(editor);
        }
    }

    fn on_ui_message(&mut self, message: &mut UiMessage, editor: &mut Editor) {
        if let Some(MenuItemMessage::Click) = message.data() {
            if let Some(&(_, action)) = self.menu.iter().find(|(item, _)| message.destination() == *item) {
                self.act(editor, action);
                return;
            }
        }
        if message.destination() != editor.scene_viewer.frame() {
            return;
        }
        let Some(msg) = message.data::<WidgetMessage>() else { return };
        match *msg {
            WidgetMessage::MouseMove { pos, .. } => self.on_frame_mouse(editor, pos),
            WidgetMessage::MouseDown { pos, .. } => {
                self.on_frame_mouse(editor, pos);
                self.held += 1;
            }
            WidgetMessage::MouseUp { .. } => self.held = self.held.saturating_sub(1),
            WidgetMessage::KeyDown(key) => {
                let m = editor.engine.user_interfaces.first().keyboard_modifiers();
                let action = match key {
                    KeyCode::KeyQ if m.control && m.alt => Some(Action::Toggle),
                    KeyCode::Space if m.control => Some(Action::Maximize),
                    KeyCode::Numpad5 => Some(Action::Show(View::Perspective)),
                    KeyCode::Numpad7 => Some(Action::Show(View::Top)),
                    KeyCode::Numpad1 => Some(Action::Show(View::Front)),
                    KeyCode::Numpad3 => Some(Action::Show(View::Side)),
                    _ => None,
                };
                if let Some(action) = action {
                    self.act(editor, action);
                    message.set_handled(true);
                }
            }
            _ => {}
        }
    }

    fn on_update(&mut self, editor: &mut Editor, _: ApplicationLoopController) {
        let scene = editor.scenes.current_scene_controller_ref().downcast_ref::<GameScene>().map(|g| g.scene);
        if self.start_split && scene.is_some() {
            self.start_split = false;
            self.act(editor, Action::Toggle);
        }
        // another scene is open: start its views from its own camera
        if self.quad && scene.is_some() && scene != self.scene {
            self.remove_extras(editor);
            self.slots.clear();
            self.active = None;
            self.ensure_slots(editor);
            self.arrange(editor);
        }
        self.update_extras(editor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_view_has_a_quarter() {
        assert_eq!(view_at(0.1, 0.1), View::Perspective);
        assert_eq!(view_at(0.9, 0.1), View::Top);
        assert_eq!(view_at(0.1, 0.9), View::Front);
        assert_eq!(view_at(0.9, 0.9), View::Side);
        for v in View::ALL {
            let [x, y, w, h] = v.part();
            assert_eq!(view_at(x + w / 2.0, y + h / 2.0), v, "{v:?}");
        }
    }

    #[test]
    fn every_view_has_its_own_camera() {
        for active in View::ALL {
            let mut cameras: Vec<usize> = View::ALL.iter().map(|v| camera_of(active, *v)).collect();
            assert_eq!(cameras[active.index()], 0);
            cameras.sort();
            assert_eq!(cameras, [0, 1, 2, 3]);
        }
    }

    #[test]
    fn flat_views_look_at_the_level() {
        let top = Slot::ortho(OrthoView::Top, 0.025, 128.0);
        let (pos, turn) = top.pose(View::Top);
        // above the origin, looking down
        assert!(pos.y > 60.0 && pos.x.abs() < 1e-4 && pos.z.abs() < 1e-4);
        assert!((turn * Vector3::z() - Vector3::new(0.0, -1.0, 0.0)).norm() < 1e-5);
        let side = Slot::ortho(OrthoView::Side, 0.025, 128.0);
        let (pos, turn) = side.pose(View::Side);
        assert!(pos.x > 60.0);
        assert!((turn * Vector3::z() - Vector3::new(-1.0, 0.0, 0.0)).norm() < 1e-5);
        // moving a flat view keeps it pulled back
        let moved = OrthoView::Top.place(Vector3::new(3.0, 1.0, 2.0), Vector2::new(1.0, 0.0), 128.0);
        assert!((moved - Vector3::new(4.0, 64.0, 2.0)).norm() < 1e-4);
    }

    #[test]
    fn the_3d_view_stands_back_from_its_pivot() {
        let slot = Slot { pivot: Vector3::new(1.0, 2.0, 3.0), yaw: 0.0, pitch: 0.0, z_offset: -3.0, projection: Projection::default() };
        let (pos, _) = slot.pose(View::Perspective);
        assert!((pos - Vector3::new(1.0, 2.0, 0.0)).norm() < 1e-5);
    }
}
