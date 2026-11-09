//! Embeds the manual's chapters, so new chapters appear without code changes.

use std::fmt::Write as _;

fn main() {
    let manual = std::path::Path::new("../../docs/manual");
    println!("cargo:rerun-if-changed=../../docs/manual");
    let mut names: Vec<String> = std::fs::read_dir(manual)
        .expect("docs/manual")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect();
    names.sort();
    let mut out = String::from("pub const CHAPTERS: &[(&str, &str)] = &[\n");
    for n in names {
        let path = std::fs::canonicalize(manual.join(&n)).expect("chapter").display().to_string();
        writeln!(out, "    ({n:?}, include_str!({path:?})),").unwrap();
    }
    out.push_str("];\n");
    let examples = std::path::Path::new("../../docs/examples");
    println!("cargo:rerun-if-changed=../../docs/examples");
    let mut files: Vec<_> = std::fs::read_dir(examples).expect("docs/examples").flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "txt")).collect();
    files.sort();
    out.push_str("pub const EXAMPLE_FILES: &[&str] = &[\n");
    for p in files {
        let path = std::fs::canonicalize(p).expect("examples").display().to_string();
        writeln!(out, "    include_str!({path:?}),").unwrap();
    }
    out.push_str("];\n");
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("manual.rs");
    std::fs::write(dest, out).expect("write manual table");
}
