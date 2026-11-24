//! File names written for Windows, found on Linux and macOS.
//!
//! Blitz3D programs were written on Windows: `LoadImage("Media\Player.PNG")`
//! uses backslashes and often a different letter case from the file on disk.
//! Windows accepts both; Linux accepts neither (and macOS not the
//! backslashes). [`host_path`] turns such a name into the one that exists.

use std::path::{Component, Path, PathBuf};

/// The name to open for `path` on this system, or `None` when `path` can be
/// used as it is (always on Windows). Backslashes become slashes, and each
/// part that does not exist is matched against its folder ignoring case;
/// parts that match nothing are kept, so a new file is created with the
/// program's own spelling inside the folder that exists.
pub fn host_path(path: &str) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let fixed = fix(path);
    (fixed != path).then_some(fixed)
}

fn fix(path: &str) -> String {
    let slashed = path.replace('\\', "/");
    if slashed.is_empty() || Path::new(&slashed).exists() {
        return slashed;
    }
    let mut out = PathBuf::new();
    let mut found = true;
    for part in Path::new(&slashed).components() {
        let Component::Normal(name) = part else {
            out.push(part);
            continue;
        };
        let next = out.join(name);
        if found && !next.exists() {
            let dir = if out.as_os_str().is_empty() { Path::new(".") } else { out.as_path() };
            let wanted = name.to_string_lossy().to_lowercase();
            let other = std::fs::read_dir(dir)
                .ok()
                .and_then(|entries| entries.flatten().map(|e| e.file_name()).find(|n| n.to_string_lossy().to_lowercase() == wanted));
            match other {
                Some(real) => out.push(real),
                None => {
                    found = false;
                    out.push(name);
                }
            }
        } else {
            out.push(name);
        }
    }
    out.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::fix;

    #[test]
    fn finds_windows_spellings() {
        let root = std::env::temp_dir().join(format!("nb_paths_{}", std::process::id()));
        std::fs::create_dir_all(root.join("Media").join("sub")).unwrap();
        std::fs::write(root.join("Media").join("sub").join("Player.png"), "x").unwrap();
        let r = root.to_string_lossy().replace('\\', "/");
        let want = format!("{r}/Media/sub/Player.png");
        // backslashes and the wrong case both find the file
        let found = fix(&format!("{r}/media\\SUB\\player.PNG")).replace('\\', "/");
        if cfg!(any(windows, target_os = "macos")) {
            // case-insensitive file systems: the name exists as written
            assert_eq!(found.to_lowercase(), want.to_lowercase());
        } else {
            assert_eq!(found, want);
        }
        // a new file keeps its own name inside the folder that exists
        assert_eq!(fix(&format!("{r}/media/Save.dat")).replace('\\', "/").to_lowercase(), format!("{r}/media/save.dat").to_lowercase());
        assert!(fix(&format!("{r}/media/Save.dat")).ends_with("Save.dat"));
        // nothing found: the rest is kept as written
        assert!(fix(&format!("{r}/nope\\a.png")).replace('\\', "/").ends_with("nope/a.png"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
