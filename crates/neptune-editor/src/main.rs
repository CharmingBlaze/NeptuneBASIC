//! Neptune Level Editor: Fyrox's scene editor (`fyroxed_base`) in Neptune
//! IDE's colours, whose Play button runs `neptune`.
//!
//!     neptune-editor [game folder] [scene.rgs ...] [--play]
//!
//! Without a folder it opens the current one. Scenes are saved as Fyrox
//! scene files (`.rgs`), which `LoadLevelFile` loads. `--play` presses Play
//! as soon as the first scene is open.

mod play;
mod theme;

use std::path::PathBuf;

use fyroxed_base::{
    fyrox::{
        core::log::Log,
        engine::{ApplicationLoopController, GraphicsContext},
        event_loop::EventLoop,
    },
    plugin::EditorPlugin,
    settings::Settings,
    Editor, Message, StartupData,
};

const TITLE: &str = "Neptune Level Editor";

/// Neptune's part of the editor that needs to run while it does.
struct Neptune {
    /// `--play`: press Play once a scene is open.
    play: bool,
}

impl EditorPlugin for Neptune {
    fn on_post_update(&mut self, editor: &mut Editor, _: ApplicationLoopController) {
        // the editor writes its own title whenever it opens a folder
        if let GraphicsContext::Initialized(ctx) = &editor.engine.graphics_context {
            if !ctx.window.title().starts_with(TITLE) {
                let folder = std::env::current_dir().unwrap_or_default();
                ctx.window.set_title(&format!("{TITLE}: {}", folder.display()));
            }
        }
        if self.play && !editor.scenes.is_empty() {
            self.play = false;
            editor.message_sender.send(Message::SwitchMode);
        }
    }
}

fn main() {
    let (flags, paths): (Vec<_>, Vec<_>) = std::env::args_os().skip(1).partition(|a| a.to_string_lossy().starts_with("--"));
    if let Some(unknown) = flags.iter().find(|f| *f != "--play") {
        eprintln!("neptune-editor: unknown option {}", unknown.to_string_lossy());
        std::process::exit(1);
    }
    let mut paths = paths.into_iter().map(PathBuf::from);
    let folder = paths.next().unwrap_or_default();
    // move there first, so the log and the editor's settings are kept with the game
    if !folder.as_os_str().is_empty() && std::env::set_current_dir(&folder).is_err() {
        eprintln!("neptune-editor: not a folder: {}", folder.display());
        std::process::exit(1);
    }
    Log::set_file_name("neptune-editor.log");
    // the editor reads its settings from the file, so Play's commands go there
    let mut settings = Settings::load().unwrap_or_default();
    play::set_profiles(&mut settings, &std::env::current_dir().unwrap_or_default());
    settings.force_save();
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("neptune-editor: cannot open a window: {e}");
            std::process::exit(1);
        }
    };
    let startup = StartupData { working_directory: PathBuf::new(), scenes: paths.collect(), named_objects: false };
    let mut editor = Editor::new(Some(startup));
    theme::apply(&mut editor);
    editor.add_editor_plugin(Neptune { play: !flags.is_empty() });
    editor.run(event_loop);
}
