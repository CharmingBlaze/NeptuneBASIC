//! The Neptune panel: buttons and fields for what `LoadLevelFile` reads
//! from an object, so nobody has to remember `col_` names or type property
//! names. It changes the selected objects' names and properties with one
//! undoable command each, under the rules in `levelprops.rs`.
//!
//! | Row | Changes |
//! |---|---|
//! | Solidity | the name's `col_` / `hit_` prefix |
//! | Type | the `type` property |
//! | Physics, Shape, Mass | `physics`, `shape` and `mass` |
//! | Prefab | the `prefab` property |

use fyroxed_base::{
    command::{CommandContext, CommandGroup, CommandTrait},
    fyrox::{
        core::pool::Handle,
        engine::ApplicationLoopController,
        graph::SceneGraph,
        gui::{
            button::{Button, ButtonBuilder, ButtonMessage},
            dock::DockingManagerMessage,
            dropdown_list::{DropdownList, DropdownListBuilder, DropdownListMessage},
            grid::{Column, GridBuilder, Row},
            message::UiMessage,
            stack_panel::StackPanelBuilder,
            text::{Text, TextBuilder, TextMessage},
            text_box::{TextBox, TextBoxBuilder, TextCommitMode},
            utils::{make_dropdown_list_option, make_simple_tooltip},
            widget::{WidgetBuilder, WidgetMessage},
            window::{Window, WindowAlignment, WindowBuilder, WindowMessage, WindowTitle},
            BuildContext, HorizontalAlignment, Orientation, Thickness, UiNode, VerticalAlignment,
        },
        scene::{base::Property, node::Node},
    },
    plugin::EditorPlugin,
    scene::{commands::GameSceneContext, GameScene},
    Editor,
};

use crate::levelprops::{self, Solidity, PHYSICS, SHAPES};

/// The panel's window name, by which the layout places it.
pub const NAME: &str = "NeptunePanel";

/// What the selection's first object holds, as the panel shows it.
#[derive(Clone, Debug, PartialEq, Default)]
struct Shown {
    /// Nothing that can be changed is selected.
    empty: bool,
    title: String,
    solid: Option<Solidity>,
    kind: String,
    physics: usize,
    shape: usize,
    mass: String,
    prefab: String,
}

impl Shown {
    fn of(name: &str, props: &[Property], count: usize) -> Self {
        let plain = levelprops::plain_name(name);
        let title = if count > 1 { format!("{plain} and {} more", count - 1) } else { plain.to_string() };
        Shown {
            empty: false,
            title,
            solid: Some(levelprops::solid_of(name)),
            kind: levelprops::text(props, "type"),
            physics: levelprops::physics_index(props),
            shape: levelprops::shape_index(props),
            mass: levelprops::text(props, "mass"),
            prefab: levelprops::text(props, "prefab"),
        }
    }

    fn nothing() -> Self {
        Shown { empty: true, title: "Select an object".into(), ..Default::default() }
    }
}

/// One edit the panel makes to every selected object.
#[derive(Clone, Debug)]
enum Edit {
    Solid(Solidity),
    Kind(String),
    Physics(usize),
    Shape(usize),
    Mass(String),
    Prefab(String),
}

impl Edit {
    /// The object's new name and properties, or `None` when the edit
    /// changes nothing (or what was typed is not a value).
    fn apply(&self, name: &str, props: &[Property]) -> Option<(String, Vec<Property>)> {
        let (new_name, new_props) = match self {
            Edit::Solid(s) => (levelprops::with_solid(name, *s), props.to_vec()),
            Edit::Kind(t) => (name.to_string(), levelprops::with_text(props, "type", t)),
            Edit::Physics(i) => (name.to_string(), levelprops::with_physics(props, *i)),
            Edit::Shape(i) => (name.to_string(), levelprops::with_shape(props, *i)),
            Edit::Mass(t) => (name.to_string(), levelprops::with_mass(props, t)?),
            Edit::Prefab(t) => (name.to_string(), levelprops::with_text(props, "prefab", t)),
        };
        (new_name != name || new_props != props).then_some((new_name, new_props))
    }
}

