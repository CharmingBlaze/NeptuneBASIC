//! Settings and save data (ext): `SetSetting`, `GetSetting`,
//! `SaveSettings`, `LoadSettings`, ... and `UserFolder`.
//!
//! Settings are named text values kept in memory and saved to a plain text
//! file, one `name = value` line each, so options, high scores and simple
//! save games need no file handling. Names are not case sensitive and may
//! use dots to group them (`audio.music`, `slot1.level`). `UserFolder`
//! gives the folder where a game should keep such files: the user's own
//! application data folder, which exists on every system and is writable.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Settings in memory, by lower-case name.
#[derive(Debug, Default)]
pub struct Settings {
    values: BTreeMap<String, String>,
}

fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// Escapes a value for a one-line file entry.
fn escape(v: &str) -> String {
    let mut s = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c => s.push(c),
        }
    }
    s
}

fn unescape(v: &str) -> String {
    let mut s = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            s.push(c);
            continue;
        }
        match it.next() {
            Some('n') => s.push('\n'),
            Some('r') => s.push('\r'),
            Some('t') => s.push('\t'),
            Some(o) => s.push(o),
            None => s.push('\\'),
        }
    }
    s
}

impl Settings {
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let k = key(name);
        if k.is_empty() || k.contains('=') {
            return Err(format!("'{name}' cannot be a setting name"));
        }
        self.values.insert(k, value.to_string());
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(&key(name)).map(String::as_str)
    }

    pub fn delete(&mut self, name: &str) {
        self.values.remove(&key(name));
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }

    pub fn to_text(&self) -> String {
        self.values.iter().map(|(k, v)| format!("{k} = {}\n", escape(v))).collect()
    }

    /// Adds the `name = value` lines of `text` (lines starting with `;` or
    /// `#`, and blank lines, are skipped).
    pub fn load_text(&mut self, text: &str) -> Result<(), String> {
        for (i, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.trim().is_empty() || t.starts_with(';') || t.starts_with('#') {
                continue;
            }
            let (k, v) = t.split_once('=').ok_or_else(|| format!("line {}: expected name = value", i + 1))?;
            let v = v.strip_prefix(' ').unwrap_or(v);
            self.set(k, &unescape(v.trim_end_matches(['\r', '\n'])))?;
        }
        Ok(())
    }
}

/// The user's application data folder for `app` (created), ending in a
/// path separator: `%APPDATA%\app` on Windows, `~/Library/Application
/// Support/app` on macOS, `$XDG_DATA_HOME/app` or `~/.local/share/app`
/// elsewhere.
pub fn user_folder(app: &str) -> Result<String, String> {
    let app: String = app.trim().chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let app = if app.is_empty() { "NeptuneBASIC".to_string() } else { app };
    let home = || std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let base: Option<PathBuf> = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| home().map(|h| h.join("AppData").join("Roaming")))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| home().map(|h| h.join(".local").join("share")))
    };
    let dir = base.unwrap_or_else(std::env::temp_dir).join(app);
    std::fs::create_dir_all(&dir).map_err(|e| format!("UserFolder: cannot make '{}': {e}", dir.display()))?;
    Ok(format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_awkward_values() {
        let mut s = Settings::default();
        s.set("Audio.Music", "0.8").unwrap();
        s.set("name", "Ann = \"the\\best\"\nline two").unwrap();
        assert!(s.set("", "x").is_err());
        let text = s.to_text();
        let mut t = Settings::default();
        t.load_text(&text).unwrap();
        assert_eq!(t.get("AUDIO.music"), Some("0.8"));
        assert_eq!(t.get("name"), Some("Ann = \"the\\best\"\nline two"));
        assert!(t.load_text("no equals here").is_err());
    }

    #[test]
    fn user_folder_exists() {
        let f = user_folder("NeptuneBASIC tests").unwrap();
        assert!(std::path::Path::new(&f).is_dir());
        assert!(f.ends_with(std::path::MAIN_SEPARATOR));
        std::fs::remove_dir(&f).ok();
    }
}
