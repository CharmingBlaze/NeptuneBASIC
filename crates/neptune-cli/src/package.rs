//! Stand-alone games (`neptune build`).
//!
//! A packaged game is a copy of the `neptune` executable with the program's
//! source files appended to it, plus the media files the program names, laid
//! out in the same relative folders. At start-up the executable looks for
//! that appended payload; if it finds one it changes to its own folder and
//! runs the embedded program, so the folder can be zipped and shared.
//!
//! Payload layout (appended to the executable):
//! `entries … | u64 payload length | MAGIC`, where each entry is three
//! length-prefixed UTF-8 strings: include key, canonical name, text. The
//! first entry is the main program (key = its file name).
//!
//! On Windows the packaged executable is switched to the GUI subsystem so no
//! console window opens next to the game (`--console` keeps it); errors are
//! then shown in a message box.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use neptune_lang::{FsLoader, SourceLoader};

const MAGIC: &[u8] = b"NEPTUNEPKG1";

/// One source file: how includes asked for it, its canonical name and text.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub canon: String,
    pub text: String,
}

fn include_key(path: &str, from: &str) -> String {
    format!("{from}\u{1f}{path}")
}

/// Loader that reads files and remembers everything it returned.
pub struct RecordingLoader {
    pub entries: Vec<Entry>,
}

impl SourceLoader for RecordingLoader {
    fn load(&mut self, path: &str, from: &str) -> Result<(String, String), String> {
        let (canon, text) = FsLoader.load(path, from)?;
        self.entries.push(Entry { key: include_key(path, from), canon: canon.clone(), text: text.clone() });
        Ok((canon, text))
    }
}

/// Loader that serves includes from a payload.
pub struct BundleLoader {
    pub entries: Vec<Entry>,
}

impl SourceLoader for BundleLoader {
    fn load(&mut self, path: &str, from: &str) -> Result<(String, String), String> {
        let key = include_key(path, from);
        self.entries
            .iter()
            .find(|e| e.key == key)
            .map(|e| (e.canon.clone(), e.text.clone()))
            .ok_or_else(|| format!("include file '{path}' is missing from the package"))
    }
}

pub fn encode(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        for s in [&e.key, &e.canon, &e.text] {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
    }
    let len = out.len() as u64;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(MAGIC);
    out
}

/// Finds and decodes a payload at the end of `bytes`.
pub fn decode(bytes: &[u8]) -> Option<Vec<Entry>> {
    let n = bytes.len();
    if n < MAGIC.len() + 8 || &bytes[n - MAGIC.len()..] != MAGIC {
        return None;
    }
    let len_at = n - MAGIC.len() - 8;
    let len = u64::from_le_bytes(bytes[len_at..len_at + 8].try_into().ok()?) as usize;
    let mut data = bytes.get(len_at.checked_sub(len)?..len_at)?;
    let mut read = || -> Option<String> {
        let l = u32::from_le_bytes(data.get(..4)?.try_into().ok()?) as usize;
        let s = String::from_utf8(data.get(4..4 + l)?.to_vec()).ok()?;
        data = &data[4 + l..];
        Some(s)
    };
    let mut entries = Vec::new();
    while let Some(key) = read() {
        let canon = read()?;
        let text = read()?;
        entries.push(Entry { key, canon, text });
    }
    Some(entries)
}

/// The payload of the running executable, if it is a packaged game.
pub fn embedded() -> Option<Vec<Entry>> {
    let exe = std::env::current_exe().ok()?;
    let bytes = std::fs::read(exe).ok()?;
    decode(&bytes)
}

/// String literals in BASIC source (`"..."`, with `""` as an escaped quote).
pub fn string_literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let mut chars = line.chars().peekable();
        let mut in_str = false;
        let mut cur = String::new();
        while let Some(c) = chars.next() {
            if in_str {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        cur.push('"');
                    } else {
                        in_str = false;
                        out.push(std::mem::take(&mut cur));
                    }
                } else {
                    cur.push(c);
                }
            } else if c == '"' {
                in_str = true;
            } else if c == ';' || c == '\'' {
                break; // comment
            }
        }
    }
    out
}

/// Files a glTF file refers to (`"uri": "..."` entries that are not data URIs).
fn gltf_uris(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("\"uri\"") {
        rest = &rest[i + 5..];
        let Some(q1) = rest.find('"') else { break };
        let after = &rest[q1 + 1..];
        let Some(q2) = after.find('"') else { break };
        let uri = &after[..q2];
        if !uri.starts_with("data:") {
            out.push(uri.replace("%20", " "));
        }
        rest = &after[q2 + 1..];
    }
    out
}

/// Media files named by string literals that exist relative to `root`.
pub fn media_files(entries: &[Entry], root: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    for e in entries {
        for lit in string_literals(&e.text) {
            if lit.is_empty() || lit.contains('\n') {
                continue;
            }
            let rel = PathBuf::from(&lit);
            if rel.is_absolute() || lit.contains("..") {
                continue;
            }
            if root.join(&rel).is_file() {
                found.insert(rel.clone());
                if rel.extension().is_some_and(|x| x.eq_ignore_ascii_case("gltf")) {
                    if let Ok(text) = std::fs::read_to_string(root.join(&rel)) {
                        let dir = rel.parent().unwrap_or(Path::new(""));
                        for uri in gltf_uris(&text) {
                            let p = dir.join(uri);
                            if root.join(&p).is_file() {
                                found.insert(p);
                            }
                        }
                    }
                }
            }
        }
    }
    found
}

