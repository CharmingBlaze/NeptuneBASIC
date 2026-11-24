//! Neptune Level Editor: Fyrox's scene editor (`fyroxed_base`) in Neptune
//! IDE's colours, whose Play button runs `neptune`.
//!
//!     neptune-editor [game folder] [scene.rgs ...] [--play] [--full-inspector] [--quad]
//!
//! Without a folder it opens the current one. Scenes are saved as Fyrox
//! scene files (`.rgs`), which `LoadLevelFile` loads. Without a scene it
//! opens a starter level (`starter.rs`). `--play` presses Play as soon as
//! the first scene is open. `--full-inspector` shows every setting Fyrox has
//! in the Inspector, which otherwise leaves the engine's own out. `--quad`
//! starts with the view split in four (`quad.rs`).
//!
//! The editor's code is our copy of `fyroxed_base` in `vendor/`; what
//! Neptune changed there is marked `// Neptune:` (see its `NEPTUNE.md`).

mod blockout;
mod keys;
mod layout;
mod levelprops;
mod panel;
mod play;
mod quad;
mod starter;
mod theme;

use std::path::PathBuf;

use fyroxed_base::{
    fyrox::{
        core::{log::Log, TypeUuidProvider},
        engine::{ApplicationLoopController, GraphicsContext},
        event_loop::EventLoop,
        gui::widget::WidgetMessage,
    },
    interaction::move_mode::MoveInteractionMode,
    plugin::EditorPlugin,
    settings::Settings,
    Editor, Message, StartupData,
};

const TITLE: &str = "Neptune Level Editor";

/// Neptune's part of the editor that needs to run while it does.
struct Neptune {
    /// `--play`: press Play once a scene is open.
    play: bool,
    /// No scene was asked for: open the starter level.
    starter: bool,
    /// The first scene has not been given its tool yet.
    choose_tool: bool,
}

impl EditorPlugin for Neptune {
    fn on_start(&mut self, editor: &mut Editor) {
        // games are packed with `neptune build`, not Fyrox's exporter
        let ui = editor.engine.user_interfaces.first();
        ui.send(editor.menu.file_menu.export_project, WidgetMessage::Visibility(false));
    }

    fn on_post_update(&mut self, editor: &mut Editor, _: ApplicationLoopController) {
        // the editor writes its own title whenever it opens a folder
        if let GraphicsContext::Initialized(ctx) = &editor.engine.graphics_context {
            if !ctx.window.title().starts_with(TITLE) {
                let folder = std::env::current_dir().unwrap_or_default();
                ctx.window.set_title(&format!("{TITLE}: {}", folder.display()));
            }
        }
        if std::mem::take(&mut self.starter) {
            starter::open(editor);
        }
        // the editor starts a scene on whichever tool comes first in a table
        // with no order, which can be the terrain brush
        if self.choose_tool && !editor.scenes.is_empty() {
            self.choose_tool = false;
            editor.message_sender.send(Message::SetInteractionMode(MoveInteractionMode::type_uuid()));
        }
        if self.play && !editor.scenes.is_empty() {
            self.play = false;
            editor.message_sender.send(Message::SwitchMode);
        }
    }
}

fn main() {
    let (flags, paths): (Vec<_>, Vec<_>) = std::env::args_os().skip(1).partition(|a| a.to_string_lossy().starts_with("--"));
    if let Some(unknown) = flags.iter().find(|f| *f != "--play" && *f != "--full-inspector" && *f != "--quad") {
        eprintln!("neptune-editor: unknown option {}", unknown.to_string_lossy());
        std::process::exit(1);
    }
    let play = flags.iter().any(|f| f == "--play");
    fyroxed_base::neptune::set_full_inspector(flags.iter().any(|f| f == "--full-inspector"));
    let mut paths = paths.into_iter().map(PathBuf::from);
    let folder = paths.next().unwrap_or_default();
    // move there first, so the log and the editor's settings are kept with the game
    if !folder.as_os_str().is_empty() && std::env::set_current_dir(&folder).is_err() {
        eprintln!("neptune-editor: not a folder: {}", folder.display());
        std::process::exit(1);
    }
    Log::set_file_name("neptune-editor.log");
    // the editor reads its settings from the file, so Play's commands go there
    let saved = Settings::load();
    let first_time = saved.is_err();
    let mut settings = saved.unwrap_or_default();
    play::set_profiles(&mut settings, &std::env::current_dir().unwrap_or_default());
    if first_time {
        // the engine's own progress reports bury what a level designer needs to see
        settings.data_mut().log.log_info = false;
        // a grid fit for blocking out levels, snapped (Fyrox starts at 0.05, unsnapped)
        let snap = &mut settings.data_mut().move_mode_settings;
        snap.grid_snapping = true;
        (snap.x_snap_step, snap.y_snap_step, snap.z_snap_step) = (0.5, 0.5, 0.5);
    }
    if layout::replaces(settings.windows.layout.as_ref()) {
        settings.data_mut().windows.layout = Some(layout::neptune());
    }
    keys::apply(&mut settings.data_mut().key_bindings);
    settings.force_save();
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("neptune-editor: cannot open a window: {e}");
            std::process::exit(1);
        }
    };
    let scenes: Vec<PathBuf> = paths.collect();
    let starter = scenes.is_empty();
    let startup = StartupData { working_directory: PathBuf::new(), scenes, named_objects: false };
    let mut editor = Editor::new(Some(startup));
    theme::apply(&mut editor);
    // View > Reset Layout goes back to Neptune's layout, not Fyrox's
    if layout::covers(&editor.default_layout) {
        editor.default_layout = layout::neptune();
    }
    editor.add_editor_plugin(Neptune { play, starter, choose_tool: true });
    editor.add_editor_plugin(panel::Panel::default());
    editor.add_editor_plugin(quad::QuadView::new(flags.iter().any(|f| f == "--quad")));
    // after the quad view, so the grid label is drawn over its pictures
    editor.add_editor_plugin(blockout::Blockout::default());
    editor.run(event_loop);
}

/// Checks of Neptune's changes to the copied editor, which the workspace's
/// tests do not reach.
#[cfg(test)]
mod tests {
    use fyroxed_base::neptune::{is_engine_setting, short_type_name};

    #[test]
    fn type_names_are_short() {
        assert_eq!(short_type_name("fyrox_impl::scene::mesh::Mesh"), "Mesh");
        assert_eq!(short_type_name("Pivot"), "Pivot");
        assert_eq!(short_type_name("a::Handle<b::Node>"), "Handle");
    }

    #[test]
    fn the_short_inspector_keeps_what_a_level_needs() {
        for name in ["Name", "Local Position", "Local Rotation", "Local Scale", "Visibility", "Cast Shadows", "Properties"] {
            assert!(!is_engine_setting(name), "{name}");
        }
        assert!(is_engine_setting("Render Mask"));
        assert!(is_engine_setting("Pre Rotation"));
    }
}