/// Gives an object a name and properties; undoing gives back the old ones.
#[derive(Debug)]
struct SetLevelObject {
    node: Handle<Node>,
    name: String,
    props: Vec<Property>,
}

impl SetLevelObject {
    fn swap(&mut self, context: &mut dyn CommandContext) {
        let context = context.get_mut::<GameSceneContext>();
        let Ok(node) = context.scene.graph.try_get_node_mut(self.node) else { return };
        let old = node.name().to_string();
        node.set_name(std::mem::replace(&mut self.name, old));
        self.props = node.set_properties(std::mem::take(&mut self.props));
    }
}

impl CommandTrait for SetLevelObject {
    fn name(&mut self, _: &dyn CommandContext) -> String {
        "Set Level Object".into()
    }

    fn execute(&mut self, context: &mut dyn CommandContext) {
        self.swap(context);
    }

    fn revert(&mut self, context: &mut dyn CommandContext) {
        self.swap(context);
    }
}

#[derive(Default)]
pub struct Panel {
    window: Handle<Window>,
    title: Handle<Text>,
    solid: [Handle<Button>; 3],
    kind: Handle<TextBox>,
    physics: Handle<DropdownList>,
    shape: Handle<DropdownList>,
    mass: Handle<TextBox>,
    prefab: Handle<TextBox>,
    fields: Handle<UiNode>,
    shown: Shown,
}

fn label(ctx: &mut BuildContext, text: &str, row: usize) -> Handle<UiNode> {
    TextBuilder::new(WidgetBuilder::new().on_row(row).on_column(0).with_margin(Thickness::uniform(3.0)).with_vertical_alignment(VerticalAlignment::Center))
        .with_text(text)
        .build(ctx)
        .to_base()
}

fn text_box(ctx: &mut BuildContext, row: usize, tooltip: &str) -> Handle<TextBox> {
    let tooltip = make_simple_tooltip(ctx, tooltip);
    TextBoxBuilder::new(WidgetBuilder::new().on_row(row).on_column(1).with_margin(Thickness::uniform(2.0)).with_height(24.0).with_tooltip(tooltip))
        .with_text_commit_mode(TextCommitMode::LostFocusPlusEnter)
        .with_vertical_text_alignment(VerticalAlignment::Center)
        .build(ctx)
}