/// Switches a Windows executable between the console (3) and GUI (2)
/// subsystems by editing its PE header. Returns false for non-PE files.
pub fn set_pe_subsystem(exe: &mut [u8], gui: bool) -> bool {
    if exe.len() < 0x40 || &exe[..2] != b"MZ" {
        return false;
    }
    let pe = u32::from_le_bytes(exe[0x3C..0x40].try_into().unwrap()) as usize;
    // signature (4) + COFF header (20), then the optional header; Subsystem is
    // at offset 68 in both PE32 and PE32+
    let field = pe + 24 + 68;
    if exe.len() < field + 2 || &exe[pe..pe + 4] != b"PE\0\0" {
        return false;
    }
    let value: u16 = if gui { 2 } else { 3 };
    exe[field..field + 2].copy_from_slice(&value.to_le_bytes());
    true
}

/// `neptune build`: writes `<out>/<name>.exe` and the media it uses.
pub fn build(main: &str, out: Option<&str>, console: bool) -> Result<PathBuf, String> {
    let src = std::fs::read_to_string(main).map_err(|e| format!("cannot read '{main}': {e}"))?;
    let mut loader = RecordingLoader { entries: Vec::new() };
    neptune_runtime::compile_source(main, &src, &mut loader).map_err(|e| e.to_string())?;
    let mut entries = vec![Entry { key: main.to_string(), canon: main.to_string(), text: src }];
    entries.extend(loader.entries);

    let stem = Path::new(main).file_stem().and_then(|s| s.to_str()).unwrap_or("game").to_string();
    let out_dir = PathBuf::from(out.map(str::to_string).unwrap_or_else(|| format!("dist/{stem}")));
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("cannot create '{}': {e}", out_dir.display()))?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut bytes = std::fs::read(&exe).map_err(|e| format!("cannot read '{}': {e}", exe.display()))?;
    // building from an already packaged game: drop its payload first
    if decode(&bytes).is_some() {
        let n = bytes.len();
        let len_at = n - MAGIC.len() - 8;
        let len = u64::from_le_bytes(bytes[len_at..len_at + 8].try_into().unwrap()) as usize;
        bytes.truncate(len_at - len);
    }
    bytes.extend(encode(&entries));
    if cfg!(windows) {
        set_pe_subsystem(&mut bytes, !console);
    }
    let ext = exe.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    let target = out_dir.join(format!("{stem}{ext}"));
    std::fs::write(&target, bytes).map_err(|e| format!("cannot write '{}': {e}", target.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }

    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut copied: BTreeMap<PathBuf, u64> = BTreeMap::new();
    for rel in media_files(&entries, &root) {
        let dst = out_dir.join(&rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let n = std::fs::copy(root.join(&rel), &dst).map_err(|e| format!("cannot copy '{}': {e}", rel.display()))?;
        copied.insert(rel, n);
    }
    for (f, n) in &copied {
        println!("  media {} ({} KB)", f.display(), n.div_ceil(1024));
    }
    // plugins go into the game's own plugins folder
    let loaded = neptune_vm::plugin::loaded();
    if !loaded.is_empty() {
        let dir = out_dir.join("plugins");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for (path, _) in loaded {
            let p = Path::new(&path);
            if let Some(name) = p.file_name() {
                std::fs::copy(p, dir.join(name)).map_err(|e| format!("cannot copy plugin '{path}': {e}"))?;
                println!("  plugin {}", name.to_string_lossy());
            }
        }
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_round_trip() {
        let entries = vec![
            Entry { key: "main.nb".into(), canon: "main.nb".into(), text: "Print \"hi\"".into() },
            Entry { key: "main.nb\u{1f}util.nb".into(), canon: "/x/util.nb".into(), text: "".into() },
        ];
        let mut exe = b"MZ fake executable".to_vec();
        exe.extend(encode(&entries));
        assert_eq!(decode(&exe), Some(entries));
        assert_eq!(decode(b"MZ plain executable"), None);
    }

    #[test]
    fn finds_string_literals_but_not_comments() {
        let src = "a = LoadImage(\"gfx/a.png\") ; \"comment.png\"\nText 0,0,\"say \"\"hi\"\"\"\n' \"also comment\"";
        assert_eq!(string_literals(src), vec!["gfx/a.png".to_string(), "say \"hi\"".to_string()]);
    }

    #[test]
    fn pe_subsystem_switch() {
        let mut exe = vec![0u8; 0x200];
        exe[..2].copy_from_slice(b"MZ");
        exe[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        exe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let field = 0x80 + 24 + 68;
        exe[field] = 3;
        assert!(set_pe_subsystem(&mut exe, true));
        assert_eq!(exe[field], 2);
        assert!(set_pe_subsystem(&mut exe, false));
        assert_eq!(exe[field], 3);
        assert!(!set_pe_subsystem(&mut b"ELF".to_vec(), true));
    }

    #[test]
    fn gltf_buffers_are_found() {
        let t = r#"{"buffers":[{"uri":"pyramid.bin"}],"images":[{"uri":"data:image/png;base64,AAA"},{"uri":"tex%20a.png"}]}"#;
        assert_eq!(gltf_uris(t), vec!["pyramid.bin".to_string(), "tex a.png".to_string()]);
    }
}
