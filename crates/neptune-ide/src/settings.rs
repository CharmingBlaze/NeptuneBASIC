//! Preferences and the remembered session, kept in one JSON file.

use std::path::PathBuf;

use crate::document::BreakRule;
use crate::json::{self, n, obj, parse, s, Json};

#[derive(Clone, PartialEq)]
pub struct Settings {
    pub theme: String,
    pub font_size: f32,
    pub ui_scale: f32,
    pub tab_size: usize,
    pub use_spaces: bool,
    pub autocap: bool,
    pub auto_pairs: bool,
    pub auto_indent: bool,
    pub completion: bool,
    pub minimap: bool,
    pub whitespace: bool,
    pub indent_guides: bool,
    pub inline_errors: bool,
    pub caret_blink: bool,
    pub trim_on_save: bool,
    pub save_before_run: bool,
    pub debug_enabled: bool,
    pub quality: String,
    pub neptune_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "Neptune Deep".into(),
            font_size: 14.0,
            ui_scale: 1.0,
            tab_size: 4,
            use_spaces: false,
            autocap: true,
            auto_pairs: true,
            auto_indent: true,
            completion: true,
            minimap: true,
            whitespace: false,
            indent_guides: true,
            inline_errors: true,
            caret_blink: true,
            trim_on_save: false,
            save_before_run: true,
            debug_enabled: true,
            quality: String::new(),
            neptune_path: String::new(),
        }
    }
}

#[derive(Clone, Default)]
pub struct SavedDoc {
    pub path: String,
    /// Unsaved text, kept so closing the IDE never loses work.
    pub text: Option<String>,
    pub untitled: usize,
    pub line: usize,
    pub col: usize,
    pub breakpoints: Vec<usize>,
    pub break_rules: Vec<(usize, BreakRule)>,
    pub bookmarks: Vec<usize>,
}

#[derive(Clone, Default)]
pub struct Session {
    pub folder: String,
    pub docs: Vec<SavedDoc>,
    pub active: usize,
    pub recent: Vec<String>,
    pub command_line: String,
    pub window: Option<(f32, f32)>,
    pub maximized: bool,
    pub sidebar: Option<bool>,
    pub bottom: Option<bool>,
    pub reference: Option<bool>,
}

pub fn config_dir() -> PathBuf {
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if let Some(dir) = var("NEPTUNE_IDE_CONFIG") {
        return dir;
    }
    let base = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("NeptuneBASIC")
}

fn file() -> PathBuf {
    config_dir().join("ide.json")
}

fn b(v: bool) -> Json {
    Json::Bool(v)
}

impl Settings {
    fn to_json(&self) -> Json {
        obj([
            ("theme", s(self.theme.clone())),
            ("fontSize", Json::Num(self.font_size as f64)),
            ("uiScale", Json::Num(self.ui_scale as f64)),
            ("tabSize", n(self.tab_size as u32)),
            ("useSpaces", b(self.use_spaces)),
            ("autocap", b(self.autocap)),
            ("autoPairs", b(self.auto_pairs)),
            ("autoIndent", b(self.auto_indent)),
            ("completion", b(self.completion)),
            ("minimap", b(self.minimap)),
            ("whitespace", b(self.whitespace)),
            ("indentGuides", b(self.indent_guides)),
            ("inlineErrors", b(self.inline_errors)),
            ("caretBlink", b(self.caret_blink)),
            ("trimOnSave", b(self.trim_on_save)),
            ("saveBeforeRun", b(self.save_before_run)),
            ("debugEnabled", b(self.debug_enabled)),
            ("quality", s(self.quality.clone())),
            ("neptunePath", s(self.neptune_path.clone())),
        ])
    }

    fn from_json(j: &Json) -> Self {
        let d = Settings::default();
        let flag = |k: &str, v: bool| match j.get(k) {
            Json::Bool(x) => *x,
            _ => v,
        };
        let text = |k: &str, v: &str| j.get(k).str().unwrap_or(v).to_string();
        let num = |k: &str, v: f32| j.get(k).num().map_or(v, |x| x as f32);
        Settings {
            theme: text("theme", &d.theme),
            font_size: num("fontSize", d.font_size).clamp(8.0, 40.0),
            ui_scale: num("uiScale", d.ui_scale).clamp(0.7, 2.5),
            tab_size: (num("tabSize", d.tab_size as f32) as usize).clamp(1, 16),
            use_spaces: flag("useSpaces", d.use_spaces),
            autocap: flag("autocap", d.autocap),
            auto_pairs: flag("autoPairs", d.auto_pairs),
            auto_indent: flag("autoIndent", d.auto_indent),
            completion: flag("completion", d.completion),
            minimap: flag("minimap", d.minimap),
            whitespace: flag("whitespace", d.whitespace),
            indent_guides: flag("indentGuides", d.indent_guides),
            inline_errors: flag("inlineErrors", d.inline_errors),
            caret_blink: flag("caretBlink", d.caret_blink),
            trim_on_save: flag("trimOnSave", d.trim_on_save),
            save_before_run: flag("saveBeforeRun", d.save_before_run),
            debug_enabled: flag("debugEnabled", d.debug_enabled),
            quality: text("quality", ""),
            neptune_path: text("neptunePath", ""),
        }
    }
}

fn nums(v: &[usize]) -> Json {
    Json::Arr(v.iter().map(|&x| n(x as u32)).collect())
}

fn read_nums(j: &Json) -> Vec<usize> {
    j.arr().iter().filter_map(|x| x.num().map(|v| v as usize)).collect()
}

