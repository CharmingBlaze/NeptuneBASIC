//! A folder people can unzip and use to make games (`neptune kit`).
//!
//! Copies this `neptune` and `neptune-ide` from the same folder, and adds
//! `START.txt`, `cube.nb`, the `examples` folder (which the IDE's Examples
//! menu lists) and the Blender add-on. The IDE looks for the compiler beside
//! itself, so the folder can be zipped and sent as it is. Levels are made in
//! Blender, which the IDE opens with the add-on loaded.

use std::path::{Path, PathBuf};

const REQUIRED: [&str; 2] = ["neptune", "neptune-ide"];

const START: &str = "\
NeptuneBASIC

Make a game by writing a program and pressing F5.

1. Start Neptune IDE (neptune-ide).
2. Open cube.nb, pick one from the Examples menu, or start a new file.
3. Press F5. A window opens and the program runs.
4. Program > Create Executable makes a folder you can zip and give to
   players. They do not need NeptuneBASIC.
5. Levels are made in Blender (free, https://www.blender.org). Program >
   Open in Blender opens the newest .blend in your game's folder with a
   Neptune tab (press N in Blender's 3D view) for marking solid objects
   and exporting the level. LoadLevelFile loads it.

Nothing else needs installing: not Rust, not Python. Blender's add-on
runs inside Blender. To use the Neptune tab when you start Blender
yourself, install blender/neptune_levels.py in Blender once (Edit >
Preferences > Add-ons > the arrow at the top right > Install from Disk).

Keep these programs in this folder. Moving the IDE away from the compiler
means it can no longer run your program.

Windows
  Unzip the folder and start neptune-ide.exe. If Windows asks before
  opening it, choose Run anyway.

macOS
  macOS blocks programs downloaded from the internet that are not from
  the App Store. Once, in Terminal, go to this folder and run:

    xattr -dr com.apple.quarantine .

  Then start neptune-ide.

Linux
  Running a game uses the system's sound, gamepad and window libraries.
  On Ubuntu or Debian:

    sudo apt-get install libasound2 libudev1 libx11-6 libxcursor1 \
      libxrandr2 libxi6 libxkbcommon0 libgl1

A game built with Create Executable runs on the same kind of computer
it was built on. Build once on Windows, once on Linux and once on macOS
to share a game for all three.
";

fn file_name(stem: &str) -> String {
    if cfg!(windows) { format!("{stem}.exe") } else { stem.to_string() }
}

/// Files the engine and the level editor write into `examples` while they run.
const NOT_EXAMPLES: [&str; 4] = ["data", "saved_level.rgs", "tilemap_saved.txt", "neptune-editor.log"];

/// Copies the compiler and the IDE from `from` into `out`, and the example
/// programs and their media from `examples` when it is given.
pub fn pack(from: &Path, out: &Path, examples: Option<&Path>) -> Result<(), String> {
    if same_folder(from, out) {
        return Err("the output folder is the folder the programs are already in".into());
    }
    let mut missing = Vec::new();
    for stem in REQUIRED {
        if !from.join(file_name(stem)).is_file() {
            missing.push(file_name(stem));
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "missing {} in {}.\nBuild them with: cargo build --release -p neptune-cli -p neptune-ide",
            missing.join(", "),
            from.display()
        ));
    }
    if out.exists() {
        std::fs::remove_dir_all(out).map_err(|e| format!("cannot replace {}: {e}", out.display()))?;
    }
    std::fs::create_dir_all(out.join("blender")).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    for stem in REQUIRED {
        let name = file_name(stem);
        std::fs::copy(from.join(&name), out.join(&name)).map_err(|e| format!("cannot copy {name}: {e}"))?;
    }
    let write = |name: &str, text: &str| std::fs::write(out.join(name), text).map_err(|e| format!("cannot write {name}: {e}"));
    write("START.txt", START)?;
    write("cube.nb", include_str!("../../../examples/cube.nb"))?;
    write("blender/neptune_levels.py", include_str!("../../../tools/blender/neptune_levels.py"))?;
    if let Some(examples) = examples {
        copy_folder(examples, &out.join("examples"), true)?;
    }
    Ok(())
}

fn copy_folder(from: &Path, to: &Path, top: bool) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let entries = std::fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if (top && NOT_EXAMPLES.iter().any(|n| name == *n)) || name == "neptune-editor.log" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_folder(&path, &to.join(&name), false)?;
        } else {
            std::fs::copy(&path, to.join(&name)).map_err(|e| format!("cannot copy {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// The `examples` folder of the NeptuneBASIC source: in the current folder or
/// above the program (`target/release` is two levels down).
fn find_examples(from: &Path) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok();
    cwd.iter()
        .map(PathBuf::as_path)
        .chain(from.ancestors())
        .map(|dir| dir.join("examples"))
        .find(|dir| dir.join("cube.nb").is_file())
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Packs the programs that sit beside this `neptune`.
pub fn pack_beside_self(out: Option<&str>) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    let from = exe.parent().ok_or("this program has no folder")?.to_path_buf();
    let out = match out {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("dist").join(format!("NeptuneBASIC-{}", std::env::consts::OS)),
    };
    let examples = find_examples(&from);
    if examples.is_none() {
        eprintln!("warning: no examples folder found; packing cube.nb only");
    }
    pack(&from, &out, examples.as_deref())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nb-kit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        root
    }

    #[test]
    fn packs_the_compiler_and_ide() {
        let root = scratch("compiler");
        let bin = root.join("bin");
        for stem in REQUIRED {
            std::fs::write(bin.join(file_name(stem)), stem).unwrap();
        }
        let examples = root.join("examples");
        std::fs::create_dir_all(examples.join("media")).unwrap();
        std::fs::create_dir_all(examples.join("data")).unwrap();
        std::fs::write(examples.join("cube.nb"), "Graphics3D").unwrap();
        std::fs::write(examples.join("media").join("a.png"), "png").unwrap();
        std::fs::write(examples.join("data").join("cache"), "x").unwrap();
        std::fs::write(examples.join("saved_level.rgs"), "x").unwrap();
        let out = root.join("out");
        pack(&bin, &out, Some(&examples)).unwrap();
        assert_eq!(std::fs::read_to_string(out.join("examples").join("media").join("a.png")).unwrap(), "png");
        assert!(out.join("examples").join("cube.nb").is_file());
        assert!(!out.join("examples").join("data").exists());
        assert!(!out.join("examples").join("saved_level.rgs").exists());
        let start = std::fs::read_to_string(out.join("START.txt")).unwrap();
        assert!(start.contains("F5"));
        assert!(start.contains("Open in Blender"));
        assert!(std::fs::read_to_string(out.join("cube.nb")).unwrap().contains("Graphics3D"));
        assert!(std::fs::read_to_string(out.join("blender").join("neptune_levels.py")).unwrap().contains("def register"));
        for stem in REQUIRED {
            assert_eq!(std::fs::read(out.join(file_name(stem))).unwrap(), stem.as_bytes());
        }
        assert!(pack(&bin, &bin, None).unwrap_err().contains("already in"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn leaves_out_the_level_editor() {
        let root = scratch("editor");
        let bin = root.join("bin");
        for stem in REQUIRED {
            std::fs::write(bin.join(file_name(stem)), stem).unwrap();
        }
        std::fs::write(bin.join(file_name("neptune-editor")), "editor").unwrap();
        let out = root.join("out");
        pack(&bin, &out, None).unwrap();
        assert!(!out.join("examples").exists());
        assert!(!out.join(file_name("neptune-editor")).exists());
        assert!(!std::fs::read_to_string(out.join("START.txt")).unwrap().contains("Level Editor"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
