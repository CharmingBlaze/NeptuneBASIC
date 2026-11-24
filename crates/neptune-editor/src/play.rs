//! What the editor's Play button runs.
//!
//! The editor plays a level by running a "build profile": commands that must
//! succeed first, then a command that is given `--override-scene <level>`.
//! Fyrox's own profiles call Cargo. Neptune's call `neptune`: `check` on the
//! program, then `run`. A game with a main program plays that program, which
//! reads the level's name from `CommandLine()` if it wants it; "Look Around"
//! plays `preview.nb`, which flies through the level and needs no program.

use std::path::{Path, PathBuf};

use fyroxed_base::settings::Settings;

/// The program "Look Around" runs.
const PREVIEW: &str = include_str!("preview.nb");

/// One choice in the editor's list beside the Play button.
#[derive(Debug, PartialEq)]
pub struct Play {
    pub name: String,
    /// The program `neptune check` and `neptune run` are given.
    pub program: String,
}

/// The program a game folder's Play runs: `main.nb`, or the only program
/// there is. `names` are the files in the folder.
pub fn main_program(names: &[String]) -> Option<String> {
    let programs: Vec<&String> = names.iter().filter(|n| n.to_ascii_lowercase().ends_with(".nb")).collect();
    match programs.iter().find(|n| n.eq_ignore_ascii_case("main.nb")) {
        Some(main) => Some(main.to_string()),
        None if programs.len() == 1 => Some(programs[0].clone()),
        None => None,
    }
}

/// The Play choices: the game's own program first, if it has one.
pub fn plays(main: Option<String>, preview: &str) -> Vec<Play> {
    let game = main.map(|program| Play { name: format!("Run {program}"), program });
    game.into_iter().chain([Play { name: "Look Around".into(), program: preview.into() }]).collect()
}

/// The `neptune` program: the one beside this editor, or the one on the path.
fn neptune() -> String {
    let name = format!("neptune{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join(&name))).filter(|p| p.is_file());
    beside.map_or(name, |p| p.to_string_lossy().into_owned())
}

/// Writes `preview.nb` where `neptune` can run it and returns its path.
fn write_preview() -> PathBuf {
    let path = std::env::temp_dir().join("neptune-editor-preview.nb");
    if let Err(e) = std::fs::write(&path, PREVIEW) {
        eprintln!("neptune-editor: cannot write {}: {e}", path.display());
    }
    path
}

/// Replaces the editor's build profiles with Neptune's for the game in
/// `folder`, keeping the one that was chosen if it is still there.
pub fn set_profiles(settings: &mut Settings, folder: &Path) {
    let names: Vec<String> = std::fs::read_dir(folder)
        .map(|dir| dir.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let neptune = neptune();
    let build = &mut settings.data_mut().build;
    let chosen = build.profiles.get(build.selected_profile).map(|p| p.name.clone());
    let mut profiles = Vec::new();
    for play in plays(main_program(&names), &write_preview().to_string_lossy()) {
        let mut profile = default_like(&build.profiles);
        profile.name = play.name;
        // the editor plays only after a command that succeeds
        let mut check = default_like(&profile.build_commands);
        check.command = neptune.clone();
        check.args = vec!["check".to_string(), play.program.clone()];
        profile.build_commands = vec![check];
        profile.run_command.command = neptune.clone();
        profile.run_command.args = vec!["run".to_string(), play.program];
        // the level follows as the program's own arguments, with no `--` before it
        profile.run_command.skip_passthrough_marker = true;
        profiles.push(profile);
    }
    build.selected_profile = profiles.iter().position(|p| Some(&p.name) == chosen.as_ref()).unwrap_or(0);
    build.profiles = profiles;
}

/// A default value of a list's item type. The profile types belong to a
/// crate the editor depends on, which this one does not name.
fn default_like<T: Default>(_: &[T]) -> T {
    T::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_games_program() {
        assert_eq!(main_program(&names(&["a.nb", "Main.nb", "level.rgs"])), Some("Main.nb".into()));
        assert_eq!(main_program(&names(&["game.NB", "notes.txt"])), Some("game.NB".into()));
        assert_eq!(main_program(&names(&["a.nb", "b.nb"])), None);
        assert_eq!(main_program(&names(&["level.rgs"])), None);
    }

    #[test]
    fn play_choices() {
        let both = plays(Some("main.nb".into()), "p.nb");
        assert_eq!(both[0], Play { name: "Run main.nb".into(), program: "main.nb".into() });
        assert_eq!(both[1], Play { name: "Look Around".into(), program: "p.nb".into() });
        assert_eq!(plays(None, "p.nb").len(), 1);
    }

    #[test]
    fn profiles_call_neptune() {
        let folder = std::env::temp_dir().join(format!("neptune_editor_play_{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("main.nb"), "End").unwrap();
        let mut settings = Settings::default();
        set_profiles(&mut settings, &folder);
        std::fs::remove_dir_all(&folder).ok();
        let build = &settings.build;
        assert_eq!(build.profiles.len(), 2);
        assert_eq!(build.selected_profile, 0);
        let game = &build.profiles[0];
        assert_eq!(game.name, "Run main.nb");
        assert_eq!(game.build_commands.len(), 1);
        assert_eq!(game.build_commands[0].args, ["check", "main.nb"]);
        assert_eq!(game.run_command.args, ["run", "main.nb"]);
        assert!(game.run_command.skip_passthrough_marker);
        assert!(game.run_command.command.contains("neptune"));
        assert_eq!(build.profiles[1].name, "Look Around");
        assert!(build.profiles[1].run_command.args[1].ends_with("neptune-editor-preview.nb"));

        // the choice is kept by name when the list is made again
        settings.data_mut().build.selected_profile = 1;
        set_profiles(&mut settings, Path::new("no such folder"));
        assert_eq!(settings.build.profiles.len(), 1);
        assert_eq!(settings.build.selected_profile, 0);
        assert_eq!(settings.build.profiles[0].name, "Look Around");
    }
}