fn dropdown(ctx: &mut BuildContext, row: usize, items: &[&str]) -> Handle<DropdownList> {
    let items = items.iter().map(|i| make_dropdown_list_option(ctx, &capitalized(i))).collect();
    DropdownListBuilder::new(WidgetBuilder::new().on_row(row).on_column(1).with_margin(Thickness::uniform(2.0)).with_height(24.0))
        .with_items(items)
        .with_selected(0)
        .with_close_on_selection(true)
        .build(ctx)
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

impl Panel {
    fn build(&mut self, ctx: &mut BuildContext) {
        self.title = TextBuilder::new(WidgetBuilder::new().with_margin(Thickness::uniform(4.0))).with_text("Select an object").build(ctx);
        let mut buttons = WidgetBuilder::new().on_row(0).on_column(1);
        for (i, solid) in Solidity::ALL.iter().enumerate() {
            self.solid[i] = ButtonBuilder::new(WidgetBuilder::new().with_margin(Thickness::uniform(2.0)).with_height(24.0).with_width(76.0))
                .with_text(solid.label())
                .build(ctx);
            buttons = buttons.with_child(self.solid[i]);
        }
        let buttons = StackPanelBuilder::new(buttons).with_orientation(Orientation::Horizontal).build(ctx);
        self.kind = text_box(ctx, 1, "Read in the game with EntityProperty(object, \"type\")");
        self.physics = dropdown(ctx, 2, &PHYSICS);
        self.shape = dropdown(ctx, 3, &SHAPES);
        self.mass = text_box(ctx, 4, "Empty is a mass of 1");
        self.prefab = text_box(ctx, 5, "Model placed here: a file beside the level or in the game folder");
        let rows = ["Solid", "Type", "Physics", "Shape", "Mass", "Prefab"];
        let mut grid = WidgetBuilder::new()
            .with_child(buttons)
            .with_child(self.kind)
            .with_child(self.physics)
            .with_child(self.shape)
            .with_child(self.mass)
            .with_child(self.prefab);
        for (i, r) in rows.iter().enumerate() {
            grid = grid.with_child(label(ctx, r, i));
        }
        self.fields = GridBuilder::new(grid.with_enabled(false))
            .add_column(Column::strict(64.0))
            .add_column(Column::stretch())
            .add_rows(rows.iter().map(|_| Row::strict(28.0)).collect())
            .build(ctx)
            .to_base();
        let content = StackPanelBuilder::new(WidgetBuilder::new().with_child(self.title).with_child(self.fields)).build(ctx);
        self.window = WindowBuilder::new(WidgetBuilder::new().with_name(NAME))
            .with_title(WindowTitle::text("Neptune"))
            .with_tab_label("Neptune")
            .can_close(false)
            .with_content(content)
            .build(ctx);
    }

    /// The selected objects of the open scene, with what the first holds.
    fn selection(editor: &Editor) -> Option<(Vec<Handle<Node>>, Shown)> {
        let entry = editor.scenes.current_scene_entry_ref();
        let game = entry.controller.downcast_ref::<GameScene>()?;
        let selected = entry.selection.as_graph()?;
        let graph = &editor.engine.scenes.try_get(game.scene).ok()?.graph;
        let nodes: Vec<_> = selected.nodes.iter().copied().filter(|&n| n != graph.get_root() && graph.try_get_node(n).is_ok()).collect();
        let first = graph.try_get_node(*nodes.first()?).ok()?;
        let shown = Shown::of(first.name(), &first.properties, nodes.len());
        Some((nodes, shown))
    }

    fn show(&mut self, editor: &Editor, shown: Shown) {
        let ui = editor.engine.user_interfaces.first();
        ui.send(self.title, TextMessage::Text(shown.title.clone()));
        ui.send(self.fields, WidgetMessage::Enabled(!shown.empty));
        ui.send(self.kind, TextMessage::Text(shown.kind.clone()));
        ui.send(self.physics, DropdownListMessage::Selection(Some(shown.physics)));
        ui.send(self.shape, DropdownListMessage::Selection(Some(shown.shape)));
        ui.send(self.mass, TextMessage::Text(shown.mass.clone()));
        ui.send(self.prefab, TextMessage::Text(shown.prefab.clone()));
        // shape and mass mean nothing without physics
        let physical = shown.physics != 0;
        ui.send(self.shape, WidgetMessage::Enabled(physical));
        ui.send(self.mass, WidgetMessage::Enabled(physical));
        self.shown = shown;
    }

    /// The edit a message from one of the panel's widgets asks for. A value
    /// the panel itself just put there is not an edit.
    fn edit(&self, message: &UiMessage) -> Option<Edit> {
        for (i, &button) in self.solid.iter().enumerate() {
            if let Some(ButtonMessage::Click) = message.data_from(button) {
                return Some(Edit::Solid(Solidity::ALL[i]));
            }
        }
        let typed = |field: Handle<TextBox>, shown: &str| match message.data_from::<TextMessage>(field) {
            Some(TextMessage::Text(t)) if t != shown => Some(t.clone()),
            _ => None,
        };
        let chosen = |list: Handle<DropdownList>, shown: usize| match message.data_from::<DropdownListMessage>(list) {
            Some(DropdownListMessage::Selection(Some(i))) if *i != shown => Some(*i),
            _ => None,
        };
        let s = &self.shown;
        typed(self.kind, &s.kind)
            .map(Edit::Kind)
            .or_else(|| typed(self.mass, &s.mass).map(Edit::Mass))
            .or_else(|| typed(self.prefab, &s.prefab).map(Edit::Prefab))
            .or_else(|| chosen(self.physics, s.physics).map(Edit::Physics))
            .or_else(|| chosen(self.shape, s.shape).map(Edit::Shape))
    }

    fn apply(editor: &Editor, edit: &Edit) {
        let Some((nodes, _)) = Self::selection(editor) else { return };
        let entry = editor.scenes.current_scene_entry_ref();
        let Some(game) = entry.controller.downcast_ref::<GameScene>() else { return };
        let Ok(scene) = editor.engine.scenes.try_get(game.scene) else { return };
        let mut commands = Vec::new();
        for node in nodes {
            let Ok(n) = scene.graph.try_get_node(node) else { continue };
            if let Some((name, props)) = edit.apply(n.name(), &n.properties) {
                commands.push(fyroxed_base::command::Command::new(SetLevelObject { node, name, props }));
            }
        }
        match commands.len() {
            0 => {}
            1 => editor.message_sender.send_command(commands.pop().unwrap()),
            _ => editor.message_sender.do_command(CommandGroup::from(commands).with_custom_name("Set Level Objects")),
        }
    }
}

impl EditorPlugin for Panel {
    fn on_start(&mut self, editor: &mut Editor) {
        let ui = editor.engine.user_interfaces.first_mut();
        self.build(&mut ui.build_ctx());
        // the layout was laid out before this window existed: lay it out again
        let layout = editor.settings.windows.layout.clone().unwrap_or_else(|| editor.default_layout.clone());
        if layout.has_window(NAME) {
            ui.send(editor.docking_manager, DockingManagerMessage::Layout(layout));
        } else {
            // a layout someone arranged before the panel existed
            ui.send(editor.docking_manager, DockingManagerMessage::AddFloatingWindow(self.window));
            ui.send(
                self.window,
                WindowMessage::Open {
                    alignment: WindowAlignment::Relative {
                        relative_to: editor.scene_viewer.frame().to_base(),
                        horizontal_alignment: HorizontalAlignment::Right,
                        vertical_alignment: VerticalAlignment::Top,
                        margin: Thickness::uniform(4.0),
                    },
                    modal: false,
                    focus_content: false,
                },
            );
        }
        self.show(editor, Shown::nothing());
    }

    fn on_ui_message(&mut self, message: &mut UiMessage, editor: &mut Editor) {
        if let Some(edit) = self.edit(message) {
            Self::apply(editor, &edit);
        }
    }

    fn on_post_update(&mut self, editor: &mut Editor, _: ApplicationLoopController) {
        let shown = Self::selection(editor).map_or_else(Shown::nothing, |(_, s)| s);
        if shown != self.shown {
            self.show(editor, shown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fyroxed_base::fyrox::scene::base::PropertyValue;

    #[test]
    fn edits_that_change_nothing_make_no_command() {
        let props = vec![Property { name: "type".into(), value: PropertyValue::String("coin".into()) }];
        assert!(Edit::Kind("coin".into()).apply("Coin", &props).is_none());
        assert!(Edit::Solid(Solidity::Solid).apply("col_Wall", &[]).is_none());
        assert!(Edit::Mass("heavy".into()).apply("Box", &[]).is_none());
        let (name, _) = Edit::Solid(Solidity::Invisible).apply("col_Wall", &[]).unwrap();
        assert_eq!(name, "hit_Wall");
        let (_, props) = Edit::Physics(2).apply("Box", &props).unwrap();
        assert_eq!(levelprops::physics_index(&props), 2);
    }

    #[test]
    fn the_panel_shows_the_plain_name() {
        let shown = Shown::of("col_Wall", &[], 3);
        assert_eq!(shown.title, "Wall and 2 more");
        assert_eq!(shown.solid, Some(Solidity::Solid));
        assert!(Shown::nothing().empty);
    }
}