impl Session {
    fn to_json(&self) -> Json {
        let docs = self
            .docs
            .iter()
            .map(|d| {
                let mut o = obj([
                    ("path", s(d.path.clone())),
                    ("untitled", n(d.untitled as u32)),
                    ("line", n(d.line as u32)),
                    ("col", n(d.col as u32)),
                    ("breakpoints", nums(&d.breakpoints)),
                    ("bookmarks", nums(&d.bookmarks)),
                ]);
                if let (Json::Obj(m), Some(t)) = (&mut o, &d.text) {
                    m.insert("text".into(), s(t.clone()));
                }
                if let (Json::Obj(m), false) = (&mut o, d.break_rules.is_empty()) {
                    let rules = d.break_rules.iter().map(|(l, r)| obj([("line", n(*l as u32)), ("condition", s(r.condition.clone())), ("hits", s(r.hits.clone()))])).collect();
                    m.insert("breakRules".into(), Json::Arr(rules));
                }
                o
            })
            .collect();
        let mut o = obj([
            ("folder", s(self.folder.clone())),
            ("docs", Json::Arr(docs)),
            ("active", n(self.active as u32)),
            ("recent", Json::Arr(self.recent.iter().map(|r| s(r.clone())).collect())),
            ("commandLine", s(self.command_line.clone())),
            ("maximized", b(self.maximized)),
        ]);
        if let Json::Obj(m) = &mut o {
            if let Some((w, h)) = self.window {
                m.insert("width".into(), Json::Num(w as f64));
                m.insert("height".into(), Json::Num(h as f64));
            }
            for (k, v) in [("sidebar", self.sidebar), ("bottom", self.bottom), ("reference", self.reference)] {
                if let Some(v) = v {
                    m.insert(k.into(), b(v));
                }
            }
        }
        o
    }

    fn from_json(j: &Json) -> Self {
        let opt = |k: &str| match j.get(k) {
            Json::Bool(x) => Some(*x),
            _ => None,
        };
        Session {
            folder: j.get("folder").str().unwrap_or("").to_string(),
            docs: j
                .get("docs")
                .arr()
                .iter()
                .map(|d| SavedDoc {
                    path: d.get("path").str().unwrap_or("").to_string(),
                    text: d.get("text").str().map(str::to_string),
                    untitled: d.get("untitled").num().unwrap_or(0.0) as usize,
                    line: d.get("line").num().unwrap_or(0.0) as usize,
                    col: d.get("col").num().unwrap_or(0.0) as usize,
                    breakpoints: read_nums(d.get("breakpoints")),
                    break_rules: d
                        .get("breakRules")
                        .arr()
                        .iter()
                        .map(|r| (r.get("line").num().unwrap_or(0.0) as usize, BreakRule { condition: r.get("condition").str().unwrap_or("").to_string(), hits: r.get("hits").str().unwrap_or("").to_string() }))
                        .collect(),
                    bookmarks: read_nums(d.get("bookmarks")),
                })
                .collect(),
            active: j.get("active").num().unwrap_or(0.0) as usize,
            recent: j.get("recent").arr().iter().filter_map(|r| r.str().map(str::to_string)).collect(),
            command_line: j.get("commandLine").str().unwrap_or("").to_string(),
            window: j.get("width").num().zip(j.get("height").num()).map(|(w, h)| (w as f32, h as f32)),
            maximized: opt("maximized").unwrap_or(false),
            sidebar: opt("sidebar"),
            bottom: opt("bottom"),
            reference: opt("reference"),
        }
    }
}

pub fn load() -> (Settings, Session) {
    let Ok(text) = std::fs::read_to_string(file()) else { return Default::default() };
    let Ok(j) = parse(&text) else { return Default::default() };
    (Settings::from_json(j.get("settings")), Session::from_json(j.get("session")))
}

/// Writes through a temporary file, so a crash cannot leave half a file.
pub fn store(settings: &Settings, session: &Session) {
    let text = json::obj([("settings", settings.to_json()), ("session", session.to_json())]).to_string();
    std::thread::spawn(move || {
        let path = file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_a_round_trip() {
        let st = Settings { theme: "Graphite".into(), tab_size: 2, minimap: false, ..Default::default() };
        let back = Settings::from_json(&parse(&st.to_json().to_string()).unwrap());
        assert!(back == st);
    }

    #[test]
    fn breakpoint_conditions_are_remembered() {
        let rule = BreakRule { condition: "a\\b = \"x\"".into(), hits: ">=3".into() };
        let mut se = Session::default();
        se.docs.push(SavedDoc { breakpoints: vec![4], break_rules: vec![(4, rule.clone())], ..Default::default() });
        let back = Session::from_json(&parse(&se.to_json().to_string()).unwrap());
        assert_eq!(back.docs[0].break_rules, vec![(4, rule)]);
        // sessions saved before conditions existed still load
        let old = Session::from_json(&parse(r#"{"docs":[{"breakpoints":[1]}]}"#).unwrap());
        assert!(old.docs[0].break_rules.is_empty());
    }

    #[test]
    fn unsaved_text_is_remembered() {
        let mut se = Session::default();
        se.docs.push(SavedDoc { text: Some("Print \"hi\"\n".into()), breakpoints: vec![3], ..Default::default() });
        se.window = Some((800.0, 600.0));
        let back = Session::from_json(&parse(&se.to_json().to_string()).unwrap());
        assert_eq!(back.docs[0].text.as_deref(), Some("Print \"hi\"\n"));
        assert_eq!(back.docs[0].breakpoints, vec![3]);
        assert_eq!(back.window, Some((800.0, 600.0)));
    }
}
